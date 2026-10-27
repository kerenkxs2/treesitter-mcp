use super::*;

pub(crate) fn extract_go_types(source: &str, relative_path: &Path) -> Result<Vec<TypeDefinition>> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_go::LANGUAGE.into())
        .wrap_err("Failed to configure Go parser")?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| eyre::eyre!("Failed to parse Go source"))?;

    let query_src = r#"
        (type_spec name: (type_identifier) @name type: (struct_type) @struct) @struct_spec
        (type_spec name: (type_identifier) @name type: (interface_type) @iface) @iface_spec
    "#;

    let query = Query::new(&tree_sitter_go::LANGUAGE.into(), query_src)
        .wrap_err("Failed to compile Go query")?;

    let source_bytes = source.as_bytes();
    let file_path = relative_path.to_path_buf();
    let mut definitions = Vec::new();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source_bytes);

    while let Some(match_) = matches.next() {
        let mut name_node = None;
        let mut def_node = None;
        let mut kind = TypeKind::Struct;
        let mut struct_node = None;
        let mut iface_node = None;

        for capture in match_.captures {
            let capture_name = query.capture_names()[capture.index as usize];
            match capture_name {
                "name" => name_node = Some(capture.node),
                "struct_spec" => {
                    def_node = Some(capture.node);
                    kind = TypeKind::Struct;
                }
                "iface_spec" => {
                    def_node = Some(capture.node);
                    kind = TypeKind::Interface;
                }
                "struct" => struct_node = Some(capture.node),
                "iface" => iface_node = Some(capture.node),
                _ => {}
            }
        }

        let Some(name_node) = name_node else {
            continue;
        };
        let Some(def_node) = def_node else {
            continue;
        };
        let Ok(name) = name_node.utf8_text(source_bytes) else {
            continue;
        };

        let mut def = TypeDefinition {
            name: name.to_string(),
            kind,
            file: file_path.clone(),
            line: def_node.start_position().row + 1,
            signature: signature_for(def_node, source_bytes),
            usage_count: 0,
            fields: None,
            variants: None,
            members: None,
        };

        match kind {
            TypeKind::Struct => {
                let Some(struct_node) = struct_node else {
                    definitions.push(def);
                    continue;
                };

                let fields_list =
                    if let Some(fields_list) = struct_node.child_by_field_name("fields") {
                        Some(fields_list)
                    } else {
                        let mut walker = struct_node.walk();
                        let mut children = struct_node.children(&mut walker);
                        children.find(|n| n.kind() == "field_declaration_list")
                    };

                let Some(fields_list) = fields_list else {
                    definitions.push(def);
                    continue;
                };

                let mut fields = Vec::new();
                let mut walker = fields_list.walk();
                for field_decl in fields_list.children(&mut walker) {
                    if field_decl.kind() != "field_declaration" {
                        continue;
                    }

                    let type_annotation = field_decl
                        .child_by_field_name("type")
                        .and_then(|t| t.utf8_text(source_bytes).ok())
                        .unwrap_or_default()
                        .to_string();

                    if type_annotation.is_empty() {
                        continue;
                    }

                    let mut name_walker = field_decl.walk();
                    let names: Vec<String> = field_decl
                        .children(&mut name_walker)
                        .filter(|n| n.kind() == "field_identifier")
                        .filter_map(|n| n.utf8_text(source_bytes).ok().map(|s| s.to_string()))
                        .collect();

                    for name in names {
                        if !name.is_empty() {
                            fields.push(Field {
                                name,
                                type_annotation: type_annotation.clone(),
                            });
                        }
                    }
                }

                if !fields.is_empty() {
                    def.fields = Some(fields);
                }
            }
            TypeKind::Interface => {
                let Some(iface_node) = iface_node else {
                    definitions.push(def);
                    continue;
                };

                let mut members = Vec::new();
                let mut stack = vec![iface_node];
                while let Some(node) = stack.pop() {
                    let mut walker = node.walk();
                    for child in node.named_children(&mut walker) {
                        if child.kind() == "method_elem" {
                            if let Some(name_node) = child.child_by_field_name("name") {
                                let name = name_node
                                    .utf8_text(source_bytes)
                                    .unwrap_or_default()
                                    .to_string();
                                if !name.is_empty() {
                                    members.push(Member {
                                        name,
                                        type_annotation: signature_for(child, source_bytes),
                                    });
                                }
                            }
                        }
                        stack.push(child);
                    }
                }

                if !members.is_empty() {
                    def.members = Some(members);
                }
            }
            _ => {}
        }

        definitions.push(def);
    }

    Ok(definitions)
}

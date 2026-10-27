use super::*;

pub(super) fn extract_csharp_types(
    source: &str,
    relative_path: &Path,
) -> Result<Vec<TypeDefinition>> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
        .wrap_err("Failed to configure C# parser")?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| eyre::eyre!("Failed to parse C# source"))?;

    let query_src = r#"
        (class_declaration name: (identifier) @name) @class
        (interface_declaration name: (identifier) @name) @interface
        (struct_declaration name: (identifier) @name) @struct
        (enum_declaration name: (identifier) @name) @enum
        (record_declaration name: (identifier) @name) @record
        (using_directive (name_equals (identifier) @name)) @alias
    "#;

    // Some versions of tree-sitter-c-sharp have different `using` alias shapes.
    // If the alias query doesn't compile, fall back to the core type patterns.
    let query_src_fallback = r#"
        (class_declaration name: (identifier) @name) @class
        (interface_declaration name: (identifier) @name) @interface
        (struct_declaration name: (identifier) @name) @struct
        (enum_declaration name: (identifier) @name) @enum
        (record_declaration name: (identifier) @name) @record
    "#;

    let query = Query::new(&tree_sitter_c_sharp::LANGUAGE.into(), query_src)
        .or_else(|_| Query::new(&tree_sitter_c_sharp::LANGUAGE.into(), query_src_fallback))
        .wrap_err("Failed to compile C# query")?;

    let source_bytes = source.as_bytes();
    let file_path = relative_path.to_path_buf();
    let mut definitions = Vec::new();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source_bytes);

    while let Some(match_) = matches.next() {
        let mut name_node = None;
        let mut def_node = None;
        let mut kind = TypeKind::Class;
        let mut is_alias = false;

        for capture in match_.captures {
            let capture_name = query.capture_names()[capture.index as usize];
            match capture_name {
                "name" => name_node = Some(capture.node),
                "class" => {
                    def_node = Some(capture.node);
                    kind = TypeKind::Class;
                }
                "interface" => {
                    def_node = Some(capture.node);
                    kind = TypeKind::Interface;
                }
                "struct" => {
                    def_node = Some(capture.node);
                    kind = TypeKind::Struct;
                }
                "enum" => {
                    def_node = Some(capture.node);
                    kind = TypeKind::Enum;
                }
                "record" => {
                    def_node = Some(capture.node);
                    kind = TypeKind::Record;
                }
                "alias" => {
                    def_node = Some(capture.node);
                    kind = TypeKind::TypeAlias;
                    is_alias = true;
                }
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

        if is_alias {
            definitions.push(TypeDefinition {
                name: name.to_string(),
                kind,
                file: file_path.clone(),
                line: def_node.start_position().row + 1,
                signature: signature_for(def_node, source_bytes),
                usage_count: 0,
                fields: None,
                variants: None,
                members: None,
            });
            continue;
        }

        let mut fields = None;
        let mut members = None;
        let mut variants = None;

        if let Some(body) = def_node.child_by_field_name("body") {
            let mut f = Vec::new();
            let mut m = Vec::new();
            let mut v = Vec::new();
            let mut walker = body.walk();

            for child in body.children(&mut walker) {
                match child.kind() {
                    "field_declaration" => {
                        if let Some(type_node) = child.child_by_field_name("type") {
                            let type_str = type_node.utf8_text(source_bytes).unwrap_or_default();
                            if let Some(declarators) = child.child_by_field_name("declarators") {
                                let mut dwalker = declarators.walk();
                                for d in declarators.children(&mut dwalker) {
                                    if d.kind() == "variable_declarator" {
                                        if let Some(n) = d.child_by_field_name("name") {
                                            f.push(Field {
                                                name: n
                                                    .utf8_text(source_bytes)
                                                    .unwrap_or_default()
                                                    .to_string(),
                                                type_annotation: type_str.to_string(),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                    "property_declaration" => {
                        if let Some(n) = child.child_by_field_name("name") {
                            let type_ann = child
                                .child_by_field_name("type")
                                .and_then(|t| t.utf8_text(source_bytes).ok())
                                .unwrap_or_default();
                            f.push(Field {
                                name: n.utf8_text(source_bytes).unwrap_or_default().to_string(),
                                type_annotation: type_ann.to_string(),
                            });
                        }
                    }
                    "method_declaration" if kind == TypeKind::Interface => {
                        if let Some(n) = child.child_by_field_name("name") {
                            m.push(Member {
                                name: n.utf8_text(source_bytes).unwrap_or_default().to_string(),
                                type_annotation: signature_for(child, source_bytes),
                            });
                        }
                    }
                    "enum_member_declaration" => {
                        if let Some(n) = child.child_by_field_name("name") {
                            v.push(Variant {
                                name: n.utf8_text(source_bytes).unwrap_or_default().to_string(),
                                type_annotation: None,
                            });
                        }
                    }
                    _ => {}
                }
            }
            if !f.is_empty() {
                fields = Some(f);
            }
            if !m.is_empty() {
                members = Some(m);
            }
            if !v.is_empty() {
                variants = Some(v);
            }
        }

        definitions.push(TypeDefinition {
            name: name.to_string(),
            kind,
            file: file_path.clone(),
            line: def_node.start_position().row + 1,
            signature: signature_for(def_node, source_bytes),
            usage_count: 0,
            fields,
            variants,
            members,
        });
    }

    Ok(definitions)
}

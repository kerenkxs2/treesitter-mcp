use super::*;

pub(super) fn extract_java_types(
    source: &str,
    relative_path: &Path,
) -> Result<Vec<TypeDefinition>> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_java::LANGUAGE.into())
        .wrap_err("Failed to configure Java parser")?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| eyre::eyre!("Failed to parse Java source"))?;

    let query_src = r#"
        (class_declaration name: (identifier) @name) @class
        (interface_declaration name: (identifier) @name) @interface
        (enum_declaration name: (identifier) @name) @enum
        (record_declaration name: (identifier) @name) @record
    "#;

    let query = Query::new(&tree_sitter_java::LANGUAGE.into(), query_src)
        .wrap_err("Failed to compile Java query")?;

    let source_bytes = source.as_bytes();
    let file_path = relative_path.to_path_buf();
    let mut definitions = Vec::new();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source_bytes);

    while let Some(match_) = matches.next() {
        let mut name_node = None;
        let mut def_node = None;
        let mut kind = TypeKind::Class;

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
                "enum" => {
                    def_node = Some(capture.node);
                    kind = TypeKind::Enum;
                }
                "record" => {
                    def_node = Some(capture.node);
                    kind = TypeKind::Record;
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
                            let mut dwalker = child.walk();
                            for d in child.children(&mut dwalker) {
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
                    "method_declaration" if kind == TypeKind::Interface => {
                        if let Some(n) = child.child_by_field_name("name") {
                            m.push(Member {
                                name: n.utf8_text(source_bytes).unwrap_or_default().to_string(),
                                type_annotation: signature_for(child, source_bytes),
                            });
                        }
                    }
                    "enum_constant" => {
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

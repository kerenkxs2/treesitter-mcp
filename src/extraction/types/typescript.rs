use super::*;

pub(crate) fn extract_typescript_types(
    source: &str,
    relative_path: &Path,
    is_typescript: bool,
) -> Result<Vec<TypeDefinition>> {
    let mut parser = Parser::new();
    let language = if is_typescript {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
    } else {
        tree_sitter_javascript::LANGUAGE.into()
    };
    parser
        .set_language(&language)
        .wrap_err("Failed to configure TypeScript parser")?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| eyre::eyre!("Failed to parse TypeScript source"))?;

    let query_src = if is_typescript {
        r#"
        (class_declaration name: (type_identifier) @name) @class
        (interface_declaration name: (type_identifier) @name) @interface
        (type_alias_declaration name: (type_identifier) @name) @alias
        (enum_declaration name: (identifier) @name) @enum
        "#
    } else {
        r#"
        (class_declaration name: (identifier) @name) @class
        "#
    };

    let query = Query::new(&language, query_src).wrap_err("Failed to compile TypeScript query")?;
    let source_bytes = source.as_bytes();
    let file_path = relative_path.to_path_buf();
    let mut definitions = Vec::new();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source_bytes);

    while let Some(match_) = matches.next() {
        let mut name_node = None;
        let mut def_node = None;
        let mut kind = TypeKind::TypeAlias;

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
                "alias" => {
                    def_node = Some(capture.node);
                    kind = TypeKind::TypeAlias;
                }
                "enum" => {
                    def_node = Some(capture.node);
                    kind = TypeKind::Enum;
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

        match kind {
            TypeKind::Class => {
                fields = collect_ts_fields(def_node, source_bytes);
            }
            TypeKind::Interface => {
                members = collect_ts_members(def_node, source_bytes);
            }
            TypeKind::Enum => {
                variants = collect_ts_variants(def_node, source_bytes);
            }
            _ => {}
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

pub(super) fn collect_ts_fields(node: Node, source: &[u8]) -> Option<Vec<Field>> {
    let body = node.child_by_field_name("body")?;
    let mut fields = Vec::new();
    let mut walker = body.walk();
    for child in body.children(&mut walker) {
        if matches!(
            child.kind(),
            "public_field_definition" | "field_definition" | "property_signature"
        ) {
            let name = child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or_default()
                .to_string();
            if name.is_empty() {
                continue;
            }

            let type_annotation = child
                .child_by_field_name("type")
                .and_then(|n| n.utf8_text(source).ok())
                .map(clean_type_annotation)
                .unwrap_or_default();

            fields.push(Field {
                name,
                type_annotation,
            });
        }
    }

    if fields.is_empty() {
        None
    } else {
        Some(fields)
    }
}

pub(super) fn collect_ts_members(node: Node, source: &[u8]) -> Option<Vec<Member>> {
    let body = node.child_by_field_name("body")?;
    let mut members = Vec::new();
    let mut walker = body.walk();
    for child in body.children(&mut walker) {
        if matches!(child.kind(), "property_signature" | "method_signature") {
            let name = child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or_default()
                .to_string();
            if name.is_empty() {
                continue;
            }

            let type_annotation = if child.kind() == "property_signature" {
                child
                    .child_by_field_name("type")
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(clean_type_annotation)
                    .unwrap_or_default()
            } else {
                signature_for(child, source)
            };

            members.push(Member {
                name,
                type_annotation,
            });
        }
    }

    if members.is_empty() {
        None
    } else {
        Some(members)
    }
}

pub(super) fn collect_ts_variants(node: Node, source: &[u8]) -> Option<Vec<Variant>> {
    let body = node.child_by_field_name("body")?;
    let mut variants = Vec::new();
    let mut walker = body.walk();
    for child in body.children(&mut walker) {
        if child.kind() == "enum_member" {
            let name = child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or_default()
                .to_string();
            if name.is_empty() {
                continue;
            }
            variants.push(Variant {
                name,
                type_annotation: None,
            });
        }
    }

    if variants.is_empty() {
        None
    } else {
        Some(variants)
    }
}

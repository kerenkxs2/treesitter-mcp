use super::*;

pub(crate) fn extract_rust_types(
    source: &str,
    relative_path: &Path,
) -> Result<Vec<TypeDefinition>> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .wrap_err("Failed to configure Rust parser")?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| eyre::eyre!("Failed to parse Rust source"))?;

    let query_src = r#"
        (struct_item name: (type_identifier) @name) @struct
        (enum_item name: (type_identifier) @name) @enum
        (trait_item name: (type_identifier) @name) @trait
        (type_item name: (type_identifier) @name) @alias
        (impl_item type: (type_identifier) @impl_name) @impl
    "#;

    let query = Query::new(&tree_sitter_rust::LANGUAGE.into(), query_src)
        .wrap_err("Failed to compile Rust query")?;

    let mut type_map: HashMap<String, TypeDefinition> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    let source_bytes = source.as_bytes();
    let file_path = relative_path.to_path_buf();

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source_bytes);
    while let Some(match_) = matches.next() {
        let mut type_name = String::new();
        let mut kind = TypeKind::TypeAlias;
        let mut def_node = None;
        let mut is_impl = false;

        for capture in match_.captures {
            let capture_name = query.capture_names()[capture.index as usize];
            match capture_name {
                "name" => {
                    type_name = capture
                        .node
                        .utf8_text(source_bytes)
                        .unwrap_or_default()
                        .to_string();
                }
                "struct" => {
                    kind = TypeKind::Struct;
                    def_node = Some(capture.node);
                }
                "enum" => {
                    kind = TypeKind::Enum;
                    def_node = Some(capture.node);
                }
                "trait" => {
                    kind = TypeKind::Trait;
                    def_node = Some(capture.node);
                }
                "alias" => {
                    kind = TypeKind::TypeAlias;
                    def_node = Some(capture.node);
                }
                "impl" | "impl_name" => {
                    is_impl = true;
                }
                _ => {}
            }
        }

        if is_impl || type_name.is_empty() {
            continue;
        }

        let Some(node) = def_node else {
            continue;
        };

        if type_map.contains_key(&type_name) {
            continue;
        }

        let mut def = TypeDefinition {
            name: type_name.clone(),
            kind,
            file: file_path.clone(),
            line: node.start_position().row + 1,
            signature: signature_for(node, source_bytes),
            usage_count: 0,
            fields: None,
            variants: None,
            members: None,
        };

        match kind {
            TypeKind::Struct => {
                if let Some(body) = node.child_by_field_name("body") {
                    let mut struct_fields = Vec::new();
                    let mut walker = body.walk();
                    for child in body.children(&mut walker) {
                        if child.kind() == "field_declaration" {
                            let name = child
                                .child_by_field_name("name")
                                .and_then(|n| n.utf8_text(source_bytes).ok())
                                .unwrap_or_default()
                                .to_string();
                            if name.is_empty() {
                                continue;
                            }
                            let type_annotation = child
                                .child_by_field_name("type")
                                .and_then(|n| n.utf8_text(source_bytes).ok())
                                .unwrap_or_default()
                                .to_string();
                            struct_fields.push(Field {
                                name,
                                type_annotation,
                            });
                        }
                    }
                    if !struct_fields.is_empty() {
                        def.fields = Some(struct_fields);
                    }
                }
            }
            TypeKind::Enum => {
                if let Some(body) = node.child_by_field_name("body") {
                    let mut enum_variants = Vec::new();
                    let mut walker = body.walk();
                    for child in body.children(&mut walker) {
                        if child.kind() == "enum_variant" {
                            let name = child
                                .child_by_field_name("name")
                                .and_then(|n| n.utf8_text(source_bytes).ok())
                                .unwrap_or_default()
                                .to_string();
                            if name.is_empty() {
                                continue;
                            }
                            enum_variants.push(Variant {
                                name,
                                type_annotation: None,
                            });
                        }
                    }
                    if !enum_variants.is_empty() {
                        def.variants = Some(enum_variants);
                    }
                }
            }
            TypeKind::Trait => {
                if let Some(body) = node.child_by_field_name("body") {
                    let mut members = Vec::new();
                    let mut walker = body.walk();
                    for child in body.children(&mut walker) {
                        if matches!(
                            child.kind(),
                            "associated_type_declaration"
                                | "associated_type"
                                | "associated_type_item"
                        ) {
                            let name = child
                                .child_by_field_name("name")
                                .and_then(|n| n.utf8_text(source_bytes).ok())
                                .or_else(|| {
                                    let mut walker = child.walk();
                                    let found = child
                                        .children(&mut walker)
                                        .find(|n| n.kind() == "type_identifier")
                                        .and_then(|n| n.utf8_text(source_bytes).ok());
                                    found
                                })
                                .unwrap_or_default()
                                .to_string();
                            if name.is_empty() {
                                continue;
                            }
                            members.push(Member {
                                name,
                                type_annotation: "associated_type".to_string(),
                            });
                            continue;
                        }

                        if child.kind() == "function_item"
                            || child.kind() == "function_signature_item"
                        {
                            let name = child
                                .child_by_field_name("name")
                                .and_then(|n| n.utf8_text(source_bytes).ok())
                                .unwrap_or_default()
                                .to_string();
                            if name.is_empty() {
                                continue;
                            }
                            members.push(Member {
                                name,
                                type_annotation: signature_for(child, source_bytes),
                            });
                        }
                    }
                    if !members.is_empty() {
                        def.members = Some(members);
                    }
                }
            }
            _ => {}
        }

        order.push(type_name.clone());
        type_map.insert(type_name, def);
    }

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source_bytes);
    while let Some(match_) = matches.next() {
        let mut type_name = String::new();
        let mut impl_node = None;
        let mut is_impl = false;

        for capture in match_.captures {
            let capture_name = query.capture_names()[capture.index as usize];
            match capture_name {
                "impl_name" => {
                    type_name = capture
                        .node
                        .utf8_text(source_bytes)
                        .unwrap_or_default()
                        .to_string();
                }
                "impl" => {
                    is_impl = true;
                    impl_node = Some(capture.node);
                }
                _ => {}
            }
        }

        if !is_impl || type_name.is_empty() {
            continue;
        }

        let Some(definition) = type_map.get_mut(&type_name) else {
            continue;
        };
        let Some(node) = impl_node else {
            continue;
        };
        let Some(body) = node.child_by_field_name("body") else {
            continue;
        };

        let mut members = definition.members.take().unwrap_or_default();
        let mut walker = body.walk();
        for child in body.children(&mut walker) {
            if child.kind() == "function_item" {
                let name = child
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source_bytes).ok())
                    .unwrap_or_default()
                    .to_string();
                if name.is_empty() {
                    continue;
                }
                members.push(Member {
                    name,
                    type_annotation: signature_for(child, source_bytes),
                });
            }
        }
        if !members.is_empty() {
            definition.members = Some(members);
        }
    }

    let mut definitions = Vec::new();
    for name in order {
        if let Some(def) = type_map.remove(&name) {
            definitions.push(def);
        }
    }

    Ok(definitions)
}

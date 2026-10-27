use super::*;

/// Extract enhanced shape from Rust source code
pub(super) fn extract_rust_enhanced(
    tree: &Tree,
    source: &str,
    include_code: bool,
) -> Result<EnhancedFileShape, io::Error> {
    let mut functions = Vec::new();
    let mut structs = Vec::new();
    let mut imports = Vec::new();
    let mut impl_blocks = Vec::new();
    let mut traits = Vec::new();

    let query = Query::new(
        &tree_sitter_rust::LANGUAGE.into(),
        r#"
        (function_item name: (identifier) @func.name) @func
        (struct_item name: (type_identifier) @struct.name) @struct
        (use_declaration) @import
        (impl_item) @impl
        (trait_item name: (type_identifier) @trait.name) @trait
        "#,
    )
    .map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to create tree-sitter query: {e}"),
        )
    })?;

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());

    while let Some(match_) = matches.next() {
        for capture in match_.captures {
            let node = capture.node;
            let name_idx = capture.index;
            let capture_name = query.capture_names()[name_idx as usize];

            match capture_name {
                "func.name" => {
                    if let Ok(func_node) = find_parent_by_type(node, "function_item") {
                        if let Ok(name) = node.utf8_text(source.as_bytes()) {
                            let line = func_node.start_position().row + 1;
                            let end_line = func_node.end_position().row + 1;
                            let signature = extract_signature(func_node, source)?;
                            let doc = extract_doc_comment(func_node, source, Language::Rust)?;
                            let code = if include_code {
                                extract_code(func_node, source)?
                            } else {
                                None
                            };

                            functions.push(EnhancedFunctionInfo {
                                name: name.to_string(),
                                signature,
                                line,
                                end_line,
                                doc,
                                code,
                                annotations: vec![],
                            });
                        }
                    }
                }
                "struct.name" => {
                    if let Ok(struct_node) = find_parent_by_type(node, "struct_item") {
                        if let Ok(name) = node.utf8_text(source.as_bytes()) {
                            let line = struct_node.start_position().row + 1;
                            let end_line = struct_node.end_position().row + 1;
                            let doc = extract_doc_comment(struct_node, source, Language::Rust)?;
                            let code = if include_code {
                                extract_code(struct_node, source)?
                            } else {
                                None
                            };

                            structs.push(EnhancedStructInfo {
                                name: name.to_string(),
                                line,
                                end_line,
                                doc,
                                code,
                            });
                        }
                    }
                }
                "import" => {
                    if let Ok(text) = node.utf8_text(source.as_bytes()) {
                        imports.push(ImportInfo {
                            text: text.to_string(),
                            line: node.start_position().row + 1,
                        });
                    }
                }
                "impl" => {
                    if let Ok(impl_info) = extract_impl_block(node, source, include_code) {
                        impl_blocks.push(impl_info);
                    }
                }
                "trait.name" => {
                    if let Ok(trait_node) = find_parent_by_type(node, "trait_item") {
                        if let Ok(trait_info) = extract_trait(trait_node, source, include_code) {
                            traits.push(trait_info);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    Ok(EnhancedFileShape {
        path: None,
        language: None,
        functions,
        structs,
        classes: vec![],
        imports,
        impl_blocks,
        traits,
        interfaces: vec![],
        properties: vec![],
        dependencies: vec![],
    })
}

/// Extract impl block information from a Rust impl_item node
pub(super) fn extract_impl_block(
    node: Node,
    source: &str,
    include_code: bool,
) -> Result<ImplBlockInfo, io::Error> {
    let line = node.start_position().row + 1;
    let end_line = node.end_position().row + 1;

    // Extract type name (e.g., "Calculator" or "Container<T>")
    let type_name = node
        .child_by_field_name("type")
        .and_then(|n| n.utf8_text(source.as_bytes()).ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "Unknown".to_string());

    // Extract trait name if it's a trait impl (e.g., "impl Display for Calculator")
    let trait_name = node
        .child_by_field_name("trait")
        .and_then(|n| n.utf8_text(source.as_bytes()).ok())
        .map(|s| {
            // Extract just the trait name, not the full path
            s.split("::").last().unwrap_or(s).to_string()
        });

    // Extract methods from the impl block body
    let mut methods = Vec::new();
    if let Some(body) = node.child_by_field_name("body") {
        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            if child.kind() == "function_item" {
                if let Ok(method) = extract_method(child, source, include_code) {
                    methods.push(method);
                }
            }
        }
    }

    Ok(ImplBlockInfo {
        type_name,
        trait_name,
        line,
        end_line,
        methods,
    })
}

/// Extract method information from a function_item node within an impl block
pub(super) fn extract_method(
    node: Node,
    source: &str,
    include_code: bool,
) -> Result<MethodInfo, io::Error> {
    let name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source.as_bytes()).ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let signature = extract_signature(node, source)?;
    let line = node.start_position().row + 1;
    let end_line = node.end_position().row + 1;
    let doc = extract_doc_comment(node, source, Language::Rust)?;
    let code = if include_code {
        extract_code(node, source)?
    } else {
        None
    };

    Ok(MethodInfo {
        name,
        signature,
        line,
        end_line,
        doc,
        code,
    })
}

/// Extract trait definition information from a Rust trait_item node
pub(super) fn extract_trait(
    node: Node,
    source: &str,
    include_code: bool,
) -> Result<TraitInfo, io::Error> {
    let name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source.as_bytes()).ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "Unknown".to_string());

    let line = node.start_position().row + 1;
    let end_line = node.end_position().row + 1;
    let doc = extract_doc_comment(node, source, Language::Rust)?;

    // Extract methods from the trait body
    let mut methods = Vec::new();
    if let Some(body) = node.child_by_field_name("body") {
        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            if child.kind() == "function_item" || child.kind() == "function_signature_item" {
                if let Ok(method) = extract_method(child, source, include_code) {
                    methods.push(method);
                }
            }
        }
    }

    Ok(TraitInfo {
        name,
        line,
        end_line,
        doc,
        methods,
    })
}

use super::*;

/// Extract enhanced shape from Go source code
pub(super) fn extract_go_enhanced(
    tree: &Tree,
    source: &str,
    include_code: bool,
) -> Result<EnhancedFileShape, io::Error> {
    let mut functions = Vec::new();
    let mut structs = Vec::new();
    let mut traits = Vec::new();
    let mut imports = Vec::new();
    let mut impl_blocks: Vec<ImplBlockInfo> = Vec::new();

    let query = Query::new(
        &tree_sitter_go::LANGUAGE.into(),
        r#"
        (function_declaration name: (identifier) @func.name) @func
        (method_declaration) @method
        (type_spec name: (type_identifier) @struct.name type: (struct_type)) @struct
        (type_spec name: (type_identifier) @iface.name type: (interface_type)) @iface
        (import_spec path: (interpreted_string_literal) @import.path) @import
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

    let mut processed_function_nodes = std::collections::HashSet::new();
    let mut processed_type_nodes = std::collections::HashSet::new();

    while let Some(match_) = matches.next() {
        for capture in match_.captures {
            let node = capture.node;
            let capture_name = query.capture_names()[capture.index as usize];

            match capture_name {
                "method" => {
                    if let Some((owner, method)) = extract_go_method(node, source, include_code)? {
                        if let Some(block) = impl_blocks
                            .iter_mut()
                            .find(|block| block.type_name == owner)
                        {
                            block.line = block.line.min(method.line);
                            block.end_line = block.end_line.max(method.end_line);
                            block.methods.push(method);
                        } else {
                            impl_blocks.push(ImplBlockInfo {
                                type_name: owner,
                                trait_name: None,
                                line: method.line,
                                end_line: method.end_line,
                                methods: vec![method],
                            });
                        }
                    }
                }
                "func.name" => {
                    if let Ok(func_node) = find_parent_by_type(node, "function_declaration") {
                        let node_id = func_node.id();
                        if processed_function_nodes.contains(&node_id) {
                            continue;
                        }
                        processed_function_nodes.insert(node_id);

                        if let Ok(name) = node.utf8_text(source.as_bytes()) {
                            let mut line = func_node.start_position().row + 1;
                            let end_line = func_node.end_position().row + 1;
                            let signature = extract_signature(func_node, source)?;
                            let doc = extract_doc_comment(func_node, source, Language::Go)?;

                            // Go tests expect the "start line" for functions to include the
                            // immediately preceding doc comment.
                            if let Some(prev) = func_node.prev_sibling() {
                                if prev.kind() == "comment"
                                    && prev.end_position().row + 1 == line.saturating_sub(1)
                                {
                                    line = prev.start_position().row + 1;
                                }
                            }

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
                    // Walk up to the full type_declaration to keep `type X ...` in code.
                    if let Ok(type_decl) = find_parent_by_type(node, "type_declaration") {
                        let node_id = type_decl.id();
                        if processed_type_nodes.contains(&node_id) {
                            continue;
                        }
                        processed_type_nodes.insert(node_id);

                        if let Ok(name) = node.utf8_text(source.as_bytes()) {
                            let line = type_decl.start_position().row + 1;
                            let end_line = type_decl.end_position().row + 1;
                            let doc = extract_doc_comment(type_decl, source, Language::Go)?;
                            let code = if include_code {
                                extract_code(type_decl, source)?
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
                "iface.name" => {
                    if let Ok(type_decl) = find_parent_by_type(node, "type_declaration") {
                        let node_id = type_decl.id();
                        if processed_type_nodes.contains(&node_id) {
                            continue;
                        }
                        processed_type_nodes.insert(node_id);

                        if let Ok(name) = node.utf8_text(source.as_bytes()) {
                            let line = type_decl.start_position().row + 1;
                            let end_line = type_decl.end_position().row + 1;
                            let doc = extract_doc_comment(type_decl, source, Language::Go)?;

                            traits.push(TraitInfo {
                                name: name.to_string(),
                                line,
                                end_line,
                                doc,
                                methods: vec![],
                            });
                        }
                    }
                }
                "import.path" => {
                    if let Ok(text) = node.utf8_text(source.as_bytes()) {
                        imports.push(ImportInfo {
                            text: text.to_string(),
                            line: node.start_position().row + 1,
                        });
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

/// Receiver ownership comes from the method declaration, independently of where
/// the receiver type is declared. Pointer and generic receiver syntax is retained
/// in the method signature; the group is named after the receiver's base type.
fn extract_go_method(
    node: Node<'_>,
    source: &str,
    include_code: bool,
) -> Result<Option<(String, MethodInfo)>, io::Error> {
    let Some(name) = node.child_by_field_name("name") else {
        return Ok(None);
    };
    let Some(receiver) = node.child_by_field_name("receiver") else {
        return Ok(None);
    };
    let receiver_type = receiver
        .named_children(&mut receiver.walk())
        .find_map(|parameter| parameter.child_by_field_name("type"));
    let Some(receiver_type) = receiver_type else {
        return Ok(None);
    };
    let owner = receiver_type
        .utf8_text(source.as_bytes())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
        .trim_start_matches('*')
        .trim()
        .to_string();
    let method = MethodInfo {
        name: name
            .utf8_text(source.as_bytes())
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
            .into(),
        signature: extract_signature(node, source)?,
        line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        doc: extract_doc_comment(node, source, Language::Go)?,
        code: if include_code {
            extract_code(node, source)?
        } else {
            None
        },
    };
    Ok(Some((owner, method)))
}

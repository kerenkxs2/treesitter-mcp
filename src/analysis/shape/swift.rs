use super::*;

/// Extract enhanced shape from Swift source code
pub(super) fn extract_swift_enhanced(
    tree: &Tree,
    source: &str,
    include_code: bool,
) -> Result<EnhancedFileShape, io::Error> {
    let mut functions = Vec::new();
    let mut structs = Vec::new();
    let mut classes = Vec::new();
    let mut imports = Vec::new();
    let mut traits = Vec::new();

    // Use tree-sitter query API for efficient extraction (Swift grammar)
    let query = Query::new(
        &tree_sitter_swift::LANGUAGE.into(),
        r#"
        (function_declaration name: (simple_identifier) @func.name) @func
        (class_declaration name: (type_identifier) @class.name) @class
        (protocol_declaration name: (type_identifier) @protocol.name) @protocol
        (import_declaration) @import
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
                    if let Ok(func_node) = find_parent_by_type(node, "function_declaration") {
                        // Skip functions inside classes/structs
                        if is_inside_class(func_node) {
                            continue;
                        }

                        if let Ok(name) = node.utf8_text(source.as_bytes()) {
                            let line = func_node.start_position().row + 1;
                            let end_line = func_node.end_position().row + 1;
                            let signature = extract_signature(func_node, source)?;
                            let doc = extract_doc_comment(func_node, source, Language::Swift)?;
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
                "class.name" => {
                    if let Ok(class_node) = find_parent_by_type(node, "class_declaration") {
                        // Skip nested classes
                        if is_inside_class(class_node) {
                            continue;
                        }

                        if let Ok(name) = node.utf8_text(source.as_bytes()) {
                            let line = class_node.start_position().row + 1;
                            let end_line = class_node.end_position().row + 1;
                            let doc = extract_doc_comment(class_node, source, Language::Swift)?;
                            let code = if include_code {
                                extract_code(class_node, source)?
                            } else {
                                None
                            };

                            // Check if this is actually a struct (both use class_declaration in Swift grammar)
                            let is_struct = class_node
                                .child(0)
                                .and_then(|first_child| {
                                    first_child.utf8_text(source.as_bytes()).ok()
                                })
                                .map(|text| text.trim_start().starts_with("struct"))
                                .unwrap_or(false);

                            if is_struct {
                                structs.push(EnhancedStructInfo {
                                    name: name.to_string(),
                                    line,
                                    end_line,
                                    doc,
                                    code,
                                });
                            } else {
                                // Extract methods from class body
                                let methods = extract_class_methods(
                                    class_node,
                                    source,
                                    Language::Swift,
                                    include_code,
                                )?;

                                classes.push(EnhancedClassInfo {
                                    name: name.to_string(),
                                    line,
                                    end_line,
                                    doc,
                                    code,
                                    methods,
                                    implements: vec![],
                                    properties: vec![],
                                    fields: vec![],
                                });
                            }
                        }
                    }
                }
                "protocol.name" => {
                    if let Ok(protocol_node) = find_parent_by_type(node, "protocol_declaration") {
                        // Skip nested protocols
                        if is_inside_class(protocol_node) {
                            continue;
                        }

                        if let Ok(name) = node.utf8_text(source.as_bytes()) {
                            let line = protocol_node.start_position().row + 1;
                            let end_line = protocol_node.end_position().row + 1;
                            let doc = extract_doc_comment(protocol_node, source, Language::Swift)?;

                            // Extract methods from protocol body and convert to MethodInfo
                            let enhanced_methods = extract_class_methods(
                                protocol_node,
                                source,
                                Language::Swift,
                                include_code,
                            )?;
                            let methods = enhanced_methods
                                .into_iter()
                                .map(|m| MethodInfo {
                                    name: m.name,
                                    signature: m.signature,
                                    line: m.line,
                                    end_line: m.end_line,
                                    doc: m.doc,
                                    code: m.code,
                                })
                                .collect();

                            traits.push(TraitInfo {
                                name: name.to_string(),
                                line,
                                end_line,
                                doc,
                                methods,
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
                _ => {}
            }
        }
    }

    Ok(EnhancedFileShape {
        path: None,
        language: None,
        functions,
        structs,
        classes,
        imports,
        impl_blocks: vec![],
        traits,
        interfaces: vec![],
        properties: vec![],
        dependencies: vec![],
    })
}

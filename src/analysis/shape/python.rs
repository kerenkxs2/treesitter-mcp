use super::*;

/// Extract enhanced shape from Python source code
pub(super) fn extract_python_enhanced(
    tree: &Tree,
    source: &str,
    include_code: bool,
) -> Result<EnhancedFileShape, io::Error> {
    let mut functions = Vec::new();
    let mut classes = Vec::new();
    let mut imports = Vec::new();

    let query = Query::new(
        &tree_sitter_python::LANGUAGE.into(),
        r#"
        (function_definition name: (identifier) @func.name) @func
        (class_definition name: (identifier) @class.name) @class
        (import_statement) @import
        (import_from_statement) @import
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
                    if let Ok(func_node) = find_parent_by_type(node, "function_definition") {
                        // Skip functions that are inside classes (they'll be extracted as methods)
                        if is_inside_class(func_node) {
                            continue;
                        }

                        if let Ok(name) = node.utf8_text(source.as_bytes()) {
                            let line = func_node.start_position().row + 1;
                            let end_line = func_node.end_position().row + 1;
                            let signature = extract_signature(func_node, source)?;
                            let doc = extract_doc_comment(func_node, source, Language::Python)?;
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
                    if let Ok(class_node) = find_parent_by_type(node, "class_definition") {
                        // Skip nested classes (only extract top-level classes)
                        if is_inside_class(class_node) {
                            continue;
                        }

                        if let Ok(name) = node.utf8_text(source.as_bytes()) {
                            let line = class_node.start_position().row + 1;
                            let end_line = class_node.end_position().row + 1;
                            let doc = extract_doc_comment(class_node, source, Language::Python)?;
                            let code = if include_code {
                                extract_code(class_node, source)?
                            } else {
                                None
                            };

                            // Extract methods from class body (excluding nested classes)
                            let methods = extract_class_methods(
                                class_node,
                                source,
                                Language::Python,
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
        structs: vec![],
        classes,
        imports,
        impl_blocks: vec![],
        traits: vec![],
        interfaces: vec![],
        properties: vec![],
        dependencies: vec![],
    })
}

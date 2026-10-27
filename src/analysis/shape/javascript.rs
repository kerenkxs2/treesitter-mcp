use super::*;

/// Extract enhanced shape from JavaScript/TypeScript source code
pub(super) fn extract_js_enhanced(
    tree: &Tree,
    source: &str,
    language: Language,
    include_code: bool,
) -> Result<EnhancedFileShape, io::Error> {
    let mut functions = Vec::new();
    let mut classes = Vec::new();
    let mut imports = Vec::new();
    let mut interfaces = Vec::new();

    // Use the correct language for the query
    let ts_language = match language {
        Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        _ => tree_sitter_javascript::LANGUAGE.into(),
    };

    // Different query patterns for TypeScript vs JavaScript
    let query_str = match language {
        Language::TypeScript => {
            r#"
        (function_declaration) @func
        (class_declaration) @class
        (interface_declaration name: (type_identifier) @interface.name) @interface
        (import_statement) @import
        "#
        }
        _ => {
            r#"
        (function_declaration name: (identifier) @func.name) @func
        (class_declaration name: (identifier) @class.name) @class
        (import_statement) @import
        "#
        }
    };

    let query = Query::new(&ts_language, query_str).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to create tree-sitter query: {e}"),
        )
    })?;

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());

    // Track processed nodes to avoid duplicates
    let mut processed_func_nodes = std::collections::HashSet::new();
    let mut processed_class_nodes = std::collections::HashSet::new();

    while let Some(match_) = matches.next() {
        for capture in match_.captures {
            let node = capture.node;
            let name_idx = capture.index;
            let capture_name = query.capture_names()[name_idx as usize];

            match capture_name {
                "func.name" => {
                    // JavaScript: named capture for function name
                    if let Ok(func_node) = find_parent_by_type(node, "function_declaration") {
                        let node_id = func_node.id();
                        if !processed_func_nodes.contains(&node_id) {
                            processed_func_nodes.insert(node_id);
                            if let Ok(name) = node.utf8_text(source.as_bytes()) {
                                let line = func_node.start_position().row + 1;
                                let end_line = func_node.end_position().row + 1;
                                let signature = extract_signature(func_node, source)?;
                                let doc =
                                    extract_doc_comment(func_node, source, Language::JavaScript)?;
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
                }
                "func" if node.kind() == "function_declaration" => {
                    // TypeScript: capture the whole function_declaration node
                    let node_id = node.id();
                    if !processed_func_nodes.contains(&node_id) {
                        processed_func_nodes.insert(node_id);
                        // Find the function name
                        if let Some(name_node) = node.child_by_field_name("name") {
                            if let Ok(name) = name_node.utf8_text(source.as_bytes()) {
                                let line = node.start_position().row + 1;
                                let end_line = node.end_position().row + 1;
                                let signature = extract_signature(node, source)?;
                                let doc = extract_doc_comment(node, source, language)?;
                                let code = if include_code {
                                    extract_code(node, source)?
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
                }
                "class.name" => {
                    // JavaScript: named capture for class name
                    if let Ok(class_node) = find_parent_by_type(node, "class_declaration") {
                        let node_id = class_node.id();
                        if !processed_class_nodes.contains(&node_id) {
                            processed_class_nodes.insert(node_id);
                            if let Ok(name) = node.utf8_text(source.as_bytes()) {
                                let line = class_node.start_position().row + 1;
                                let end_line = class_node.end_position().row + 1;
                                let doc =
                                    extract_doc_comment(class_node, source, Language::JavaScript)?;
                                let code = if include_code {
                                    extract_code(class_node, source)?
                                } else {
                                    None
                                };

                                // Extract methods from class body
                                let methods = extract_class_methods(
                                    class_node,
                                    source,
                                    Language::JavaScript,
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
                "class" if node.kind() == "class_declaration" => {
                    // TypeScript: capture the whole class_declaration node
                    let node_id = node.id();
                    if !processed_class_nodes.contains(&node_id) {
                        processed_class_nodes.insert(node_id);
                        if let Some(name_node) = node.child_by_field_name("name") {
                            if let Ok(name) = name_node.utf8_text(source.as_bytes()) {
                                let line = node.start_position().row + 1;
                                let end_line = node.end_position().row + 1;
                                let doc = extract_doc_comment(node, source, language)?;
                                let code = if include_code {
                                    extract_code(node, source)?
                                } else {
                                    None
                                };

                                // Extract methods from class body
                                let methods =
                                    extract_class_methods(node, source, language, include_code)?;

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
                "interface.name" => {
                    if let Ok(interface_node) = find_parent_by_type(node, "interface_declaration") {
                        if let Ok(interface_info) =
                            extract_interface(interface_node, source, include_code)
                        {
                            interfaces.push(interface_info);
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
        interfaces,
        properties: vec![],
        dependencies: vec![],
    })
}

/// Extract interface definition information from a TypeScript interface_declaration node
pub(super) fn extract_interface(
    node: Node,
    source: &str,
    include_code: bool,
) -> Result<InterfaceInfo, io::Error> {
    let name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source.as_bytes()).ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "Unknown".to_string());

    let line = node.start_position().row + 1;
    let end_line = node.end_position().row + 1;
    let doc = extract_doc_comment(node, source, Language::TypeScript)?;

    // Extract methods from the interface body
    let mut methods = Vec::new();
    if let Some(body) = node.child_by_field_name("body") {
        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            // TypeScript interfaces have method_signature and property_signature nodes
            if child.kind() == "method_signature" || child.kind() == "property_signature" {
                if let Some(name_node) = child.child_by_field_name("name") {
                    if let Ok(method_name) = name_node.utf8_text(source.as_bytes()) {
                        let method_line = child.start_position().row + 1;
                        let method_end_line = child.end_position().row + 1;
                        let signature = extract_signature(child, source)?;
                        let method_doc = extract_doc_comment(child, source, Language::TypeScript)?;

                        // Interfaces don't have code bodies, but we respect the include_code flag
                        let code = if include_code {
                            extract_code(child, source)?
                        } else {
                            None
                        };

                        methods.push(EnhancedFunctionInfo {
                            name: method_name.to_string(),
                            signature,
                            line: method_line,
                            end_line: method_end_line,
                            doc: method_doc,
                            code,
                            annotations: vec![],
                        });
                    }
                }
            }
        }
    }

    let code = if include_code {
        extract_code(node, source)?
    } else {
        None
    };

    Ok(InterfaceInfo {
        name,
        line,
        end_line,
        doc,
        code,
        methods,
        properties: vec![],
    })
}

/// Check if a node is inside a class definition
pub(super) fn is_inside_class(node: Node) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == "class_definition"
            || parent.kind() == "class_declaration"
            || parent.kind() == "struct_declaration"
        {
            return true;
        }
        current = parent.parent();
    }
    false
}

/// Extract methods from a class body (Python, JavaScript, TypeScript)
pub(super) fn extract_class_methods(
    class_node: Node,
    source: &str,
    language: Language,
    include_code: bool,
) -> Result<Vec<EnhancedFunctionInfo>, io::Error> {
    let mut methods = Vec::new();

    // Find the class body
    let body = match language {
        Language::Python => class_node.child_by_field_name("body"),
        Language::JavaScript | Language::TypeScript => class_node.child_by_field_name("body"),
        Language::Swift => class_node.child_by_field_name("body"),
        _ => None,
    };

    if let Some(body_node) = body {
        let mut cursor = body_node.walk();
        for child in body_node.children(&mut cursor) {
            // Skip nested classes
            if child.kind() == "class_definition"
                || child.kind() == "class_declaration"
                || child.kind() == "struct_declaration"
            {
                continue;
            }

            let is_method = match language {
                Language::Python => child.kind() == "function_definition",
                Language::JavaScript | Language::TypeScript => {
                    child.kind() == "method_definition" || child.kind() == "function_declaration"
                }
                Language::Swift => child.kind() == "function_declaration",
                _ => false,
            };

            if is_method {
                // Extract method name
                if let Some(name_node) = child.child_by_field_name("name") {
                    if let Ok(name) = name_node.utf8_text(source.as_bytes()) {
                        let line = child.start_position().row + 1;
                        let end_line = child.end_position().row + 1;
                        let signature = extract_signature(child, source)?;
                        let doc = extract_doc_comment(child, source, language)?;
                        let code = if include_code {
                            extract_code(child, source)?
                        } else {
                            None
                        };

                        methods.push(EnhancedFunctionInfo {
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
        }
    }

    Ok(methods)
}

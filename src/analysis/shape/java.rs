use super::*;

/// Extract enhanced shape from Java source code
pub(super) fn extract_java_enhanced(
    tree: &Tree,
    source: &str,
    include_code: bool,
) -> Result<EnhancedFileShape, io::Error> {
    let mut functions = Vec::new();
    let mut classes = Vec::new();
    let mut imports = Vec::new();
    let mut interfaces = Vec::new();

    let ts_language = tree_sitter_java::LANGUAGE.into();

    let query_str = r#"
        (method_declaration) @method
        (class_declaration) @class
        (interface_declaration) @interface
        (import_declaration) @import
    "#;

    let query = Query::new(&ts_language, query_str).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to create tree-sitter query: {e}"),
        )
    })?;

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());

    let mut processed_method_nodes = std::collections::HashSet::new();
    let mut processed_class_nodes = std::collections::HashSet::new();

    while let Some(match_) = matches.next() {
        for capture in match_.captures {
            let node = capture.node;
            let name_idx = capture.index;
            let capture_name = query.capture_names()[name_idx as usize];

            match capture_name {
                "method" => {
                    let node_id = node.id();
                    if !processed_method_nodes.contains(&node_id) {
                        processed_method_nodes.insert(node_id);

                        if let Some(name_node) = node.child_by_field_name("name") {
                            if let Ok(name) = name_node.utf8_text(source.as_bytes()) {
                                let line = node.start_position().row + 1;
                                let end_line = node.end_position().row + 1;
                                let signature = extract_signature(node, source)?;
                                let doc = extract_doc_comment(node, source, Language::Java)?;
                                let code = if include_code {
                                    extract_code(node, source)?
                                } else {
                                    None
                                };

                                // Extract annotations
                                let annotations = extract_java_annotations(node, source);

                                functions.push(EnhancedFunctionInfo {
                                    name: name.to_string(),
                                    signature,
                                    line,
                                    end_line,
                                    doc,
                                    code,
                                    annotations,
                                });
                            }
                        }
                    }
                }
                "class" if node.kind() == "class_declaration" => {
                    let node_id = node.id();
                    if !processed_class_nodes.contains(&node_id) {
                        processed_class_nodes.insert(node_id);

                        if let Some(name_node) = node.child_by_field_name("name") {
                            if let Ok(name) = name_node.utf8_text(source.as_bytes()) {
                                let line = node.start_position().row + 1;
                                let end_line = node.end_position().row + 1;
                                let doc = extract_doc_comment(node, source, Language::Java)?;
                                let code = if include_code {
                                    extract_code(node, source)?
                                } else {
                                    None
                                };

                                // Extract implements interfaces
                                let implements = extract_java_implemented_interfaces(node, source);

                                // Extract methods from class
                                let methods =
                                    extract_java_class_methods(node, source, include_code)?;

                                classes.push(EnhancedClassInfo {
                                    name: name.to_string(),
                                    line,
                                    end_line,
                                    doc,
                                    code,
                                    methods,
                                    implements,
                                    properties: vec![],
                                    fields: vec![],
                                });
                            }
                        }
                    }
                }
                "interface" if node.kind() == "interface_declaration" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        if let Ok(name) = name_node.utf8_text(source.as_bytes()) {
                            let line = node.start_position().row + 1;
                            let end_line = node.end_position().row + 1;
                            let doc = extract_doc_comment(node, source, Language::Java)?;
                            let code = if include_code {
                                extract_code(node, source)?
                            } else {
                                None
                            };

                            // Extract methods from interface
                            let methods =
                                extract_java_interface_methods(node, source, include_code)?;

                            interfaces.push(InterfaceInfo {
                                name: name.to_string(),
                                line,
                                end_line,
                                doc,
                                code,
                                methods,
                                properties: vec![],
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
        interfaces,
        properties: vec![],
        dependencies: vec![],
    })
}

/// Helper function to extract methods from a Java class
pub(super) fn extract_java_class_methods(
    class_node: Node,
    source: &str,
    include_code: bool,
) -> Result<Vec<EnhancedFunctionInfo>, io::Error> {
    let mut methods = Vec::new();

    if let Some(body) = class_node.child_by_field_name("body") {
        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            if child.kind() == "method_declaration" {
                if let Some(method_info) = extract_java_method_info(child, source, include_code)? {
                    methods.push(method_info);
                }
            }
        }
    }

    Ok(methods)
}

/// Extract method information from a Java method declaration node
pub(super) fn extract_java_method_info(
    method_node: Node,
    source: &str,
    include_code: bool,
) -> Result<Option<EnhancedFunctionInfo>, io::Error> {
    if let Some(name_node) = method_node.child_by_field_name("name") {
        if let Ok(name) = name_node.utf8_text(source.as_bytes()) {
            let line = method_node.start_position().row + 1;
            let end_line = method_node.end_position().row + 1;
            let signature = extract_signature(method_node, source)?;
            let doc = extract_doc_comment(method_node, source, Language::Java)?;
            let code = if include_code {
                extract_code(method_node, source)?
            } else {
                None
            };

            // Extract annotations
            let annotations = extract_java_annotations(method_node, source);

            return Ok(Some(EnhancedFunctionInfo {
                name: name.to_string(),
                signature,
                line,
                end_line,
                doc,
                code,
                annotations,
            }));
        }
    }
    Ok(None)
}

/// Extract methods from a Java interface body
pub(super) fn extract_java_interface_methods(
    interface_node: Node,
    source: &str,
    include_code: bool,
) -> Result<Vec<EnhancedFunctionInfo>, io::Error> {
    let mut methods = Vec::new();

    if let Some(body) = interface_node.child_by_field_name("body") {
        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            if child.kind() == "method_declaration" {
                if let Some(method_info) = extract_java_method_info(child, source, include_code)? {
                    methods.push(method_info);
                }
            }
        }
    }

    Ok(methods)
}

/// Extract implemented interface names from a Java class node
pub(super) fn extract_java_implemented_interfaces(class_node: Node, source: &str) -> Vec<String> {
    let mut implements = Vec::new();

    // Look for the super_interfaces field (kind: "interfaces")
    if let Some(super_interfaces) = class_node.child_by_field_name("interfaces") {
        // The super_interfaces node contains a type_list with type_identifier children
        let mut cursor = super_interfaces.walk();
        for child in super_interfaces.children(&mut cursor) {
            if child.kind() == "type_list" {
                // type_list contains the actual type_identifier nodes
                let mut type_cursor = child.walk();
                for type_child in child.children(&mut type_cursor) {
                    if type_child.kind() == "type_identifier" {
                        if let Ok(interface_name) = type_child.utf8_text(source.as_bytes()) {
                            implements.push(interface_name.to_string());
                        }
                    }
                }
            } else if child.kind() == "type_identifier" {
                // Fallback: sometimes type_identifier is direct child
                if let Ok(interface_name) = child.utf8_text(source.as_bytes()) {
                    implements.push(interface_name.to_string());
                }
            }
        }
    }

    implements
}

/// Extract annotations from a Java node (method or class)
pub(super) fn extract_java_annotations(node: Node, source: &str) -> Vec<String> {
    let mut annotations = Vec::new();

    // In Java tree-sitter grammar, modifiers node exists but has no field name (empty string)
    // We need to look for 'modifiers' node kind among children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "modifiers" {
            // Found the modifiers node, now look for annotations inside it
            let mut mod_cursor = child.walk();
            for mod_child in child.children(&mut mod_cursor) {
                if mod_child.kind() == "marker_annotation" || mod_child.kind() == "annotation" {
                    // For marker_annotation: @Override has child with field 'name' that is an identifier
                    // For annotation: @SuppressWarnings(...) has a 'name' field
                    if let Some(name_node) = mod_child.child_by_field_name("name") {
                        if let Ok(annotation_name) = name_node.utf8_text(source.as_bytes()) {
                            annotations.push(annotation_name.to_string());
                        }
                    }
                }
            }
            break;
        }
    }

    annotations
}

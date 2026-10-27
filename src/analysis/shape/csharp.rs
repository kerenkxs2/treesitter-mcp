use super::*;

/// Extract enhanced shape from C# source code
pub(super) fn extract_csharp_enhanced(
    tree: &Tree,
    source: &str,
    include_code: bool,
) -> Result<EnhancedFileShape, io::Error> {
    let mut functions = Vec::new();
    let mut classes = Vec::new();
    let mut imports = Vec::new();
    let mut interfaces = Vec::new();
    let mut properties = Vec::new();

    let ts_language = tree_sitter_c_sharp::LANGUAGE.into();

    let query_str = r#"
        (method_declaration) @method
        (class_declaration) @class
        (interface_declaration) @interface
        (property_declaration) @property
        (using_directive) @import
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
    let mut processed_property_nodes = std::collections::HashSet::new();

    while let Some(match_) = matches.next() {
        for capture in match_.captures {
            let node = capture.node;
            let name_idx = capture.index;
            let capture_name = query.capture_names()[name_idx as usize];

            match capture_name {
                "method" => {
                    // In C#, all methods are inside classes, so we extract them all as functions
                    // (unlike JS/TS where we skip class methods)

                    let node_id = node.id();
                    if !processed_method_nodes.contains(&node_id) {
                        processed_method_nodes.insert(node_id);

                        if let Some(name_node) = node.child_by_field_name("name") {
                            if let Ok(name) = name_node.utf8_text(source.as_bytes()) {
                                let line = node.start_position().row + 1;
                                let end_line = node.end_position().row + 1;
                                let signature = extract_signature(node, source)?;
                                let doc = extract_doc_comment(node, source, Language::CSharp)?;
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
                "class" if node.kind() == "class_declaration" => {
                    let node_id = node.id();
                    if !processed_class_nodes.contains(&node_id) {
                        processed_class_nodes.insert(node_id);

                        if let Some(name_node) = node.child_by_field_name("name") {
                            if let Ok(name) = name_node.utf8_text(source.as_bytes()) {
                                let line = node.start_position().row + 1;
                                let end_line = node.end_position().row + 1;
                                let doc = extract_doc_comment(node, source, Language::CSharp)?;
                                let code = if include_code {
                                    extract_code(node, source)?
                                } else {
                                    None
                                };

                                // Extract implements interfaces from base_list
                                let implements =
                                    extract_csharp_implemented_interfaces(node, source);

                                // Extract methods from class
                                let methods =
                                    extract_csharp_class_methods(node, source, include_code)?;

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
                            let doc = extract_doc_comment(node, source, Language::CSharp)?;
                            let code = if include_code {
                                extract_code(node, source)?
                            } else {
                                None
                            };

                            // Extract methods from interface
                            let methods =
                                extract_csharp_interface_methods(node, source, include_code)?;

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
                "property" => {
                    let node_id = node.id();
                    if !processed_property_nodes.contains(&node_id) {
                        processed_property_nodes.insert(node_id);

                        if let Some(name_node) = node.child_by_field_name("name") {
                            if let Ok(name) = name_node.utf8_text(source.as_bytes()) {
                                let line = node.start_position().row + 1;
                                let end_line = node.end_position().row + 1;
                                let doc = extract_doc_comment(node, source, Language::CSharp)?;

                                // Extract property type
                                let property_type = node
                                    .child_by_field_name("type")
                                    .and_then(|n| n.utf8_text(source.as_bytes()).ok())
                                    .map(|s| s.to_string());

                                properties.push(PropertyInfo {
                                    name: name.to_string(),
                                    line,
                                    end_line,
                                    property_type,
                                    doc,
                                });
                            }
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
        properties,
        dependencies: vec![],
    })
}

/// Helper function to extract methods from a C# class
pub(super) fn extract_csharp_class_methods(
    class_node: Node,
    source: &str,
    include_code: bool,
) -> Result<Vec<EnhancedFunctionInfo>, io::Error> {
    let mut methods = Vec::new();

    if let Some(body) = class_node.child_by_field_name("body") {
        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            if child.kind() == "method_declaration" {
                if let Some(method_info) = extract_csharp_method_info(child, source, include_code)?
                {
                    methods.push(method_info);
                }
            }
        }
    }

    Ok(methods)
}

/// Extract method information from a C# method declaration node
pub(super) fn extract_csharp_method_info(
    method_node: Node,
    source: &str,
    include_code: bool,
) -> Result<Option<EnhancedFunctionInfo>, io::Error> {
    if let Some(name_node) = method_node.child_by_field_name("name") {
        if let Ok(name) = name_node.utf8_text(source.as_bytes()) {
            let line = method_node.start_position().row + 1;
            let end_line = method_node.end_position().row + 1;
            let signature = extract_signature(method_node, source)?;
            let doc = extract_doc_comment(method_node, source, Language::CSharp)?;
            let code = if include_code {
                extract_code(method_node, source)?
            } else {
                None
            };

            return Ok(Some(EnhancedFunctionInfo {
                name: name.to_string(),
                signature,
                line,
                end_line,
                doc,
                code,
                annotations: vec![],
            }));
        }
    }
    Ok(None)
}

/// Extract methods from a C# interface body
pub(super) fn extract_csharp_interface_methods(
    interface_node: Node,
    source: &str,
    include_code: bool,
) -> Result<Vec<EnhancedFunctionInfo>, io::Error> {
    let mut methods = Vec::new();

    if let Some(body) = interface_node.child_by_field_name("body") {
        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            if child.kind() == "method_declaration" {
                if let Some(method_info) = extract_csharp_method_info(child, source, include_code)?
                {
                    methods.push(method_info);
                }
            }
        }
    }

    Ok(methods)
}

/// Extract implemented interface names from a C# class node
pub(super) fn extract_csharp_implemented_interfaces(class_node: Node, source: &str) -> Vec<String> {
    let mut implements = Vec::new();

    let mut cursor = class_node.walk();
    for child in class_node.children(&mut cursor) {
        if child.kind() == "base_list" {
            let mut bases_cursor = child.walk();
            for base_child in child.children(&mut bases_cursor) {
                // Look for identifier or generic_name nodes
                if base_child.kind() == "identifier" || base_child.kind() == "generic_name" {
                    if let Ok(interface_name) = base_child.utf8_text(source.as_bytes()) {
                        implements.push(interface_name.to_string());
                    }
                }
            }
            break; // base_list is unique, no need to continue
        }
    }

    implements
}

use super::*;

pub(super) fn extract_rust_symbols(
    tree: &tree_sitter::Tree,
    source: &str,
    symbols: &mut HashMap<String, ExtractedSymbol>,
) -> Result<(), io::Error> {
    use streaming_iterator::StreamingIterator;
    use tree_sitter::{Query, QueryCursor};

    let query = Query::new(
        &tree_sitter_rust::LANGUAGE.into(),
        r#"
        (function_item name: (identifier) @func.name) @func
        (struct_item name: (type_identifier) @struct.name) @struct
        (enum_item name: (type_identifier) @enum.name) @enum
        (const_item name: (identifier) @const.name) @const
        (static_item name: (identifier) @static.name) @static
        "#,
    )
    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Query error: {e}")))?;

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());

    while let Some(match_) = matches.next() {
        for capture in match_.captures {
            let capture_name = query.capture_names()[capture.index as usize];
            let node = capture.node;

            // Get the full node (not just the name)
            let (symbol_type, full_node) = match capture_name {
                "func.name" => (SymbolType::Function, node.parent()),
                "struct.name" => (SymbolType::Struct, node.parent()),
                "enum.name" => (SymbolType::Enum, node.parent()),
                "const.name" => (SymbolType::Constant, node.parent()),
                "static.name" => (SymbolType::Static, node.parent()),
                _ => continue,
            };

            if let (Ok(name), Some(full_node)) = (node.utf8_text(source.as_bytes()), full_node) {
                let signature = extract_signature_from_node(&full_node, source);
                let body_hash = hash_node_body(&full_node, source);

                symbols.insert(
                    format!("{:?}::{}", symbol_type, name),
                    ExtractedSymbol {
                        symbol_type,
                        name: name.to_string(),
                        line: node.start_position().row + 1,
                        signature,
                        body_hash,
                    },
                );
            }
        }
    }

    Ok(())
}

pub(super) fn extract_python_symbols(
    tree: &tree_sitter::Tree,
    source: &str,
    symbols: &mut HashMap<String, ExtractedSymbol>,
) -> Result<(), io::Error> {
    use streaming_iterator::StreamingIterator;
    use tree_sitter::{Query, QueryCursor};

    let query = Query::new(
        &tree_sitter_python::LANGUAGE.into(),
        r#"
        (function_definition name: (identifier) @func.name) @func
        (class_definition name: (identifier) @class.name) @class
        "#,
    )
    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Query error: {e}")))?;

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());

    while let Some(match_) = matches.next() {
        for capture in match_.captures {
            let capture_name = query.capture_names()[capture.index as usize];
            let node = capture.node;

            let (symbol_type, full_node) = match capture_name {
                "func.name" => (SymbolType::Function, node.parent()),
                "class.name" => (SymbolType::Class, node.parent()),
                _ => continue,
            };

            if let (Ok(name), Some(full_node)) = (node.utf8_text(source.as_bytes()), full_node) {
                let signature = extract_signature_from_node(&full_node, source);
                let body_hash = hash_node_body(&full_node, source);

                symbols.insert(
                    format!("{:?}::{}", symbol_type, name),
                    ExtractedSymbol {
                        symbol_type,
                        name: name.to_string(),
                        line: node.start_position().row + 1,
                        signature,
                        body_hash,
                    },
                );
            }
        }
    }

    Ok(())
}

pub(super) fn extract_js_symbols(
    tree: &tree_sitter::Tree,
    source: &str,
    symbols: &mut HashMap<String, ExtractedSymbol>,
) -> Result<(), io::Error> {
    use streaming_iterator::StreamingIterator;
    use tree_sitter::{Query, QueryCursor};

    let query = Query::new(
        &tree_sitter_javascript::LANGUAGE.into(),
        r#"
        (function_declaration name: (identifier) @func.name) @func
        (class_declaration name: (identifier) @class.name) @class
        (method_definition name: (property_identifier) @method.name) @method
        "#,
    )
    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Query error: {e}")))?;

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());

    while let Some(match_) = matches.next() {
        for capture in match_.captures {
            let capture_name = query.capture_names()[capture.index as usize];
            let node = capture.node;

            let (symbol_type, full_node) = match capture_name {
                "func.name" => (SymbolType::Function, node.parent()),
                "class.name" => (SymbolType::Class, node.parent()),
                "method.name" => (SymbolType::Function, node.parent()),
                _ => continue,
            };

            if let (Ok(name), Some(full_node)) = (node.utf8_text(source.as_bytes()), full_node) {
                let signature = extract_signature_from_node(&full_node, source);
                let body_hash = hash_node_body(&full_node, source);

                symbols.insert(
                    format!("{:?}::{}", symbol_type, name),
                    ExtractedSymbol {
                        symbol_type,
                        name: name.to_string(),
                        line: node.start_position().row + 1,
                        signature,
                        body_hash,
                    },
                );
            }
        }
    }

    Ok(())
}

pub(super) fn extract_ts_symbols(
    tree: &tree_sitter::Tree,
    source: &str,
    symbols: &mut HashMap<String, ExtractedSymbol>,
) -> Result<(), io::Error> {
    use streaming_iterator::StreamingIterator;
    use tree_sitter::{Query, QueryCursor};

    let query = Query::new(
        &tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        r#"
        (function_declaration name: (identifier) @func.name) @func
        (class_declaration name: (type_identifier) @class.name) @class
        (method_definition name: (property_identifier) @method.name) @method
        (interface_declaration name: (type_identifier) @interface.name) @interface
        "#,
    )
    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Query error: {e}")))?;

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());

    while let Some(match_) = matches.next() {
        for capture in match_.captures {
            let capture_name = query.capture_names()[capture.index as usize];
            let node = capture.node;

            let (symbol_type, full_node) = match capture_name {
                "func.name" => (SymbolType::Function, node.parent()),
                "class.name" => (SymbolType::Class, node.parent()),
                "method.name" => (SymbolType::Function, node.parent()),
                "interface.name" => (SymbolType::Interface, node.parent()),
                _ => continue,
            };

            if let (Ok(name), Some(full_node)) = (node.utf8_text(source.as_bytes()), full_node) {
                let signature = extract_signature_from_node(&full_node, source);
                let body_hash = hash_node_body(&full_node, source);

                symbols.insert(
                    format!("{:?}::{}", symbol_type, name),
                    ExtractedSymbol {
                        symbol_type,
                        name: name.to_string(),
                        line: node.start_position().row + 1,
                        signature,
                        body_hash,
                    },
                );
            }
        }
    }

    Ok(())
}

pub(super) fn extract_go_symbols(
    tree: &tree_sitter::Tree,
    source: &str,
    symbols: &mut HashMap<String, ExtractedSymbol>,
) -> Result<(), io::Error> {
    use streaming_iterator::StreamingIterator;
    use tree_sitter::{Node, Query, QueryCursor};

    fn find_parent_by_kind<'a>(mut node: Node<'a>, kind: &str) -> Option<Node<'a>> {
        loop {
            if node.kind() == kind {
                return Some(node);
            }
            node = node.parent()?;
        }
    }

    let query = Query::new(
        &tree_sitter_go::LANGUAGE.into(),
        r#"
        (function_declaration name: (identifier) @func.name) @func
        (method_declaration name: (field_identifier) @method.name) @method
        (type_spec name: (type_identifier) @struct.name type: (struct_type)) @struct
        (type_spec name: (type_identifier) @iface.name type: (interface_type)) @iface
        "#,
    )
    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Query error: {e}")))?;

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());

    while let Some(match_) = matches.next() {
        for capture in match_.captures {
            let capture_name = query.capture_names()[capture.index as usize];
            let node = capture.node;

            let (symbol_type, full_node) = match capture_name {
                "func.name" => (
                    SymbolType::Function,
                    find_parent_by_kind(node, "function_declaration"),
                ),
                "method.name" => (
                    SymbolType::Function,
                    find_parent_by_kind(node, "method_declaration"),
                ),
                "struct.name" => (
                    SymbolType::Struct,
                    find_parent_by_kind(node, "type_declaration"),
                ),
                "iface.name" => (
                    SymbolType::Interface,
                    find_parent_by_kind(node, "type_declaration"),
                ),
                _ => continue,
            };

            if let (Ok(name), Some(full_node)) = (node.utf8_text(source.as_bytes()), full_node) {
                let signature = extract_signature_from_node(&full_node, source);
                let body_hash = hash_node_body(&full_node, source);

                symbols.insert(
                    format!("{:?}::{}", symbol_type, name),
                    ExtractedSymbol {
                        symbol_type,
                        name: name.to_string(),
                        line: node.start_position().row + 1,
                        signature,
                        body_hash,
                    },
                );
            }
        }
    }

    Ok(())
}

/// Extract signature (first line or up to opening brace)
pub(super) fn extract_signature_from_node(
    node: &tree_sitter::Node,
    source: &str,
) -> Option<String> {
    let text = node.utf8_text(source.as_bytes()).ok()?;

    // Find the first '{' or ':' (for Python) to get just the signature
    if let Some(brace_pos) = text.find('{') {
        Some(text[..brace_pos].trim().to_string())
    } else if let Some(colon_pos) = text.find(':') {
        // For Python-style definitions
        Some(text[..=colon_pos].trim().to_string())
    } else {
        // For declarations without body
        Some(text.lines().next()?.trim().to_string())
    }
}

/// Hash the body of a node (excluding signature) for change detection
///
/// Note: Uses DefaultHasher which is fast but not stable across Rust versions.
/// This is acceptable for runtime comparison within a single execution.
/// If persistence is needed in the future, consider using a stable hash like xxhash.
pub(super) fn hash_node_body(node: &tree_sitter::Node, source: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;

    let text = node.utf8_text(source.as_bytes()).unwrap_or("");

    // Skip to body (after first '{' or ':')
    let body = if let Some(brace_pos) = text.find('{') {
        &text[brace_pos..]
    } else {
        text
    };

    // Normalize whitespace for comparison
    let normalized: String = body.split_whitespace().collect();

    let mut hasher = DefaultHasher::new();
    normalized.hash(&mut hasher);
    hasher.finish()
}

pub(super) fn find_symbol_in_current_file(
    file_path: &Path,
    symbol_name: &str,
) -> Result<CurrentSymbol, io::Error> {
    let language = detect_language(file_path).map_err(|e| {
        io::Error::new(
            io::ErrorKind::Unsupported,
            format!("Cannot detect language: {e}"),
        )
    })?;
    let current_content = fs::read_to_string(file_path)?;
    let tree = parse_code(&current_content, language).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to parse current version: {e}"),
        )
    })?;
    let mut symbols = extract_symbols(&tree, &current_content, language)?
        .into_values()
        .filter(|symbol| symbol.name == symbol_name)
        .collect::<Vec<_>>();
    symbols.sort_by_key(|symbol| symbol.line);
    let symbol = symbols.into_iter().next().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "Symbol '{symbol_name}' not found in {}",
                file_path.display()
            ),
        )
    })?;

    Ok(CurrentSymbol {
        symbol_type: symbol.symbol_type,
        name: symbol.name,
        line: symbol.line,
        signature: symbol.signature,
    })
}

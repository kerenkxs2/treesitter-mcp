use super::*;

/// Extract the signature line of a function or struct
/// Uses tree-sitter to find the body node and extract signature efficiently
pub(super) fn extract_signature(node: Node, source: &str) -> Result<String, io::Error> {
    let source_bytes = source.as_bytes();

    // Try to find the body node using tree-sitter
    // Body node types: block, statement_block, body, compound_statement
    let mut body_start_byte = None;
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        let kind = child.kind();
        if kind == "block"
            || kind == "statement_block"
            || kind == "body"
            || kind == "compound_statement"
            || kind == "field_declaration_list"
        // For structs
        {
            body_start_byte = Some(child.start_byte());
            break;
        }
    }

    // Determine the end of the signature
    let end_byte = if let Some(body_start) = body_start_byte {
        // Signature is everything before the body
        body_start
    } else {
        // No body found (e.g., trait method declaration), use the entire node
        node.end_byte()
    };

    // Extract the signature text
    let start_byte = node.start_byte();
    let signature_bytes = &source_bytes[start_byte..end_byte];
    let signature_text = String::from_utf8_lossy(signature_bytes);

    // Find where the actual declaration starts (after attributes/decorators)
    // Look for keywords that indicate the start of the declaration
    let mut lines: Vec<&str> = signature_text.lines().collect();
    let mut declaration_start_idx = 0;

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("fn ")
            || trimmed.starts_with("pub fn ")
            || trimmed.starts_with("async fn ")
            || trimmed.starts_with("pub async fn ")
            || trimmed.starts_with("struct ")
            || trimmed.starts_with("pub struct ")
            || trimmed.starts_with("class ")
            || trimmed.starts_with("def ")
            || trimmed.starts_with("async def ")
            || trimmed.starts_with("function ")
            || trimmed.starts_with("export function ")
            || trimmed.starts_with("export async function ")
            || trimmed.starts_with("func ")
            || trimmed.starts_with("type ")
        {
            declaration_start_idx = idx;
            break;
        }
    }

    // Take lines from declaration start onwards
    let signature_lines: Vec<&str> = lines.drain(declaration_start_idx..).collect();
    let signature = signature_lines.join("\n").trim().to_string();

    Ok(signature)
}

/// Extract the full code block of a function or struct
pub(super) fn extract_code(node: Node, source: &str) -> Result<Option<String>, io::Error> {
    let start_byte = node.start_byte();
    let end_byte = node.end_byte();

    if start_byte >= end_byte {
        return Ok(None);
    }

    let code_bytes = &source.as_bytes()[start_byte..end_byte];
    let code = String::from_utf8_lossy(code_bytes).to_string();

    if code.is_empty() {
        Ok(None)
    } else {
        Ok(Some(code))
    }
}

pub fn prepend_leading_comments_to_code(
    source: &str,
    start_line: usize,
    language: Language,
    code: Option<String>,
    mode: CommentMode,
) -> Option<String> {
    let code = code?;
    if !mode.includes_leading() {
        return Some(code);
    }

    let comments = extract_leading_comment_block(source, start_line, language)?;
    Some(format!("{comments}\n{code}"))
}

pub(super) fn extract_leading_comment_block(
    source: &str,
    start_line: usize,
    language: Language,
) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    let mut idx = start_line.checked_sub(2)? as isize;
    if idx < 0 || idx as usize >= lines.len() {
        return None;
    }

    if lines[idx as usize].trim().is_empty() {
        return None;
    }

    let mut collected = Vec::new();

    while idx >= 0 {
        let line = lines[idx as usize];
        let trimmed = line.trim();

        if trimmed.is_empty() {
            break;
        }

        if is_line_comment_text(trimmed, language) {
            collected.push(line);
            idx -= 1;
            continue;
        }

        if supports_block_comments(language) && trimmed.ends_with("*/") {
            collected.push(line);
            idx -= 1;

            while idx >= 0 {
                let block_line = lines[idx as usize];
                collected.push(block_line);
                if block_line.contains("/*") {
                    idx -= 1;
                    break;
                }
                idx -= 1;
            }
            continue;
        }

        break;
    }

    if collected.is_empty() {
        None
    } else {
        collected.reverse();
        Some(collected.join("\n"))
    }
}

pub(super) fn is_line_comment_text(trimmed: &str, language: Language) -> bool {
    match language {
        Language::Python | Language::GDScript => trimmed.starts_with('#'),
        Language::Lua => trimmed.starts_with("--"),
        Language::Rust
        | Language::JavaScript
        | Language::TypeScript
        | Language::Tsx
        | Language::Swift
        | Language::CSharp
        | Language::Java
        | Language::Go
        | Language::C
        | Language::Cpp => trimmed.starts_with("//"),
        _ => false,
    }
}

pub(super) fn supports_block_comments(language: Language) -> bool {
    matches!(
        language,
        Language::Rust
            | Language::JavaScript
            | Language::TypeScript
            | Language::Tsx
            | Language::Swift
            | Language::CSharp
            | Language::Java
            | Language::Go
            | Language::C
            | Language::Cpp
    )
}

/// Extract doc comment from a node
pub(super) fn extract_doc_comment(
    node: Node,
    source: &str,
    language: Language,
) -> Result<Option<String>, io::Error> {
    // Collect all consecutive doc comment lines before the current node
    let mut doc_lines = Vec::new();
    let mut prev_sibling = node.prev_sibling();

    while let Some(sibling) = prev_sibling {
        if is_comment_node(&sibling, language) {
            if let Ok(comment_text) = sibling.utf8_text(source.as_bytes()) {
                let doc = extract_doc_from_comment(comment_text, language);
                // Collect all doc lines, even empty ones (they separate sections)
                doc_lines.insert(0, doc);
            }
        } else if sibling.kind() != "ERROR" && !sibling.kind().is_empty() {
            // Stop if we hit a non-comment node
            break;
        }
        prev_sibling = sibling.prev_sibling();
    }

    if !doc_lines.is_empty() {
        // Find the first non-empty doc line (the actual description)
        if let Some(first_doc) = doc_lines.iter().find(|d| !d.is_empty()) {
            return Ok(Some(first_doc.clone()));
        }
        // If all are empty, return the joined version
        return Ok(Some(doc_lines.join("\n")));
    }

    // Also check parent's previous sibling for doc comments
    if let Some(parent) = node.parent() {
        if let Some(parent_prev) = parent.prev_sibling() {
            if is_comment_node(&parent_prev, language) {
                if let Ok(comment_text) = parent_prev.utf8_text(source.as_bytes()) {
                    let doc = extract_doc_from_comment(comment_text, language);
                    if !doc.is_empty() {
                        return Ok(Some(doc));
                    }
                }
            }
        }
    }

    Ok(None)
}

/// Check if a node is a comment node
pub(super) fn is_comment_node(node: &Node, language: Language) -> bool {
    let kind = node.kind();
    match language {
        Language::Rust
        | Language::JavaScript
        | Language::TypeScript
        | Language::Tsx
        | Language::Swift
        | Language::CSharp
        | Language::Java
        | Language::Go
        | Language::C
        | Language::Cpp => kind == "line_comment" || kind == "block_comment" || kind == "comment",
        Language::Python => kind == "comment",
        _ => false,
    }
}

/// Extract documentation text from a comment
pub(super) fn extract_doc_from_comment(comment_text: &str, language: Language) -> String {
    let trimmed = comment_text.trim();

    match language {
        Language::Rust | Language::Swift | Language::CSharp => {
            // Handle /// doc comments
            if trimmed.starts_with("///") {
                trimmed.strip_prefix("///").unwrap_or("").trim().to_string()
            } else if trimmed.starts_with("//!") {
                trimmed.strip_prefix("//!").unwrap_or("").trim().to_string()
            } else {
                String::new()
            }
        }
        Language::Python => {
            // Handle # comments
            if trimmed.starts_with("#") {
                trimmed.strip_prefix("#").unwrap_or("").trim().to_string()
            } else {
                String::new()
            }
        }
        Language::JavaScript
        | Language::TypeScript
        | Language::Tsx
        | Language::Java
        | Language::Go
        | Language::C
        | Language::Cpp => {
            // Handle /** */ and // comments
            if trimmed.starts_with("/**") && trimmed.ends_with("*/") {
                trimmed
                    .strip_prefix("/**")
                    .and_then(|s| s.strip_suffix("*/"))
                    .unwrap_or("")
                    .trim()
                    .to_string()
            } else if trimmed.starts_with("//") {
                trimmed.strip_prefix("//").unwrap_or("").trim().to_string()
            } else {
                String::new()
            }
        }
        _ => String::new(),
    }
}

/// Find a parent node of a given type
pub(super) fn find_parent_by_type<'a>(
    mut node: Node<'a>,
    target_type: &str,
) -> Result<Node<'a>, io::Error> {
    while let Some(parent) = node.parent() {
        if parent.kind() == target_type {
            return Ok(parent);
        }
        node = parent;
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("Parent node of type '{}' not found", target_type),
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentMode {
    None,
    Leading,
}

impl CommentMode {
    pub fn from_option(value: Option<&str>) -> Self {
        match value {
            Some("leading") => CommentMode::Leading,
            _ => CommentMode::None,
        }
    }

    pub fn includes_leading(self) -> bool {
        matches!(self, CommentMode::Leading)
    }
}

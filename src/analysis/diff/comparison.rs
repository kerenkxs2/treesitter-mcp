use super::*;

// ============================================================================
// Symbol Comparison
// ============================================================================

/// Compare old and new symbols to find structural changes
pub(super) fn compare_symbols(
    old_symbols: &HashMap<String, ExtractedSymbol>,
    new_symbols: &HashMap<String, ExtractedSymbol>,
    _old_source: &str,
    _new_source: &str,
) -> Result<Vec<StructuralChange>, io::Error> {
    let mut changes = Vec::new();

    // Find removed symbols
    for (key, old_sym) in old_symbols {
        if !new_symbols.contains_key(key) {
            changes.push(StructuralChange {
                change_type: ChangeType::Removed,
                symbol_type: old_sym.symbol_type.clone(),
                name: old_sym.name.clone(),
                line: old_sym.line,
                before: old_sym.signature.clone(),
                after: None,
                details: vec![],
            });
        }
    }

    // Find added symbols
    for (key, new_sym) in new_symbols {
        if !old_symbols.contains_key(key) {
            changes.push(StructuralChange {
                change_type: ChangeType::Added,
                symbol_type: new_sym.symbol_type.clone(),
                name: new_sym.name.clone(),
                line: new_sym.line,
                before: None,
                after: new_sym.signature.clone(),
                details: vec![],
            });
        }
    }

    // Find modified symbols
    for (key, old_sym) in old_symbols {
        if let Some(new_sym) = new_symbols.get(key) {
            let signature_changed = old_sym.signature != new_sym.signature;
            let body_changed = old_sym.body_hash != new_sym.body_hash;

            if signature_changed {
                let details = analyze_signature_changes(
                    old_sym.signature.as_deref(),
                    new_sym.signature.as_deref(),
                );

                changes.push(StructuralChange {
                    change_type: ChangeType::SignatureChanged,
                    symbol_type: new_sym.symbol_type.clone(),
                    name: new_sym.name.clone(),
                    line: new_sym.line,
                    before: old_sym.signature.clone(),
                    after: new_sym.signature.clone(),
                    details,
                });
            } else if body_changed {
                changes.push(StructuralChange {
                    change_type: ChangeType::BodyChanged,
                    symbol_type: new_sym.symbol_type.clone(),
                    name: new_sym.name.clone(),
                    line: new_sym.line,
                    before: None,
                    after: None,
                    details: vec![ChangeDetail {
                        kind: "implementation_changed".to_string(),
                        name: None,
                        from: None,
                        to: None,
                    }],
                });
            }
        }
    }

    // Sort by line number
    changes.sort_by_key(|c| c.line);

    Ok(changes)
}

/// Analyze what specifically changed in a signature
pub(super) fn analyze_signature_changes(
    old_sig: Option<&str>,
    new_sig: Option<&str>,
) -> Vec<ChangeDetail> {
    let mut details = Vec::new();

    let (old_sig, new_sig) = match (old_sig, new_sig) {
        (Some(o), Some(n)) => (o, n),
        _ => return details,
    };

    // Simple heuristic: check for common patterns
    // This could be made more sophisticated with proper parsing

    // Check for return type changes (Rust: -> Type, TS: : Type)
    let old_return = extract_return_type(old_sig);
    let new_return = extract_return_type(new_sig);
    if old_return != new_return {
        details.push(ChangeDetail {
            kind: "return_type".to_string(),
            name: None,
            from: old_return,
            to: new_return,
        });
    }

    // Check for parameter changes
    let old_params = extract_parameters(old_sig);
    let new_params = extract_parameters(new_sig);

    if old_params.len() != new_params.len() {
        details.push(ChangeDetail {
            kind: "parameter_count".to_string(),
            name: None,
            from: Some(old_params.len().to_string()),
            to: Some(new_params.len().to_string()),
        });
    }

    // Check individual parameter changes
    for (i, (old_p, new_p)) in old_params.iter().zip(new_params.iter()).enumerate() {
        if old_p != new_p {
            details.push(ChangeDetail {
                kind: "parameter_changed".to_string(),
                name: Some(format!("param_{}", i)),
                from: Some(old_p.clone()),
                to: Some(new_p.clone()),
            });
        }
    }

    details
}

pub(super) fn extract_return_type(sig: &str) -> Option<String> {
    // Rust: fn foo() -> Type
    if let Some(pos) = sig.find("->") {
        return Some(sig[pos + 2..].trim().to_string());
    }
    // TypeScript: function foo(): Type
    if let Some(pos) = sig.rfind("):") {
        return Some(sig[pos + 2..].trim().to_string());
    }
    None
}

/// Extract parameters from a function signature
///
/// Note: This is a heuristic parser that handles nested generics but may not
/// correctly handle all edge cases (e.g., string literals containing brackets).
/// For production use, consider using tree-sitter to parse parameter lists.
pub(super) fn extract_parameters(sig: &str) -> Vec<String> {
    // Find content between parentheses
    let start = sig.find('(').map(|i| i + 1).unwrap_or(0);
    let end = sig.rfind(')').unwrap_or(sig.len());

    if start >= end {
        return vec![];
    }

    let params_str = &sig[start..end];

    // Split by comma, handling nested generics/brackets
    let mut params = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    let mut in_string = false;
    let mut escape_next = false;

    for c in params_str.chars() {
        if escape_next {
            current.push(c);
            escape_next = false;
            continue;
        }

        match c {
            '\\' if in_string => {
                escape_next = true;
                current.push(c);
            }
            '"' | '\'' => {
                in_string = !in_string;
                current.push(c);
            }
            '<' | '(' | '[' if !in_string => {
                depth += 1;
                current.push(c);
            }
            '>' | ')' | ']' if !in_string => {
                depth -= 1;
                current.push(c);
            }
            ',' if depth == 0 && !in_string => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    params.push(trimmed);
                }
                current.clear();
            }
            _ => current.push(c),
        }
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        params.push(trimmed);
    }

    params
}

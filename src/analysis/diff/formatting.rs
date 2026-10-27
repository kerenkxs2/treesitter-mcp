use super::*;

pub(super) fn abbreviate_symbol_type(symbol_type: &SymbolType) -> &'static str {
    match symbol_type {
        SymbolType::Function => "fn",
        SymbolType::Struct => "s",
        SymbolType::Class => "c",
        SymbolType::Enum => "e",
        SymbolType::Interface => "iface",
        SymbolType::Import => "imp",
        SymbolType::Constant => "const",
        SymbolType::Static => "static",
    }
}

pub(super) fn change_key(change_type: &ChangeType) -> &'static str {
    match change_type {
        ChangeType::Added => "added",
        ChangeType::Removed => "removed",
        ChangeType::SignatureChanged => "sig_changed",
        ChangeType::BodyChanged => "body_changed",
    }
}

pub(super) fn risk_key(risk: &RiskLevel) -> &'static str {
    match risk {
        RiskLevel::High => "high",
        RiskLevel::Medium => "medium",
        RiskLevel::Low => "low",
    }
}

pub(crate) fn format_change(change: &StructuralChange) -> String {
    match change.change_type {
        ChangeType::Added => "added".to_string(),
        ChangeType::Removed => "removed".to_string(),
        ChangeType::BodyChanged => "body_changed".to_string(),
        ChangeType::SignatureChanged => match &change.after {
            Some(after) if !after.is_empty() => format!("sig_changed: {after}"),
            _ => "sig_changed".to_string(),
        },
    }
}

pub(super) fn change_detail_rows(details: &[ChangeDetail]) -> String {
    details
        .iter()
        .map(|detail| {
            format::format_row(&[
                &detail.kind,
                detail.name.as_deref().unwrap_or(""),
                detail.from.as_deref().unwrap_or(""),
                detail.to.as_deref().unwrap_or(""),
            ])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn parse_compact_row(row: &str) -> Vec<String> {
    let mut fields: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut escaped = false;

    for ch in row.chars() {
        if escaped {
            match ch {
                'n' => current.push('\n'),
                'r' => current.push('\r'),
                '|' => current.push('|'),
                '\\' => current.push('\\'),
                other => {
                    current.push('\\');
                    current.push(other);
                }
            }
            escaped = false;
            continue;
        }

        if ch == '\\' {
            escaped = true;
            continue;
        }

        if ch == '|' {
            fields.push(std::mem::take(&mut current));
            continue;
        }

        current.push(ch);
    }

    if escaped {
        current.push('\\');
    }

    fields.push(current);
    fields
}

/// Assess the risk level of a usage based on the change type
///
/// Future enhancement: Consider file proximity (same module = higher risk)
/// and whether usage is in test code (typically lower risk for production).
pub(super) fn parse_confidence(value: &str) -> MatchConfidence {
    match value {
        "high" => MatchConfidence::High,
        "medium" => MatchConfidence::Medium,
        _ => MatchConfidence::Low,
    }
}

pub(super) fn affected_rows(affected_changes: &[AffectedChange]) -> String {
    affected_changes
        .iter()
        .flat_map(|chg| {
            let change_key = change_key(&chg.change_type);
            chg.potentially_affected.iter().map(move |u| {
                let line = u.line.to_string();
                let risk = risk_key(&u.risk);
                format::format_row(&[&chg.symbol, change_key, &u.file, &line, risk])
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

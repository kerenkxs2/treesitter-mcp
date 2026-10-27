use super::*;

pub(super) fn reclassify_confidence_for_change(
    inherited: MatchConfidence,
    change_owner: Option<&str>,
    owner_hint: &str,
    usage_scope: &str,
    usage_file: &str,
    changed_file: &str,
) -> MatchConfidence {
    let normalized_owner = owner_hint.trim();

    if let Some(change_owner) = change_owner {
        if normalized_owner.eq_ignore_ascii_case(change_owner) {
            return MatchConfidence::High;
        }

        if (normalized_owner.eq_ignore_ascii_case("self")
            || normalized_owner.eq_ignore_ascii_case("this"))
            && usage_scope
                .split("::")
                .any(|segment| segment == change_owner)
        {
            return MatchConfidence::High;
        }

        if !normalized_owner.is_empty() {
            return MatchConfidence::Low;
        }
    }

    if usage_file == changed_file {
        return MatchConfidence::Medium;
    }

    inherited
}

pub(super) fn assess_risk(
    change: &StructuralChange,
    usage_type: &str,
    confidence: MatchConfidence,
) -> (RiskLevel, String) {
    if confidence == MatchConfidence::Low {
        return (
            RiskLevel::Low,
            "Low-confidence lexical match; verify the symbol identity before acting".to_string(),
        );
    }

    let (base_risk, reason) = match (&change.change_type, usage_type) {
        // Signature changes are high risk for calls
        (ChangeType::SignatureChanged, "call") => {
            let reason = if change.details.iter().any(|d| d.kind == "parameter_count") {
                "Call site may have wrong number of arguments"
            } else if change.details.iter().any(|d| d.kind.contains("parameter")) {
                "Call site may pass wrong argument types"
            } else if change.details.iter().any(|d| d.kind == "return_type") {
                "Return type changed - check how result is used"
            } else {
                "Signature changed - verify call is still valid"
            };
            (RiskLevel::High, reason.to_string())
        }

        // Signature changes are medium risk for type references
        (ChangeType::SignatureChanged, "type_reference") => (
            RiskLevel::Medium,
            "Type signature changed - verify compatibility".to_string(),
        ),

        // Body changes are low risk (behavior might change but API is same)
        (ChangeType::BodyChanged, _) => (
            RiskLevel::Low,
            "Implementation changed - behavior may differ".to_string(),
        ),

        // Added symbols are low risk (new code, no existing usages should break)
        (ChangeType::Added, _) => (
            RiskLevel::Low,
            "New symbol - this is a new usage".to_string(),
        ),

        // Default case
        _ => (
            RiskLevel::Medium,
            "Symbol changed - verify usage is still valid".to_string(),
        ),
    };

    if confidence == MatchConfidence::Medium && base_risk == RiskLevel::High {
        (
            RiskLevel::Medium,
            format!("{reason} (medium-confidence match)"),
        )
    } else {
        (base_risk, reason)
    }
}

/// Helper to extract text from CallToolResult
/// Uses JSON serialization approach consistent with tests/common/mod.rs
pub(super) fn get_result_text(result: &CallToolResult) -> String {
    if let Some(first_content) = result.content.first() {
        // Serialize and deserialize to extract the text field
        if let Ok(json_str) = serde_json::to_string(first_content) {
            if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&json_str) {
                if let Some(text) = json_val["text"].as_str() {
                    return text.to_string();
                }
            }
        }
    }
    String::new()
}

pub(super) fn resolve_search_path(scope: Option<&str>, file_path: &Path) -> std::path::PathBuf {
    if let Some(scope) = scope {
        Path::new(scope).to_path_buf()
    } else {
        find_project_root(file_path)
            .unwrap_or_else(|| file_path.parent().unwrap_or(Path::new(".")).to_path_buf())
    }
}

pub(super) fn collect_affected_changes_for(
    file_path: &Path,
    search_path: &Path,
    changes: &[StructuralChange],
) -> Result<Vec<AffectedChange>, io::Error> {
    let mut affected_changes = Vec::new();
    let rel_changed_file = path_utils::to_relative_path(&file_path.to_string_lossy());

    for change in changes {
        if change.change_type == ChangeType::Removed {
            continue;
        }

        let usages_args = serde_json::json!({
            "symbol": change.name,
            "path": search_path.to_str().unwrap_or("."),
            "context_lines": 1
        });

        let usages_result = crate::analysis::find_usages::execute(&usages_args)?;
        let usages_text = get_result_text(&usages_result);
        let usages: serde_json::Value = serde_json::from_str(&usages_text).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Failed to parse usages: {e}"),
            )
        })?;

        let change_owner = resolve_change_owner(file_path, change.line);
        let usage_rows = usages
            .get("u")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let mut potentially_affected = Vec::new();

        for row in usage_rows.lines() {
            let fields = parse_compact_row(row);
            let usage_file = fields.first().map(String::as_str).unwrap_or("");
            let usage_line = fields
                .get(1)
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(0);
            let usage_column = fields
                .get(2)
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(0);
            let usage_type = fields.get(3).map(String::as_str).unwrap_or("reference");
            let context = fields.get(4).map(String::as_str).unwrap_or("");
            let usage_scope = fields.get(5).map(String::as_str).unwrap_or("");
            let confidence = parse_confidence(fields.get(6).map(String::as_str).unwrap_or("low"));
            let owner_hint = fields.get(7).map(String::as_str).unwrap_or("");

            if usage_type == "definition" {
                continue;
            }

            if usage_file == rel_changed_file && usage_line == change.line {
                continue;
            }

            let match_confidence = reclassify_confidence_for_change(
                confidence,
                change_owner.as_deref(),
                owner_hint,
                usage_scope,
                usage_file,
                &rel_changed_file,
            );
            let (risk, reason) = assess_risk(change, usage_type, match_confidence);

            potentially_affected.push(AffectedUsage {
                file: usage_file.to_string(),
                line: usage_line,
                column: usage_column,
                usage_type: usage_type.to_string(),
                code: context.to_string(),
                confidence: match_confidence,
                risk,
                reason,
            });
        }

        if !potentially_affected.is_empty() {
            potentially_affected.sort_by(|a, b| {
                let risk_order = |r: &RiskLevel| match r {
                    RiskLevel::High => 0,
                    RiskLevel::Medium => 1,
                    RiskLevel::Low => 2,
                };
                risk_order(&a.risk).cmp(&risk_order(&b.risk))
            });
            affected_changes.push(AffectedChange {
                symbol: change.name.clone(),
                change_type: change.change_type.clone(),
                change_details: change.after.clone(),
                potentially_affected,
            });
        }
    }

    Ok(affected_changes)
}

pub(super) fn resolve_change_owner(file_path: &Path, line: usize) -> Option<String> {
    let args = json!({
        "file_path": file_path.to_str()?,
        "line": line,
        "column": 1
    });

    let result = crate::analysis::symbol_at_line::execute(&args).ok()?;
    let text = get_result_text(&result);
    let parsed: serde_json::Value = serde_json::from_str(&text).ok()?;
    let scope = parsed.get("scope")?.as_str()?;

    scope
        .rsplit("::")
        .nth(1)
        .map(ToOwned::to_owned)
        .or_else(|| {
            file_path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(ToOwned::to_owned)
        })
}

/// Find project root by walking up to the nearest directory containing Cargo.toml
/// Duplicated from file_shape.rs since that function is private
pub(super) fn find_project_root(start: &Path) -> Option<std::path::PathBuf> {
    let mut current = if start.is_dir() {
        start.to_path_buf()
    } else {
        start.parent()?.to_path_buf()
    };

    loop {
        let candidate = current.join("Cargo.toml");
        if candidate.is_file() {
            return Some(current);
        }

        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => break,
        }
    }

    None
}

use super::*;

pub(super) fn build_compact_output_combined(
    result: &ExtractionResult,
    detail_level: DetailLevel,
    max_tokens: usize,
    with_types: bool,
) -> Result<(Map<String, Value>, bool), io::Error> {
    let bpe = cl100k_base()
        .map_err(|e| io::Error::other(format!("Failed to initialize tiktoken tokenizer: {e}")))?;

    let mut output = Map::new();
    let mut ordered_files: Vec<String> = Vec::new();

    // Reserve some budget for types if needed
    let types_budget = if with_types && !result.types.is_empty() {
        max_tokens / 4 // Reserve 25% for types
    } else {
        0
    };
    let files_budget = max_tokens - types_budget;

    // 10% buffer for files
    let mut budget_tracker = BudgetTracker::new((files_budget * 9) / 10);

    for file in &result.files {
        let file_value = build_compact_file(file, detail_level);
        let file_json = serde_json::to_string(&file_value).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Failed to serialize code map file entry: {e}"),
            )
        })?;

        let estimated = budget::estimate_symbol_tokens(file_json.len());
        if !budget_tracker.add(estimated) {
            break;
        }

        output.insert(file.path.clone(), file_value);
        ordered_files.push(file.path.clone());
    }

    let mut truncated = ordered_files.len() < result.files.len();

    // If budget is extremely small, still return at least one file.
    if output.is_empty() && !result.files.is_empty() {
        let first = &result.files[0];
        output.insert(first.path.clone(), build_compact_file(first, detail_level));
        ordered_files.push(first.path.clone());
        truncated = true;
    }

    // Add types if requested
    if with_types && !result.types.is_empty() {
        let types_output = build_types_output(&result.types, types_budget);
        output.insert("types".to_string(), types_output);
    }

    // Hard enforcement with real token counts
    loop {
        let json_text = serde_json::to_string(&Value::Object(output.clone())).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Failed to serialize code map to JSON: {e}"),
            )
        })?;

        if bpe.encode_with_special_tokens(&json_text).len() <= max_tokens {
            break;
        }

        // First try to shrink types
        if with_types {
            if let Some(types_val) = output.get_mut("types") {
                if shrink_types_output(types_val) {
                    truncated = true;
                    continue;
                }
            }
        }

        // Then drop files
        if ordered_files.len() > 1 {
            let Some(last_path) = ordered_files.pop() else {
                break;
            };
            output.remove(&last_path);
            truncated = true;
            continue;
        }

        let Some(only_path) = ordered_files.first().cloned() else {
            break;
        };

        let Some(file_value) = output.get_mut(&only_path) else {
            break;
        };

        if !shrink_single_file_to_fit(file_value, &bpe, max_tokens) {
            truncated = true;
            break;
        }

        truncated = true;
    }

    if truncated {
        output.insert("@".to_string(), json!({"t": true}));
    }

    Ok((output, truncated))
}

pub(super) fn build_types_output(types: &[TypeDefinition], _max_tokens: usize) -> Value {
    let rows: Vec<String> = types
        .iter()
        .map(|ty| {
            let file = path_utils::to_relative_path(ty.file.to_string_lossy().as_ref());
            let kind = type_kind_str(ty.kind);
            let fields = [
                ty.name.as_str(),
                kind,
                file.as_str(),
                &ty.line.to_string(),
                &ty.usage_count.to_string(),
            ];
            format::format_row(&fields)
        })
        .collect();

    json!({
        "h": "name|kind|file|line|usage_count",
        "rows": rows.join("\n")
    })
}

pub(super) fn shrink_types_output(types_val: &mut Value) -> bool {
    let Some(obj) = types_val.as_object_mut() else {
        return false;
    };
    let Some(rows) = obj.get("rows").and_then(Value::as_str) else {
        return false;
    };
    if rows.is_empty() {
        return false;
    }

    let mut lines: Vec<&str> = rows.lines().collect();
    lines.pop();
    if lines.is_empty() {
        obj.remove("rows");
    } else {
        obj.insert("rows".to_string(), json!(lines.join("\n")));
    }
    true
}

pub(super) fn type_kind_str(kind: TypeKind) -> &'static str {
    match kind {
        TypeKind::Interface => "interface",
        TypeKind::Class => "class",
        TypeKind::Struct => "struct",
        TypeKind::Enum => "enum",
        TypeKind::Trait => "trait",
        TypeKind::Protocol => "protocol",
        TypeKind::TypeAlias => "type_alias",
        TypeKind::Record => "record",
        TypeKind::TypedDict => "typed_dict",
        TypeKind::NamedTuple => "named_tuple",
    }
}

pub(super) fn build_compact_file(file: &FileSymbols, detail_level: DetailLevel) -> Value {
    let mut file_obj = Map::new();
    file_obj.insert("h".to_string(), json!(detail_level.header()));

    if !file.functions.is_empty() {
        file_obj.insert(
            "f".to_string(),
            json!(symbols_to_rows(
                &file.functions,
                detail_level,
                SymbolKind::Function,
            )),
        );
    }

    if !file.structs.is_empty() {
        file_obj.insert(
            "s".to_string(),
            json!(symbols_to_rows(
                &file.structs,
                detail_level,
                SymbolKind::Struct,
            )),
        );
    }

    if !file.classes.is_empty() {
        file_obj.insert(
            "c".to_string(),
            json!(symbols_to_rows(
                &file.classes,
                detail_level,
                SymbolKind::Class,
            )),
        );
    }

    Value::Object(file_obj)
}

pub(super) fn shrink_single_file_to_fit(
    file_value: &mut Value,
    bpe: &tiktoken_rs::CoreBPE,
    max_tokens: usize,
) -> bool {
    let Some(file_obj) = file_value.as_object_mut() else {
        return false;
    };

    // Prefer removing rows from the largest table first.
    let mut candidates: Vec<(&str, usize)> = Vec::new();
    for key in ["f", "s", "c"] {
        if let Some(rows) = file_obj.get(key).and_then(Value::as_str) {
            let count = if rows.is_empty() {
                0
            } else {
                rows.lines().count()
            };
            candidates.push((key, count));
        }
    }

    candidates.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    let Some((key, _)) = candidates.first().copied() else {
        return false;
    };

    let Some(rows) = file_obj.get(key).and_then(Value::as_str) else {
        return false;
    };

    if rows.is_empty() {
        file_obj.remove(key);
        return true;
    }

    let mut lines: Vec<&str> = rows.lines().collect();
    if lines.pop().is_none() {
        file_obj.remove(key);
        return true;
    }

    let new_rows = lines.join("\n");
    if new_rows.is_empty() {
        file_obj.remove(key);
    } else {
        file_obj.insert(key.to_string(), json!(new_rows));
    }

    // If we're still wildly over budget, allow dropping entire tables.
    let snapshot = Value::Object(file_obj.clone());
    let tmp_json = serde_json::to_string(&json!({"_": snapshot})).unwrap_or_default();
    if bpe.encode_with_special_tokens(&tmp_json).len() > max_tokens {
        // Prefer dropping `c`, then `s`, then `f`.
        for drop_key in ["c", "s", "f"] {
            if file_obj.contains_key(drop_key) {
                file_obj.remove(drop_key);
                break;
            }
        }
    }

    true
}

pub(super) fn symbols_to_rows(
    symbols: &[Value],
    detail_level: DetailLevel,
    kind: SymbolKind,
) -> String {
    symbols
        .iter()
        .filter_map(|sym| sym.as_object())
        .map(|obj| {
            let name = obj.get("name").and_then(Value::as_str).unwrap_or_default();
            let line = obj
                .get("line")
                .and_then(Value::as_u64)
                .map(|v| v.to_string())
                .unwrap_or_default();

            let mut fields: Vec<String> = Vec::new();
            fields.push(name.to_string());
            fields.push(line);

            match detail_level {
                DetailLevel::Minimal => {}
                DetailLevel::Signatures => {
                    let signature = match kind {
                        SymbolKind::Function => obj
                            .get("signature")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                        SymbolKind::Struct | SymbolKind::Class => "",
                    };
                    fields.push(signature.to_string());
                }
                DetailLevel::Full => {
                    let signature = match kind {
                        SymbolKind::Function => obj
                            .get("signature")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                        SymbolKind::Struct | SymbolKind::Class => "",
                    };
                    let doc = obj.get("doc").and_then(Value::as_str).unwrap_or_default();
                    let code = obj.get("code").and_then(Value::as_str).unwrap_or_default();

                    fields.push(signature.to_string());
                    fields.push(doc.to_string());
                    fields.push(code.to_string());
                }
            }

            let field_refs: Vec<&str> = fields.iter().map(|s| s.as_str()).collect();
            format::format_row(&field_refs)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn symbol_count(entry: &FileSymbols) -> usize {
    entry.functions.len() + entry.structs.len() + entry.classes.len()
}

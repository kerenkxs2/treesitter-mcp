use super::*;

pub(super) fn build_dependency_rows(
    dep_paths: &[PathBuf],
    main_file: &Path,
    referenced: &HashSet<String>,
    detail: DetailLevel,
    max_tokens: usize,
    comment_mode: CommentMode,
) -> Result<Map<String, Value>, io::Error> {
    if dep_paths.is_empty() {
        return Ok(Map::new());
    }

    let bpe = cl100k_base()
        .map_err(|e| io::Error::other(format!("Failed to initialize tiktoken tokenizer: {e}")))?;

    // 10% buffer: estimates should err on the safe side.
    let mut budget_tracker = BudgetTracker::new((max_tokens * 9) / 10);

    // Mark main file as visited.
    let mut visited = HashSet::new();
    if let Ok(canonical) = fs::canonicalize(main_file) {
        visited.insert(canonical);
    }

    let mut deps: Map<String, Value> = Map::new();

    // Preserve dependency ordering: HashMap iteration order is randomized and would
    // make output flaky (tests and clients expect stable selection).
    let mut dep_type_candidates: Vec<(String, Vec<TypeRow>)> = Vec::new();

    for dep_path in dep_paths {
        let canonical = match fs::canonicalize(dep_path) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("Failed to canonicalize {}: {}", dep_path.display(), e);
                continue;
            }
        };

        if visited.contains(&canonical) {
            continue;
        }
        visited.insert(canonical);

        let dep_rows = extract_dependency_types(dep_path, referenced, detail, comment_mode)?;
        if dep_rows.is_empty() {
            continue;
        }

        let rel = path_utils::to_relative_path(dep_path.to_string_lossy().as_ref());
        dep_type_candidates.push((rel, dep_rows));
    }

    // Only return explicitly referenced dependency types.
    for (dep_path, rows) in &dep_type_candidates {
        let selected: Vec<&TypeRow> = rows.iter().filter(|row| row.referenced).collect();
        if selected.is_empty() {
            continue;
        }

        let rows_str = type_rows_to_string(&selected, detail);
        if rows_str.is_empty() {
            continue;
        }

        let estimated = budget::estimate_symbol_tokens(rows_str.len() + dep_path.len() + 16);
        if !budget_tracker.add(estimated) {
            break;
        }

        deps.insert(dep_path.clone(), json!(rows_str));
    }

    // Hard enforcement: drop deps until within token budget.
    loop {
        if deps.is_empty() {
            break;
        }

        let snapshot = serde_json::to_string(&json!({"deps": deps.clone()})).unwrap_or_default();
        if bpe.encode_with_special_tokens(&snapshot).len() <= max_tokens {
            break;
        }

        // Remove last inserted key (Map iteration order is insertion order).
        let Some(last_key) = deps.keys().next_back().cloned() else {
            break;
        };
        deps.remove(&last_key);
    }

    Ok(deps)
}

pub(super) fn extract_dependency_types(
    file_path: &Path,
    referenced: &HashSet<String>,
    detail: DetailLevel,
    comment_mode: CommentMode,
) -> Result<Vec<TypeRow>, io::Error> {
    let source = fs::read_to_string(file_path)?;
    let language = detect_language(file_path).map_err(|e| {
        io::Error::new(
            io::ErrorKind::Unsupported,
            format!("Cannot detect language: {e}"),
        )
    })?;

    let tree = parse_code(&source, language).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to parse code: {e}"),
        )
    })?;

    // Always parse deps with code: signatures mode still needs a signature snippet.
    let shape = extract_enhanced_shape(
        &tree,
        &source,
        language,
        Some(file_path.to_str().unwrap_or("unknown")),
        true,
    )?;
    let type_signatures: HashMap<String, String> =
        extract_local_type_definitions(language, &source, file_path)
            .unwrap_or_default()
            .into_iter()
            .map(|ty| (ty.name, ty.signature))
            .collect();

    let mut rows = Vec::new();

    for s in shape.structs {
        rows.push(TypeRow {
            referenced: referenced.contains(&s.name),
            signature: type_signatures.get(&s.name).cloned(),
            name: s.name,
            line: s.line,
            doc: if detail == DetailLevel::Full {
                s.doc
            } else {
                None
            },
            code: prepend_leading_comments_to_code(&source, s.line, language, s.code, comment_mode),
        });
    }

    for c in shape.classes {
        rows.push(TypeRow {
            referenced: referenced.contains(&c.name),
            signature: type_signatures.get(&c.name).cloned(),
            name: c.name,
            line: c.line,
            doc: if detail == DetailLevel::Full {
                c.doc
            } else {
                None
            },
            code: prepend_leading_comments_to_code(&source, c.line, language, c.code, comment_mode),
        });
    }

    for i in shape.interfaces {
        rows.push(TypeRow {
            referenced: referenced.contains(&i.name),
            signature: type_signatures.get(&i.name).cloned(),
            name: i.name,
            line: i.line,
            doc: if detail == DetailLevel::Full {
                i.doc
            } else {
                None
            },
            code: prepend_leading_comments_to_code(&source, i.line, language, i.code, comment_mode),
        });
    }

    for tr in shape.traits {
        rows.push(TypeRow {
            referenced: referenced.contains(&tr.name),
            signature: type_signatures.get(&tr.name).cloned(),
            name: tr.name,
            line: tr.line,
            doc: if detail == DetailLevel::Full {
                tr.doc
            } else {
                None
            },
            code: None,
        });
    }

    // Stable ordering: file order.
    rows.sort_by_key(|row| row.line);

    Ok(rows)
}

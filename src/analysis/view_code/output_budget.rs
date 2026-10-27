use super::*;

pub(super) fn remove_last_dep_entry(out: &mut Map<String, Value>) -> bool {
    let Some(deps_value) = out.get_mut("deps") else {
        return false;
    };

    let Some(deps_obj) = deps_value.as_object_mut() else {
        out.remove("deps");
        return true;
    };

    let Some(last_key) = deps_obj.keys().next_back().cloned() else {
        out.remove("deps");
        return true;
    };

    deps_obj.remove(&last_key);
    if deps_obj.is_empty() {
        out.remove("deps");
    }

    true
}

pub(super) fn shrink_symbol_tables(out: &mut Map<String, Value>) -> bool {
    // Prefer removing rows from the largest table first.
    let mut candidates: Vec<(&str, usize)> = Vec::new();
    for key in ["f", "s", "c", "im", "tm", "i", "pr", "ci", "cm", "bm"] {
        if let Some(rows) = out.get(key).and_then(Value::as_str) {
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

    let Some(rows) = out.get(key).and_then(Value::as_str) else {
        return false;
    };

    if rows.is_empty() {
        remove_table_and_header(out, key);
        return true;
    }

    let mut lines: Vec<&str> = rows.lines().collect();
    if lines.pop().is_none() {
        remove_table_and_header(out, key);
        return true;
    }

    let new_rows = lines.join("\n");
    if new_rows.is_empty() {
        remove_table_and_header(out, key);
    } else {
        out.insert(key.to_string(), json!(new_rows));
    }

    true
}

pub(super) fn remove_table_and_header(out: &mut Map<String, Value>, key: &str) {
    out.remove(key);

    let header_key = match key {
        "im" => Some("ih"),
        "tm" => Some("th"),
        "i" => Some("ah"),
        "pr" => Some("ph"),
        "ci" => Some("ch"),
        "cm" => Some("mh"),
        "bm" => Some("bh"),
        _ => None,
    };

    if let Some(header_key) = header_key {
        out.remove(header_key);
    }
}

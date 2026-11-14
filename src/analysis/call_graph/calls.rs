use super::*;

pub(super) fn collect_called_names(
    tree: &Tree,
    source: &str,
    language: Language,
    start_line: usize,
    end_line: usize,
) -> HashSet<String> {
    let mut names = HashSet::new();
    collect_called_names_from_node(
        tree.root_node(),
        source,
        language,
        start_line.saturating_sub(1),
        end_line.saturating_sub(1),
        &mut names,
    );
    names
}

pub(super) fn collect_called_names_from_node(
    node: Node<'_>,
    source: &str,
    language: Language,
    start_row: usize,
    end_row: usize,
    names: &mut HashSet<String>,
) {
    let node_start = node.start_position().row;
    let node_end = node.end_position().row;
    if node_end < start_row || node_start > end_row {
        return;
    }

    if is_call_node(node.kind(), language) {
        if let Some(name) = call_name(node, source) {
            names.insert(name);
        }
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_called_names_from_node(child, source, language, start_row, end_row, names);
    }
}

pub(super) fn collect_call_sites(
    tree: &Tree,
    source: &str,
    language: Language,
    symbol_name: &str,
) -> Vec<usize> {
    let mut lines = Vec::new();
    collect_call_sites_from_node(tree.root_node(), source, language, symbol_name, &mut lines);
    lines
}

pub(super) fn collect_call_sites_from_node(
    node: Node<'_>,
    source: &str,
    language: Language,
    symbol_name: &str,
    lines: &mut Vec<usize>,
) {
    if is_call_node(node.kind(), language)
        && call_name(node, source).is_some_and(|name| {
            name == symbol_name || name.rsplit("::").next() == Some(symbol_name)
        })
    {
        lines.push(node.start_position().row + 1);
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_call_sites_from_node(child, source, language, symbol_name, lines);
    }
}

pub(super) fn is_call_node(kind: &str, language: Language) -> bool {
    match language {
        Language::GDScript => matches!(kind, "call" | "attribute_call"),
        Language::Lua => kind == "function_call",
        Language::Rust => matches!(kind, "call_expression" | "method_call_expression"),
        Language::Python => kind == "call",
        Language::JavaScript
        | Language::TypeScript
        | Language::Tsx
        | Language::Go
        | Language::C
        | Language::Cpp => kind == "call_expression",
        Language::Java | Language::CSharp | Language::Swift => kind.ends_with("invocation"),
        Language::Html | Language::Css => false,
    }
}

pub(super) fn call_name(node: Node<'_>, source: &str) -> Option<String> {
    for field in ["function", "name", "method", "field"] {
        if let Some(child) = node.child_by_field_name(field) {
            if let Some(name) = last_identifier_text(child, source) {
                return Some(name);
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if let Some(name) = last_identifier_text(child, source) {
            return Some(name);
        }
    }

    None
}

pub(super) fn last_identifier_text(node: Node<'_>, source: &str) -> Option<String> {
    if crate::parser::scripting::identifier(node.kind()) {
        return node
            .utf8_text(source.as_bytes())
            .ok()
            .map(ToOwned::to_owned);
    }

    let mut found = None;
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if let Some(name) = last_identifier_text(child, source) {
            found = Some(name);
        }
    }

    found
}

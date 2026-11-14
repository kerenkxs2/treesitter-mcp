use super::*;

pub(crate) fn classify_usage_type(node: &Node) -> String {
    if let Some(parent) = node.parent() {
        let parent_kind = parent.kind();

        if matches!(
            parent_kind,
            "class_specifier"
                | "struct_specifier"
                | "union_specifier"
                | "enum_specifier"
                | "alias_declaration"
        ) {
            return "definition".into();
        }
        if parent_kind == "function_declarator"
            && parent
                .child_by_field_name("declarator")
                .is_some_and(|name| name.id() == node.id())
        {
            return "definition".into();
        }
        if parent_kind == "qualified_identifier" {
            if let Some(declarator) = parent.parent() {
                if declarator.kind() == "function_declarator"
                    && declarator
                        .child_by_field_name("declarator")
                        .is_some_and(|name| name.id() == parent.id())
                {
                    return "definition".into();
                }
            }
        }

        if parent_kind == "function_item"
            || parent_kind == "function_declaration"
            || parent_kind == "method_definition"
            || parent_kind == "method_declaration"
            || parent_kind == "struct_item"
            || parent_kind == "class_definition"
            || parent_kind == "class_declaration"
            || parent_kind == "enum_item"
            || parent_kind == "interface_declaration"
            || parent_kind == "type_alias_declaration"
        {
            return "definition".to_string();
        }

        if parent_kind == "let_declaration"
            || parent_kind == "const_item"
            || parent_kind == "static_item"
            || parent_kind == "variable_declarator"
            || parent_kind == "lexical_declaration"
        {
            return "definition".to_string();
        }

        if parent_kind == "use_declaration"
            || parent_kind == "import_statement"
            || parent_kind == "import_clause"
            || parent_kind == "import_specifier"
        {
            return "import".to_string();
        }

        if parent_kind == "call_expression"
            || parent_kind == "method_call_expression"
            || parent_kind == "call"
        {
            return "call".to_string();
        }

        if parent_kind == "type_annotation"
            || parent_kind == "type_identifier"
            || parent_kind == "generic_type"
            || parent_kind == "type_arguments"
            || parent_kind == "type_parameter"
            || node.kind() == "type_identifier"
        {
            return "type_reference".to_string();
        }

        if let Some(grandparent) = parent.parent() {
            let grandparent_kind = grandparent.kind();

            if grandparent_kind == "let_declaration"
                || grandparent_kind == "const_item"
                || grandparent_kind == "variable_declaration"
            {
                return "definition".to_string();
            }

            if grandparent_kind == "parameter"
                || grandparent_kind == "formal_parameter"
                || grandparent_kind == "return_type"
            {
                return "type_reference".to_string();
            }

            if grandparent_kind == "call_expression" || grandparent_kind == "method_call_expression"
            {
                return "call".to_string();
            }

            if let Some(great_grandparent) = grandparent.parent() {
                let great_grandparent_kind = great_grandparent.kind();

                if great_grandparent_kind == "let_declaration"
                    || great_grandparent_kind == "const_item"
                    || great_grandparent_kind == "variable_declaration"
                {
                    return "definition".to_string();
                }
            }
        }
    }

    "reference".to_string()
}

pub(crate) fn scope_for_node(node: Node, source: &str, language: Language) -> String {
    let mut parts = Vec::new();
    let mut current = Some(node);

    while let Some(candidate) = current {
        if is_context_node(candidate.kind(), language) {
            let name = if matches!(language, Language::C | Language::Cpp) {
                crate::analysis::shape::c_family::context_name(candidate, source)
            } else if matches!(language, Language::GDScript | Language::Lua) {
                crate::parser::scripting::context_name(candidate, source)
            } else {
                extract_scope_name(candidate, source)
            };
            if let Some(name) = name {
                parts.push(name);
            }
        }
        current = candidate.parent();
    }

    parts.reverse();
    parts.join("::")
}

pub(super) fn is_context_node(node_type: &str, language: Language) -> bool {
    match language {
        Language::GDScript => matches!(
            node_type,
            "function_definition" | "lambda" | "class_definition"
        ),
        Language::Lua => matches!(
            node_type,
            "function_declaration" | "function_definition" | "table_constructor"
        ),
        Language::Rust => matches!(
            node_type,
            "function_item" | "impl_item" | "trait_item" | "struct_item" | "enum_item" | "mod_item"
        ),
        Language::Python => matches!(node_type, "function_definition" | "class_definition"),
        Language::JavaScript | Language::TypeScript | Language::Tsx => matches!(
            node_type,
            "function_declaration"
                | "method_definition"
                | "class_declaration"
                | "arrow_function"
                | "function_expression"
        ),
        Language::Go => matches!(
            node_type,
            "function_declaration" | "method_declaration" | "type_spec"
        ),
        Language::CSharp => matches!(
            node_type,
            "method_declaration"
                | "constructor_declaration"
                | "property_declaration"
                | "class_declaration"
                | "interface_declaration"
                | "struct_declaration"
                | "namespace_declaration"
        ),
        Language::Java => matches!(
            node_type,
            "method_declaration"
                | "constructor_declaration"
                | "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
        ),
        Language::C | Language::Cpp => matches!(
            node_type,
            "function_definition"
                | "class_specifier"
                | "struct_specifier"
                | "union_specifier"
                | "namespace_definition"
        ),
        Language::Html | Language::Css | Language::Swift => false,
    }
}

pub(super) fn extract_scope_name(node: Node, source: &str) -> Option<String> {
    match node.kind() {
        "impl_item" => extract_impl_scope_name(node, source),
        "method_declaration" | "method_definition" => extract_method_name(node, source),
        _ => extract_named_field(node, source),
    }
}

pub(super) fn extract_impl_scope_name(node: Node, source: &str) -> Option<String> {
    if let Some(type_node) = node.child_by_field_name("type") {
        return type_node
            .utf8_text(source.as_bytes())
            .ok()
            .map(normalize_scope_token);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind().contains("type_identifier") || child.kind() == "scoped_type_identifier" {
            if let Ok(text) = child.utf8_text(source.as_bytes()) {
                return Some(normalize_scope_token(text));
            }
        }
    }

    None
}

pub(super) fn extract_method_name(node: Node, source: &str) -> Option<String> {
    node.child_by_field_name("name")
        .or_else(|| node.child_by_field_name("property"))
        .or_else(|| node.child_by_field_name("field"))
        .and_then(|child| child.utf8_text(source.as_bytes()).ok())
        .map(ToOwned::to_owned)
        .or_else(|| extract_named_field(node, source))
}

pub(super) fn extract_named_field(node: Node, source: &str) -> Option<String> {
    for field_name in ["name", "property", "field"] {
        if let Some(name_node) = node.child_by_field_name(field_name) {
            if let Ok(text) = name_node.utf8_text(source.as_bytes()) {
                return Some(normalize_scope_token(text));
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind().contains("identifier") {
            if let Ok(text) = child.utf8_text(source.as_bytes()) {
                return Some(normalize_scope_token(text));
            }
        }
    }

    None
}

pub(crate) fn owner_hint(node: Node, source: &str) -> Option<String> {
    let parent = node.parent()?;

    for field_name in [
        "function", "receiver", "object", "value", "argument", "module", "scope",
    ] {
        if let Some(candidate) = parent.child_by_field_name(field_name) {
            if candidate.id() == node.id() {
                continue;
            }
            if let Ok(text) = candidate.utf8_text(source.as_bytes()) {
                if let Some(owner) = normalize_owner_hint(text) {
                    return Some(owner);
                }
            }
        }
    }

    let mut cursor = parent.walk();
    let named_children: Vec<Node> = parent
        .children(&mut cursor)
        .filter(|child| child.is_named())
        .collect();
    let position = named_children
        .iter()
        .position(|child| child.id() == node.id())?;
    let previous = named_children.get(position.checked_sub(1)?)?;
    previous
        .utf8_text(source.as_bytes())
        .ok()
        .and_then(normalize_owner_hint)
}

pub(super) fn normalize_owner_hint(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    let without_generics = trimmed
        .split('<')
        .next()
        .unwrap_or(trimmed)
        .split('(')
        .next()
        .unwrap_or(trimmed)
        .trim();

    without_generics
        .rsplit([':', '.'])
        .find(|segment| !segment.is_empty())
        .map(normalize_scope_token)
}

pub(super) fn normalize_scope_token(text: &str) -> String {
    text.trim().trim_matches('&').trim().to_string()
}

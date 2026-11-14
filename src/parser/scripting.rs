//! Shared syntax primitives for GDScript and Lua's named and assigned functions.

use tree_sitter::Node;

pub(crate) fn text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}

pub(crate) fn identifier(kind: &str) -> bool {
    kind == "name" || kind == "identifier" || kind.ends_with("_identifier")
}

pub(crate) fn signature(node: Node<'_>, source: &str) -> String {
    let end = node
        .child_by_field_name("body")
        .map_or(node.end_byte(), |body| body.start_byte());
    source[node.start_byte()..end]
        .trim()
        .trim_end_matches("end")
        .trim()
        .to_string()
}

/// Lua anonymous functions/tables acquire their name from the owning assignment or field.
pub(crate) fn assigned_name(node: Node<'_>, source: &str) -> Option<String> {
    let parent = node.parent()?;
    match parent.kind() {
        "field" => parent
            .child_by_field_name("name")
            .map(|name| text(name, source).trim_matches(['\'', '"']).into()),
        "expression_list" => {
            let index = parent
                .named_children(&mut parent.walk())
                .position(|child| child.id() == node.id())?;
            let assignment = parent.parent()?;
            let variables = assignment
                .named_children(&mut assignment.walk())
                .find(|child| child.kind() == "variable_list")?;
            variables
                .children_by_field_name("name", &mut variables.walk())
                .nth(index)
                .map(|name| text(name, source).into())
        }
        "variable_statement" | "const_statement" => parent
            .child_by_field_name("name")
            .map(|name| text(name, source).into()),
        _ => None,
    }
}

pub(crate) fn context_name(node: Node<'_>, source: &str) -> Option<String> {
    node.child_by_field_name("name")
        .map(|name| text(name, source).into())
        .or_else(|| assigned_name(node, source))
}

pub(crate) fn import_path(node: Node<'_>, source: &str) -> Option<String> {
    if node.kind() == "extends_statement" {
        return node
            .named_child(0)
            .filter(|child| child.kind() == "string")
            .map(|path| string_text(path, source));
    }
    if !matches!(node.kind(), "call" | "function_call") {
        return None;
    }
    let callee = node
        .child_by_field_name("name")
        .or_else(|| node.named_child(0))?;
    if !matches!(text(callee, source), "preload" | "load" | "require") {
        return None;
    }
    let args = node.child_by_field_name("arguments")?;
    args.named_child(0)
        .filter(|arg| arg.kind() == "string")
        .map(|path| string_text(path, source))
}

fn string_text(node: Node<'_>, source: &str) -> String {
    node.child_by_field_name("content").map_or_else(
        || text(node, source).trim_matches(['\'', '"']).to_string(),
        |content| text(content, source).into(),
    )
}

pub(crate) fn usage_kind(node: Node<'_>) -> Option<&'static str> {
    let parent = node.parent()?;
    if matches!(
        parent.kind(),
        "function_definition"
            | "function_declaration"
            | "class_definition"
            | "class_name_statement"
            | "variable_statement"
            | "const_statement"
            | "signal_statement"
            | "enum_definition"
    ) && parent
        .child_by_field_name("name")
        .is_some_and(|name| name.id() == node.id())
    {
        return Some("definition");
    }
    if parent.kind() == "variable_list" {
        return Some("definition");
    }
    if parent.kind() == "type" {
        return Some("type_reference");
    }
    let mut current = node;
    while let Some(owner) = current.parent() {
        if matches!(owner.kind(), "call" | "attribute_call" | "function_call") {
            let callee = owner
                .child_by_field_name("name")
                .or_else(|| owner.named_child(0));
            return callee
                .filter(|callee| callee.id() == current.id())
                .map(|_| "call");
        }
        if matches!(
            owner.kind(),
            "dot_index_expression" | "method_index_expression" | "attribute"
        ) {
            if let Some(declaration) = owner.parent() {
                if declaration.kind() == "function_declaration" {
                    return Some("definition");
                }
            }
            current = owner;
        } else {
            break;
        }
    }
    None
}

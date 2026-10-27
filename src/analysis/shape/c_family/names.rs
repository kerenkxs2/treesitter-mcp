use super::*;

pub(crate) fn qualified_name(scope: &str, name: &str) -> String {
    if scope.is_empty() || name.starts_with("::") || name.starts_with(&format!("{scope}::")) {
        name.into()
    } else {
        format!("{scope}::{name}")
    }
}

pub(crate) fn declarator_name(node: Node<'_>, source: &str) -> Option<String> {
    match node.kind() {
        "identifier"
        | "field_identifier"
        | "type_identifier"
        | "qualified_identifier"
        | "destructor_name"
        | "operator_name" => Some(text(node, source).into()),
        "template_function" | "template_method" | "template_type" => node
            .child_by_field_name("name")
            .and_then(|name| declarator_name(name, source)),
        _ => node
            .child_by_field_name("declarator")
            .and_then(|name| declarator_name(name, source)),
    }
}

pub(crate) fn signature(node: Node<'_>, source: &str) -> String {
    let body = node.child_by_field_name("body").or_else(|| {
        node.named_children(&mut node.walk())
            .find(|child| {
                matches!(
                    child.kind(),
                    "function_definition"
                        | "class_specifier"
                        | "struct_specifier"
                        | "union_specifier"
                )
            })
            .and_then(|child| child.child_by_field_name("body"))
    });
    let end = body
        .map(|body| body.start_byte())
        .unwrap_or(node.end_byte());
    source
        .get(node.start_byte()..end)
        .unwrap_or("")
        .trim()
        .trim_end_matches(';')
        .trim()
        .into()
}

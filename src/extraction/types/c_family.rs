use super::*;
use crate::analysis::shape::c_family::{declarator_name, qualified_name, signature, text};
use crate::parser::{parse_code, Language};

pub(crate) fn extract_c_family_types(
    source: &str,
    relative_path: &Path,
    language: Language,
) -> Result<Vec<TypeDefinition>> {
    let tree = parse_code(source, language)?;
    let mut types = Vec::new();
    visit(tree.root_node(), source, relative_path, "", &mut types);
    Ok(types)
}

fn visit(node: Node<'_>, source: &str, path: &Path, scope: &str, types: &mut Vec<TypeDefinition>) {
    if node.kind() == "namespace_definition" {
        let name = node
            .child_by_field_name("name")
            .map(|name| text(name, source));
        let scope = name
            .map(|name| qualified_name(scope, name))
            .unwrap_or(scope.into());
        if let Some(body) = node.child_by_field_name("body") {
            visit(body, source, path, &scope, types);
        }
        return;
    }
    let kind = match node.kind() {
        "class_specifier" => Some(TypeKind::Class),
        "struct_specifier" | "union_specifier" => Some(TypeKind::Struct),
        "enum_specifier" => Some(TypeKind::Enum),
        "alias_declaration" | "type_definition" => Some(TypeKind::TypeAlias),
        _ => None,
    };
    let mut child_scope = scope.to_string();
    if let Some(kind) = kind {
        let name = node
            .child_by_field_name("name")
            .or_else(|| node.child_by_field_name("declarator"));
        if let Some(name) = name.and_then(|name| declarator_name(name, source)) {
            let name = qualified_name(scope, &name);
            let fields = node
                .child_by_field_name("body")
                .and_then(|body| fields(body, source));
            let variants = node
                .child_by_field_name("body")
                .and_then(|body| variants(body, source));
            types.push(TypeDefinition {
                name: name.clone(),
                kind,
                file: path.into(),
                line: node.start_position().row + 1,
                signature: signature(node, source),
                usage_count: 0,
                fields,
                variants,
                members: None,
            });
            if matches!(kind, TypeKind::Class | TypeKind::Struct) {
                child_scope = name;
            }
        }
    }
    for child in node.named_children(&mut node.walk()) {
        if !matches!(child.kind(), "compound_statement" | "function_definition") {
            visit(child, source, path, &child_scope, types);
        }
    }
}

fn fields(body: Node<'_>, source: &str) -> Option<Vec<Field>> {
    let mut fields = Vec::new();
    for node in body
        .named_children(&mut body.walk())
        .filter(|node| node.kind() == "field_declaration")
    {
        let ty = node
            .child_by_field_name("type")
            .map(|ty| text(ty, source))
            .unwrap_or("");
        for declarator in node.children_by_field_name("declarator", &mut node.walk()) {
            if crate::analysis::shape::c_family::function_declarator(declarator).is_none() {
                if let Some(name) = declarator_name(declarator, source) {
                    fields.push(Field {
                        name,
                        type_annotation: ty.into(),
                    });
                }
            }
        }
    }
    (!fields.is_empty()).then_some(fields)
}

fn variants(body: Node<'_>, source: &str) -> Option<Vec<Variant>> {
    let variants: Vec<_> = body
        .named_children(&mut body.walk())
        .filter(|node| node.kind() == "enumerator")
        .filter_map(|node| node.child_by_field_name("name"))
        .map(|name| Variant {
            name: text(name, source).into(),
            type_annotation: None,
        })
        .collect();
    (!variants.is_empty()).then_some(variants)
}

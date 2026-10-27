use super::*;

pub(super) fn functions(
    node: Node<'_>,
    outer: Node<'_>,
    source: &str,
    scope: &str,
    include_code: bool,
    output: &mut Vec<EnhancedFunctionInfo>,
) {
    let candidates: Vec<_> = if node.kind() == "function_definition" {
        vec![node]
    } else {
        node.children_by_field_name("declarator", &mut node.walk())
            .collect()
    };
    for candidate in candidates {
        let Some(function) = function_declarator(candidate) else {
            continue;
        };
        let Some(name) = function
            .child_by_field_name("declarator")
            .and_then(|name| declarator_name(name, source))
        else {
            continue;
        };
        output.push(EnhancedFunctionInfo {
            name: qualified_name(scope, &name),
            signature: signature(outer, source),
            line: outer.start_position().row + 1,
            end_line: outer.end_position().row + 1,
            doc: None,
            code: include_code.then(|| text(outer, source).into()),
            annotations: vec![],
        });
    }
}

pub(super) fn class(
    node: Node<'_>,
    outer: Node<'_>,
    source: &str,
    scope: &str,
    include_code: bool,
    shape: &mut EnhancedFileShape,
) {
    let Some(name) = node.child_by_field_name("name") else {
        return;
    };
    let name = qualified_name(scope, text(name, source));
    let mut methods = Vec::new();
    let mut fields = Vec::new();
    if let Some(body) = node.child_by_field_name("body") {
        members(
            body,
            source,
            &name,
            include_code,
            shape,
            &mut methods,
            &mut fields,
        );
    }
    let implements = node
        .named_children(&mut node.walk())
        .find(|child| child.kind() == "base_class_clause")
        .map(|bases| {
            bases
                .named_children(&mut bases.walk())
                .filter(|base| {
                    matches!(
                        base.kind(),
                        "type_identifier" | "qualified_identifier" | "template_type"
                    )
                })
                .map(|base| text(base, source).into())
                .collect()
        })
        .unwrap_or_default();
    shape.classes.push(EnhancedClassInfo {
        name,
        line: outer.start_position().row + 1,
        end_line: outer.end_position().row + 1,
        doc: None,
        code: include_code.then(|| text(outer, source).into()),
        methods,
        fields,
        properties: vec![],
        implements,
    });
}

fn members(
    body: Node<'_>,
    source: &str,
    scope: &str,
    include_code: bool,
    shape: &mut EnhancedFileShape,
    methods: &mut Vec<EnhancedFunctionInfo>,
    fields: &mut Vec<PropertyInfo>,
) {
    for child in body.named_children(&mut body.walk()) {
        match child.kind() {
            "function_definition" | "declaration" | "field_declaration" => {
                let before = methods.len();
                functions(child, child, source, "", include_code, methods);
                if before == methods.len() {
                    for declarator in child.children_by_field_name("declarator", &mut child.walk())
                    {
                        if let Some(name) = declarator_name(declarator, source) {
                            fields.push(PropertyInfo {
                                name,
                                line: child.start_position().row + 1,
                                end_line: child.end_position().row + 1,
                                property_type: child
                                    .child_by_field_name("type")
                                    .map(|ty| text(ty, source).into()),
                                doc: None,
                            });
                        }
                    }
                }
                for nested in child.named_children(&mut child.walk()) {
                    if matches!(
                        nested.kind(),
                        "class_specifier" | "struct_specifier" | "union_specifier"
                    ) {
                        class(nested, child, source, scope, include_code, shape);
                    }
                }
            }
            "class_specifier" | "struct_specifier" | "union_specifier" => {
                class(child, child, source, scope, include_code, shape)
            }
            "template_declaration" => {
                for declaration in child.named_children(&mut child.walk()) {
                    if matches!(
                        declaration.kind(),
                        "function_definition" | "declaration" | "field_declaration"
                    ) {
                        functions(declaration, child, source, "", include_code, methods);
                    }
                }
            }
            _ if child.kind().starts_with("preproc_") => {
                members(child, source, scope, include_code, shape, methods, fields)
            }
            _ => {}
        }
    }
}

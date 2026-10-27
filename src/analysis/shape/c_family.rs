//! C and C++ structural extraction using their actual grammars.

use super::*;

mod declarations;
mod names;
pub(crate) use names::{declarator_name, qualified_name, signature};

pub(super) fn extract(
    tree: &Tree,
    source: &str,
    include_code: bool,
) -> Result<EnhancedFileShape, io::Error> {
    let mut shape = EnhancedFileShape {
        path: None,
        language: None,
        functions: vec![],
        structs: vec![],
        classes: vec![],
        imports: vec![],
        impl_blocks: vec![],
        traits: vec![],
        interfaces: vec![],
        properties: vec![],
        dependencies: vec![],
    };
    visit(tree.root_node(), source, "", include_code, &mut shape);
    Ok(shape)
}

fn visit(
    node: Node<'_>,
    source: &str,
    scope: &str,
    include_code: bool,
    shape: &mut EnhancedFileShape,
) {
    match node.kind() {
        "namespace_definition" => {
            let name = node.child_by_field_name("name").map(|n| text(n, source));
            let scope = name
                .map(|n| qualified_name(scope, n))
                .unwrap_or(scope.into());
            if let Some(body) = node.child_by_field_name("body") {
                visit(body, source, &scope, include_code, shape);
            }
        }
        "class_specifier" | "struct_specifier" | "union_specifier" => {
            declarations::class(node, node, source, scope, include_code, shape);
        }
        "template_declaration" => {
            for child in node.named_children(&mut node.walk()) {
                match child.kind() {
                    "class_specifier" | "struct_specifier" | "union_specifier" => {
                        declarations::class(child, node, source, scope, include_code, shape);
                    }
                    "function_definition" | "declaration" => {
                        declarations::functions(
                            child,
                            node,
                            source,
                            scope,
                            include_code,
                            &mut shape.functions,
                        );
                    }
                    _ => {}
                }
            }
        }
        "function_definition" | "declaration" => {
            declarations::functions(
                node,
                node,
                source,
                scope,
                include_code,
                &mut shape.functions,
            );
            // A C declaration can also own a named struct/union definition.
            for child in node.named_children(&mut node.walk()) {
                if matches!(
                    child.kind(),
                    "struct_specifier" | "union_specifier" | "class_specifier"
                ) {
                    declarations::class(child, node, source, scope, include_code, shape);
                }
            }
        }
        "preproc_include" => shape.imports.push(ImportInfo {
            text: text(node, source).trim().into(),
            line: node.start_position().row + 1,
        }),
        // Conditional compilation retains discoverable declarations on each branch.
        _ => {
            for child in node.named_children(&mut node.walk()) {
                visit(child, source, scope, include_code, shape);
            }
        }
    }
}

pub(crate) fn text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    source.get(node.byte_range()).unwrap_or("")
}

pub(crate) fn function_declarator(node: Node<'_>) -> Option<Node<'_>> {
    if node.kind() == "function_declarator" {
        return Some(node);
    }
    if let Some(declarator) = node.child_by_field_name("declarator") {
        return function_declarator(declarator);
    }
    for child in node.named_children(&mut node.walk()) {
        if matches!(
            child.kind(),
            "compound_statement" | "parameter_list" | "argument_list"
        ) {
            continue;
        }
        if let Some(declarator) = function_declarator(child) {
            return Some(declarator);
        }
    }
    None
}

pub(crate) fn context_name(node: Node<'_>, source: &str) -> Option<String> {
    match node.kind() {
        "function_definition" | "declaration" | "field_declaration" => {
            let function = function_declarator(node)?;
            function
                .child_by_field_name("declarator")
                .and_then(|n| declarator_name(n, source))
        }
        "namespace_definition"
        | "class_specifier"
        | "struct_specifier"
        | "union_specifier"
        | "enum_specifier" => node
            .child_by_field_name("name")
            .map(|name| text(name, source).into()),
        _ => None,
    }
}

//! GDScript file classes, nested classes, typed methods and properties.

use super::*;
use crate::parser::scripting::{context_name, text};

pub(super) fn extract(tree: &Tree, source: &str, include_code: bool) -> EnhancedFileShape {
    let mut shape = scripting::empty();
    let root = tree.root_node();
    let class_name = root
        .named_children(&mut root.walk())
        .find(|node| node.kind() == "class_name_statement");
    let owner = class_name
        .and_then(|node| context_name(node, source))
        .map(|name| {
            shape
                .classes
                .push(scripting::class(root, name, source, include_code));
            0
        });
    visit(root, source, include_code, owner, &mut shape);
    scripting::collect_imports(root, source, &mut shape);
    shape
}

fn visit(
    node: Node<'_>,
    source: &str,
    include_code: bool,
    owner: Option<usize>,
    shape: &mut EnhancedFileShape,
) {
    match node.kind() {
        "class_definition" => {
            if let Some(name) = context_name(node, source) {
                let name = owner.map_or(name.clone(), |index| {
                    format!("{}::{name}", shape.classes[index].name)
                });
                let index = shape.classes.len();
                shape
                    .classes
                    .push(scripting::class(node, name, source, include_code));
                if let Some(body) = node.child_by_field_name("body") {
                    visit(body, source, include_code, Some(index), shape);
                }
            }
        }
        "function_definition" | "lambda" => {
            if let Some(name) = context_name(node, source) {
                let function =
                    scripting::function(node, name, source, Language::GDScript, include_code);
                if let Some(index) = owner {
                    shape.classes[index].methods.push(function);
                } else {
                    shape.functions.push(function);
                }
            }
        }
        "variable_statement" | "const_statement" => {
            if let Some(property) = scripting::property(node, source) {
                if let Some(index) = owner {
                    shape.classes[index].fields.push(property);
                } else {
                    shape.properties.push(property);
                }
            }
            if let Some(value) = node
                .child_by_field_name("value")
                .filter(|value| value.kind() == "lambda")
            {
                visit(value, source, include_code, owner, shape);
            }
        }
        "extends_statement" => {
            if let Some(index) = owner {
                if let Some(base) = node.named_child(0) {
                    shape.classes[index]
                        .implements
                        .push(text(base, source).into());
                }
            }
        }
        _ => {
            for child in node.named_children(&mut node.walk()) {
                visit(child, source, include_code, owner, shape);
            }
        }
    }
}

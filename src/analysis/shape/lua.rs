//! Lua declarations, assigned functions and named module tables.

use super::*;
use crate::parser::scripting::{assigned_name, context_name};

pub(super) fn extract(tree: &Tree, source: &str, include_code: bool) -> EnhancedFileShape {
    let mut shape = scripting::empty();
    visit(tree.root_node(), source, include_code, None, &mut shape);
    scripting::collect_imports(tree.root_node(), source, &mut shape);
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
        "table_constructor" => {
            let index = assigned_name(node, source).map(|name| {
                let name = owner.map_or(name.clone(), |owner| {
                    format!("{}::{name}", shape.classes[owner].name)
                });
                let index = shape.classes.len();
                shape
                    .classes
                    .push(scripting::class(node, name, source, include_code));
                index
            });
            for child in node.named_children(&mut node.walk()) {
                visit(child, source, include_code, index, shape);
            }
        }
        "function_declaration" | "function_definition" => {
            if let Some(name) = context_name(node, source) {
                let split = name.rsplit_once(['.', ':']);
                let (index, name) = if let Some((table, method)) = split {
                    let index = shape.classes.iter().position(|class| class.name == table);
                    (index, method.to_string())
                } else {
                    (owner, name.clone())
                };
                let function = scripting::function(node, name, source, Language::Lua, include_code);
                if let Some(index) = index {
                    shape.classes[index].methods.push(function);
                } else {
                    let mut function = function;
                    if let Some((table, _)) = split {
                        function.name = format!("{table}::{}", function.name);
                    }
                    shape.functions.push(function);
                }
            }
        }
        "field" => {
            if let Some(value) = node.child_by_field_name("value") {
                if value.kind() != "function_definition" {
                    if let Some(index) = owner {
                        if let Some(property) = scripting::property(node, source) {
                            shape.classes[index].fields.push(property);
                        }
                    }
                }
                visit(value, source, include_code, owner, shape);
            }
        }
        _ => {
            for child in node.named_children(&mut node.walk()) {
                visit(child, source, include_code, owner, shape);
            }
        }
    }
}

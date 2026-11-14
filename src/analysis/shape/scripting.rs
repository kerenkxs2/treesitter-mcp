//! Shared shape constructors used by the two scripting grammars.

use super::*;
use crate::parser::scripting::{signature, text};

pub(super) fn empty() -> EnhancedFileShape {
    EnhancedFileShape {
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
    }
}

pub(super) fn function(
    node: Node<'_>,
    name: String,
    source: &str,
    language: Language,
    include_code: bool,
) -> EnhancedFunctionInfo {
    EnhancedFunctionInfo {
        name,
        signature: signature(node, source),
        line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        doc: common::extract_leading_comment_block(source, node.start_position().row + 1, language),
        code: include_code.then(|| text(node, source).into()),
        annotations: vec![],
    }
}

pub(super) fn class(
    node: Node<'_>,
    name: String,
    source: &str,
    include_code: bool,
) -> EnhancedClassInfo {
    EnhancedClassInfo {
        name,
        line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        doc: None,
        code: include_code.then(|| text(node, source).into()),
        methods: vec![],
        properties: vec![],
        fields: vec![],
        implements: vec![],
    }
}

pub(super) fn property(node: Node<'_>, source: &str) -> Option<PropertyInfo> {
    let name = node.child_by_field_name("name")?;
    Some(PropertyInfo {
        name: text(name, source).trim_matches(['\'', '"']).into(),
        line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        property_type: node
            .child_by_field_name("type")
            .map(|ty| text(ty, source).into()),
        doc: None,
    })
}

pub(super) fn collect_imports(node: Node<'_>, source: &str, shape: &mut EnhancedFileShape) {
    if crate::parser::scripting::import_path(node, source).is_some() {
        let mut statement = node;
        let mut ancestor = node.parent();
        while let Some(parent) = ancestor {
            if matches!(
                parent.kind(),
                "block" | "chunk" | "source" | "body" | "class_body"
            ) {
                break;
            }
            if matches!(
                parent.kind(),
                "expression_statement"
                    | "const_statement"
                    | "variable_statement"
                    | "assignment_statement"
                    | "variable_declaration"
            ) {
                statement = parent;
            }
            ancestor = parent.parent();
        }
        shape.imports.push(ImportInfo {
            text: text(statement, source).trim().into(),
            line: node.start_position().row + 1,
        });
    }
    for child in node.named_children(&mut node.walk()) {
        collect_imports(child, source, shape);
    }
}

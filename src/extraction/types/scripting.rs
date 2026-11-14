//! Type-map projection of the canonical GDScript/Lua structural shape.

use super::*;
use crate::parser::{parse_code, Language};

pub(crate) fn extract(
    source: &str,
    path: &Path,
    language: Language,
) -> Result<Vec<TypeDefinition>> {
    let tree = parse_code(source, language)?;
    let shape =
        crate::analysis::shape::extract_enhanced_shape(&tree, source, language, None, false)?;
    Ok(shape
        .classes
        .into_iter()
        .map(|class| TypeDefinition {
            signature: format!(
                "{} {}",
                if language == Language::Lua {
                    "table"
                } else {
                    "class"
                },
                class.name
            ),
            name: class.name,
            kind: if language == Language::Lua {
                TypeKind::Record
            } else {
                TypeKind::Class
            },
            file: path.into(),
            line: class.line,
            usage_count: 0,
            fields: (!class.fields.is_empty()).then(|| {
                class
                    .fields
                    .into_iter()
                    .map(|field| Field {
                        name: field.name,
                        type_annotation: field.property_type.unwrap_or_default(),
                    })
                    .collect()
            }),
            variants: None,
            members: (!class.methods.is_empty()).then(|| {
                class
                    .methods
                    .into_iter()
                    .map(|method| Member {
                        name: method.name,
                        type_annotation: method.signature,
                    })
                    .collect()
            }),
        })
        .collect())
}

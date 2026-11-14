//! Structural diff uses the same scripting declarations as view_code and code_map.

use super::*;

pub(super) fn extract(
    tree: &tree_sitter::Tree,
    source: &str,
    language: Language,
    symbols: &mut HashMap<String, ExtractedSymbol>,
) -> Result<(), io::Error> {
    let shape = crate::analysis::shape::extract_enhanced_shape(tree, source, language, None, true)?;
    for function in shape.functions {
        insert(function, "", symbols);
    }
    for class in shape.classes {
        for method in class.methods {
            insert(method, &class.name, symbols);
        }
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for field in class.fields {
            field.name.hash(&mut hasher);
            field.property_type.hash(&mut hasher);
        }
        symbols.insert(
            format!("class:{}", class.name),
            ExtractedSymbol {
                symbol_type: SymbolType::Class,
                name: class.name.clone(),
                line: class.line,
                signature: Some(class.name),
                body_hash: hasher.finish(),
            },
        );
    }
    Ok(())
}

fn insert(
    function: crate::analysis::shape::EnhancedFunctionInfo,
    owner: &str,
    symbols: &mut HashMap<String, ExtractedSymbol>,
) {
    let name = if owner.is_empty() {
        function.name
    } else {
        format!("{owner}::{}", function.name)
    };
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    function.code.hash(&mut hasher);
    symbols.insert(
        format!("fn:{name}"),
        ExtractedSymbol {
            symbol_type: SymbolType::Function,
            name,
            line: function.line,
            signature: Some(function.signature),
            body_hash: hasher.finish(),
        },
    );
}

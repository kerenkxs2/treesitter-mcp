use super::*;

pub(super) fn extract(
    tree: &tree_sitter::Tree,
    source: &str,
    language: Language,
    symbols: &mut HashMap<String, ExtractedSymbol>,
) -> Result<(), io::Error> {
    let shape = crate::analysis::shape::extract_enhanced_shape(tree, source, language, None, true)?;
    for function in shape.functions {
        insert_function(function, "", symbols);
    }
    for class in shape.classes {
        for method in class.methods {
            insert_function(method, &class.name, symbols);
        }
    }
    let types = crate::extraction::types::extract_c_family_types(source, Path::new(""), language)
        .map_err(|error| io::Error::other(error.to_string()))?;
    for ty in types {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        serde_json::to_string(&(&ty.fields, &ty.variants, &ty.members))
            .map_err(|error| io::Error::other(error.to_string()))?
            .hash(&mut hasher);
        let symbol_type = match ty.kind {
            crate::extraction::types::TypeKind::Class => SymbolType::Class,
            crate::extraction::types::TypeKind::Enum => SymbolType::Enum,
            _ => SymbolType::Struct,
        };
        symbols.insert(
            format!("type:{}", ty.name),
            ExtractedSymbol {
                symbol_type,
                name: ty.name,
                line: ty.line,
                signature: Some(ty.signature),
                body_hash: hasher.finish(),
            },
        );
    }
    Ok(())
}

fn insert_function(
    function: crate::analysis::shape::EnhancedFunctionInfo,
    scope: &str,
    symbols: &mut HashMap<String, ExtractedSymbol>,
) {
    let name = crate::analysis::shape::c_family::qualified_name(scope, &function.name);
    let code = function.code.unwrap_or_default();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    code.hash(&mut hasher);
    let mut ordinal = 0;
    while symbols.contains_key(&format!("fn:{name}#{ordinal}")) {
        ordinal += 1;
    }
    symbols.insert(
        format!("fn:{name}#{ordinal}"),
        ExtractedSymbol {
            symbol_type: SymbolType::Function,
            name,
            line: function.line,
            signature: Some(function.signature),
            body_hash: hasher.finish(),
        },
    );
}

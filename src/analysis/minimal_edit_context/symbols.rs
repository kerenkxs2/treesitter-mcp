use super::*;

pub(super) fn find_target_symbol(shape: &EnhancedFileShape, symbol: &str) -> Option<TargetSymbol> {
    for function in &shape.functions {
        if function.name == symbol {
            return Some(target_from_function(function, ""));
        }
    }

    for class in &shape.classes {
        if class.name == symbol {
            return class.code.as_ref().map(|code| TargetSymbol {
                name: class.name.clone(),
                line: class.line,
                end_line: class.end_line,
                signature: class.name.clone(),
                code: code.clone(),
                scope: class.name.clone(),
            });
        }

        for method in &class.methods {
            if method.name == symbol {
                return Some(target_from_function(method, &class.name));
            }
        }
    }

    for block in &shape.impl_blocks {
        for method in &block.methods {
            if method.name == symbol || format!("{}::{}", block.type_name, method.name) == symbol {
                return Some(TargetSymbol {
                    name: method.name.clone(),
                    line: method.line,
                    end_line: method.end_line,
                    signature: method.signature.clone(),
                    code: method.code.clone().unwrap_or_default(),
                    scope: block.type_name.clone(),
                });
            }
        }
    }

    None
}

pub(super) fn target_from_function(
    function: &EnhancedFunctionInfo,
    parent_scope: &str,
) -> TargetSymbol {
    TargetSymbol {
        name: function.name.clone(),
        line: function.line,
        end_line: function.end_line,
        signature: function.signature.clone(),
        code: function.code.clone().unwrap_or_default(),
        scope: parent_scope.to_string(),
    }
}

pub(super) fn collect_same_file_signatures(
    shape: &EnhancedFileShape,
    source_file: Option<&Path>,
) -> Vec<SymbolSignature> {
    let mut signatures = Vec::new();

    for function in &shape.functions {
        signatures.push(SymbolSignature {
            kind: dependency_kind("fn", source_file),
            name: function.name.clone(),
            line: function.line,
            signature: function.signature.clone(),
        });
    }

    for class in &shape.classes {
        for method in &class.methods {
            signatures.push(SymbolSignature {
                kind: dependency_kind("method", source_file),
                name: method.name.clone(),
                line: method.line,
                signature: method.signature.clone(),
            });
        }
    }

    for block in &shape.impl_blocks {
        for method in &block.methods {
            signatures.push(SymbolSignature {
                kind: dependency_kind("method", source_file),
                name: method.name.clone(),
                line: method.line,
                signature: method.signature.clone(),
            });
        }
    }

    signatures
}

pub(super) fn dependency_kind(base: &'static str, source_file: Option<&Path>) -> &'static str {
    match (base, source_file.is_some()) {
        ("fn", true) => "dep_fn",
        ("method", true) => "dep_method",
        ("fn", false) => "fn",
        ("method", false) => "method",
        _ => base,
    }
}

pub(super) fn collect_project_dependency_signatures(
    language: Language,
    source: &str,
    file_path: &Path,
    called_names: &HashSet<String>,
    seen_names: &mut HashSet<String>,
) -> Vec<SymbolSignature> {
    if called_names.is_empty() {
        return Vec::new();
    }

    let project_root = path_utils::find_project_root(file_path)
        .or_else(|| file_path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    let dependency_files = resolve_dependencies(language, source, file_path, &project_root);
    let mut signatures = Vec::new();

    for dependency_file in dependency_files {
        let Ok(dependency_source) = fs::read_to_string(&dependency_file) else {
            continue;
        };
        let Ok(dependency_language) = detect_language(&dependency_file) else {
            continue;
        };
        let Ok(dependency_tree) = parse_code(&dependency_source, dependency_language) else {
            continue;
        };
        let Ok(dependency_shape) = extract_enhanced_shape(
            &dependency_tree,
            &dependency_source,
            dependency_language,
            dependency_file.to_str(),
            false,
        ) else {
            continue;
        };

        for signature in collect_same_file_signatures(&dependency_shape, Some(&dependency_file)) {
            if called_names.contains(&signature.name) && seen_names.insert(signature.name.clone()) {
                signatures.push(signature);
            }
        }
    }

    signatures
}

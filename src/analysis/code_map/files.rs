use super::*;

pub(super) fn collect_files_combined(
    dir: &Path,
    result: &mut ExtractionResult,
    options: &ExtractionOptions,
    pattern: Option<&str>,
) -> Result<(), io::Error> {
    for path in collect_project_files(dir)? {
        if detect_language(&path).is_ok() {
            if let Some(pat) = pattern {
                if !matches_pattern(&path, pat) {
                    continue;
                }
            }

            if let Ok(entry) = process_file_combined(&path, options, result) {
                result.files.push(entry);
            }
        }
    }

    Ok(())
}

pub(super) fn process_file_combined(
    path: &Path,
    options: &ExtractionOptions,
    result: &mut ExtractionResult,
) -> Result<FileSymbols, io::Error> {
    let source = fs::read_to_string(path).map_err(|e| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("Failed to read file {}: {e}", path.display()),
        )
    })?;

    let language = detect_language(path).map_err(|e| {
        io::Error::new(
            io::ErrorKind::Unsupported,
            format!("Cannot detect language for file {}: {e}", path.display()),
        )
    })?;

    let tree = crate::parser::parse_code(&source, language).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to parse {} code: {e}", language.name()),
        )
    })?;

    let include_code = options.detail_level == DetailLevel::Full;
    let enhanced_shape = crate::analysis::shape::extract_enhanced_shape(
        &tree,
        &source,
        language,
        Some(&path.to_string_lossy()),
        include_code,
    )?;

    // Extract types if requested
    if options.with_types {
        let rel_path = std::path::PathBuf::from(path);
        if let Ok(file_types) = extract_types_from_source(&source, &rel_path, language) {
            result.types.extend(file_types);
        }
    }

    let mut functions: Vec<_> = enhanced_shape
        .functions
        .iter()
        .map(|function| filter_function_by_detail(function, options.detail_level))
        .collect();
    for class in &enhanced_shape.classes {
        for method in &class.methods {
            let mut method = method.clone();
            method.name = format!("{}::{}", class.name, method.name);
            functions.push(filter_function_by_detail(&method, options.detail_level));
        }
    }

    let structs = if enhanced_shape.structs.is_empty() {
        Vec::new()
    } else {
        enhanced_shape
            .structs
            .iter()
            .map(|s| filter_struct_by_detail(s, options.detail_level))
            .collect()
    };

    let classes = if enhanced_shape.classes.is_empty() {
        Vec::new()
    } else {
        enhanced_shape
            .classes
            .iter()
            .map(|c| filter_class_by_detail(c, options.detail_level))
            .collect()
    };

    Ok(FileSymbols {
        path: path.to_string_lossy().to_string(),
        functions,
        structs,
        classes,
    })
}

/// Extract types from source code - wrapper around extraction/types functions
pub(super) fn extract_types_from_source(
    source: &str,
    path: &std::path::Path,
    language: crate::parser::Language,
) -> Result<Vec<TypeDefinition>, io::Error> {
    use crate::parser::Language;

    let types = match language {
        Language::GDScript | Language::Lua => {
            crate::extraction::types::extract_scripting_types(source, path, language)
                .map_err(|e| io::Error::other(e.to_string()))?
        }
        Language::Rust => crate::extraction::types::extract_rust_types(source, path)
            .map_err(|e| io::Error::other(e.to_string()))?,
        Language::TypeScript | Language::Tsx => {
            crate::extraction::types::extract_typescript_types(source, path, true)
                .map_err(|e| io::Error::other(e.to_string()))?
        }
        Language::JavaScript => {
            crate::extraction::types::extract_typescript_types(source, path, false)
                .map_err(|e| io::Error::other(e.to_string()))?
        }
        Language::Python => crate::extraction::types::extract_python_types(source, path)
            .map_err(|e| io::Error::other(e.to_string()))?,
        Language::C | Language::Cpp => {
            crate::extraction::types::extract_c_family_types(source, path, language)
                .map_err(|e| io::Error::other(e.to_string()))?
        }
        Language::Java | Language::Go | Language::CSharp => {
            // Type extraction for these languages uses different extractors
            Vec::new()
        }
        Language::Html | Language::Css | Language::Swift => {
            // These languages don't have type definitions
            Vec::new()
        }
    };

    Ok(types)
}

pub(super) fn filter_function_by_detail(
    func: &EnhancedFunctionInfo,
    detail_level: DetailLevel,
) -> Value {
    match detail_level {
        DetailLevel::Minimal => {
            json!({
                "name": func.name,
                "line": func.line,
            })
        }
        DetailLevel::Signatures => {
            json!({
                "name": func.name,
                "line": func.line,
                "signature": func.signature,
            })
        }
        DetailLevel::Full => {
            json!({
                "name": func.name,
                "line": func.line,
                "signature": func.signature,
                "doc": func.doc,
                "code": func.code,
            })
        }
    }
}

pub(super) fn filter_struct_by_detail(s: &EnhancedStructInfo, detail_level: DetailLevel) -> Value {
    match detail_level {
        DetailLevel::Minimal => {
            json!({
                "name": s.name,
                "line": s.line,
            })
        }
        DetailLevel::Signatures => {
            json!({
                "name": s.name,
                "line": s.line,
            })
        }
        DetailLevel::Full => {
            json!({
                "name": s.name,
                "line": s.line,
                "doc": s.doc,
                "code": s.code,
            })
        }
    }
}

pub(super) fn filter_class_by_detail(cls: &EnhancedClassInfo, detail_level: DetailLevel) -> Value {
    match detail_level {
        DetailLevel::Minimal => {
            json!({
                "name": cls.name,
                "line": cls.line,
            })
        }
        DetailLevel::Signatures => {
            json!({
                "name": cls.name,
                "line": cls.line,
            })
        }
        DetailLevel::Full => {
            json!({
                "name": cls.name,
                "line": cls.line,
                "doc": cls.doc,
                "code": cls.code,
            })
        }
    }
}

pub(super) fn matches_pattern(path: &Path, pattern: &str) -> bool {
    match Glob::new(pattern) {
        Ok(glob) => {
            let matcher = glob.compile_matcher();
            if pattern.contains('/') || pattern.contains("**") {
                matcher.is_match(path)
            } else if let Some(file_name) = path.file_name() {
                matcher.is_match(file_name)
            } else {
                false
            }
        }
        Err(e) => {
            log::warn!("Invalid glob pattern '{}': {}", pattern, e);
            false
        }
    }
}

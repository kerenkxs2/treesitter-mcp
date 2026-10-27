use super::*;

pub(super) fn extract_referenced_type_names(
    language: crate::parser::Language,
    source: &str,
    main_shape: &EnhancedFileShape,
    file_path: &Path,
    focus_symbol: Option<&str>,
) -> HashSet<String> {
    match language {
        crate::parser::Language::Rust
        | crate::parser::Language::TypeScript
        | crate::parser::Language::Python
        | crate::parser::Language::Go
        | crate::parser::Language::C
        | crate::parser::Language::Cpp => {
            extract_ast_position_type_names(language, source, main_shape, file_path, focus_symbol)
        }
        _ => extract_referenced_type_names_fallback(source, main_shape),
    }
}

pub(super) fn referenced_type_from_definition_location(
    location: &DefinitionLocation,
) -> Result<HashSet<String>, io::Error> {
    let rows = extract_dependency_types(
        &location.file,
        &HashSet::new(),
        DetailLevel::Signatures,
        CommentMode::None,
    )?;
    let mut referenced = HashSet::new();

    if let Some(row) = rows.iter().find(|row| row.line == location.line) {
        log::debug!(
            "Resolved definition location {}:{}:{} to type {}",
            location.file.display(),
            location.line,
            location.column,
            row.name
        );
        referenced.insert(row.name.clone());
    }

    Ok(referenced)
}

pub(super) fn extract_ast_position_type_names(
    language: crate::parser::Language,
    source: &str,
    main_shape: &EnhancedFileShape,
    file_path: &Path,
    focus_symbol: Option<&str>,
) -> HashSet<String> {
    let mut names = HashSet::new();
    collect_shape_type_references(main_shape, focus_symbol, &mut names);

    let local_types = extract_local_type_definitions(language, source, file_path);
    if let Some(type_defs) = local_types.as_deref() {
        collect_type_definition_references(type_defs, focus_symbol, &mut names);
        remove_local_type_names(main_shape, Some(type_defs), &mut names);
    } else {
        remove_local_type_names(main_shape, None, &mut names);
    }

    names
}

pub(super) fn extract_local_type_definitions(
    language: crate::parser::Language,
    source: &str,
    file_path: &Path,
) -> Option<Vec<TypeDefinition>> {
    match language {
        crate::parser::Language::Rust => extract_rust_types(source, file_path).ok(),
        crate::parser::Language::TypeScript => {
            extract_typescript_types(source, file_path, true).ok()
        }
        crate::parser::Language::Python => extract_python_types(source, file_path).ok(),
        crate::parser::Language::Go => extract_go_types(source, file_path).ok(),
        crate::parser::Language::C | crate::parser::Language::Cpp => {
            crate::extraction::types::extract_c_family_types(source, file_path, language).ok()
        }
        _ => None,
    }
}

pub(super) fn collect_shape_type_references(
    shape: &EnhancedFileShape,
    focus_symbol: Option<&str>,
    out: &mut HashSet<String>,
) {
    if focus_symbol.is_none() {
        for import in &shape.imports {
            collect_type_like_tokens(&import.text, out);
        }
    }

    for func in &shape.functions {
        if matches_focus(&func.name, focus_symbol) {
            collect_type_like_tokens(&func.signature, out);
        }
    }

    for class in &shape.classes {
        let class_selected = matches_focus(&class.name, focus_symbol);

        if class_selected {
            for implemented in &class.implements {
                collect_type_like_tokens(implemented, out);
            }

            for property in class.properties.iter().chain(class.fields.iter()) {
                if let Some(property_type) = property.property_type.as_deref() {
                    collect_type_like_tokens(property_type, out);
                }
            }
        }

        for method in &class.methods {
            if focus_symbol.is_none() || class_selected || matches_focus(&method.name, focus_symbol)
            {
                collect_type_like_tokens(&method.signature, out);
            }
        }
    }

    for interface in &shape.interfaces {
        let interface_selected = matches_focus(&interface.name, focus_symbol);

        if interface_selected {
            for property in &interface.properties {
                if let Some(property_type) = property.property_type.as_deref() {
                    collect_type_like_tokens(property_type, out);
                }
            }
        }

        for method in &interface.methods {
            if focus_symbol.is_none()
                || interface_selected
                || matches_focus(&method.name, focus_symbol)
            {
                collect_type_like_tokens(&method.signature, out);
            }
        }
    }

    for tr in &shape.traits {
        let trait_selected = matches_focus(&tr.name, focus_symbol);
        for method in &tr.methods {
            if focus_symbol.is_none() || trait_selected || matches_focus(&method.name, focus_symbol)
            {
                collect_type_like_tokens(&method.signature, out);
            }
        }
    }

    for block in &shape.impl_blocks {
        let impl_selected = matches_focus(&block.type_name, focus_symbol)
            || block
                .trait_name
                .as_deref()
                .is_some_and(|trait_name| matches_focus(trait_name, focus_symbol));

        if impl_selected {
            collect_type_like_tokens(&block.type_name, out);
            if let Some(trait_name) = block.trait_name.as_deref() {
                collect_type_like_tokens(trait_name, out);
            }
        }

        for method in &block.methods {
            if focus_symbol.is_none() || impl_selected || matches_focus(&method.name, focus_symbol)
            {
                collect_type_like_tokens(&method.signature, out);
            }
        }
    }
}

pub(super) fn collect_type_definition_references(
    type_defs: &[TypeDefinition],
    focus_symbol: Option<&str>,
    out: &mut HashSet<String>,
) {
    for ty in type_defs {
        if !matches_focus(&ty.name, focus_symbol) {
            continue;
        }

        collect_type_like_tokens(&ty.signature, out);

        if let Some(fields) = ty.fields.as_deref() {
            for field in fields {
                collect_type_like_tokens(&field.type_annotation, out);
            }
        }

        if let Some(members) = ty.members.as_deref() {
            for member in members {
                collect_type_like_tokens(&member.type_annotation, out);
            }
        }

        if let Some(variants) = ty.variants.as_deref() {
            for variant in variants {
                if let Some(type_annotation) = variant.type_annotation.as_deref() {
                    collect_type_like_tokens(type_annotation, out);
                }
            }
        }
    }
}

pub(super) fn remove_local_type_names(
    shape: &EnhancedFileShape,
    local_types: Option<&[TypeDefinition]>,
    out: &mut HashSet<String>,
) {
    for s in &shape.structs {
        out.remove(&s.name);
    }
    for c in &shape.classes {
        out.remove(&c.name);
    }
    for i in &shape.interfaces {
        out.remove(&i.name);
    }
    for tr in &shape.traits {
        out.remove(&tr.name);
    }

    if let Some(type_defs) = local_types {
        for ty in type_defs {
            out.remove(&ty.name);
        }
    }
}

pub(super) fn matches_focus(name: &str, focus_symbol: Option<&str>) -> bool {
    focus_symbol.is_none_or(|focus| focus == name)
}

pub(super) fn extract_referenced_type_names_fallback(
    source: &str,
    main_shape: &EnhancedFileShape,
) -> HashSet<String> {
    let mut names = HashSet::new();

    for func in &main_shape.functions {
        collect_type_like_tokens(&func.signature, &mut names);
    }

    collect_type_like_tokens(source, &mut names);
    remove_local_type_names(main_shape, None, &mut names);

    names
}

pub(super) fn collect_type_like_tokens(text: &str, out: &mut HashSet<String>) {
    let mut current = String::new();

    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            current.push(ch);
            continue;
        }

        flush_type_token(&mut current, out);
    }

    flush_type_token(&mut current, out);
}

pub(super) fn flush_type_token(token: &mut String, out: &mut HashSet<String>) {
    if token.is_empty() {
        return;
    }

    if token
        .chars()
        .next()
        .map(|c| c.is_ascii_uppercase())
        .unwrap_or(false)
    {
        // Heuristic: ignore single-letter generics like `T`.
        if token.len() > 1 {
            out.insert(token.clone());
        }
    }

    token.clear();
}

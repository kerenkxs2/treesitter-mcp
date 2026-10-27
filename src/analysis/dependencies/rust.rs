use super::*;

/// For Rust files, find file dependencies that live in this project.
///
/// Supports:
/// - `mod foo;` declarations (resolves to `foo.rs` / `foo/mod.rs`)
/// - common `use crate::foo::...` imports (heuristic: resolves `foo.rs` / `foo/mod.rs`)
pub fn find_rust_dependencies(source: &str, file_path: &Path, project_root: &Path) -> Vec<PathBuf> {
    let mut deps = Vec::new();
    let mut seen = HashSet::new();

    let dir = file_path.parent().unwrap_or(project_root);

    let mut push_dep = |path: PathBuf| {
        if path.is_file() && path.starts_with(project_root) && seen.insert(path.clone()) {
            deps.push(path);
        }
    };

    // Parse the source with tree-sitter
    let language = tree_sitter_rust::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&language).is_err() {
        log::warn!("Failed to set Rust language for parser");
        return deps;
    }

    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => {
            log::warn!("Failed to parse Rust source for module dependencies");
            return deps;
        }
    };

    // Query for mod declarations (excluding inline modules with bodies)
    // We want: `mod foo;` or `pub mod foo;`
    // We don't want: `mod foo { ... }`
    let query_str = r#"
        (mod_item
            name: (identifier) @mod_name
            !body
        )
    "#;

    let query = match Query::new(&language, query_str) {
        Ok(q) => q,
        Err(e) => {
            log::warn!("Failed to create Rust mod query: {e}");
            return deps;
        }
    };

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());

    while let Some(match_) = matches.next() {
        for capture in match_.captures {
            if let Ok(mod_name) = capture.node.utf8_text(source.as_bytes()) {
                // Try foo.rs
                let candidate = dir.join(format!("{mod_name}.rs"));
                if candidate.is_file() && candidate.starts_with(project_root) {
                    push_dep(candidate);
                    continue;
                }

                // Try foo/mod.rs
                let candidate = dir.join(mod_name).join("mod.rs");
                push_dep(candidate);
            }
        }
    }

    // Also include `use crate::...` imports (common in real Rust code).
    // This is a heuristic (not a full module resolver), but it covers the common
    // project pattern where `crate::foo` maps to `src/foo.rs` or `src/foo/mod.rs`.
    let crate_src_root = rust_crate_src_root(file_path, project_root);

    for line in source.lines() {
        let line = line.trim_start();
        if !line.starts_with("use ") {
            continue;
        }

        let rest = line.trim_start_matches("use ").trim_start();
        let Some(rest) = rest.strip_prefix("crate::") else {
            continue;
        };

        for module in rust_use_crate_modules(rest) {
            if module.is_empty() {
                continue;
            }

            push_dep(crate_src_root.join(format!("{module}.rs")));
            push_dep(crate_src_root.join(module).join("mod.rs"));
        }
    }

    deps
}

pub(super) fn rust_use_crate_modules(rest: &str) -> Vec<&str> {
    // Handles:
    // - `foo::bar::Baz;`
    // - `foo::{A, B};`
    // - `{foo::A, bar::B};`
    let rest = rest.trim();

    if let Some(rest) = rest.strip_prefix('{') {
        let inner = rest.split('}').next().unwrap_or("");
        return inner
            .split(',')
            .filter_map(|part| first_rust_path_segment(part.trim()))
            .collect();
    }

    first_rust_path_segment(rest).into_iter().collect()
}

pub(super) fn first_rust_path_segment(rest: &str) -> Option<&str> {
    let rest = rest.trim_start();
    let mut end = 0;

    for (idx, ch) in rest.char_indices() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            end = idx + ch.len_utf8();
        } else {
            break;
        }
    }

    if end == 0 {
        None
    } else {
        Some(&rest[..end])
    }
}

pub(super) fn rust_crate_src_root(file_path: &Path, project_root: &Path) -> PathBuf {
    // Prefer the closest `<something>/src/...` directory so `crate::foo` resolves
    // to `<that>/src/foo.rs` rather than the repo workspace root.
    let mut dir = file_path.parent();

    while let Some(current) = dir {
        if current.file_name().and_then(|n| n.to_str()) == Some("src") {
            return current.to_path_buf();
        }

        if current == project_root {
            break;
        }

        dir = current.parent();
    }

    let candidate = project_root.join("src");
    if candidate.is_dir() {
        candidate
    } else {
        project_root.to_path_buf()
    }
}

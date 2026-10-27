use super::*;

/// Resolve literal quoted includes relative to the including file.
/// System headers, macro includes and compiler-specific include paths are not guessed.
pub(super) fn find_c_family_dependencies(
    language: Language,
    source: &str,
    file_path: &Path,
    project_root: &Path,
) -> Vec<PathBuf> {
    let tree = crate::parser::parse_code(source, language)
        .expect("the compiled-in C/C++ grammar must parse source");
    let mut dependencies = Vec::new();
    let mut seen = HashSet::new();
    let directory = file_path.parent().unwrap_or(project_root);
    collect(
        tree.root_node(),
        source,
        directory,
        project_root,
        &mut seen,
        &mut dependencies,
    );
    dependencies
}

fn collect(
    node: tree_sitter::Node<'_>,
    source: &str,
    directory: &Path,
    root: &Path,
    seen: &mut HashSet<PathBuf>,
    dependencies: &mut Vec<PathBuf>,
) {
    if node.kind() == "preproc_include" {
        if let Some(path) = node.child_by_field_name("path") {
            if path.kind() == "string_literal" {
                let text = crate::analysis::shape::c_family::text(path, source);
                if let Some(relative) = text
                    .strip_prefix('"')
                    .and_then(|text| text.strip_suffix('"'))
                {
                    let candidate = directory.join(relative);
                    if candidate.is_file() {
                        let candidate = candidate
                            .canonicalize()
                            .expect("existing include must resolve");
                        let root = root.canonicalize().expect("project root must resolve");
                        if candidate.starts_with(root) && seen.insert(candidate.clone()) {
                            dependencies.push(candidate);
                        }
                    }
                }
            }
        }
    }
    for child in node.named_children(&mut node.walk()) {
        collect(child, source, directory, root, seen, dependencies);
    }
}

//! Resolve only project-local literal GDScript resource loads and Lua require modules.

use crate::parser::{parse_code, scripting::import_path, Language};
use std::path::{Path, PathBuf};

pub(super) fn resolve(language: Language, source: &str, file: &Path, root: &Path) -> Vec<PathBuf> {
    // This existing resolver API is infallible. A compiled grammar failure is an
    // infrastructure defect; malformed source still yields a recoverable AST.
    let tree = parse_code(source, language)
        .expect("Compiled scripting dependency grammar must remain loadable");
    let resource_root = file
        .ancestors()
        .find(|path| path.join("project.godot").is_file())
        .unwrap_or(root);
    let mut paths = vec![];
    visit(
        tree.root_node(),
        source,
        language,
        file,
        root,
        resource_root,
        &mut paths,
    );
    paths
}

fn visit(
    node: tree_sitter::Node<'_>,
    source: &str,
    language: Language,
    file: &Path,
    root: &Path,
    resource_root: &Path,
    paths: &mut Vec<PathBuf>,
) {
    if let Some(spec) = import_path(node, source) {
        let candidates = if language == Language::GDScript {
            if let Some(resource) = spec.strip_prefix("res://") {
                vec![resource_root.join(resource)]
            } else {
                vec![file.parent().unwrap_or(root).join(spec)]
            }
        } else {
            // Lua's two standard project-local module layouts are root/name.lua and root/name/init.lua.
            let module = root.join(spec.replace('.', "/"));
            vec![module.with_extension("lua"), module.join("init.lua")]
        };
        if let Some(path) = candidates
            .into_iter()
            .find(|path| path.is_file())
            .and_then(|path| path.canonicalize().ok())
        {
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    for child in node.named_children(&mut node.walk()) {
        visit(child, source, language, file, root, resource_root, paths);
    }
}

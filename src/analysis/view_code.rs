//! View Code Tool
//!
//! Token-efficient, single-file view with optional dependency type context.
//!
//! Breaking schema change (v1):
//! - `p`: relative path of the main file
//! - `h`: header string (pipe-delimited column names)
//! - `f`/`s`/`c`: newline-delimited row strings for functions/structs/classes
//! - `deps`: map of dependency file path -> newline-delimited type rows
//! - Optional meta is under `@` (e.g. `{ "t": true }` for truncated)

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::json;
use serde_json::Map;
use serde_json::Value;
use tiktoken_rs::cl100k_base;

use crate::analysis::dependencies::resolve_dependencies;
use crate::analysis::path_utils;
use crate::analysis::shape::{
    extract_enhanced_shape, prepend_leading_comments_to_code, CommentMode, EnhancedClassInfo,
    EnhancedFileShape, EnhancedFunctionInfo, EnhancedStructInfo, ImplBlockInfo, ImportInfo,
    InterfaceInfo, MethodInfo, PropertyInfo, TraitInfo,
};
use crate::common::budget;
use crate::common::budget::BudgetTracker;
use crate::common::format;
use crate::extraction::types::{
    extract_go_types, extract_python_types, extract_rust_types, extract_typescript_types,
    TypeDefinition,
};
use crate::mcp_types::{CallToolResult, CallToolResultExt};
use crate::parser::{detect_language, parse_code};

#[derive(Debug, Clone, Copy, PartialEq)]
enum DetailLevel {
    Signatures,
    Full,
}

#[derive(Debug, Clone)]
struct DefinitionLocation {
    file: PathBuf,
    line: usize,
    column: usize,
}

impl DetailLevel {
    fn from_args(arguments: &Value) -> Self {
        // Back-compat: some tests call this tool with include_code.
        if let Some(detail) = arguments.get("detail").and_then(Value::as_str) {
            return match detail {
                "full" => DetailLevel::Full,
                _ => DetailLevel::Signatures,
            };
        }

        if let Some(include_code) = arguments.get("include_code").and_then(Value::as_bool) {
            return if include_code {
                DetailLevel::Full
            } else {
                DetailLevel::Signatures
            };
        }

        DetailLevel::Full
    }

    fn header(self) -> &'static str {
        match self {
            DetailLevel::Signatures => "name|line|sig",
            DetailLevel::Full => "name|line|sig|doc|code",
        }
    }

    fn include_code(self) -> bool {
        matches!(self, DetailLevel::Full)
    }

    fn trait_header(self) -> &'static str {
        match self {
            DetailLevel::Signatures => "trait|name|line|sig",
            DetailLevel::Full => "trait|name|line|sig|doc|code",
        }
    }

    fn class_method_header(self) -> &'static str {
        match self {
            DetailLevel::Signatures => "class|name|line|sig|ann",
            DetailLevel::Full => "class|name|line|sig|ann|doc|code",
        }
    }

    fn impl_method_header(self) -> &'static str {
        match self {
            DetailLevel::Signatures => "impl|trait|name|line|sig",
            DetailLevel::Full => "impl|trait|name|line|sig|doc|code",
        }
    }
}

pub fn execute(arguments: &Value) -> Result<CallToolResult, io::Error> {
    let file_path = arguments["file_path"].as_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Missing or invalid 'file_path' argument",
        )
    })?;

    let detail = DetailLevel::from_args(arguments);
    let focus_symbol = arguments.get("focus_symbol").and_then(Value::as_str);
    let comment_mode = parse_comment_mode(arguments);

    // Back-compat: tests pass include_deps without tool schema.
    let include_deps = arguments
        .get("include_deps")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    let max_tokens = arguments
        .get("max_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(2000) as usize;

    let definition_location = arguments
        .get("definition_location")
        .filter(|value| !value.is_null())
        .map(parse_definition_location)
        .transpose()?;

    log::info!(
        "Viewing code: {file_path} (detail: {:?}, focus_symbol: {:?}, include_deps: {include_deps}, max_tokens: {max_tokens})",
        detail,
        focus_symbol
    );

    // Parse main file
    let source = fs::read_to_string(file_path).map_err(|e| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("Failed to read file {file_path}: {e}"),
        )
    })?;

    let language = detect_language(file_path).map_err(|e| {
        io::Error::new(
            io::ErrorKind::Unsupported,
            format!("Cannot detect language for file {file_path}: {e}"),
        )
    })?;

    let tree = parse_code(&source, language).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to parse {} code: {e}", language.name()),
        )
    })?;

    let mut main_shape = extract_enhanced_shape(
        &tree,
        &source,
        language,
        Some(file_path),
        detail.include_code(),
    )?;

    if let Some(symbol) = focus_symbol {
        apply_focus(&mut main_shape, symbol);
    }
    apply_comment_mode(&mut main_shape, &source, language, comment_mode);

    // Convert main file path to relative
    let main_path = path_utils::to_relative_path(file_path);

    let bpe = cl100k_base()
        .map_err(|e| io::Error::other(format!("Failed to initialize tiktoken tokenizer: {e}")))?;

    // Build output map (compact)
    let mut out = Map::new();
    out.insert("p".to_string(), json!(main_path));
    out.insert("h".to_string(), json!(detail.header()));

    insert_symbol_tables(&mut out, &main_shape, detail);
    insert_imports_and_traits(&mut out, &main_shape, detail);
    insert_interfaces_and_properties(&mut out, &main_shape, detail);
    insert_class_methods(&mut out, &main_shape, detail);
    insert_impl_methods(&mut out, &main_shape, detail);

    let mut truncated = false;

    if include_deps {
        let project_root = path_utils::find_project_root(Path::new(file_path))
            .or_else(|| Path::new(file_path).parent().map(|p| p.to_path_buf()))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "Could not determine project root or parent directory",
                )
            })?;

        let mut dep_paths =
            resolve_dependencies(language, &source, Path::new(file_path), &project_root);
        if let Some(location) = definition_location.as_ref() {
            push_unique_path(&mut dep_paths, location.file.clone());
        }
        let project_deps = filter_project_dependencies(dep_paths, &project_root);

        let referenced = if let Some(location) = definition_location.as_ref() {
            referenced_type_from_definition_location(location)?
        } else {
            extract_referenced_type_names(
                language,
                &source,
                &main_shape,
                Path::new(file_path),
                focus_symbol,
            )
        };

        let deps_obj = build_dependency_rows(
            &project_deps,
            Path::new(file_path),
            &referenced,
            detail,
            max_tokens,
            comment_mode,
        )?;

        if !deps_obj.is_empty() {
            out.insert("deps".to_string(), Value::Object(deps_obj));
        }
    }

    // Early estimate-based budget: only used to stop adding deps.
    // Hard enforcement below uses actual token count.
    loop {
        let json_text = serde_json::to_string(&Value::Object(out.clone())).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Failed to serialize view_code output to JSON: {e}"),
            )
        })?;

        if bpe.encode_with_special_tokens(&json_text).len() <= max_tokens {
            break;
        }

        if remove_last_dep_entry(&mut out) {
            truncated = true;
            continue;
        }

        if !shrink_symbol_tables(&mut out) {
            truncated = true;
            break;
        }

        truncated = true;
    }

    if truncated {
        out.insert("@".to_string(), json!({"t": true}));
    }

    let output_json = serde_json::to_string(&Value::Object(out)).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to serialize output to JSON: {e}"),
        )
    })?;

    Ok(CallToolResult::success(output_json))
}

#[derive(Debug, Clone)]
struct TypeRow {
    name: String,
    line: usize,
    signature: Option<String>,
    doc: Option<String>,
    code: Option<String>,
    referenced: bool,
}

fn type_rows_to_string(rows: &[&TypeRow], detail: DetailLevel) -> String {
    rows.iter()
        .map(|row| {
            let line = row.line.to_string();
            let sig = row
                .signature
                .clone()
                .unwrap_or_else(|| signature_snippet_from_code(row.code.as_deref()));

            if detail == DetailLevel::Full {
                let owned = [
                    row.name.clone(),
                    line,
                    sig,
                    row.doc.clone().unwrap_or_default(),
                    row.code.clone().unwrap_or_default(),
                ];
                let refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
                return format::format_row(&refs);
            }

            let owned = [row.name.clone(), line, sig];
            let refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
            format::format_row(&refs)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Filter dependencies to only include project files, not external libraries
fn filter_project_dependencies(dep_paths: Vec<PathBuf>, project_root: &Path) -> Vec<PathBuf> {
    dep_paths
        .into_iter()
        .filter(|path| {
            // Include if path is inside project_root
            path.starts_with(project_root)
                // Exclude external dependency directories
                && !path.to_string_lossy().contains("/target/")
                && !path.to_string_lossy().contains("/node_modules/")
                && !path.to_string_lossy().contains("/venv/")
                && !path.to_string_lossy().contains("/.venv/")
                && !path.to_string_lossy().contains("/site-packages/")
                && !path.to_string_lossy().contains("\\target\\")
                && !path.to_string_lossy().contains("\\node_modules\\")
                && !path.to_string_lossy().contains("\\venv\\")
                && !path.to_string_lossy().contains("\\.venv\\")
                && !path.to_string_lossy().contains("\\site-packages\\")
        })
        .collect()
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if paths.iter().any(|existing| existing == &path) {
        return;
    }

    paths.push(path);
}

/// Apply focus to show full code only for the specified symbol
fn apply_focus(shape: &mut EnhancedFileShape, focus_symbol: &str) {
    let mut found = false;

    for func in &mut shape.functions {
        if func.name == focus_symbol {
            found = true;
        } else {
            func.code = None;
        }
    }

    for struct_info in &mut shape.structs {
        if struct_info.name == focus_symbol {
            found = true;
        } else {
            struct_info.code = None;
        }
    }

    for class in &mut shape.classes {
        if class.name == focus_symbol {
            found = true;
        } else {
            class.code = None;
            for method in &mut class.methods {
                if method.name == focus_symbol
                    || format!("{}::{}", class.name, method.name) == focus_symbol
                {
                    found = true;
                } else {
                    method.code = None;
                }
            }
        }
    }

    for tr in &mut shape.traits {
        for method in &mut tr.methods {
            if method.name == focus_symbol {
                found = true;
            } else {
                method.code = None;
            }
        }
    }

    for block in &mut shape.impl_blocks {
        for method in &mut block.methods {
            if method.name == focus_symbol {
                found = true;
            } else {
                method.code = None;
            }
        }
    }

    if !found {
        log::warn!("Focus symbol '{}' not found in file", focus_symbol);
    }
}

mod comments;
use comments::*;

mod rows;
use rows::*;

mod type_references;
use type_references::*;

mod dependency_types;
use dependency_types::*;

mod output_budget;
use output_budget::*;

mod locations;
use locations::*;

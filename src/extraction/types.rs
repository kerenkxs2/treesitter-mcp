use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use eyre::{bail, Result, WrapErr};
use globset::{Glob, GlobSet, GlobSetBuilder};
use log::debug;
use serde::{Deserialize, Serialize};
use streaming_iterator::StreamingIterator;
use tree_sitter::{Node, Parser, Query, QueryCursor};

use crate::common::project_files::collect_project_files;

const HARD_TYPE_LIMIT: usize = 1000;

mod c_family;
pub(crate) use c_family::extract_c_family_types;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TypeExtractionResult {
    pub types: Vec<TypeDefinition>,
    pub total_types: usize,
    pub types_included: usize,
    pub limit_hit: Option<LimitHit>,
    pub truncated: bool,
}

impl TypeExtractionResult {
    fn new() -> Self {
        Self {
            types: Vec::new(),
            total_types: 0,
            types_included: 0,
            limit_hit: None,
            truncated: false,
        }
    }

    fn finalize(&mut self) {
        self.types_included = self.types.len();
        self.truncated = self.limit_hit.is_some();
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LimitHit {
    TypeLimit,
    TokenLimit,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TypeKind {
    Struct,
    Class,
    Enum,
    Trait,
    Interface,
    Protocol,
    TypeAlias,
    Record,
    TypedDict,
    NamedTuple,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    #[serde(rename = "type")]
    pub type_annotation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Variant {
    pub name: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_annotation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Member {
    pub name: String,
    #[serde(rename = "type")]
    pub type_annotation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TypeDefinition {
    pub name: String,
    pub kind: TypeKind,
    pub file: PathBuf,
    pub line: usize,
    pub signature: String,
    pub usage_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<Field>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variants: Option<Vec<Variant>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub members: Option<Vec<Member>>,
}

/// Compatibility wrapper for callers that do not need usage counting.
#[allow(dead_code)]
pub fn extract_types(
    path: impl AsRef<Path>,
    pattern: Option<&str>,
    max_types: usize,
) -> Result<TypeExtractionResult> {
    extract_types_with_options(path, pattern, max_types, false)
}

/// Extract types from a path (file or directory).
///
/// Set `count_usages` to true to also count usages during extraction (single-pass).
pub fn extract_types_with_options(
    path: impl AsRef<Path>,
    pattern: Option<&str>,
    max_types: usize,
    count_usages: bool,
) -> Result<TypeExtractionResult> {
    let path = path.as_ref();
    if !path.exists() {
        bail!("Path does not exist: {}", path.display());
    }

    let matcher = match pattern {
        Some(pat) => Some(build_globset(pat)?),
        None => None,
    };

    let root_dir = if path.is_file() {
        path.parent()
            .map(PathBuf::from)
            .unwrap_or_else(PathBuf::new)
    } else {
        path.to_path_buf()
    };

    let effective_limit = if max_types == 0 {
        HARD_TYPE_LIMIT
    } else {
        max_types.min(HARD_TYPE_LIMIT)
    };

    let mut result = TypeExtractionResult::new();

    if path.is_file() {
        process_single_file(path, &root_dir, &matcher, effective_limit, &mut result)?;
    } else {
        let files = collect_project_files(path)
            .map_err(|err| eyre::eyre!("Failed to walk {}: {err}", path.display()))?;
        for file_path in files {
            let rel_path = relative_path(&root_dir, &file_path);
            if let Some(matcher) = matcher.as_ref() {
                if !matcher.is_match(normalize_path(&rel_path)) {
                    continue;
                }
            }

            // Read file once for both word counting and type extraction
            let content = match fs::read_to_string(&file_path) {
                Ok(c) => c,
                Err(err) => {
                    debug!("Skipping file {}: {err}", file_path.display());
                    continue;
                }
            };

            // Extract types from supported languages
            if let Some(language) = detect_language(&file_path) {
                if let Err(err) = process_file_with_source(
                    &content,
                    &rel_path,
                    language,
                    effective_limit,
                    &mut result,
                ) {
                    debug!("Skipping file {}: {err}", file_path.display());
                }
            }

            if result.limit_hit.is_some() {
                break;
            }
        }
    }

    result.finalize();

    if count_usages {
        crate::analysis::usage_counter::count_all_usages(&mut result.types, &root_dir)?;
    }

    Ok(result)
}

fn process_single_file(
    path: &Path,
    root_dir: &Path,
    matcher: &Option<GlobSet>,
    limit: usize,
    result: &mut TypeExtractionResult,
) -> Result<()> {
    if is_hidden(path) {
        return Ok(());
    }

    let rel_path = relative_path(root_dir, path);
    if let Some(matcher) = matcher.as_ref() {
        if !matcher.is_match(normalize_path(&rel_path)) {
            return Ok(());
        }
    }

    // Read file once for both extraction and counting
    let source =
        fs::read_to_string(path).wrap_err_with(|| format!("Failed to read {}", path.display()))?;

    if let Some(language) = detect_language(path) {
        process_file_with_source(&source, &rel_path, language, limit, result)?;
    }
    Ok(())
}

fn process_file_with_source(
    source: &str,
    relative_path: &Path,
    language: SupportedLanguage,
    limit: usize,
    result: &mut TypeExtractionResult,
) -> Result<()> {
    let file_types = match language {
        SupportedLanguage::Rust => extract_rust_types(source, relative_path)?,
        SupportedLanguage::TypeScript => extract_typescript_types(source, relative_path, true)?,
        SupportedLanguage::JavaScript => extract_typescript_types(source, relative_path, false)?,
        SupportedLanguage::Python => extract_python_types(source, relative_path)?,
        SupportedLanguage::Java => extract_java_types(source, relative_path)?,
        SupportedLanguage::CSharp => extract_csharp_types(source, relative_path)?,
        SupportedLanguage::Go => extract_go_types(source, relative_path)?,
        SupportedLanguage::C => {
            extract_c_family_types(source, relative_path, crate::parser::Language::C)?
        }
        SupportedLanguage::Cpp => {
            extract_c_family_types(source, relative_path, crate::parser::Language::Cpp)?
        }
    };

    for ty in file_types {
        result.total_types += 1;
        if result.types.len() < limit {
            result.types.push(ty);
        } else {
            result.limit_hit = Some(LimitHit::TypeLimit);
            break;
        }
    }

    Ok(())
}

fn build_globset(pattern: &str) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    builder.add(Glob::new(pattern)?);
    builder.build().map_err(Into::into)
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|name| name.starts_with('.'))
        .unwrap_or(false)
}

fn relative_path(root: &Path, file: &Path) -> PathBuf {
    if root.as_os_str().is_empty() {
        return file.to_path_buf();
    }

    if let Ok(relative) = file.strip_prefix(root) {
        if relative.as_os_str().is_empty() {
            if let Some(name) = file.file_name() {
                return PathBuf::from(name);
            }
        } else {
            return relative.to_path_buf();
        }
    }

    file.to_path_buf()
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[derive(Debug, Clone, Copy)]
enum SupportedLanguage {
    Rust,
    TypeScript,
    JavaScript,
    Python,
    Java,
    CSharp,
    Go,
    C,
    Cpp,
}

fn detect_language(path: &Path) -> Option<SupportedLanguage> {
    if let Ok(language) = crate::parser::detect_language(path) {
        match language {
            crate::parser::Language::C => return Some(SupportedLanguage::C),
            crate::parser::Language::Cpp => return Some(SupportedLanguage::Cpp),
            _ => {}
        }
    }
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "rs" => Some(SupportedLanguage::Rust),
        "ts" | "tsx" => Some(SupportedLanguage::TypeScript),
        "js" | "jsx" | "mjs" | "cjs" => Some(SupportedLanguage::JavaScript),
        "py" => Some(SupportedLanguage::Python),
        "java" => Some(SupportedLanguage::Java),
        "cs" => Some(SupportedLanguage::CSharp),
        "go" => Some(SupportedLanguage::Go),
        _ => None,
    }
}

fn signature_for(node: Node, source: &[u8]) -> String {
    if let Ok(text) = node.utf8_text(source) {
        text.lines().next().unwrap_or("").trim().to_string()
    } else {
        String::new()
    }
}

fn clean_type_annotation(text: &str) -> String {
    text.trim()
        .trim_start_matches(':')
        .trim()
        .trim_end_matches(';')
        .trim()
        .to_string()
}

mod rust;

mod typescript;

mod python;

mod java;
use java::*;

mod go;

mod csharp;
use csharp::*;

pub(crate) use go::extract_go_types;
pub(crate) use python::extract_python_types;
pub(crate) use rust::extract_rust_types;
pub(crate) use typescript::extract_typescript_types;

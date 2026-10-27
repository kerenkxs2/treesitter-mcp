//! Code Map Tool
//!
//! Generates a compact, token-efficient overview of a codebase.
//!
//! Breaking schema change (v1):
//! - Top-level JSON object is keyed by *relative* file path.
//! - Each file maps to an object with abbreviated keys:
//!   - `h`: header string (pipe-delimited column names)
//!   - `f`: functions (newline-delimited rows)
//!   - `s`: structs (newline-delimited rows)
//!   - `c`: classes (newline-delimited rows)
//! - Optional meta is under `@` (e.g. `{ "t": true }` for truncated).
//! - When `with_types=true`, also includes `types` key with type definitions.

use std::cmp::Reverse;
use std::fs;
use std::io;
use std::path::Path;

use globset::Glob;
use serde_json::json;
use serde_json::Map;
use serde_json::Value;
use tiktoken_rs::cl100k_base;

use crate::analysis::path_utils;
use crate::analysis::shape::{EnhancedClassInfo, EnhancedFunctionInfo, EnhancedStructInfo};
use crate::common::budget;
use crate::common::budget::BudgetTracker;
use crate::common::format;
use crate::common::project_files::collect_project_files;
use crate::extraction::types::{TypeDefinition, TypeKind};
use crate::mcp_types::{CallToolResult, CallToolResultExt};
use crate::parser::detect_language;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DetailLevel {
    Minimal,
    Signatures,
    Full,
}

impl DetailLevel {
    fn from_str(s: &str) -> Self {
        match s {
            "minimal" => DetailLevel::Minimal,
            "signatures" => DetailLevel::Signatures,
            "full" => DetailLevel::Full,
            _ => DetailLevel::Signatures,
        }
    }

    fn header(self) -> &'static str {
        match self {
            DetailLevel::Minimal => "name|line",
            DetailLevel::Signatures => "name|line|sig",
            DetailLevel::Full => "name|line|sig|doc|code",
        }
    }
}

#[derive(Debug, Clone)]
struct FileSymbols {
    path: String,
    functions: Vec<Value>,
    structs: Vec<Value>,
    classes: Vec<Value>,
}

/// Options for combined code map and type extraction
#[derive(Debug, Clone, Copy)]
struct ExtractionOptions {
    detail_level: DetailLevel,
    with_types: bool,
}

/// Result of combined extraction
struct ExtractionResult {
    files: Vec<FileSymbols>,
    types: Vec<TypeDefinition>,
}

pub fn execute(arguments: &Value) -> Result<CallToolResult, io::Error> {
    let path_str = arguments["path"].as_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Missing or invalid 'path' argument",
        )
    })?;

    let max_tokens = arguments["max_tokens"].as_i64().unwrap_or(2000) as usize;
    let detail_str = arguments["detail"].as_str().unwrap_or("signatures");
    let detail_level = DetailLevel::from_str(detail_str);
    let pattern = arguments["pattern"].as_str();
    let with_types = arguments["with_types"].as_bool().unwrap_or(false);
    let count_usages = arguments["count_usages"].as_bool().unwrap_or(false);

    log::info!(
        "Generating compact code map for: {path_str} (max_tokens: {max_tokens}, detail: {detail_str}, with_types: {with_types})"
    );

    let path = Path::new(path_str);
    if !path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Path does not exist: {path_str}"),
        ));
    }

    let options = ExtractionOptions {
        detail_level,
        with_types,
    };

    let mut result = ExtractionResult {
        files: Vec::new(),
        types: Vec::new(),
    };

    if path.is_file() {
        if let Ok(entry) = process_file_combined(path, &options, &mut result) {
            result.files.push(entry);
        }
    } else if path.is_dir() {
        collect_files_combined(path, &mut result, &options, pattern)?;
        result
            .files
            .sort_by_key(|entry| Reverse(symbol_count(entry)));
    }

    // Apply usage counts to types if requested
    if with_types && count_usages {
        let usage_root = if path.is_file() {
            path.parent().unwrap_or(path)
        } else {
            path
        };
        crate::analysis::usage_counter::count_all_usages(&mut result.types, usage_root)
            .map_err(|e| io::Error::other(e.to_string()))?;

        result.types.sort_by(|a, b| {
            b.usage_count
                .cmp(&a.usage_count)
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.file.cmp(&b.file))
                .then_with(|| a.line.cmp(&b.line))
        });
    } else if with_types {
        // Sort by file then line when not counting usages
        result
            .types
            .sort_by(|a, b| a.file.cmp(&b.file).then_with(|| a.line.cmp(&b.line)));
    }

    // Convert all file paths to relative paths
    for entry in &mut result.files {
        entry.path = path_utils::to_relative_path(&entry.path);
    }

    let (result_map, _truncated) =
        build_compact_output_combined(&result, detail_level, max_tokens, with_types)?;

    let json_text = serde_json::to_string(&Value::Object(result_map)).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to serialize code map to JSON: {e}"),
        )
    })?;

    Ok(CallToolResult::success(json_text))
}

#[derive(Debug, Clone, Copy)]
enum SymbolKind {
    Function,
    Struct,
    Class,
}

mod output;
use output::*;

mod files;
use files::*;

//! Diff-Aware Analysis Tools
//!
//! Provides tools for understanding structural changes between file versions
//! and identifying potentially affected code across the codebase.

use crate::analysis::path_utils;
use crate::common::format;
use crate::mcp_types::{CallToolResult, CallToolResultExt};
use crate::parser::{detect_language, parse_code, Language};
use regex::Regex;
use serde::Serialize;
use serde_json::json;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::Path;
use std::process::Command;

mod c_family;

// ============================================================================
// Data Structures
// ============================================================================

/// Result of parsing a diff for structural changes
#[derive(Debug, Serialize, serde::Deserialize)]
pub struct DiffAnalysis {
    pub file_path: String,
    pub compare_to: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compare_to_sha: Option<String>, // Full commit SHA for reference
    pub structural_changes: Vec<StructuralChange>,
    /// True if only non-structural changes (comments, whitespace, formatting)
    pub no_structural_change: bool,
    /// Summary counts
    pub summary: DiffSummary,
}

#[derive(Debug, Serialize, serde::Deserialize)]
pub struct DiffSummary {
    pub added: usize,
    pub removed: usize,
    pub modified: usize,
}

/// A single structural change detected in the diff
#[derive(Debug, Serialize, serde::Deserialize)]
pub struct StructuralChange {
    pub change_type: ChangeType,
    pub symbol_type: SymbolType,
    pub name: String,
    pub line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<ChangeDetail>,
}

#[derive(Debug, Serialize, serde::Deserialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeType {
    Added,
    Removed,
    SignatureChanged,
    BodyChanged,
}

#[derive(Debug, Serialize, serde::Deserialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SymbolType {
    Function,
    Struct,
    Class,
    Enum,
    Interface,
    Import,
    Constant,
    Static, // Rust static items (renamed from Variable for clarity)
}

/// Detailed information about what changed in a symbol
#[derive(Debug, Serialize, serde::Deserialize)]
pub struct ChangeDetail {
    pub kind: String, // "parameter_type", "return_type", "parameter_added", etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
}

// ============================================================================
// Affected Usages (internal)
// ============================================================================

#[derive(Debug, Serialize)]
pub struct AffectedChange {
    pub symbol: String,
    pub change_type: ChangeType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_details: Option<String>,
    pub potentially_affected: Vec<AffectedUsage>,
}

#[derive(Debug, Serialize)]
pub struct AffectedUsage {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub usage_type: String,
    pub code: String,
    pub confidence: MatchConfidence,
    pub risk: RiskLevel,
    pub reason: String,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    High,
    Medium,
    Low,
}

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MatchConfidence {
    High,
    Medium,
    Low,
}

// ============================================================================
// Symbol Extraction
// ============================================================================

/// Extracted symbol information for comparison
#[derive(Debug, Clone)]
struct ExtractedSymbol {
    symbol_type: SymbolType,
    name: String,
    line: usize,
    signature: Option<String>,
    body_hash: u64, // Hash of body for detecting body-only changes
}

/// Extract all symbols from a parsed tree
fn extract_symbols(
    tree: &tree_sitter::Tree,
    source: &str,
    language: Language,
) -> Result<HashMap<String, ExtractedSymbol>, io::Error> {
    let mut symbols = HashMap::new();

    match language {
        Language::Rust => extract_rust_symbols(tree, source, &mut symbols)?,
        Language::Python => extract_python_symbols(tree, source, &mut symbols)?,
        Language::JavaScript => extract_js_symbols(tree, source, &mut symbols)?,
        Language::TypeScript => extract_ts_symbols(tree, source, &mut symbols)?,
        Language::Go => extract_go_symbols(tree, source, &mut symbols)?,
        Language::C | Language::Cpp => c_family::extract(tree, source, language, &mut symbols)?,
        Language::Html | Language::Css | Language::Swift | Language::CSharp | Language::Java => {
            // These languages don't have structural-diff extraction implemented yet.
            // Return empty - structural diff not applicable.
            log::debug!("Structural diff not applicable for {:?}", language);
        }
    }

    Ok(symbols)
}

// ============================================================================
// parse_diff Implementation
// ============================================================================

pub fn execute_parse_diff(arguments: &Value) -> Result<CallToolResult, io::Error> {
    let file_path_str = arguments["file_path"].as_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Missing or invalid 'file_path' argument",
        )
    })?;

    let compare_to = arguments["compare_to"]
        .as_str()
        .unwrap_or("HEAD")
        .to_string();

    let analysis = analyze_diff(file_path_str, compare_to)?;

    let header = "type|name|line|change";
    let changes = analysis
        .structural_changes
        .iter()
        .map(|c| {
            let symbol_type = abbreviate_symbol_type(&c.symbol_type);
            let line = c.line.to_string();
            let change = format_change(c);
            format::format_row(&[symbol_type, &c.name, &line, &change])
        })
        .collect::<Vec<_>>()
        .join("\n");

    let result = json!({
        "p": analysis.file_path,
        "cmp": analysis.compare_to,
        "h": header,
        "changes": changes,
    });

    let result_json = serde_json::to_string(&result).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to serialize result: {e}"),
        )
    })?;

    Ok(CallToolResult::success(result_json))
}

// ============================================================================
// affected_by_diff Implementation
// ============================================================================

pub fn execute_affected_by_diff(arguments: &Value) -> Result<CallToolResult, io::Error> {
    let file_path_str = arguments["file_path"].as_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Missing or invalid 'file_path' argument",
        )
    })?;

    let compare_to = arguments["compare_to"]
        .as_str()
        .unwrap_or("HEAD")
        .to_string();

    let scope = arguments["scope"].as_str();

    log::info!("Finding affected usages for: {file_path_str}");

    let file_path = Path::new(file_path_str);

    // Get the structural changes (internal representation)
    let diff_analysis = analyze_diff(file_path_str, compare_to)?;

    if diff_analysis.no_structural_change {
        let result = json!({
            "p": diff_analysis.file_path,
            "h": "symbol|change|file|line|risk",
            "affected": "",
        });

        let result_json = serde_json::to_string(&result).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Failed to serialize: {e}"),
            )
        })?;

        return Ok(CallToolResult::success(result_json));
    }

    // Determine search scope
    let search_path = resolve_search_path(scope, file_path);
    let affected_changes =
        collect_affected_changes_for(file_path, &search_path, &diff_analysis.structural_changes)?;
    let affected_rows = affected_rows(&affected_changes);

    let result = json!({
        "p": diff_analysis.file_path,
        "h": "symbol|change|file|line|risk",
        "affected": affected_rows,
    });

    let result_json = serde_json::to_string(&result).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to serialize: {e}"),
        )
    })?;

    Ok(CallToolResult::success(result_json))
}

pub fn execute_preview_impact(arguments: &Value) -> Result<CallToolResult, io::Error> {
    let file_path_str = arguments["file_path"].as_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Missing or invalid 'file_path' argument",
        )
    })?;
    let symbol_name = arguments["symbol_name"]
        .as_str()
        .or_else(|| arguments["symbol"].as_str())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Missing or invalid 'symbol_name' argument",
            )
        })?;
    let new_signature = arguments["new_signature"].as_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Missing or invalid 'new_signature' argument",
        )
    })?;
    let scope = arguments["scope"].as_str();

    let file_path = Path::new(file_path_str);
    let current_symbol = find_symbol_in_current_file(file_path, symbol_name)?;
    let before_signature = current_symbol.signature.clone().unwrap_or_default();
    let details = analyze_signature_changes(Some(&before_signature), Some(new_signature));
    let change = StructuralChange {
        change_type: ChangeType::SignatureChanged,
        symbol_type: current_symbol.symbol_type,
        name: current_symbol.name.clone(),
        line: current_symbol.line,
        before: Some(before_signature.clone()),
        after: Some(new_signature.to_string()),
        details,
    };

    let search_path = resolve_search_path(scope, file_path);
    let affected_changes = if before_signature == new_signature {
        Vec::new()
    } else {
        collect_affected_changes_for(file_path, &search_path, &[change])?
    };

    let details = analyze_signature_changes(Some(&before_signature), Some(new_signature));
    let result = json!({
        "p": path_utils::to_relative_path(file_path_str),
        "sym": current_symbol.name,
        "before": before_signature,
        "after": new_signature,
        "dh": "kind|name|from|to",
        "d": change_detail_rows(&details),
        "h": "symbol|change|file|line|risk",
        "affected": affected_rows(&affected_changes),
    });

    let result_json = serde_json::to_string(&result).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to serialize preview impact result: {e}"),
        )
    })?;

    Ok(CallToolResult::success(result_json))
}

pub(crate) fn analyze_diff(
    file_path_str: &str,
    compare_to: String,
) -> Result<DiffAnalysis, io::Error> {
    log::info!("Analyzing diff for: {file_path_str} against {compare_to}");

    let file_path = Path::new(file_path_str);

    if !file_path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("File does not exist: {file_path_str}"),
        ));
    }

    let language = detect_language(file_path).map_err(|e| {
        io::Error::new(
            io::ErrorKind::Unsupported,
            format!("Cannot detect language: {e}"),
        )
    })?;

    let current_content = std::fs::read_to_string(file_path)?;
    let old_content = get_git_file_content(file_path, &compare_to)?;

    let old_tree = parse_code(&old_content, language).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to parse old version: {e}"),
        )
    })?;

    let new_tree = parse_code(&current_content, language).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to parse current version: {e}"),
        )
    })?;

    let old_symbols = extract_symbols(&old_tree, &old_content, language)?;
    let new_symbols = extract_symbols(&new_tree, &current_content, language)?;

    let structural_changes =
        compare_symbols(&old_symbols, &new_symbols, &old_content, &current_content)?;

    let summary = DiffSummary {
        added: structural_changes
            .iter()
            .filter(|c| c.change_type == ChangeType::Added)
            .count(),
        removed: structural_changes
            .iter()
            .filter(|c| c.change_type == ChangeType::Removed)
            .count(),
        modified: structural_changes
            .iter()
            .filter(|c| {
                c.change_type == ChangeType::SignatureChanged
                    || c.change_type == ChangeType::BodyChanged
            })
            .count(),
    };

    let compare_to_sha = resolve_git_sha(&compare_to, file_path).ok();

    Ok(DiffAnalysis {
        file_path: path_utils::to_relative_path(file_path_str),
        compare_to,
        compare_to_sha,
        no_structural_change: structural_changes.is_empty(),
        structural_changes,
        summary,
    })
}

struct CurrentSymbol {
    symbol_type: SymbolType,
    name: String,
    line: usize,
    signature: Option<String>,
}

mod git;
use git::*;

mod symbols;
use symbols::*;

mod comparison;
use comparison::*;

mod formatting;
use formatting::*;

mod affected;
use affected::*;

pub(crate) use formatting::format_change;

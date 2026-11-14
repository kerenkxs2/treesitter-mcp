//! Find Usages Tool
//!
//! Searches for all usages of a symbol across files.
//!
//! Breaking schema change (v1):
//! ```json
//! {
//!   "sym": "parse",
//!   "h": "file|line|col|type|context|scope|conf|owner",
//!   "u": "src/main.rs|42|10|call|let x = parse(input)|main|low|\n..."
//! }
//! ```

use std::fs;
use std::io;
use std::path::Path;

use serde_json::json;
use serde_json::Value;
use tiktoken_rs::cl100k_base;
use tree_sitter::{Node, Tree};

use crate::analysis::path_utils;
use crate::common::budget;
use crate::common::budget::BudgetTracker;
use crate::common::compact::CompactOutput;
use crate::common::project_files::collect_project_files;
use crate::mcp_types::{CallToolResult, CallToolResultExt};
use crate::parser::{detect_language, parse_code, Language};

pub(crate) const USAGE_HEADER: &str = "file|line|col|type|context|scope|conf|owner";

#[derive(Debug, Clone)]
pub(crate) struct UsageRow {
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) usage_type: String,
    pub(crate) context: String,
    pub(crate) scope: String,
    pub(crate) confidence: String,
    pub(crate) owner_hint: Option<String>,
}

#[derive(Clone, Copy)]
struct SearchTarget<'a> {
    source: &'a str,
    symbol: &'a str,
    language: Language,
    path: &'a Path,
    context_lines: u32,
}

pub fn execute(arguments: &Value) -> Result<CallToolResult, io::Error> {
    let symbol = arguments["symbol"].as_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Missing or invalid 'symbol' argument",
        )
    })?;

    let path_str = arguments["path"].as_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Missing or invalid 'path' argument",
        )
    })?;

    let context_lines = arguments["context_lines"]
        .as_u64()
        .map(|v| v as u32)
        .unwrap_or(3);

    let max_context_lines = arguments["max_context_lines"].as_u64().map(|v| v as u32);
    let max_tokens = arguments["max_tokens"].as_u64().map(|v| v as usize);

    log::info!("Finding usages of '{symbol}' in: {path_str}");

    let path = Path::new(path_str);
    if !path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Path does not exist: {path_str}"),
        ));
    }

    let mut usages: Vec<UsageRow> = Vec::new();
    let mut context_budget = ContextBudget::new(max_context_lines);

    if path.is_file() {
        let _ = search_file(
            path,
            symbol,
            context_lines,
            &mut context_budget,
            &mut usages,
        )?;
    } else if path.is_dir() {
        let _ = search_directory(
            path,
            symbol,
            context_lines,
            &mut context_budget,
            &mut usages,
        )?;
    }

    assign_confidence(&mut usages);
    usages.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.column.cmp(&b.column))
            .then_with(|| a.usage_type.cmp(&b.usage_type))
            .then_with(|| a.scope.cmp(&b.scope))
    });

    // Convert all file paths to relative paths
    for usage in &mut usages {
        usage.file = path_utils::to_relative_path(&usage.file);
    }

    let (rows, truncated_by_budget) = build_rows_with_budget(
        &usages,
        symbol,
        USAGE_HEADER,
        max_tokens.unwrap_or(usize::MAX),
        max_tokens.is_some(),
    )?;

    let mut result = json!({
        "sym": symbol,
        "h": USAGE_HEADER,
        "u": rows,
    });

    if truncated_by_budget {
        result["@"] = json!({"t": true});
    }

    let json_text = serde_json::to_string(&result).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to serialize result to JSON: {e}"),
        )
    })?;

    Ok(CallToolResult::success(json_text))
}

pub(crate) fn build_rows_with_budget(
    usages: &[UsageRow],
    symbol: &str,
    header: &str,
    max_tokens: usize,
    enforce: bool,
) -> Result<(String, bool), io::Error> {
    if !enforce {
        return Ok((usages_to_rows(usages, header), false));
    }

    let bpe = cl100k_base()
        .map_err(|e| io::Error::other(format!("Failed to initialize tiktoken tokenizer: {e}")))?;

    // 10% buffer for conservative estimate.
    let mut tracker = BudgetTracker::new((max_tokens * 9) / 10);

    let mut kept: Vec<UsageRow> = Vec::new();
    for usage in usages {
        // Estimate without serialization (conservative).
        let line = usage.line.to_string();
        let column = usage.column.to_string();
        let total_chars = usage.file.len()
            + line.len()
            + column.len()
            + usage.usage_type.len()
            + usage.context.len()
            + usage.scope.len()
            + usage.confidence.len()
            + usage.owner_hint.as_deref().unwrap_or("").len()
            + 7;

        let estimated = budget::estimate_symbol_tokens(total_chars);
        if !tracker.add(estimated) {
            break;
        }
        kept.push(usage.clone());
    }

    let mut truncated = kept.len() < usages.len();

    // Hard enforcement by truncating rows from the end until we fit.
    loop {
        let candidate_rows = usages_to_rows(&kept, header);
        let mut candidate = json!({
            "sym": "_",
            "h": header,
            "u": candidate_rows,
        });
        candidate["sym"] = json!(symbol);
        if truncated {
            candidate["@"] = json!({"t": true});
        }

        let candidate_json = serde_json::to_string(&candidate).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Failed to serialize result to JSON: {e}"),
            )
        })?;

        if bpe.encode_with_special_tokens(&candidate_json).len() <= max_tokens {
            return Ok((candidate_rows, truncated));
        }

        if kept.pop().is_none() {
            return Ok((String::new(), true));
        }
        truncated = true;
    }
}

fn usages_to_rows(usages: &[UsageRow], header: &str) -> String {
    let mut output = CompactOutput::new(header);

    for usage in usages {
        let line = usage.line.to_string();
        let column = usage.column.to_string();

        output.add_row(&[
            &usage.file,
            &line,
            &column,
            &usage.usage_type,
            &usage.context,
            &usage.scope,
            &usage.confidence,
            usage.owner_hint.as_deref().unwrap_or(""),
        ]);
    }

    output.rows_string()
}

struct ContextBudget {
    max_total_lines: Option<u32>,
    used_lines: u32,
}

impl ContextBudget {
    fn new(max_total_lines: Option<u32>) -> Self {
        Self {
            max_total_lines,
            used_lines: 0,
        }
    }

    fn can_add_lines(&self, additional: u32) -> bool {
        match self.max_total_lines {
            None => true,
            Some(max) => self.used_lines + additional <= max,
        }
    }

    fn add_lines(&mut self, additional: u32) -> bool {
        if self.can_add_lines(additional) {
            self.used_lines += additional;
            true
        } else {
            false
        }
    }

    fn max_is_zero(&self) -> bool {
        matches!(self.max_total_lines, Some(0))
    }
}

fn search_directory(
    dir: &Path,
    symbol: &str,
    context_lines: u32,
    budget: &mut ContextBudget,
    usages: &mut Vec<UsageRow>,
) -> Result<bool, io::Error> {
    for path in collect_project_files(dir)? {
        if detect_language(&path).is_ok()
            && !search_file(&path, symbol, context_lines, budget, usages)?
        {
            return Ok(false);
        }
    }

    Ok(true)
}

fn search_file(
    path: &Path,
    symbol: &str,
    context_lines: u32,
    budget: &mut ContextBudget,
    usages: &mut Vec<UsageRow>,
) -> Result<bool, io::Error> {
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

    let tree = parse_code(&source, language).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to parse {} code: {e}", language.name()),
        )
    })?;

    let search = SearchTarget {
        source: &source,
        symbol,
        language,
        path,
        context_lines,
    };

    Ok(find_identifiers(&tree, search, budget, usages))
}

fn find_identifiers(
    tree: &Tree,
    search: SearchTarget<'_>,
    budget: &mut ContextBudget,
    usages: &mut Vec<UsageRow>,
) -> bool {
    let root = tree.root_node();
    let mut cursor = root.walk();
    visit_node(&mut cursor, search, budget, usages)
}

fn visit_node(
    cursor: &mut tree_sitter::TreeCursor,
    search: SearchTarget<'_>,
    budget: &mut ContextBudget,
    usages: &mut Vec<UsageRow>,
) -> bool {
    let node = cursor.node();

    if crate::parser::scripting::identifier(node.kind()) {
        if let Ok(text) = node.utf8_text(search.source.as_bytes()) {
            if text == search.symbol {
                let start_pos = node.start_position();
                let usage_type = if matches!(search.language, Language::GDScript | Language::Lua) {
                    crate::parser::scripting::usage_kind(node)
                        .unwrap_or("reference")
                        .to_string()
                } else {
                    classify_usage_type(&node)
                };

                let context = if budget.max_is_zero() {
                    String::new()
                } else {
                    extract_code_with_context(search.source, start_pos.row, search.context_lines)
                };

                let context_line_count = if budget.max_is_zero() {
                    0
                } else {
                    context.lines().count() as u32
                };

                if !budget.add_lines(context_line_count) {
                    return false;
                }

                usages.push(UsageRow {
                    file: search.path.to_string_lossy().to_string(),
                    line: start_pos.row + 1,
                    column: start_pos.column + 1,
                    usage_type,
                    context,
                    scope: scope_for_node(node, search.source, search.language),
                    confidence: "low".to_string(),
                    owner_hint: owner_hint(node, search.source),
                });
            }
        }
    }

    if cursor.goto_first_child() {
        loop {
            if !visit_node(cursor, search, budget, usages) {
                cursor.goto_parent();
                return false;
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }

    true
}

pub(crate) fn extract_code_with_context(source: &str, line: usize, context_lines: u32) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let context_lines = context_lines as usize;

    let start_line = line.saturating_sub(context_lines);
    let end_line = std::cmp::min(line + context_lines + 1, lines.len());

    lines[start_line..end_line].join("\n")
}

mod syntax;

mod confidence;
use confidence::*;
pub(crate) use syntax::{classify_usage_type, owner_hint, scope_for_node};

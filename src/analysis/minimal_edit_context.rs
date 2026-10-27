//! Minimal edit context for one symbol.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};
use tiktoken_rs::cl100k_base;
use tree_sitter::{Node, Tree};

use crate::analysis::dependencies::resolve_dependencies;
use crate::analysis::path_utils;
use crate::analysis::shape::{
    extract_enhanced_shape, prepend_leading_comments_to_code, CommentMode, EnhancedFileShape,
    EnhancedFunctionInfo, EnhancedStructInfo, ImportInfo, InterfaceInfo,
};
use crate::common::format;
use crate::mcp_types::{CallToolResult, CallToolResultExt};
use crate::parser::{detect_language, parse_code, Language};

const TARGET_HEADER: &str = "name|line|sig|code";
const DEP_HEADER: &str = "kind|name|line|sig";
const IMPORT_HEADER: &str = "line|text";
const TYPE_HEADER: &str = "kind|name|line|sig";

#[derive(Debug, Clone)]
struct TargetSymbol {
    name: String,
    line: usize,
    end_line: usize,
    signature: String,
    code: String,
    scope: String,
}

#[derive(Debug, Clone)]
struct SymbolSignature {
    kind: &'static str,
    name: String,
    line: usize,
    signature: String,
}

fn parse_comment_mode(arguments: &Value) -> CommentMode {
    if arguments
        .get("include_comments")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return CommentMode::Leading;
    }

    CommentMode::from_option(arguments.get("comment_mode").and_then(Value::as_str))
}

/// Return compact context needed to edit one symbol.
pub fn execute(arguments: &Value) -> Result<CallToolResult, io::Error> {
    let file_path = arguments["file_path"].as_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Missing or invalid 'file_path' argument",
        )
    })?;
    let symbol = arguments["symbol_name"]
        .as_str()
        .or_else(|| arguments["symbol"].as_str())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Missing or invalid 'symbol_name' argument",
            )
        })?;
    let comment_mode = parse_comment_mode(arguments);
    let max_tokens = arguments["max_tokens"]
        .as_u64()
        .map(|value| value as usize)
        .unwrap_or(2000);

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
    let shape = extract_enhanced_shape(&tree, &source, language, Some(file_path), true)?;
    let target = find_target_symbol(&shape, symbol).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("Symbol '{symbol}' not found in {file_path}"),
        )
    })?;

    let called_names = collect_called_names(&tree, &source, language, target.line, target.end_line);
    let same_file_symbols = collect_same_file_signatures(&shape, None);
    let mut deps: Vec<SymbolSignature> = same_file_symbols
        .iter()
        .filter(|dep| dep.name != target.name && called_names.contains(&dep.name))
        .cloned()
        .collect();
    let mut seen_dep_names: HashSet<String> = deps.iter().map(|dep| dep.name.clone()).collect();
    seen_dep_names.insert(target.name.clone());
    deps.extend(collect_project_dependency_signatures(
        language,
        &source,
        Path::new(file_path),
        &called_names,
        &mut seen_dep_names,
    ));

    let target_text = format!("{}\n{}", target.signature, target.code);
    let imports = relevant_imports(&shape.imports, &target_text);
    let types = relevant_types(&shape, &target_text);
    let target_code = prepend_leading_comments_to_code(
        &source,
        target.line,
        language,
        Some(target.code.clone()),
        comment_mode,
    )
    .unwrap_or_else(|| target.code.clone());

    let mut out = Map::new();
    out.insert(
        "p".to_string(),
        json!(path_utils::to_relative_path(file_path)),
    );
    out.insert("sym".to_string(), json!(symbol));
    out.insert("scope".to_string(), json!(target.scope));
    out.insert("h".to_string(), json!(TARGET_HEADER));
    out.insert(
        "target".to_string(),
        json!(target_to_row(&target, target_code.as_str())),
    );

    let dep_rows = signatures_to_rows(&deps);
    if !dep_rows.is_empty() {
        out.insert("dh".to_string(), json!(DEP_HEADER));
        out.insert("deps".to_string(), json!(dep_rows));
    }

    let type_rows = signatures_to_rows(&types);
    if !type_rows.is_empty() {
        out.insert("tyh".to_string(), json!(TYPE_HEADER));
        out.insert("types".to_string(), json!(type_rows));
    }

    let import_rows = imports_to_rows(&imports);
    if !import_rows.is_empty() {
        out.insert("ih".to_string(), json!(IMPORT_HEADER));
        out.insert("imports".to_string(), json!(import_rows));
    }

    enforce_budget(&mut out, max_tokens)?;

    let json_text = serde_json::to_string(&Value::Object(out)).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to serialize result to JSON: {e}"),
        )
    })?;

    Ok(CallToolResult::success(json_text))
}

fn relevant_imports(imports: &[ImportInfo], target_text: &str) -> Vec<ImportInfo> {
    imports
        .iter()
        .filter(|import| {
            import_tokens(&import.text)
                .iter()
                .any(|tok| token_in_text(tok, target_text))
        })
        .cloned()
        .collect()
}

fn import_tokens(import_text: &str) -> Vec<String> {
    import_text
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .filter(|token| token.len() > 1)
        .filter(|token| {
            !matches!(
                *token,
                "use" | "import" | "from" | "as" | "type" | "crate" | "self" | "super"
            )
        })
        .map(ToOwned::to_owned)
        .collect()
}

fn relevant_types(shape: &EnhancedFileShape, target_text: &str) -> Vec<SymbolSignature> {
    let mut types = Vec::new();

    for s in &shape.structs {
        push_type_if_relevant(
            &mut types,
            "struct",
            &s.name,
            s.line,
            signature_for_struct(s),
            target_text,
        );
    }
    for c in &shape.classes {
        push_type_if_relevant(
            &mut types,
            "class",
            &c.name,
            c.line,
            c.name.clone(),
            target_text,
        );
    }
    for i in &shape.interfaces {
        push_type_if_relevant(
            &mut types,
            "interface",
            &i.name,
            i.line,
            signature_for_interface(i),
            target_text,
        );
    }

    types
}

fn push_type_if_relevant(
    types: &mut Vec<SymbolSignature>,
    kind: &'static str,
    name: &str,
    line: usize,
    signature: String,
    target_text: &str,
) {
    if token_in_text(name, target_text) {
        types.push(SymbolSignature {
            kind,
            name: name.to_string(),
            line,
            signature,
        });
    }
}

fn signature_for_struct(info: &EnhancedStructInfo) -> String {
    info.code
        .as_deref()
        .and_then(|code| code.lines().next())
        .unwrap_or(&info.name)
        .to_string()
}

fn signature_for_interface(info: &InterfaceInfo) -> String {
    info.code
        .as_deref()
        .and_then(|code| code.lines().next())
        .unwrap_or(&info.name)
        .to_string()
}

fn token_in_text(token: &str, text: &str) -> bool {
    text.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .any(|part| part == token)
}

fn target_to_row(target: &TargetSymbol, code: &str) -> String {
    let line = target.line.to_string();
    format::format_row(&[&target.name, &line, &target.signature, code])
}

fn signatures_to_rows(signatures: &[SymbolSignature]) -> String {
    signatures
        .iter()
        .map(|sig| {
            let line = sig.line.to_string();
            format::format_row(&[sig.kind, &sig.name, &line, &sig.signature])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn imports_to_rows(imports: &[ImportInfo]) -> String {
    imports
        .iter()
        .map(|import| {
            let line = import.line.to_string();
            format::format_row(&[&line, &import.text])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn enforce_budget(out: &mut Map<String, Value>, max_tokens: usize) -> Result<(), io::Error> {
    let bpe = cl100k_base()
        .map_err(|e| io::Error::other(format!("Failed to initialize tiktoken tokenizer: {e}")))?;

    let mut truncated = false;

    loop {
        let json_text = serde_json::to_string(&Value::Object(out.clone())).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Failed to serialize result to JSON: {e}"),
            )
        })?;
        if bpe.encode_with_special_tokens(&json_text).len() <= max_tokens {
            if truncated {
                out.insert("@".to_string(), json!({"t": true}));
            }
            return Ok(());
        }

        if trim_multiline_field(out, "imports", "ih")
            || trim_multiline_field(out, "types", "tyh")
            || trim_multiline_field(out, "deps", "dh")
        {
            truncated = true;
            continue;
        }

        let mut removed_any = false;
        for keys in [
            &["imports", "ih"][..],
            &["types", "tyh"][..],
            &["deps", "dh"][..],
        ] {
            let mut removed = false;
            for key in keys {
                if out.remove(*key).is_some() {
                    removed = true;
                }
            }
            if removed {
                truncated = true;
                removed_any = true;
                break;
            }
        }

        if removed_any {
            continue;
        }

        out.insert("@".to_string(), json!({"t": true}));
        return Ok(());
    }
}

fn trim_multiline_field(out: &mut Map<String, Value>, field: &str, header: &str) -> bool {
    let Some(value) = out.get(field).and_then(Value::as_str) else {
        return false;
    };
    let mut lines = value.lines().collect::<Vec<_>>();
    if lines.is_empty() {
        out.remove(field);
        out.remove(header);
        return true;
    }

    lines.pop();
    if lines.is_empty() {
        out.remove(field);
        out.remove(header);
    } else {
        out.insert(field.to_string(), json!(lines.join("\n")));
    }
    true
}

mod symbols;
use symbols::*;

mod calls;
use calls::*;

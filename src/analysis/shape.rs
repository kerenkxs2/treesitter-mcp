//! Enhanced Shape Extraction Module
//!
//! Extracts detailed file structure with signatures, doc comments, and full code blocks.
//! Supports Rust, Python, JavaScript, TypeScript, Swift, C#, and Java.

use crate::parser::Language;
use std::io;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Node, Query, QueryCursor, Tree};

pub(crate) mod c_family;
mod gdscript;
mod lua;
mod scripting;

/// Enhanced function information with signature and documentation
#[derive(Debug, serde::Serialize, Clone)]
pub struct EnhancedFunctionInfo {
    pub name: String,
    pub signature: String,
    pub line: usize,
    pub end_line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<String>,
}

/// Enhanced struct information with documentation
#[derive(Debug, serde::Serialize, Clone)]
pub struct EnhancedStructInfo {
    pub name: String,
    pub line: usize,
    pub end_line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// Enhanced class information with documentation
#[derive(Debug, serde::Serialize, Clone)]
pub struct EnhancedClassInfo {
    pub name: String,
    pub line: usize,
    pub end_line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,

    // NEW: Methods nested in class (Python, JavaScript, TypeScript, C#)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub methods: Vec<EnhancedFunctionInfo>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<PropertyInfo>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<PropertyInfo>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub implements: Vec<String>,
}

/// Import information with text and line number
#[derive(Debug, serde::Serialize, Clone)]
pub struct ImportInfo {
    pub text: String,
    pub line: usize,
}

/// Method information from impl blocks
#[derive(Debug, serde::Serialize, Clone)]
pub struct MethodInfo {
    pub name: String,
    pub signature: String,
    pub line: usize,
    pub end_line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// Impl block information (Rust)
#[derive(Debug, serde::Serialize, Clone)]
pub struct ImplBlockInfo {
    pub type_name: String, // "Calculator", "Vec<T>", "Container<T>", etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trait_name: Option<String>, // For trait impls: "Display", "Add", etc.
    pub line: usize,
    pub end_line: usize,
    pub methods: Vec<MethodInfo>,
}

/// Trait definition information (Rust)
#[derive(Debug, serde::Serialize, Clone)]
pub struct TraitInfo {
    pub name: String,
    pub line: usize,
    pub end_line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    pub methods: Vec<MethodInfo>,
}

/// Interface information (TypeScript, C#)
#[derive(Debug, serde::Serialize, Clone)]
pub struct InterfaceInfo {
    pub name: String,
    pub line: usize,
    pub end_line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub methods: Vec<EnhancedFunctionInfo>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<PropertyInfo>,
}

/// Property information (C#, TypeScript, etc.)
#[derive(Debug, serde::Serialize, Clone)]
pub struct PropertyInfo {
    pub name: String,
    pub line: usize,
    pub end_line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub property_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

/// Enhanced file shape with detailed information
#[derive(Debug, serde::Serialize)]
pub struct EnhancedFileShape {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    pub functions: Vec<EnhancedFunctionInfo>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub structs: Vec<EnhancedStructInfo>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub classes: Vec<EnhancedClassInfo>,
    pub imports: Vec<ImportInfo>,

    // NEW: Impl blocks for Rust
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub impl_blocks: Vec<ImplBlockInfo>,

    // NEW: Traits for Rust
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub traits: Vec<TraitInfo>,

    // NEW: Interfaces for TypeScript, C#
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub interfaces: Vec<InterfaceInfo>,

    // NEW: Properties for C#, TypeScript
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<PropertyInfo>,

    // NEW: Dependencies (will populate in later phase)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<EnhancedFileShape>,
}

/// Extract enhanced shape from a parsed tree
pub fn extract_enhanced_shape(
    tree: &Tree,
    source: &str,
    language: Language,
    file_path: Option<&str>,
    include_code: bool,
) -> Result<EnhancedFileShape, io::Error> {
    let shape = match language {
        Language::GDScript => gdscript::extract(tree, source, include_code),
        Language::Lua => lua::extract(tree, source, include_code),
        Language::Rust => extract_rust_enhanced(tree, source, include_code)?,
        Language::Python => extract_python_enhanced(tree, source, include_code)?,
        Language::JavaScript => {
            extract_js_enhanced(tree, source, Language::JavaScript, include_code)?
        }
        Language::TypeScript | Language::Tsx => {
            extract_js_enhanced(tree, source, language, include_code)?
        }
        Language::Swift => extract_swift_enhanced(tree, source, include_code)?,
        Language::CSharp => extract_csharp_enhanced(tree, source, include_code)?,
        Language::Java => extract_java_enhanced(tree, source, include_code)?,
        Language::Go => extract_go_enhanced(tree, source, include_code)?,
        Language::C | Language::Cpp => c_family::extract(tree, source, include_code)?,
        Language::Html | Language::Css => {
            // HTML and CSS are markup/styling languages and are not suitable for
            // structural shape analysis. They lack the function/class/module structure
            // that other programming languages have. Tools like view_code, code_map,
            // and find_usages are designed for languages with well-defined symbols
            // and scopes (functions, classes, methods, etc.).
            //
            // For HTML/CSS analysis, consider using language-specific tools or parsers
            // designed for markup and styling languages.
            EnhancedFileShape {
                path: None,
                language: None,
                functions: vec![],
                structs: vec![],
                classes: vec![],
                traits: vec![],
                interfaces: vec![],
                properties: vec![],
                imports: vec![],
                impl_blocks: vec![],
                dependencies: vec![],
            }
        }
    };

    Ok(EnhancedFileShape {
        path: file_path.map(|p| p.to_string()),
        language: Some(language.name().to_string()),
        ..shape
    })
}

// ============================================================================
// CSS/HTML Shape Structures
// ============================================================================

use std::borrow::Cow;

// ============================================================================
// CSS Extraction (Regex-based for Tailwind)
// ============================================================================

use regex::Regex;

// ============================================================================
// HTML Extraction (Tree-sitter)
// ============================================================================

use std::collections::HashSet;

/// Helper to extract attribute value from a node
#[allow(dead_code)]
fn extract_attribute(node: &tree_sitter::Node, source: &str, attr_name: &str) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "attribute" {
            let mut attr_cursor = child.walk();
            let mut found_name = false;
            for attr_child in child.children(&mut attr_cursor) {
                if attr_child.kind() == "attribute_name" {
                    if let Ok(name) = attr_child.utf8_text(source.as_bytes()) {
                        if name == attr_name {
                            found_name = true;
                        }
                    }
                } else if found_name && attr_child.kind() == "quoted_attribute_value" {
                    if let Ok(value) = attr_child.utf8_text(source.as_bytes()) {
                        return Some(value.trim_matches('"').trim_matches('\'').to_string());
                    }
                }
            }
        }
    }
    None
}

mod rust;
use rust::*;

mod python;
use python::*;

mod javascript;
use javascript::*;

mod swift;
use swift::*;

mod csharp;
use csharp::*;

mod java;
use java::*;

mod go;
use go::*;

mod common;
use common::*;

mod web_types;
pub use web_types::*;

mod css;
use css::*;

mod html;
use html::*;

#[cfg(test)]
mod tests;
pub use common::{prepend_leading_comments_to_code, CommentMode};
pub fn extract_css_tailwind(
    source: &str,
    file_path: Option<&str>,
) -> Result<CssFileShape, io::Error> {
    css::extract_css_tailwind(source, file_path)
}

pub fn extract_html_shape(
    tree: &Tree,
    source: &str,
    file_path: Option<&str>,
) -> Result<HtmlFileShape, io::Error> {
    html::extract_html_shape(tree, source, file_path)
}

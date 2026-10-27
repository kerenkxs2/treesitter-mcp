use super::*;
use crate::parser::parse_code;

#[test]
fn test_extract_rust_function_signature() {
    let source = r#"
 /// Adds two numbers
 pub fn add(a: i32, b: i32) -> i32 {
     a + b
 }
 "#;
    let tree = parse_code(source, Language::Rust).expect("Failed to parse");
    let shape = extract_rust_enhanced(&tree, source, true).expect("Failed to extract shape");

    assert_eq!(shape.functions.len(), 1);
    let func = &shape.functions[0];
    assert_eq!(func.name, "add");
    assert!(func.signature.contains("pub fn add"));
    assert_eq!(func.line, 3);
    assert_eq!(func.end_line, 5);
}

#[test]
fn test_extract_python_function() {
    let source = r#"
def greet(name):
     """Greets a person"""
     return f"Hello, {name}!"
"#;
    let tree = parse_code(source, Language::Python).expect("Failed to parse");
    let shape = extract_python_enhanced(&tree, source, true).expect("Failed to extract shape");

    assert_eq!(shape.functions.len(), 1);
    let func = &shape.functions[0];
    assert_eq!(func.name, "greet");
    assert_eq!(func.line, 2);
}

#[test]
fn test_extract_js_class() {
    let source = r#"
 class Calculator {
     add(a, b) {
         return a + b;
     }
 }
 "#;
    let tree = parse_code(source, Language::JavaScript).expect("Failed to parse");
    let shape = extract_js_enhanced(&tree, source, Language::JavaScript, true)
        .expect("Failed to extract shape");

    assert_eq!(shape.classes.len(), 1);
    let cls = &shape.classes[0];
    assert_eq!(cls.name, "Calculator");
    assert_eq!(cls.line, 2);
}

#[test]
fn test_extract_imports() {
    let source = r#"
 use std::fmt;
 use std::io;

 fn main() {}
 "#;
    let tree = parse_code(source, Language::Rust).expect("Failed to parse");
    let shape = extract_rust_enhanced(&tree, source, true).expect("Failed to extract shape");

    assert_eq!(shape.imports.len(), 2);
    assert_eq!(shape.imports[0].text, "use std::fmt;");
    assert_eq!(shape.imports[0].line, 2);
}

// ========================================================================
// CSS Extraction Tests
// ========================================================================

#[test]
fn test_extract_css_tailwind_theme_variables() {
    let source = r#"
@theme {
  --color-primary: oklch(0.6 0.2 250);
  --color-secondary: #3b82f6;
  --spacing-lg: 1.5rem;
}
"#;
    let shape = extract_css_tailwind(source, Some("test.css")).expect("Failed to extract CSS");

    assert_eq!(shape.theme.len(), 3);
    assert_eq!(shape.theme[0].name, "--color-primary");
    assert_eq!(shape.theme[0].value, "oklch(0.6 0.2 250)");
    assert_eq!(shape.theme[0].line, 3);

    assert_eq!(shape.theme[1].name, "--color-secondary");
    assert_eq!(shape.theme[1].value, "#3b82f6");

    assert_eq!(shape.theme[2].name, "--spacing-lg");
    assert_eq!(shape.theme[2].value, "1.5rem");
}

#[test]
fn test_extract_css_tailwind_custom_classes() {
    let source = r#"
@layer components {
  .btn-primary {
    @apply bg-blue-500 text-white px-4 py-2 rounded;
  }
  .card {
    @apply border rounded-lg p-4;
  }
}
"#;
    let shape = extract_css_tailwind(source, None).expect("Failed to extract CSS");

    assert_eq!(shape.custom_classes.len(), 2);

    let btn = &shape.custom_classes[0];
    assert_eq!(btn.name, "btn-primary");
    assert_eq!(btn.applied_utilities.len(), 5);
    assert!(btn.applied_utilities.contains(&"bg-blue-500".to_string()));
    assert!(btn.applied_utilities.contains(&"text-white".to_string()));
    assert_eq!(btn.layer, Some(std::borrow::Cow::Borrowed("components")));

    let card = &shape.custom_classes[1];
    assert_eq!(card.name, "card");
    assert_eq!(card.applied_utilities.len(), 3);
    assert!(card.applied_utilities.contains(&"border".to_string()));
}

#[test]
fn test_extract_css_tailwind_utilities_layer() {
    let source = r#"
@layer utilities {
  .text-balance {
    text-wrap: balance;
  }
}
"#;
    let shape = extract_css_tailwind(source, None).expect("Failed to extract CSS");

    assert_eq!(shape.custom_classes.len(), 1);
    assert_eq!(shape.custom_classes[0].name, "text-balance");
    assert_eq!(
        shape.custom_classes[0].layer,
        Some(std::borrow::Cow::Borrowed("utilities"))
    );
}

#[test]
fn test_extract_css_tailwind_keyframes() {
    let source = r#"
@keyframes spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}

@keyframes fade-in {
  0% { opacity: 0; }
  100% { opacity: 1; }
}
"#;
    let shape = extract_css_tailwind(source, None).expect("Failed to extract CSS");

    assert_eq!(shape.keyframes.len(), 2);
    assert_eq!(shape.keyframes[0].name, "spin");
    assert_eq!(shape.keyframes[0].line, 2);
    assert_eq!(shape.keyframes[1].name, "fade-in");
    assert_eq!(shape.keyframes[1].line, 7);
}

#[test]
fn test_extract_css_tailwind_empty_source() {
    let source = "";
    let shape = extract_css_tailwind(source, None).expect("Failed to extract CSS");

    assert_eq!(shape.theme.len(), 0);
    assert_eq!(shape.custom_classes.len(), 0);
    assert_eq!(shape.keyframes.len(), 0);
}

#[test]
fn test_extract_css_tailwind_nested_braces() {
    let source = r#"
@layer components {
  .complex {
    @apply bg-blue-500;
    &:hover {
      @apply bg-blue-600;
    }
  }
}
"#;
    let shape = extract_css_tailwind(source, None).expect("Failed to extract CSS");

    // Should extract the outer class
    assert!(shape.custom_classes.iter().any(|c| c.name == "complex"));
}

#[test]
fn test_extract_css_tailwind_multiple_layers() {
    let source = r#"
@layer components {
  .btn { @apply px-4 py-2; }
}

@layer utilities {
  .custom-util { color: red; }
}
"#;
    let shape = extract_css_tailwind(source, None).expect("Failed to extract CSS");

    assert_eq!(shape.custom_classes.len(), 2);
    assert_eq!(
        shape.custom_classes[0].layer,
        Some(std::borrow::Cow::Borrowed("components"))
    );
    assert_eq!(
        shape.custom_classes[1].layer,
        Some(std::borrow::Cow::Borrowed("utilities"))
    );
}

#[test]
fn test_extract_css_tailwind_malformed_no_panic() {
    // Test that malformed CSS doesn't panic
    let source = r#"
@theme {
  --incomplete
}
@layer components {
  .unclosed {
"#;
    // Should not panic, just extract what it can
    let result = extract_css_tailwind(source, None);
    assert!(result.is_ok());
}

// ========================================================================
// HTML Extraction Tests
// ========================================================================

#[test]
fn test_extract_html_shape_ids() {
    let source = r#"
<!DOCTYPE html>
<html>
<body>
  <div id="header">Header</div>
  <nav id="main-nav">Navigation</nav>
  <section id="content">Content</section>
</body>
</html>
"#;
    let tree = parse_code(source, Language::Html).expect("Failed to parse HTML");
    let shape =
        extract_html_shape(&tree, source, Some("test.html")).expect("Failed to extract HTML");

    assert_eq!(shape.ids.len(), 3);
    assert_eq!(shape.ids[0].tag, "div");
    assert_eq!(shape.ids[0].id, "header");
    assert_eq!(shape.ids[1].tag, "nav");
    assert_eq!(shape.ids[1].id, "main-nav");
    assert_eq!(shape.ids[2].tag, "section");
    assert_eq!(shape.ids[2].id, "content");
}

#[test]
fn test_extract_html_shape_custom_classes() {
    let source = r#"
<div class="custom-card bg-white p-4">
  <h1 class="title text-xl font-bold">Hello</h1>
  <p class="description">Text</p>
</div>
"#;
    let tree = parse_code(source, Language::Html).expect("Failed to parse HTML");
    let shape = extract_html_shape(&tree, source, None).expect("Failed to extract HTML");

    // Should filter out Tailwind utilities and keep only custom classes
    assert!(shape.classes_used.contains(&"custom-card".to_string()));
    assert!(shape.classes_used.contains(&"title".to_string()));
    assert!(shape.classes_used.contains(&"description".to_string()));

    // Should NOT contain Tailwind utilities
    assert!(!shape.classes_used.contains(&"bg-white".to_string()));
    assert!(!shape.classes_used.contains(&"p-4".to_string()));
    assert!(!shape.classes_used.contains(&"text-xl".to_string()));
    assert!(!shape.classes_used.contains(&"font-bold".to_string()));
}

#[test]
fn test_extract_html_shape_scripts() {
    let source = r#"
<html>
<head>
  <script src="app.js"></script>
  <script>console.log('inline');</script>
</head>
</html>
"#;
    let tree = parse_code(source, Language::Html).expect("Failed to parse HTML");
    let shape = extract_html_shape(&tree, source, None).expect("Failed to extract HTML");

    assert_eq!(shape.scripts.len(), 2);

    // External script
    assert_eq!(shape.scripts[0].src, Some("app.js".to_string()));
    assert!(!shape.scripts[0].inline);

    // Inline script
    assert_eq!(shape.scripts[1].src, None);
    assert!(shape.scripts[1].inline);
}

#[test]
fn test_extract_html_shape_styles() {
    let source = r#"
<html>
<head>
  <link rel="stylesheet" href="styles.css">
  <style>body { margin: 0; }</style>
</head>
</html>
"#;
    let tree = parse_code(source, Language::Html).expect("Failed to parse HTML");
    let shape = extract_html_shape(&tree, source, None).expect("Failed to extract HTML");

    assert_eq!(shape.styles.len(), 2);

    // External stylesheet
    assert_eq!(shape.styles[0].href, Some("styles.css".to_string()));
    assert!(!shape.styles[0].inline);

    // Inline style
    assert_eq!(shape.styles[1].href, None);
    assert!(shape.styles[1].inline);
}

#[test]
fn test_extract_html_shape_tailwind_variants() {
    let source = r#"
<div class="hover:bg-blue-500 dark:text-white sm:p-4 custom-class">
  Content
</div>
"#;
    let tree = parse_code(source, Language::Html).expect("Failed to parse HTML");
    let shape = extract_html_shape(&tree, source, None).expect("Failed to extract HTML");

    // Should keep custom class
    assert!(shape.classes_used.contains(&"custom-class".to_string()));

    // Should filter out Tailwind utilities with variants
    assert!(!shape
        .classes_used
        .contains(&"hover:bg-blue-500".to_string()));
    assert!(!shape.classes_used.contains(&"dark:text-white".to_string()));
    assert!(!shape.classes_used.contains(&"sm:p-4".to_string()));
}

#[test]
fn test_extract_html_shape_empty_document() {
    let source = "<html></html>";
    let tree = parse_code(source, Language::Html).expect("Failed to parse HTML");
    let shape = extract_html_shape(&tree, source, None).expect("Failed to extract HTML");

    assert_eq!(shape.ids.len(), 0);
    assert_eq!(shape.classes_used.len(), 0);
    assert_eq!(shape.scripts.len(), 0);
    assert_eq!(shape.styles.len(), 0);
}

#[test]
fn test_extract_html_shape_deduplicates_classes() {
    let source = r#"
<div class="custom-class">
  <span class="custom-class">Text</span>
  <p class="custom-class">More</p>
</div>
"#;
    let tree = parse_code(source, Language::Html).expect("Failed to parse HTML");
    let shape = extract_html_shape(&tree, source, None).expect("Failed to extract HTML");

    // Should only have one instance of custom-class
    assert_eq!(
        shape
            .classes_used
            .iter()
            .filter(|c| *c == "custom-class")
            .count(),
        1
    );
}

#[test]
fn test_extract_html_shape_line_numbers() {
    let source = r#"
<html>
<body>
  <div id="first">Line 4</div>
  <div id="second">Line 5</div>
</body>
</html>
"#;
    let tree = parse_code(source, Language::Html).expect("Failed to parse HTML");
    let shape = extract_html_shape(&tree, source, None).expect("Failed to extract HTML");

    assert_eq!(shape.ids.len(), 2);
    assert_eq!(shape.ids[0].line, 4);
    assert_eq!(shape.ids[1].line, 5);
}

#[test]
fn test_extract_html_shape_malformed_no_panic() {
    // Test that malformed HTML doesn't panic
    let source = r#"
<div id="test" class="custom
<p>Unclosed
"#;
    let tree = parse_code(source, Language::Html).expect("Failed to parse HTML");
    // Should not panic, tree-sitter handles malformed HTML gracefully
    let result = extract_html_shape(&tree, source, None);
    assert!(result.is_ok());
}

#[test]
fn test_extract_html_shape_arbitrary_tailwind_values() {
    let source = r#"
<div class="w-[300px] h-[calc(100vh-64px)] custom-width">
  Content
</div>
"#;
    let tree = parse_code(source, Language::Html).expect("Failed to parse HTML");
    let shape = extract_html_shape(&tree, source, None).expect("Failed to extract HTML");

    // Should keep custom class
    assert!(shape.classes_used.contains(&"custom-width".to_string()));

    // Should filter out arbitrary Tailwind values
    assert!(!shape.classes_used.contains(&"w-[300px]".to_string()));
    assert!(!shape
        .classes_used
        .contains(&"h-[calc(100vh-64px)]".to_string()));
}

#[test]
fn test_extract_html_shape_important_modifier() {
    let source = r#"
<div class="!bg-red-500 !important-custom">
  Content
</div>
"#;
    let tree = parse_code(source, Language::Html).expect("Failed to parse HTML");
    let shape = extract_html_shape(&tree, source, None).expect("Failed to extract HTML");

    // Should keep custom class with ! prefix
    assert!(shape
        .classes_used
        .contains(&"!important-custom".to_string()));

    // Should filter out Tailwind utility with ! prefix
    assert!(!shape.classes_used.contains(&"!bg-red-500".to_string()));
}

use super::*;

// ============================================================================
// Tailwind Utility Detection
// ============================================================================

/// Check if a class name is a Tailwind utility (to filter out)
///
/// NOTE: This list covers common Tailwind v4 utilities but is not exhaustive.
/// It may need updates as Tailwind evolves. Consider making this configurable
/// in the future to allow users to add custom utility patterns.
#[allow(dead_code)]
pub(super) fn is_tailwind_utility(class: &str) -> bool {
    // Handle important modifier at the start
    let class = class.strip_prefix('!').unwrap_or(class);

    // Handle variant prefixes (hover:, dark:, sm:, etc.)
    let base = class.split(':').next_back().unwrap_or(class);

    // Exact match utilities
    let exact = [
        // Layout
        "flex",
        "grid",
        "block",
        "inline",
        "inline-block",
        "inline-flex",
        "inline-grid",
        "hidden",
        "container",
        "table",
        "table-row",
        "table-cell",
        // Position
        "relative",
        "absolute",
        "fixed",
        "sticky",
        "static",
        // Display
        "visible",
        "invisible",
        "collapse",
        // Accessibility
        "sr-only",
        "not-sr-only",
        // Interactivity
        "pointer-events-none",
        "pointer-events-auto",
        // Other common utilities
        "truncate",
        "italic",
        "underline",
        "line-through",
        "no-underline",
        "uppercase",
        "lowercase",
        "capitalize",
        "normal-case",
    ];
    if exact.contains(&base) {
        return true;
    }

    // Prefix-based utilities
    let prefixes = [
        // Spacing
        "p-",
        "px-",
        "py-",
        "pt-",
        "pr-",
        "pb-",
        "pl-",
        "ps-",
        "pe-",
        "m-",
        "mx-",
        "my-",
        "mt-",
        "mr-",
        "mb-",
        "ml-",
        "ms-",
        "me-",
        "-m",
        "gap-",
        "space-",
        // Sizing
        "w-",
        "h-",
        "min-w-",
        "min-h-",
        "max-w-",
        "max-h-",
        "size-",
        // Typography
        "text-",
        "font-",
        "leading-",
        "tracking-",
        "indent-",
        "decoration-",
        "underline-offset-",
        // Colors
        "bg-",
        "from-",
        "via-",
        "to-",
        "fill-",
        "stroke-",
        "border-",
        "outline-",
        "ring-",
        "shadow-",
        // Borders
        "rounded-",
        "divide-",
        // Layout
        "flex-",
        "grid-",
        "col-",
        "row-",
        "order-",
        "items-",
        "justify-",
        "content-",
        "place-",
        "self-",
        "auto-cols-",
        "auto-rows-",
        // Position
        "z-",
        "top-",
        "right-",
        "bottom-",
        "left-",
        "inset-",
        // Transforms
        "scale-",
        "rotate-",
        "translate-",
        "skew-",
        "origin-",
        // Transitions & Animations
        "transition-",
        "duration-",
        "delay-",
        "ease-",
        "animate-",
        // Effects
        "opacity-",
        "mix-blend-",
        "bg-blend-",
        "backdrop-blur-",
        "backdrop-brightness-",
        "backdrop-contrast-",
        "backdrop-grayscale-",
        "backdrop-hue-rotate-",
        "backdrop-invert-",
        "backdrop-opacity-",
        "backdrop-saturate-",
        "backdrop-sepia-",
        // Filters
        "blur-",
        "brightness-",
        "contrast-",
        "drop-shadow-",
        "grayscale-",
        "hue-rotate-",
        "invert-",
        "saturate-",
        "sepia-",
        // Interactivity
        "cursor-",
        "pointer-events-",
        "resize-",
        "select-",
        "user-select-",
        "caret-",
        "accent-",
        // Overflow
        "overflow-",
        "overscroll-",
        "scroll-",
        "snap-",
        // Other
        "aspect-",
        "columns-",
        "break-",
        "break-after-",
        "break-before-",
        "break-inside-",
        "float-",
        "clear-",
        "object-",
        "isolation-",
        "list-",
        "placeholder-",
        "will-change-",
        "touch-",
    ];

    prefixes.iter().any(|p| base.starts_with(p)) || base.contains('[') // Arbitrary values like w-[300px]
}

/// Extract CSS shape from Tailwind v4 source code
///
/// This function uses regex to parse Tailwind-specific directives (@theme, @layer, @apply)
/// which are not part of standard CSS and thus not handled by tree-sitter-css.
#[allow(dead_code)]
pub fn extract_css_tailwind(
    source: &str,
    file_path: Option<&str>,
) -> Result<CssFileShape, io::Error> {
    let mut theme = Vec::new();
    let mut custom_classes = Vec::new();
    let mut keyframes = Vec::new();

    // 1. Extract @theme block variables
    let theme_block_re = Regex::new(r"@theme\s*\{([\s\S]*?)\}")
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Invalid regex: {e}")))?;

    if let Some(cap) = theme_block_re.captures(source) {
        let theme_content_start = cap.get(1).unwrap().start(); // Start of captured group 1
        let theme_content = &cap[1];

        let var_re = Regex::new(r"(?m)^\s*(--[\w-]+)\s*:\s*([^;]+);").map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidData, format!("Invalid regex: {e}"))
        })?;

        for var_cap in var_re.captures_iter(theme_content) {
            // Use the start of the variable name (group 1), not the whole match
            let var_name_start_in_theme = var_cap.get(1).unwrap().start();
            let absolute_offset = theme_content_start + var_name_start_in_theme;

            theme.push(ThemeVariable {
                name: var_cap[1].to_string(),
                value: var_cap[2].trim().to_string(),
                line: calculate_line(source, absolute_offset),
            });
        }
    }

    // 2. Extract @layer components/utilities blocks
    // We need to manually parse nested braces since regex can't handle them properly
    let layer_start_re = Regex::new(r"@layer\s+(components|utilities)\s*\{")
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Invalid regex: {e}")))?;

    // Extract class definitions within layer
    let class_re = Regex::new(r"\.([\w-]+)\s*\{([^}]*)\}")
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Invalid regex: {e}")))?;
    let apply_re = Regex::new(r"@apply\s+([^;]+);")
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Invalid regex: {e}")))?;

    for layer_match in layer_start_re.captures_iter(source) {
        let layer_name = match &layer_match[1] {
            "components" => Cow::Borrowed("components"),
            "utilities" => Cow::Borrowed("utilities"),
            _ => Cow::Owned(layer_match[1].to_string()),
        };
        let layer_start = layer_match.get(0).unwrap().end(); // Start after the opening brace

        // Find the matching closing brace
        let mut brace_count = 1;
        let mut layer_end = layer_start;
        let source_bytes = source.as_bytes();

        for (i, &byte) in source_bytes.iter().enumerate().skip(layer_start) {
            match byte {
                b'{' => brace_count += 1,
                b'}' => {
                    brace_count -= 1;
                    if brace_count == 0 {
                        layer_end = i;
                        break;
                    }
                }
                _ => {}
            }
        }

        if layer_end == layer_start {
            continue; // No matching closing brace found
        }

        let layer_content = &source[layer_start..layer_end];

        for class_cap in class_re.captures_iter(layer_content) {
            let class_start_in_layer = class_cap.get(0).unwrap().start();
            let absolute_offset = layer_start + class_start_in_layer;
            let class_name = class_cap[1].to_string();
            let class_body = &class_cap[2];

            // Extract @apply utilities
            let mut applied = Vec::new();

            for apply_cap in apply_re.captures_iter(class_body) {
                applied.extend(apply_cap[1].split_whitespace().map(String::from));
            }

            custom_classes.push(CustomClass {
                name: class_name,
                applied_utilities: applied,
                layer: Some(layer_name.clone()),
                line: calculate_line(source, absolute_offset),
            });
        }
    }

    // 3. Extract @keyframes
    let keyframes_re = Regex::new(r"@keyframes\s+([\w-]+)\s*\{")
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Invalid regex: {e}")))?;

    for kf_cap in keyframes_re.captures_iter(source) {
        keyframes.push(KeyframeInfo {
            name: kf_cap[1].to_string(),
            line: calculate_line(source, kf_cap.get(0).unwrap().start()),
        });
    }

    Ok(CssFileShape {
        path: file_path.map(String::from),
        theme,
        custom_classes,
        keyframes,
    })
}

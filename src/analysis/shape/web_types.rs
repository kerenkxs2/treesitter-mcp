use std::borrow::Cow;

/// Theme variable from @theme block
#[allow(dead_code)]
#[derive(Debug, serde::Serialize, Clone)]
pub struct ThemeVariable {
    pub name: String,  // "--color-primary", "--spacing-lg"
    pub value: String, // "oklch(0.6 0.2 250)", "1.5rem"
    pub line: usize,
}

/// Custom component class (defined with @apply or custom styles)
#[allow(dead_code)]
#[derive(Debug, serde::Serialize, Clone)]
pub struct CustomClass {
    pub name: String,                     // "btn-primary", "card"
    pub applied_utilities: Vec<String>,   // ["bg-primary", "text-white", "px-4"]
    pub layer: Option<Cow<'static, str>>, // "components", "utilities", or None
    pub line: usize,
}

/// Keyframe animation
#[allow(dead_code)]
#[derive(Debug, serde::Serialize, Clone)]
pub struct KeyframeInfo {
    pub name: String,
    pub line: usize,
}

/// CSS file shape (Tailwind v4 focused)
#[allow(dead_code)]
#[derive(Debug, serde::Serialize)]
pub struct CssFileShape {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    /// Theme variables from @theme block
    pub theme: Vec<ThemeVariable>,

    /// Custom component/utility classes (reusable)
    pub custom_classes: Vec<CustomClass>,

    /// @keyframes animations
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub keyframes: Vec<KeyframeInfo>,
}

/// HTML element with id
#[allow(dead_code)]
#[derive(Debug, serde::Serialize, Clone)]
pub struct HtmlIdInfo {
    pub tag: String,
    pub id: String,
    pub line: usize,
}

/// Script reference
#[allow(dead_code)]
#[derive(Debug, serde::Serialize, Clone)]
pub struct ScriptInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub src: Option<String>,
    pub inline: bool,
    pub line: usize,
}

/// Style reference
#[allow(dead_code)]
#[derive(Debug, serde::Serialize, Clone)]
pub struct StyleInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,
    pub inline: bool,
    pub line: usize,
}

/// HTML file shape
#[allow(dead_code)]
#[derive(Debug, serde::Serialize)]
pub struct HtmlFileShape {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    /// Elements with IDs (for JS/navigation)
    pub ids: Vec<HtmlIdInfo>,

    /// All unique custom classes used (non-Tailwind utilities)
    pub classes_used: Vec<String>,

    /// Script references
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<ScriptInfo>,

    /// Style references
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub styles: Vec<StyleInfo>,
}

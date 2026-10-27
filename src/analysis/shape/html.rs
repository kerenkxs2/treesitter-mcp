use super::*;

/// Calculate line number from byte offset
#[allow(dead_code)]
pub(super) fn calculate_line(source: &str, byte_offset: usize) -> usize {
    source[..byte_offset].matches('\n').count() + 1
}

/// Extract HTML shape from parsed tree
#[allow(dead_code)]
pub fn extract_html_shape(
    tree: &Tree,
    source: &str,
    file_path: Option<&str>,
) -> Result<HtmlFileShape, io::Error> {
    let mut ids = Vec::new();
    let mut all_classes = Vec::new();
    let mut scripts = Vec::new();
    let mut styles = Vec::new();

    // Use a simpler query that captures elements
    let query = Query::new(
        &tree_sitter_html::LANGUAGE.into(),
        r#"
        (element (start_tag) @start_tag)
        (script_element (start_tag) @script_tag)
        (style_element (start_tag) @style_tag)
        "#,
    )
    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Query error: {e}")))?;

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());

    while let Some(match_) = matches.next() {
        for capture in match_.captures {
            let node = capture.node;
            let capture_name = query.capture_names()[capture.index as usize];

            match capture_name {
                "start_tag" => {
                    // Extract tag name - look for child with kind "tag_name"
                    let mut tag_name = String::new();
                    let mut tag_cursor = node.walk();
                    for child in node.children(&mut tag_cursor) {
                        if child.kind() == "tag_name" {
                            if let Ok(name) = child.utf8_text(source.as_bytes()) {
                                tag_name = name.to_string();
                                break;
                            }
                        }
                    }

                    let line = node.start_position().row + 1;

                    // Extract attributes
                    let id_attr = extract_attribute(&node, source, "id");
                    let class_attr = extract_attribute(&node, source, "class");
                    let rel_attr = extract_attribute(&node, source, "rel");
                    let href_attr = extract_attribute(&node, source, "href");

                    // Handle id
                    if let Some(id) = id_attr {
                        ids.push(HtmlIdInfo {
                            tag: tag_name.to_string(),
                            id,
                            line,
                        });
                    }

                    // Handle classes
                    if let Some(classes) = class_attr {
                        all_classes.extend(classes.split_whitespace().map(String::from));
                    }

                    // Handle link elements (stylesheets)
                    if tag_name == "link" {
                        if let Some(rel) = rel_attr {
                            if rel == "stylesheet" {
                                styles.push(StyleInfo {
                                    href: href_attr,
                                    inline: false,
                                    line,
                                });
                            }
                        }
                    }
                }
                "script_tag" => {
                    let line = node.start_position().row + 1;
                    let src = extract_attribute(&node, source, "src");
                    scripts.push(ScriptInfo {
                        src: src.clone(),
                        inline: src.is_none(),
                        line,
                    });
                }
                "style_tag" => {
                    let line = node.start_position().row + 1;
                    styles.push(StyleInfo {
                        href: None,
                        inline: true,
                        line,
                    });
                }
                _ => {}
            }
        }
    }

    // Deduplicate and filter classes
    let classes_used: Vec<String> = all_classes
        .into_iter()
        .filter(|c| !is_tailwind_utility(c))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    Ok(HtmlFileShape {
        path: file_path.map(String::from),
        ids,
        classes_used,
        scripts,
        styles,
    })
}

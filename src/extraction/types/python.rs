use super::*;

pub(crate) fn extract_python_types(
    source: &str,
    relative_path: &Path,
) -> Result<Vec<TypeDefinition>> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .wrap_err("Failed to configure Python parser")?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| eyre::eyre!("Failed to parse Python source"))?;

    let query_src = r#"
        (class_definition name: (identifier) @name) @class
        (assignment
            left: (identifier) @name
            right: (call
                function: (identifier) @func
                arguments: (argument_list) @args)
        ) @special_assignment
    "#;

    let query = Query::new(&tree_sitter_python::LANGUAGE.into(), query_src)
        .wrap_err("Failed to compile Python query")?;

    let source_bytes = source.as_bytes();
    let file_path = relative_path.to_path_buf();
    let mut definitions = Vec::new();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source_bytes);

    while let Some(match_) = matches.next() {
        let mut name_node = None;
        let mut def_node = None;
        let mut is_special = false;
        let mut func_name = String::new();

        for capture in match_.captures {
            let capture_name = query.capture_names()[capture.index as usize];
            match capture_name {
                "name" => name_node = Some(capture.node),
                "class" => def_node = Some(capture.node),
                "special_assignment" => {
                    def_node = Some(capture.node);
                    is_special = true;
                }
                "func" => {
                    func_name = capture
                        .node
                        .utf8_text(source_bytes)
                        .unwrap_or_default()
                        .to_string();
                }
                _ => {}
            }
        }

        let Some(name_node) = name_node else {
            continue;
        };
        let Some(def_node) = def_node else {
            continue;
        };
        let Ok(name) = name_node.utf8_text(source_bytes) else {
            continue;
        };

        if is_special {
            let kind = match func_name.as_str() {
                "TypedDict" => TypeKind::TypedDict,
                "NamedTuple" => TypeKind::NamedTuple,
                _ => continue,
            };

            let fields = if let Some(args) = def_node
                .child_by_field_name("right")
                .and_then(|right| right.child_by_field_name("arguments"))
            {
                match kind {
                    TypeKind::TypedDict => parse_python_typed_dict_fields(args, source_bytes),
                    TypeKind::NamedTuple => parse_python_named_tuple_fields(args, source_bytes),
                    _ => None,
                }
            } else {
                None
            };

            definitions.push(TypeDefinition {
                name: name.to_string(),
                kind,
                file: file_path.clone(),
                line: def_node.start_position().row + 1,
                signature: signature_for(def_node, source_bytes),
                usage_count: 0,
                fields,
                variants: None,
                members: None,
            });
            continue;
        }

        let mut kind = TypeKind::Class;
        let mut fields = Vec::new();
        let mut variants = None;
        let mut members = None;

        // Check if it's an Enum or Protocol
        if let Some(superclasses) = def_node.child_by_field_name("superclasses") {
            let text = superclasses.utf8_text(source_bytes).unwrap_or_default();
            if text.contains("Enum") {
                kind = TypeKind::Enum;
            } else if text.contains("Protocol") {
                kind = TypeKind::Protocol;
            }
        }

        if let Some(body) = def_node.child_by_field_name("body") {
            let mut walker = body.walk();
            for child in body.children(&mut walker) {
                match child.kind() {
                    "function_definition" => {
                        let fname = child
                            .child_by_field_name("name")
                            .and_then(|n| n.utf8_text(source_bytes).ok())
                            .unwrap_or_default();
                        if fname == "__init__" {
                            // Extract fields from self assignments
                            if let Some(fbody) = child.child_by_field_name("body") {
                                let mut fwalker = fbody.walk();
                                for stmt in fbody.children(&mut fwalker) {
                                    if stmt.kind() == "expression_statement" {
                                        if let Some(assignment) = stmt.child(0) {
                                            if assignment.kind() == "assignment" {
                                                if let Some(left) =
                                                    assignment.child_by_field_name("left")
                                                {
                                                    if left.kind() == "attribute" {
                                                        if let Some(obj) =
                                                            left.child_by_field_name("object")
                                                        {
                                                            if obj
                                                                .utf8_text(source_bytes)
                                                                .unwrap_or_default()
                                                                == "self"
                                                            {
                                                                if let Some(attr) = left
                                                                    .child_by_field_name(
                                                                        "attribute",
                                                                    )
                                                                {
                                                                    let field_name = attr
                                                                        .utf8_text(source_bytes)
                                                                        .unwrap_or_default()
                                                                        .to_string();
                                                                    fields.push(Field {
                                                                        name: field_name,
                                                                        type_annotation: "Any"
                                                                            .to_string(),
                                                                    });
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Track methods for all classes/protocols (legacy type_map expects this)
                        let m = members.get_or_insert_with(Vec::new);
                        m.push(Member {
                            name: fname.to_string(),
                            type_annotation: signature_for(child, source_bytes),
                        });
                    }
                    "decorated_definition" => {
                        let definition = child.child_by_field_name("definition").or_else(|| {
                            let mut dwalker = child.walk();
                            let found = child
                                .children(&mut dwalker)
                                .find(|n| n.kind() == "function_definition");
                            found
                        });

                        let Some(definition) = definition else {
                            continue;
                        };

                        let fname = definition
                            .child_by_field_name("name")
                            .and_then(|n| n.utf8_text(source_bytes).ok())
                            .unwrap_or_default();

                        if !fname.is_empty() {
                            let m = members.get_or_insert_with(Vec::new);
                            m.push(Member {
                                name: fname.to_string(),
                                type_annotation: signature_for(definition, source_bytes),
                            });
                        }
                    }
                    "expression_statement" => {
                        if let Some(statement) = child.child(0) {
                            if statement.kind() == "assignment" {
                                if kind == TypeKind::Enum {
                                    let v = variants.get_or_insert_with(Vec::new);
                                    if let Some(left) = statement.child_by_field_name("left") {
                                        let vname =
                                            left.utf8_text(source_bytes).unwrap_or_default();
                                        if !vname.is_empty() {
                                            v.push(Variant {
                                                name: vname.to_string(),
                                                type_annotation: None,
                                            });
                                        }
                                    }
                                    continue;
                                }

                                // Protocol: treat typed class attributes as members
                                if kind == TypeKind::Protocol {
                                    if let Some(left) = statement.child_by_field_name("left") {
                                        if left.kind() == "identifier" {
                                            let name = left
                                                .utf8_text(source_bytes)
                                                .unwrap_or_default()
                                                .to_string();
                                            if !name.is_empty() {
                                                let type_annotation = statement
                                                    .child_by_field_name("type")
                                                    .and_then(|n| n.utf8_text(source_bytes).ok())
                                                    .unwrap_or("Any")
                                                    .to_string();
                                                let m = members.get_or_insert_with(Vec::new);
                                                m.push(Member {
                                                    name,
                                                    type_annotation,
                                                });
                                            }
                                        }
                                    }
                                    continue;
                                }

                                // Class attributes / fields
                                if let Some(left) = statement.child_by_field_name("left") {
                                    if left.kind() == "identifier" {
                                        let name = left
                                            .utf8_text(source_bytes)
                                            .unwrap_or_default()
                                            .to_string();
                                        if !name.is_empty() {
                                            let type_annotation = statement
                                                .child_by_field_name("type")
                                                .and_then(|n| n.utf8_text(source_bytes).ok())
                                                .unwrap_or("Any")
                                                .to_string();
                                            fields.push(Field {
                                                name,
                                                type_annotation,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        definitions.push(TypeDefinition {
            name: name.to_string(),
            kind,
            file: file_path.clone(),
            line: def_node.start_position().row + 1,
            signature: signature_for(def_node, source_bytes),
            usage_count: 0,
            fields: if fields.is_empty() {
                None
            } else {
                Some(fields)
            },
            variants,
            members,
        });
    }

    Ok(definitions)
}

pub(super) fn parse_python_typed_dict_fields(args: Node, source: &[u8]) -> Option<Vec<Field>> {
    let mut walker = args.walk();
    let dict_node = args
        .children(&mut walker)
        .find(|child| child.kind() == "dictionary")?;

    let mut fields = Vec::new();
    let mut dict_walker = dict_node.walk();
    for pair in dict_node.children(&mut dict_walker) {
        if pair.kind() != "pair" {
            continue;
        }

        let key_node = pair.child_by_field_name("key").or_else(|| pair.child(0))?;
        let value_node = pair
            .child_by_field_name("value")
            .or_else(|| pair.child(pair.child_count().saturating_sub(1) as u32))?;

        let raw_key = key_node.utf8_text(source).ok()?;
        let name = unquote_python_string(raw_key);
        if name.is_empty() {
            continue;
        }

        let type_annotation = value_node.utf8_text(source).unwrap_or("Any").to_string();

        fields.push(Field {
            name,
            type_annotation,
        });
    }

    if fields.is_empty() {
        None
    } else {
        Some(fields)
    }
}

pub(super) fn parse_python_named_tuple_fields(args: Node, source: &[u8]) -> Option<Vec<Field>> {
    let mut walker = args.walk();
    let list_node = args
        .children(&mut walker)
        .find(|child| child.kind() == "list")?;

    let mut fields = Vec::new();
    let mut list_walker = list_node.walk();
    for tuple_node in list_node.children(&mut list_walker) {
        if tuple_node.kind() != "tuple" {
            continue;
        }

        let mut tuple_walker = tuple_node.walk();
        let mut items: Vec<Node> = tuple_node
            .children(&mut tuple_walker)
            .filter(|n| n.is_named())
            .collect();
        if items.len() < 2 {
            continue;
        }

        let raw_name = items.remove(0).utf8_text(source).ok()?;
        let name = unquote_python_string(raw_name);
        if name.is_empty() {
            continue;
        }

        let type_annotation = items[0].utf8_text(source).unwrap_or("Any").to_string();

        fields.push(Field {
            name,
            type_annotation,
        });
    }

    if fields.is_empty() {
        None
    } else {
        Some(fields)
    }
}

pub(super) fn unquote_python_string(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.len() >= 2 {
        let bytes = trimmed.as_bytes();
        let first = bytes[0] as char;
        let last = bytes[trimmed.len() - 1] as char;
        if (first == '\'' && last == '\'') || (first == '"' && last == '"') {
            return trimmed[1..trimmed.len() - 1].to_string();
        }
    }
    trimmed.to_string()
}

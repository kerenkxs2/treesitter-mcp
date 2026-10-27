use super::*;

pub(super) fn insert_symbol_tables(
    out: &mut Map<String, Value>,
    shape: &EnhancedFileShape,
    detail: DetailLevel,
) {
    let functions = functions_to_rows(&shape.functions, detail);
    if !functions.is_empty() {
        out.insert("f".to_string(), json!(functions));
    }

    let structs = structs_to_rows(&shape.structs, detail);
    if !structs.is_empty() {
        out.insert("s".to_string(), json!(structs));
    }

    let classes = classes_to_rows(&shape.classes, detail);
    if !classes.is_empty() {
        out.insert("c".to_string(), json!(classes));
    }
}

pub(super) fn insert_imports_and_traits(
    out: &mut Map<String, Value>,
    shape: &EnhancedFileShape,
    detail: DetailLevel,
) {
    let imports = imports_to_rows(&shape.imports);
    if !imports.is_empty() {
        out.insert("ih".to_string(), json!("line|text"));
        out.insert("im".to_string(), json!(imports));
    }

    let trait_methods = trait_methods_to_rows(&shape.traits, detail);
    if !trait_methods.is_empty() {
        out.insert("th".to_string(), json!(detail.trait_header()));
        out.insert("tm".to_string(), json!(trait_methods));
    }
}

pub(super) fn insert_interfaces_and_properties(
    out: &mut Map<String, Value>,
    shape: &EnhancedFileShape,
    detail: DetailLevel,
) {
    let interfaces = interfaces_to_rows(&shape.interfaces, detail);
    if !interfaces.is_empty() {
        out.insert("ah".to_string(), json!(detail.header()));
        out.insert("i".to_string(), json!(interfaces));
    }

    let properties = properties_to_rows(&shape.properties);
    if !properties.is_empty() {
        out.insert("ph".to_string(), json!("name|line|type|doc"));
        out.insert("pr".to_string(), json!(properties));
    }

    let implements = class_implements_to_rows(&shape.classes);
    if !implements.is_empty() {
        out.insert("ch".to_string(), json!("class|iface"));
        out.insert("ci".to_string(), json!(implements));
    }
}

pub(super) fn insert_class_methods(
    out: &mut Map<String, Value>,
    shape: &EnhancedFileShape,
    detail: DetailLevel,
) {
    let rows = class_methods_to_rows(&shape.classes, detail);
    if rows.is_empty() {
        return;
    }

    out.insert("mh".to_string(), json!(detail.class_method_header()));
    out.insert("cm".to_string(), json!(rows));
}

pub(super) fn insert_impl_methods(
    out: &mut Map<String, Value>,
    shape: &EnhancedFileShape,
    detail: DetailLevel,
) {
    let rows = impl_methods_to_rows(&shape.impl_blocks, detail);
    if rows.is_empty() {
        return;
    }

    out.insert("bh".to_string(), json!(detail.impl_method_header()));
    out.insert("bm".to_string(), json!(rows));
}

pub(super) fn functions_to_rows(functions: &[EnhancedFunctionInfo], detail: DetailLevel) -> String {
    functions
        .iter()
        .map(|func| {
            let line = func.line.to_string();

            let mut fields: Vec<String> = Vec::new();
            fields.push(func.name.clone());
            fields.push(line);
            fields.push(func.signature.clone());

            if detail == DetailLevel::Full {
                fields.push(func.doc.clone().unwrap_or_default());
                fields.push(func.code.clone().unwrap_or_default());
            }

            let refs: Vec<&str> = fields.iter().map(|s| s.as_str()).collect();
            format::format_row(&refs)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn structs_to_rows(structs: &[EnhancedStructInfo], detail: DetailLevel) -> String {
    structs
        .iter()
        .map(|s| {
            let mut fields: Vec<String> = Vec::new();
            fields.push(s.name.clone());
            fields.push(s.line.to_string());

            let sig = signature_snippet_from_code(s.code.as_deref());
            fields.push(sig);

            if detail == DetailLevel::Full {
                fields.push(s.doc.clone().unwrap_or_default());
                fields.push(s.code.clone().unwrap_or_default());
            }

            let refs: Vec<&str> = fields.iter().map(|v| v.as_str()).collect();
            format::format_row(&refs)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn classes_to_rows(classes: &[EnhancedClassInfo], detail: DetailLevel) -> String {
    classes
        .iter()
        .map(|c| {
            let mut fields: Vec<String> = Vec::new();
            fields.push(c.name.clone());
            fields.push(c.line.to_string());

            let sig = signature_snippet_from_code(c.code.as_deref());
            fields.push(sig);

            if detail == DetailLevel::Full {
                fields.push(c.doc.clone().unwrap_or_default());
                fields.push(c.code.clone().unwrap_or_default());
            }

            let refs: Vec<&str> = fields.iter().map(|v| v.as_str()).collect();
            format::format_row(&refs)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn imports_to_rows(imports: &[ImportInfo]) -> String {
    imports
        .iter()
        .map(|import| {
            let line = import.line.to_string();
            let fields = [line.as_str(), import.text.as_str()];
            format::format_row(&fields)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn interfaces_to_rows(interfaces: &[InterfaceInfo], detail: DetailLevel) -> String {
    interfaces
        .iter()
        .map(|interface| {
            let mut fields: Vec<String> = Vec::new();
            fields.push(interface.name.clone());
            fields.push(interface.line.to_string());

            let sig = signature_snippet_from_code(interface.code.as_deref());
            fields.push(sig);

            if detail == DetailLevel::Full {
                fields.push(interface.doc.clone().unwrap_or_default());
                fields.push(interface.code.clone().unwrap_or_default());
            }

            let refs: Vec<&str> = fields.iter().map(|s| s.as_str()).collect();
            format::format_row(&refs)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn properties_to_rows(properties: &[PropertyInfo]) -> String {
    properties
        .iter()
        .map(|prop| {
            let line = prop.line.to_string();
            let prop_type = prop.property_type.clone().unwrap_or_default();
            let doc = prop.doc.clone().unwrap_or_default();

            let owned = [prop.name.clone(), line, prop_type, doc];
            let refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
            format::format_row(&refs)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn class_implements_to_rows(classes: &[EnhancedClassInfo]) -> String {
    let mut rows = Vec::new();

    for class in classes {
        for iface in &class.implements {
            let owned = [class.name.clone(), iface.clone()];
            let refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
            rows.push(format::format_row(&refs));
        }
    }

    rows.join("\n")
}

pub(super) fn trait_methods_to_rows(traits: &[TraitInfo], detail: DetailLevel) -> String {
    let mut rows = Vec::new();

    for tr in traits {
        for method in &tr.methods {
            rows.push(trait_method_to_row(tr.name.as_str(), method, detail));
        }
    }

    rows.join("\n")
}

pub(super) fn class_methods_to_rows(classes: &[EnhancedClassInfo], detail: DetailLevel) -> String {
    let mut rows = Vec::new();

    for class in classes {
        for method in &class.methods {
            rows.push(class_method_to_row(class.name.as_str(), method, detail));
        }
    }

    rows.join("\n")
}

pub(super) fn class_method_to_row(
    class_name: &str,
    method: &EnhancedFunctionInfo,
    detail: DetailLevel,
) -> String {
    let line = method.line.to_string();
    let ann = if method.annotations.is_empty() {
        String::new()
    } else {
        method.annotations.join(",")
    };

    if detail == DetailLevel::Full {
        let owned = [
            class_name.to_string(),
            method.name.clone(),
            line,
            method.signature.clone(),
            ann,
            method.doc.clone().unwrap_or_default(),
            method.code.clone().unwrap_or_default(),
        ];
        let refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
        return format::format_row(&refs);
    }

    let owned = [
        class_name.to_string(),
        method.name.clone(),
        line,
        method.signature.clone(),
        ann,
    ];
    let refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
    format::format_row(&refs)
}

pub(super) fn impl_methods_to_rows(impl_blocks: &[ImplBlockInfo], detail: DetailLevel) -> String {
    let mut rows = Vec::new();

    for block in impl_blocks {
        for method in &block.methods {
            rows.push(impl_method_to_row(block, method, detail));
        }
    }

    rows.join("\n")
}

pub(super) fn impl_method_to_row(
    block: &ImplBlockInfo,
    method: &MethodInfo,
    detail: DetailLevel,
) -> String {
    let line = method.line.to_string();
    let trait_name = block.trait_name.clone().unwrap_or_default();

    if detail == DetailLevel::Full {
        let owned = [
            block.type_name.clone(),
            trait_name,
            method.name.clone(),
            line,
            method.signature.clone(),
            method.doc.clone().unwrap_or_default(),
            method.code.clone().unwrap_or_default(),
        ];
        let refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
        return format::format_row(&refs);
    }

    let owned = [
        block.type_name.clone(),
        trait_name,
        method.name.clone(),
        line,
        method.signature.clone(),
    ];
    let refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
    format::format_row(&refs)
}

pub(super) fn trait_method_to_row(
    trait_name: &str,
    method: &MethodInfo,
    detail: DetailLevel,
) -> String {
    let line = method.line.to_string();

    if detail == DetailLevel::Full {
        let owned = [
            trait_name.to_string(),
            method.name.clone(),
            line,
            method.signature.clone(),
            method.doc.clone().unwrap_or_default(),
            method.code.clone().unwrap_or_default(),
        ];
        let refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
        return format::format_row(&refs);
    }

    let owned = [
        trait_name.to_string(),
        method.name.clone(),
        line,
        method.signature.clone(),
    ];
    let refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
    format::format_row(&refs)
}

pub(super) fn signature_snippet_from_code(code: Option<&str>) -> String {
    let Some(code) = code else {
        return String::new();
    };

    code.lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
        .unwrap_or_default()
}

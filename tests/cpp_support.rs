use serde_json::{json, Value};
use std::path::PathBuf;
use treesitter_mcp::analysis::{
    call_graph, code_map, diff, find_usages, minimal_edit_context, shape, symbol_at_line, type_map,
    view_code,
};
use treesitter_mcp::mcp_types::CallToolResult;
use treesitter_mcp::parser::{detect_language, parse_code, Language};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from("tests/fixtures/cpp_terrain").join(name)
}

#[test]
fn cpp_call_graph_and_quoted_include_resolution_work() {
    let path = fixture("terrain.cpp").canonicalize().unwrap();
    let source = std::fs::read_to_string(&path).unwrap();
    let dependencies = treesitter_mcp::analysis::dependencies::resolve_dependencies(
        Language::Cpp,
        &source,
        &path,
        path.parent().unwrap(),
    );
    assert_eq!(
        dependencies,
        vec![fixture("terrain.hpp").canonicalize().unwrap()]
    );
    let callees = output(
        call_graph::execute(&json!({
            "file_path":path,"symbol":"game::Terrain::update","direction":"callees",
        }))
        .unwrap(),
    );
    assert!(callees.to_string().contains("twice"), "{callees}");
    let callers = output(
        call_graph::execute(&json!({
            "file_path":path,"symbol":"game::twice","direction":"callers",
        }))
        .unwrap(),
    );
    assert!(callers.to_string().contains("update"), "{callers}");
}

#[test]
fn cpp_structural_diff_tracks_method_bodies_and_record_fields() {
    let temp = tempfile::tempdir().unwrap();
    for file in ["terrain.cpp", "terrain.hpp"] {
        std::fs::copy(fixture(file), temp.path().join(file)).unwrap();
    }
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(temp.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(&["init", "-b", "main"]);
    git(&["add", "terrain.cpp", "terrain.hpp"]);
    git(&[
        "-c",
        "user.name=MCP Test",
        "-c",
        "user.email=mcp-test@example.invalid",
        "commit",
        "-m",
        "baseline",
    ]);
    let source = temp.path().join("terrain.cpp");
    let content = std::fs::read_to_string(&source)
        .unwrap()
        .replace("twice(value)", "twice(value + 1)");
    std::fs::write(&source, content).unwrap();
    let changes =
        output(diff::execute_parse_diff(&json!({"file_path":source,"compare_to":"HEAD"})).unwrap());
    assert!(
        changes.to_string().contains("game::Terrain::update"),
        "{changes}"
    );
    let header = temp.path().join("terrain.hpp");
    let content = std::fs::read_to_string(&header)
        .unwrap()
        .replace("float height;", "double height;");
    std::fs::write(&header, content).unwrap();
    let changes =
        output(diff::execute_parse_diff(&json!({"file_path":header,"compare_to":"HEAD"})).unwrap());
    assert!(changes.to_string().contains("game::Sample"), "{changes}");
}

fn output(result: CallToolResult) -> Value {
    let value = serde_json::to_value(result).unwrap();
    let text = value["content"][0]["text"].as_str().unwrap();
    serde_json::from_str(text).unwrap()
}

#[test]
fn cpp_grammar_extracts_header_declarations_templates_and_members() {
    let path = fixture("terrain.hpp");
    let source = std::fs::read_to_string(&path).unwrap();
    assert_eq!(detect_language(&path).unwrap(), Language::Cpp);
    let tree = parse_code(&source, Language::Cpp).unwrap();
    assert!(!tree.root_node().has_error());
    let shape = shape::extract_enhanced_shape(&tree, &source, Language::Cpp, None, true).unwrap();
    let terrain = shape
        .classes
        .iter()
        .find(|class| class.name == "game::Terrain")
        .unwrap();
    for name in ["Terrain", "~Terrain", "update", "size"] {
        assert!(
            terrain.methods.iter().any(|method| method.name == name),
            "missing {name}"
        );
    }
    assert!(terrain.fields.iter().any(|field| field.name == "count"));
    let buffer = shape
        .classes
        .iter()
        .find(|class| class.name == "game::Buffer")
        .unwrap();
    assert!(buffer
        .code
        .as_ref()
        .unwrap()
        .starts_with("template<class T>"));
    assert!(shape
        .functions
        .iter()
        .any(|function| function.name == "game::twice"));
    assert!(shape
        .imports
        .iter()
        .any(|import| import.text.contains("<vector>")));
}

#[test]
fn cpp_tools_return_source_symbols_usage_scope_and_edit_context() {
    let source = fixture("terrain.cpp");
    let path = source.to_str().unwrap();
    let view =
        output(view_code::execute(&json!({"file_path":path,"detail":"signatures"})).unwrap());
    assert!(view["f"]
        .as_str()
        .unwrap()
        .contains("game::Terrain::update"));
    let usages = output(
        find_usages::execute(&json!({"path":path,"symbol":"twice","context_lines":0})).unwrap(),
    );
    let rows = usages["u"].as_str().unwrap();
    assert!(rows.contains("call"), "{usages}");
    assert!(rows.contains("game::Terrain::update"), "{usages}");
    let symbol =
        output(symbol_at_line::execute(&json!({"file_path":path,"line":8,"column":6})).unwrap());
    assert_eq!(symbol["sym"], "Terrain::update");
    let context = output(
        minimal_edit_context::execute(
            &json!({"file_path":path,"symbol_name":"game::Terrain::update","max_tokens":2000}),
        )
        .unwrap(),
    );
    assert!(context["target"].as_str().unwrap().contains("twice(value)"));
}

#[test]
fn cpp_code_and_type_maps_preserve_named_types() {
    let root = fixture("");
    let types = output(type_map::execute(&json!({"path":root,"count_usages":false})).unwrap());
    let rows = types["types"].as_str().unwrap();
    for name in [
        "game::Terrain",
        "game::Sample",
        "game::Mode",
        "game::Samples",
        "game::Buffer",
    ] {
        assert!(rows.contains(name), "missing {name}: {types}");
    }
    let map = output(
        code_map::execute(
            &json!({"path":root,"detail":"signatures","with_types":true,"max_tokens":5000}),
        )
        .unwrap(),
    );
    assert!(map.to_string().contains("game::Terrain"));
}

#[test]
fn c_uses_c_grammar_and_supports_functions_and_structs() {
    assert_eq!(detect_language("sample.c").unwrap(), Language::C);
    for extension in ["h", "hpp", "hh", "hxx", "cpp", "cc", "cxx", "C"] {
        assert_eq!(
            detect_language(format!("sample.{extension}")).unwrap(),
            Language::Cpp
        );
    }
    let source =
        "struct Sample { int value; }; int read(struct Sample *sample) { return sample->value; }";
    let tree = parse_code(source, Language::C).unwrap();
    assert!(!tree.root_node().has_error());
    let shape = shape::extract_enhanced_shape(&tree, source, Language::C, None, true).unwrap();
    assert!(shape.classes.iter().any(|class| class.name == "Sample"));
    assert!(shape
        .functions
        .iter()
        .any(|function| function.name == "read"));
}

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use treesitter_mcp::analysis::{
    call_graph, code_map, minimal_edit_context, review_context, shape, view_code,
};
use treesitter_mcp::mcp_types::CallToolResult;
use treesitter_mcp::parser::{parse_code, Language};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/go_receivers")
        .join(name)
}

fn output(result: CallToolResult) -> Value {
    let value = serde_json::to_value(result).unwrap();
    serde_json::from_str(value["content"][0]["text"].as_str().unwrap()).unwrap()
}

#[test]
fn go_receiver_shape_keeps_value_pointer_and_generic_owners_without_local_types() {
    let source = std::fs::read_to_string(fixture("methods.go")).unwrap();
    let tree = parse_code(&source, Language::Go).unwrap();
    assert!(!tree.root_node().has_error());
    let shape = shape::extract_enhanced_shape(&tree, &source, Language::Go, None, true).unwrap();
    assert!(shape.structs.is_empty());
    let planner = shape
        .impl_blocks
        .iter()
        .find(|block| block.type_name == "Planner")
        .unwrap();
    assert!(planner
        .methods
        .iter()
        .any(|method| method.name == "buildPodman" && method.signature.contains("(p Planner)")));
    assert!(planner
        .methods
        .iter()
        .any(|method| method.name == "Reset" && method.signature.contains("(p *Planner)")));
    assert!(shape
        .impl_blocks
        .iter()
        .any(|block| block.type_name == "Box[T]"
            && block.methods.iter().any(|method| method.name == "Value")));
}

#[test]
fn go_receiver_methods_reach_focus_map_context_and_call_graph() {
    let path = fixture("methods.go");
    let view = output(
        view_code::execute(
            &json!({"file_path":path,"detail":"full","focus_symbol":"Planner::buildPodman"}),
        )
        .unwrap(),
    );
    let rows = view["bm"].as_str().unwrap();
    let target = rows
        .lines()
        .find(|row| row.contains("|buildPodman|"))
        .unwrap();
    assert!(target.contains("twice(value)"), "{view}");
    assert!(
        !rows
            .lines()
            .find(|row| row.contains("|Reset|"))
            .unwrap()
            .contains("p.Base = 0"),
        "{view}"
    );
    for symbol in ["buildPodman", "Planner::buildPodman"] {
        let context = output(
            minimal_edit_context::execute(&json!({"file_path":path,"symbol_name":symbol})).unwrap(),
        );
        assert_eq!(context["scope"], "Planner", "{context}");
        assert!(
            context["target"].as_str().unwrap().contains("twice(value)"),
            "{context}"
        );
    }
    let map = output(code_map::execute(&json!({"path":path,"detail":"signatures"})).unwrap());
    assert!(map.to_string().contains("Planner::buildPodman"), "{map}");
    let graph = output(
        call_graph::execute(
            &json!({"file_path":path,"symbol_name":"Planner::buildPodman","direction":"callees"}),
        )
        .unwrap(),
    );
    assert!(
        graph["edges"].as_str().unwrap().contains("callee|twice|"),
        "{graph}"
    );
}

#[test]
fn review_context_handles_a_changed_receiver_method_with_its_type_in_another_file() {
    let temp = tempfile::tempdir().unwrap();
    for name in ["go.mod", "types.go", "methods.go"] {
        std::fs::copy(fixture(name), temp.path().join(name)).unwrap();
    }
    let git = |args: &[&str]| {
        assert!(std::process::Command::new("git")
            .args(args)
            .current_dir(temp.path())
            .output()
            .unwrap()
            .status
            .success());
    };
    git(&["init", "-b", "main"]);
    git(&["add", "go.mod", "types.go", "methods.go"]);
    git(&[
        "-c",
        "user.name=MCP Test",
        "-c",
        "user.email=test@example.invalid",
        "commit",
        "-m",
        "baseline",
    ]);
    let path = temp.path().join("methods.go");
    let changed = std::fs::read_to_string(&path)
        .unwrap()
        .replace("twice(value)", "twice(value + 1)");
    std::fs::write(&path, changed).unwrap();
    let review = output(
        review_context::execute(&json!({"file_path":path,"scope":temp.path(),"max_tokens":6000}))
            .unwrap(),
    );
    assert!(
        review["changes"].as_str().unwrap().contains("buildPodman"),
        "{review}"
    );
    assert_eq!(review["ctx"]["buildPodman"]["scope"], "Planner", "{review}");
}

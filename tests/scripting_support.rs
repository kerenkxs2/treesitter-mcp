use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use treesitter_mcp::analysis::{
    call_graph, code_map, dependencies, diff, find_usages, minimal_edit_context, shape,
    symbol_at_line, type_map, view_code,
};
use treesitter_mcp::mcp_types::CallToolResult;
use treesitter_mcp::parser::{detect_language, parse_code};

fn fixture(language: &str, name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(format!("{language}_project"))
        .join(name)
}

fn output(result: CallToolResult) -> Value {
    let value = serde_json::to_value(result).unwrap();
    serde_json::from_str(value["content"][0]["text"].as_str().unwrap()).unwrap()
}

#[test]
fn scripting_structures_signatures_imports_and_local_dependencies() {
    for (language, name, dependency) in [
        ("gdscript", "player.gd", "math.gdscript"),
        ("lua", "player.lua", "math_helpers.lua"),
    ] {
        let path = fixture(language, name);
        let source = std::fs::read_to_string(&path).unwrap();
        let language = detect_language(&path).unwrap();
        let tree = parse_code(&source, language).unwrap();
        assert!(
            !tree.root_node().has_error(),
            "{}",
            tree.root_node().to_sexp()
        );
        let shape = shape::extract_enhanced_shape(&tree, &source, language, None, true).unwrap();
        let player = shape
            .classes
            .iter()
            .find(|class| class.name == "Player")
            .unwrap();
        assert!(player
            .methods
            .iter()
            .any(|method| method.name == "update" && method.signature.contains("delta")));
        assert!(player.fields.iter().any(|field| field.name == "speed"));
        assert!(shape
            .imports
            .iter()
            .any(|import| import.text.contains("Math")));
        let deps =
            dependencies::resolve_dependencies(language, &source, &path, path.parent().unwrap());
        assert_eq!(
            deps,
            vec![path
                .parent()
                .unwrap()
                .join(dependency)
                .canonicalize()
                .unwrap()]
        );
        let result = output(type_map::execute(&json!({"path":path,"count_usages":false})).unwrap());
        assert!(
            result["types"].as_str().unwrap().contains("Player|"),
            "{result}"
        );
        let result = output(
            code_map::execute(&json!({"path":path.parent(),"detail":"signatures"})).unwrap(),
        );
        assert!(result.to_string().contains("update"), "{result}");
    }
    let path = fixture("gdscript", "math.gdscript");
    assert_eq!(detect_language(path).unwrap().name(), "GDScript");
}

#[test]
fn scripting_generic_navigation_tools_use_real_ast_structure() {
    for (language, file, line) in [("gdscript", "player.gd", 9), ("lua", "player.lua", 11)] {
        let path = fixture(language, file);
        let view = output(
            view_code::execute(&json!({"file_path":path,"detail":"full","focus_symbol":"update"}))
                .unwrap(),
        );
        assert!(view.to_string().contains("twice(delta)"), "{view}");
        let usages = output(
            find_usages::execute(&json!({"path":path,"symbol":"twice","context_lines":0})).unwrap(),
        );
        let rows = usages["u"].as_str().unwrap();
        assert!(rows.contains("|definition|"), "{usages}");
        assert!(rows.contains("|call|"), "{usages}");
        let symbol =
            output(symbol_at_line::execute(&json!({"file_path":path,"line":line})).unwrap());
        assert!(symbol.to_string().contains("update"), "{symbol}");
        let context = output(
            minimal_edit_context::execute(&json!({"file_path":path,"symbol":"update"})).unwrap(),
        );
        assert!(context.to_string().contains("twice"), "{context}");
        let graph = output(
            call_graph::execute(&json!({"file_path":path,"symbol":"update","direction":"callees"}))
                .unwrap(),
        );
        assert!(
            graph["edges"].as_str().unwrap().contains("twice"),
            "{graph}"
        );
        let graph = output(
            call_graph::execute(&json!({"file_path":path,"symbol":"twice","direction":"callers"}))
                .unwrap(),
        );
        assert!(
            graph["edges"].as_str().unwrap().contains("update"),
            "{graph}"
        );
    }
}

#[test]
fn lua_assigned_and_table_functions_have_names_and_signatures() {
    let path = fixture("lua", "player.lua");
    let source = std::fs::read_to_string(path).unwrap();
    let language = detect_language("/example/player.lua").unwrap();
    let tree = parse_code(&source, language).unwrap();
    let shape = shape::extract_enhanced_shape(&tree, &source, language, None, false).unwrap();
    for (owner, method) in [("Player", "reset"), ("Tools", "clamp")] {
        assert!(shape.classes.iter().any(|class| class.name == owner
            && class
                .methods
                .iter()
                .any(|m| m.name == method && m.signature.contains("function"))));
    }
}

#[test]
fn scripting_structural_diff_tracks_method_edits() {
    for (language, name) in [("gdscript", "player.gd"), ("lua", "player.lua")] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(name);
        std::fs::copy(fixture(language, name), &path).unwrap();
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
        git(&["add", name]);
        git(&[
            "-c",
            "user.name=MCP Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-m",
            "baseline",
        ]);
        let source = std::fs::read_to_string(&path)
            .unwrap()
            .replace("twice(delta)", "twice(delta + 1)");
        std::fs::write(&path, source).unwrap();
        let changes = output(
            diff::execute_parse_diff(&json!({"file_path":path,"compare_to":"HEAD"})).unwrap(),
        );
        assert!(
            changes["changes"]
                .as_str()
                .unwrap()
                .contains("Player::update"),
            "{changes}"
        );
    }
}

#[test]
fn tsx_uses_its_grammar_and_malformed_scripts_keep_discoverable_symbols() {
    let path = fixture("typescript", "panel.tsx");
    let source = std::fs::read_to_string(&path).unwrap();
    let language = detect_language(&path).unwrap();
    let tree = parse_code(&source, language).unwrap();
    assert!(
        !tree.root_node().has_error(),
        "{}",
        tree.root_node().to_sexp()
    );
    let view =
        output(view_code::execute(&json!({"file_path":path,"detail":"signatures"})).unwrap());
    assert!(view.to_string().contains("panel"), "{view}");
    for (path, source) in [
        (
            "/fixture/player.gd",
            "func intact(value: int) -> int:\n    return value\n\nfunc broken(\n",
        ),
        (
            "/fixture/player.lua",
            "local function intact(value)\n return value\nend\nfunction broken(\n",
        ),
    ] {
        let language = detect_language(path).unwrap();
        let tree = parse_code(source, language).unwrap();
        assert!(tree.root_node().has_error());
        let shape = shape::extract_enhanced_shape(&tree, source, language, None, false).unwrap();
        assert!(shape
            .functions
            .iter()
            .any(|function| function.name == "intact"));
    }
}

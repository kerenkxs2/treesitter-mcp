// Command smoke exercises the real MCP stdio protocol with existing language fixtures.
package main

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"
)

type response struct {
	ID     int             `json:"id"`
	Result json.RawMessage `json:"result"`
	Error  json.RawMessage `json:"error"`
}
type client struct {
	input    io.WriteCloser
	encoder  *json.Encoder
	messages chan response
	errors   chan error
	nextID   int
}

func (c *client) request(method string, params any) (json.RawMessage, error) {
	c.nextID++
	if err := c.encoder.Encode(map[string]any{"jsonrpc": "2.0", "id": c.nextID, "method": method, "params": params}); err != nil {
		return nil, err
	}
	deadline := time.NewTimer(45 * time.Second)
	defer deadline.Stop()
	for {
		select {
		case msg := <-c.messages:
			if msg.ID != c.nextID {
				continue
			}
			if len(msg.Error) > 0 && string(msg.Error) != "null" {
				return nil, fmt.Errorf("%s: %s", method, msg.Error)
			}
			return msg.Result, nil
		case err := <-c.errors:
			return nil, err
		case <-deadline.C:
			return nil, fmt.Errorf("%s exceeded 45 seconds", method)
		}
	}
}

func (c *client) tool(name string, args map[string]any, marker string) error {
	raw, err := c.request("tools/call", map[string]any{"name": name, "arguments": args})
	if err != nil {
		return err
	}
	var result struct {
		IsError bool `json:"isError"`
		Content []struct {
			Type string `json:"type"`
			Text string `json:"text"`
		} `json:"content"`
	}
	if err = json.Unmarshal(raw, &result); err != nil {
		return err
	}
	if result.IsError {
		return fmt.Errorf("%s returned error: %s", name, raw)
	}
	var text strings.Builder
	for _, content := range result.Content {
		if content.Type == "text" {
			text.WriteString(content.Text)
		}
	}
	if !strings.Contains(text.String(), marker) {
		return fmt.Errorf("%s response does not contain %q: %.500s", name, marker, text.String())
	}
	fmt.Printf("PASS %s: %s\n", name, marker)
	return nil
}

func run(binary, root, voxel string) error {
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	command := exec.CommandContext(ctx, binary)
	command.WaitDelay = 3 * time.Second
	command.Dir = root
	var stderr bytes.Buffer
	command.Stderr = &stderr
	input, err := command.StdinPipe()
	if err != nil {
		return err
	}
	output, err := command.StdoutPipe()
	if err != nil {
		return err
	}
	if err = command.Start(); err != nil {
		return err
	}
	defer func() { _ = input.Close(); cancel(); _ = command.Wait() }()
	c := &client{input: input, encoder: json.NewEncoder(input), messages: make(chan response, 8), errors: make(chan error, 1)}
	go func() {
		scanner := bufio.NewScanner(output)
		scanner.Buffer(make([]byte, 65536), 16*1024*1024)
		for scanner.Scan() {
			var msg response
			if err := json.Unmarshal(scanner.Bytes(), &msg); err != nil {
				c.errors <- err
				return
			}
			if msg.ID != 0 {
				c.messages <- msg
			}
		}
		if err := scanner.Err(); err != nil {
			c.errors <- err
		} else {
			c.errors <- fmt.Errorf("MCP stdout closed: %s", stderr.String())
		}
	}()
	_, err = c.request("initialize", map[string]any{"protocolVersion": "2025-06-18", "capabilities": map[string]any{}, "clientInfo": map[string]any{"name": "treesitter-native-smoke", "version": "1"}})
	if err != nil {
		return err
	}
	if err = c.encoder.Encode(map[string]any{"jsonrpc": "2.0", "method": "notifications/initialized"}); err != nil {
		return err
	}
	fixtures := []struct{ path, marker string }{
		{"tests/fixtures/python_project/calculator.py", "add"},
		{"tests/fixtures/go_project/main.go", "main"},
		{"tests/fixtures/rust_project/src/calculator.rs", "add"},
		{"tests/fixtures/typescript_project/calculator.ts", "add"},
		{"tests/fixtures/typescript_project/panel.tsx", "panel"},
		{"tests/fixtures/gdscript_project/player.gd", "update"},
		{"tests/fixtures/gdscript_project/math.gdscript", "scale"},
		{"tests/fixtures/lua_project/player.lua", "local Math = require"},
		{"tests/fixtures/cpp_terrain/terrain.hpp", "game::Terrain"},
		{"tests/fixtures/cpp_terrain/terrain.cpp", "game::Terrain::update"},
	}
	for _, fixture := range fixtures {
		if err = c.tool("view_code", map[string]any{"file_path": filepath.Join(root, fixture.path), "detail": "signatures"}, fixture.marker); err != nil {
			return err
		}
	}
	for _, fixture := range []struct{ path, typeName string }{
		{"tests/fixtures/typescript_project/navigation.ts", "Counter"},
		{"tests/fixtures/rust_project/src/navigation.rs", "Counter"},
		{"tests/fixtures/go_project/navigation.go", "Counter"},
		{"tests/fixtures/gdscript_project/player.gd", "Player"},
		{"tests/fixtures/lua_project/player.lua", "Player"},
	} {
		path := filepath.Join(root, fixture.path)
		for _, call := range []struct {
			name   string
			args   map[string]any
			marker string
		}{
			{"view_code", map[string]any{"file_path": path, "detail": "full", "focus_symbol": "update"}, "twice(delta)"},
			{"code_map", map[string]any{"path": path, "detail": "signatures", "with_types": true}, "update"},
			{"find_usages", map[string]any{"path": path, "symbol": "twice", "context_lines": 0}, "|call|"},
			{"type_map", map[string]any{"path": path, "count_usages": false}, fixture.typeName},
			{"minimal_edit_context", map[string]any{"file_path": path, "symbol_name": "update"}, "twice"},
			{"call_graph", map[string]any{"file_path": path, "symbol_name": "update", "direction": "callees"}, "callee|twice|"},
		} {
			if err = c.tool(call.name, call.args, call.marker); err != nil {
				return fmt.Errorf("%s: %w", path, err)
			}
		}
	}
	cpp := filepath.Join(root, "tests/fixtures/cpp_terrain")
	receivers := filepath.Join(root, "tests/fixtures/go_receivers/methods.go")
	for _, call := range []struct {
		name   string
		args   map[string]any
		marker string
	}{
		{"view_code", map[string]any{"file_path": receivers, "detail": "full", "focus_symbol": "Planner::buildPodman"}, "twice(value)"},
		{"code_map", map[string]any{"path": receivers, "detail": "signatures"}, "Planner::buildPodman"},
		{"minimal_edit_context", map[string]any{"file_path": receivers, "symbol_name": "Planner::buildPodman"}, "Planner"},
		{"call_graph", map[string]any{"file_path": receivers, "symbol_name": "Planner::buildPodman", "direction": "callees"}, "callee|twice|"},
		{"find_usages", map[string]any{"path": receivers, "symbol": "buildPodman", "context_lines": 0}, "|call|"},
	} {
		if err = c.tool(call.name, call.args, call.marker); err != nil {
			return err
		}
	}
	for _, call := range []struct {
		name   string
		args   map[string]any
		marker string
	}{
		{"code_map", map[string]any{"path": cpp, "detail": "signatures", "with_types": true, "max_tokens": 5000}, "game::Terrain"},
		{"find_usages", map[string]any{"path": filepath.Join(cpp, "terrain.cpp"), "symbol": "twice", "context_lines": 1}, "call"},
		{"type_map", map[string]any{"path": cpp, "count_usages": false}, "game::Sample"},
		{"minimal_edit_context", map[string]any{"file_path": filepath.Join(cpp, "terrain.cpp"), "symbol_name": "game::Terrain::update"}, "twice(value)"},
		{"call_graph", map[string]any{"file_path": filepath.Join(cpp, "terrain.cpp"), "symbol_name": "game::Terrain::update", "direction": "callees"}, "twice"},
	} {
		if err = c.tool(call.name, call.args, call.marker); err != nil {
			return err
		}
	}
	if voxel != "" {
		directory := filepath.Join(voxel, "terrain/variable_lod")
		source := filepath.Join(directory, "voxel_lod_terrain.cpp")
		header := filepath.Join(directory, "voxel_lod_terrain.h")
		for _, call := range []struct {
			name   string
			args   map[string]any
			marker string
		}{
			{"view_code", map[string]any{"file_path": header, "detail": "signatures"}, "VoxelLodTerrain"},
			{"view_code", map[string]any{"file_path": source, "detail": "signatures"}, "VoxelLodTerrain::set_material"},
			{"code_map", map[string]any{"path": directory, "pattern": "voxel_lod_terrain.*", "detail": "signatures", "with_types": true, "max_tokens": 5000}, "VoxelLodTerrain"},
			{"find_usages", map[string]any{"path": source, "symbol": "set_material", "context_lines": 1, "max_tokens": 3000}, "set_material"},
			{"type_map", map[string]any{"path": header, "count_usages": false}, "VoxelLodTerrain"},
		} {
			if err = c.tool(call.name, call.args, call.marker); err != nil {
				return err
			}
		}
	}
	fmt.Println("MCP stdio smoke passed; the test server is separate from connected agent sessions.")
	return nil
}

func main() {
	binary := flag.String("binary", "", "MCP executable path (required)")
	root := flag.String("root", "", "repository root containing fixtures (required)")
	voxel := flag.String("voxel-root", "", "optional existing Voxel Tools checkout for real-file validation")
	flag.Parse()
	if *binary == "" || *root == "" {
		fmt.Fprintln(os.Stderr, "--binary and --root are required")
		os.Exit(2)
	}
	binaryPath, err := filepath.Abs(*binary)
	if err == nil {
		*root, err = filepath.Abs(*root)
	}
	if err == nil && *voxel != "" {
		*voxel, err = filepath.Abs(*voxel)
	}
	if err == nil {
		err = run(binaryPath, *root, *voxel)
	}
	if err != nil && !errors.Is(err, context.Canceled) {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

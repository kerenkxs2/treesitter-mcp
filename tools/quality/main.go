// Command quality prevents oversized maintained files in the current change.
package main

import (
	"bytes"
	"flag"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

func changed(root string) ([]string, error) {
	names := map[string]bool{}
	for _, args := range [][]string{{"diff", "--name-only", "-z", "HEAD"}, {"ls-files", "--others", "--exclude-standard", "-z"}} {
		command := exec.Command("git", args...)
		command.Dir = root
		output, err := command.Output()
		if err != nil {
			return nil, err
		}
		for _, name := range bytes.Split(output, []byte{0}) {
			if len(name) > 0 {
				names[string(name)] = true
			}
		}
	}
	result := make([]string, 0, len(names))
	for name := range names {
		result = append(result, name)
	}
	return result, nil
}

func run(root string) error {
	paths, err := changed(root)
	if err != nil {
		return err
	}
	checked := 0
	for _, name := range paths {
		if strings.HasPrefix(name, ".serena/") {
			continue
		}
		extension := strings.ToLower(filepath.Ext(name))
		switch extension {
		case ".rs", ".go", ".c", ".cpp", ".cc", ".cxx", ".h", ".hpp", ".hh", ".hxx":
		default:
			if filepath.Base(name) != "Taskfile.yml" {
				continue
			}
		}
		path := filepath.Join(root, name)
		info, err := os.Stat(path)
		if os.IsNotExist(err) {
			continue
		}
		if err != nil {
			return err
		}
		if !info.Mode().IsRegular() {
			continue
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		lines := bytes.Count(data, []byte{'\n'})
		if len(data) > 0 && data[len(data)-1] != '\n' {
			lines++
		}
		if lines > 500 {
			return fmt.Errorf("%s has %d lines; changed maintained sources must be at most 500", name, lines)
		}
		checked++
	}
	fmt.Printf("Source size check passed for %d changed maintained files.\n", checked)
	return nil
}

func main() {
	root := flag.String("root", "", "repository root (required)")
	flag.Parse()
	if *root == "" {
		fmt.Fprintln(os.Stderr, "--root is required")
		os.Exit(2)
	}
	if err := run(*root); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

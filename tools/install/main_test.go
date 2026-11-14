package main

import (
	"io"
	"os"
	"path/filepath"
	"testing"
)

func TestAtomicInstallPreservesOpenExecutableInode(t *testing.T) {
	root := t.TempDir()
	source := filepath.Join(root, "staged")
	destination := filepath.Join(root, "registered")
	if err := os.WriteFile(source, []byte("new executable"), 0755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(destination, []byte("old executable"), 0755); err != nil {
		t.Fatal(err)
	}
	old, err := os.Open(destination)
	if err != nil {
		t.Fatal(err)
	}
	defer old.Close()
	if err := install(source, destination); err != nil {
		t.Fatal(err)
	}
	retained, err := io.ReadAll(old)
	if err != nil {
		t.Fatal(err)
	}
	if string(retained) != "old executable" {
		t.Fatalf("old client inode changed: %q", retained)
	}
	published, err := os.ReadFile(destination)
	if err != nil {
		t.Fatal(err)
	}
	if string(published) != "new executable" {
		t.Fatalf("wrong published executable: %q", published)
	}
	if err := install(source, source); err == nil {
		t.Fatal("accepted same source/destination")
	}
}

// Command install atomically publishes a staged MCP executable without disturbing running clients.
package main

import (
	"crypto/sha256"
	"flag"
	"fmt"
	"io"
	"os"
	"path/filepath"
)

func install(source, destination string) error {
	if !filepath.IsAbs(source) || !filepath.IsAbs(destination) {
		return fmt.Errorf("--source and --destination must be absolute paths")
	}
	if source == destination {
		return fmt.Errorf("source and destination must differ")
	}
	input, err := os.Open(source)
	if err != nil {
		return err
	}
	defer input.Close()
	info, err := input.Stat()
	if err != nil {
		return err
	}
	if !info.Mode().IsRegular() || info.Mode().Perm()&0111 == 0 {
		return fmt.Errorf("source must be a regular executable")
	}
	if info, err := os.Lstat(destination); err == nil {
		if !info.Mode().IsRegular() {
			return fmt.Errorf("destination must be a regular file")
		}
	} else if !os.IsNotExist(err) {
		return err
	}
	directory := filepath.Dir(destination)
	if err = os.MkdirAll(directory, 0755); err != nil {
		return err
	}
	staged, err := os.CreateTemp(directory, ".treesitter-mcp-install-*")
	if err != nil {
		return err
	}
	defer os.Remove(staged.Name())
	defer staged.Close()
	hash := sha256.New()
	if _, err = io.Copy(io.MultiWriter(staged, hash), input); err != nil {
		return err
	}
	if err = staged.Chmod(0755); err != nil {
		return err
	}
	if err = staged.Sync(); err != nil {
		return err
	}
	if err = staged.Close(); err != nil {
		return err
	}
	if err = os.Rename(staged.Name(), destination); err != nil {
		return err
	}
	dir, err := os.Open(directory)
	if err != nil {
		return err
	}
	defer dir.Close()
	if err = dir.Sync(); err != nil {
		return err
	}
	fmt.Printf("Installed %s\nSHA256 %x\nExisting MCP clients retain their executable; reconnect to load this build.\n", destination, hash.Sum(nil))
	return nil
}

func main() {
	source := flag.String("source", "", "absolute staged executable path")
	destination := flag.String("destination", "", "absolute registered executable path")
	flag.Parse()
	if err := install(*source, *destination); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

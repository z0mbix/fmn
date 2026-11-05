package main

import (
	"bytes"
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"slices"
	"strconv"
	"strings"

	"github.com/pelletier/go-toml/v2/unstable"
	"github.com/z0mbix/fmn/internal/search"
)

func savePreference(path string, change search.Change) error {
	if info, err := os.Lstat(path); err == nil && info.Mode()&os.ModeSymlink != 0 {
		target, err := filepath.EvalSymlinks(path)
		if err != nil {
			return err
		}
		path = target
	} else if err != nil && !errors.Is(err, os.ErrNotExist) {
		return err
	}
	contents, err := os.ReadFile(path)
	if err != nil && !errors.Is(err, os.ErrNotExist) {
		return err
	}
	if _, err := parseConfig(contents); err != nil {
		return fmt.Errorf("invalid config: %w", err)
	}
	updated := contents
	if bindings, ok := change.Value.(search.Bindings); ok {
		values := bindings.ConfigValues()
		keys := make([]string, 0, len(values))
		for key := range values {
			keys = append(keys, key)
		}
		slices.Sort(keys)
		for _, key := range keys {
			updated, err = editConfig(updated, []string{"ui", "keybindings", key}, strconv.Quote(values[key]))
			if err != nil {
				return err
			}
		}
	} else {
		var value string
		switch v := change.Value.(type) {
		case string:
			value = strconv.Quote(v)
		case bool:
			value = strconv.FormatBool(v)
		default:
			return fmt.Errorf("unsupported preference %q", change.Key)
		}
		updated, err = editConfig(updated, append([]string{"ui"}, strings.Split(change.Key, ".")...), value)
		if err != nil {
			return err
		}
	}
	if _, err := parseConfig(updated); err != nil {
		return fmt.Errorf("invalid updated config: %w", err)
	}
	if bytes.Equal(contents, updated) {
		return nil
	}
	mode := fs.FileMode(0o600)
	if info, err := os.Stat(path); err == nil {
		mode = info.Mode().Perm()
		if mode&0o222 == 0 {
			return errors.New("configuration file is read-only")
		}
	} else if !errors.Is(err, os.ErrNotExist) {
		return err
	}
	parent := filepath.Dir(path)
	if err := os.MkdirAll(parent, 0o700); err != nil {
		return err
	}
	temp, err := os.CreateTemp(parent, ".fmn-config-")
	if err != nil {
		return err
	}
	defer os.Remove(temp.Name())
	defer temp.Close()
	if err := temp.Chmod(mode); err != nil {
		return err
	}
	if _, err := temp.Write(updated); err != nil {
		return err
	}
	if err := temp.Sync(); err != nil {
		return err
	}
	if err := temp.Close(); err != nil {
		return err
	}
	return os.Rename(temp.Name(), path)
}

type editLocation struct {
	offset, length, depth      int
	inline, hasValues, replace bool
}

func keyParts(node *unstable.Node) []string {
	var keys []string
	iter := node.Key()
	for iter.Next() {
		keys = append(keys, string(iter.Node().Data))
	}
	return keys
}

func prefix(path, start []string) bool {
	return len(path) >= len(start) && slices.Equal(path[:len(start)], start)
}

// Locate only the value or insertion point. Leave comments, quoting on other
// values, and the document's table layout intact.
func editConfig(contents []byte, path []string, value string) ([]byte, error) {
	var parser unstable.Parser
	parser.Reset(contents)
	var section []string
	block := editLocation{offset: len(contents), depth: -1}
	found := editLocation{depth: -1}
	rootDotted := false
	rootEnd := len(contents)

	var visit func([]string, *unstable.Node)
	visit = func(parent []string, node *unstable.Node) {
		keys := append(slices.Clone(parent), keyParts(node)...)
		item := node.Value()
		if slices.Equal(keys, path) {
			found = editLocation{offset: int(item.Raw.Offset), length: int(item.Raw.Length), depth: len(keys), replace: true}
			return
		}
		if item.Kind == unstable.InlineTable && prefix(path, keys) {
			found = editLocation{offset: int(item.Raw.Offset) + 1, depth: len(keys), inline: true, hasValues: item.Child() != nil}
			children := item.Children()
			for children.Next() {
				visit(keys, children.Node())
			}
		}
	}
	for parser.NextExpression() {
		node := parser.Expression()
		switch node.Kind {
		case unstable.Table:
			offset := int(node.Child().Raw.Offset)
			start := bytes.LastIndexByte(contents[:offset], '\n') + 1
			if len(section) == 0 {
				rootEnd = start
			}
			if block.depth == len(section) && prefix(path, section) {
				block.offset = start
			}
			section = keyParts(node)
			if prefix(path, section) {
				block = editLocation{offset: len(contents), depth: len(section)}
			}
		case unstable.KeyValue:
			keys := keyParts(node)
			if len(section) == 0 && len(keys) > 1 && keys[0] == path[0] {
				rootDotted = true
			}
			visit(section, node)
		}
	}
	if err := parser.Error(); err != nil {
		return nil, err
	}
	if found.replace {
		return splice(contents, found.offset, found.length, value), nil
	}
	if found.inline {
		text := " " + strings.Join(path[found.depth:], ".") + " = " + value
		if found.hasValues {
			text += ", "
		} else {
			text += " "
		}
		return splice(contents, found.offset, 0, text), nil
	}
	if block.depth < 0 && rootDotted {
		block = editLocation{offset: rootEnd, depth: 0}
	}
	eol := "\n"
	if bytes.Contains(contents, []byte("\r\n")) {
		eol = "\r\n"
	}
	header := ""
	if block.depth < 0 {
		block.depth = 1
		header = "[" + path[0] + "]" + eol
	}
	if block.offset > 0 && contents[block.offset-1] != '\n' {
		header = eol + header
	}
	text := header + strings.Join(path[block.depth:], ".") + " = " + value + eol
	return splice(contents, block.offset, 0, text), nil
}

func splice(contents []byte, offset, length int, value string) []byte {
	out := make([]byte, 0, len(contents)-length+len(value))
	out = append(out, contents[:offset]...)
	out = append(out, value...)
	return append(out, contents[offset+length:]...)
}

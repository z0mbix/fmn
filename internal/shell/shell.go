package shell

import (
	"embed"
	"fmt"
)

//go:embed scripts/*
var scripts embed.FS

func Supported(name string) bool {
	return name == "bash" || name == "zsh" || name == "fish"
}

func Script(name string) (string, error) {
	if !Supported(name) {
		return "", fmt.Errorf("unsupported shell %q (expected bash, zsh, or fish)", name)
	}
	content, err := scripts.ReadFile("scripts/fmn." + name)
	return string(content), err
}

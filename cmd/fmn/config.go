package main

import (
	"bytes"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"

	"github.com/pelletier/go-toml/v2"
	"github.com/z0mbix/fmn/internal/history"
	"github.com/z0mbix/fmn/internal/search"
)

type paths struct {
	config, database, diagnostics string
}

func discoverPaths() (paths, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		return paths{}, err
	}
	configPath, exists := os.LookupEnv("FMN_CONFIG")
	if !exists {
		base, exists := os.LookupEnv("XDG_CONFIG_HOME")
		if !exists {
			base = filepath.Join(home, ".config")
		}
		configPath = filepath.Join(base, "fmn", "config.toml")
	}
	data, exists := os.LookupEnv("FMN_DATA_DIR")
	if !exists {
		base, exists := os.LookupEnv("XDG_DATA_HOME")
		if !exists {
			base = filepath.Join(home, ".local", "share")
		}
		data = filepath.Join(base, "fmn")
	}
	return paths{configPath, filepath.Join(data, "history.db"), filepath.Join(data, "diagnostics.log")}, nil
}

type config struct {
	History struct {
		IgnoreLeadingSpace bool     `toml:"ignore_leading_space"`
		IgnorePatterns     []string `toml:"ignore_patterns"`
	} `toml:"history"`
	UI struct {
		Theme           string            `toml:"theme"`
		ShowInspector   bool              `toml:"show_inspector"`
		ExecuteOnSelect bool              `toml:"execute_on_select"`
		DefaultScope    history.Scope     `toml:"default_scope"`
		Keymap          string            `toml:"keymap"`
		Keybindings     map[string]string `toml:"keybindings"`
	} `toml:"ui"`
}

func loadConfig(path string) (config, error) {
	data, err := os.ReadFile(path)
	if err != nil && !errors.Is(err, os.ErrNotExist) {
		return config{}, err
	}
	cfg, err := parseConfig(data)
	if err != nil {
		return cfg, fmt.Errorf("invalid config %s: %w", path, err)
	}
	return cfg, nil
}

func parseConfig(data []byte) (config, error) {
	var cfg config
	cfg.History.IgnoreLeadingSpace = true
	cfg.UI.ShowInspector = true
	cfg.UI.DefaultScope = history.Global
	cfg.UI.Theme = "mint"
	cfg.UI.Keymap = "control"
	if err := toml.NewDecoder(bytes.NewReader(data)).DisallowUnknownFields().Decode(&cfg); err != nil {
		return cfg, err
	}
	switch cfg.UI.DefaultScope {
	case history.Global, history.Directory, history.Repository, history.Session:
	default:
		return cfg, fmt.Errorf("invalid default scope %q", cfg.UI.DefaultScope)
	}
	if !search.ValidTheme(cfg.UI.Theme) {
		return cfg, fmt.Errorf("invalid theme %q", cfg.UI.Theme)
	}
	if cfg.UI.Keymap != "control" && cfg.UI.Keymap != "function" {
		return cfg, fmt.Errorf("invalid keymap %q", cfg.UI.Keymap)
	}
	if _, err := search.ParseBindings(cfg.UI.Keybindings); err != nil {
		return cfg, err
	}
	return cfg, nil
}

func (c config) shouldRecord(command string) (bool, error) {
	filter, err := c.privacyFilter()
	if err != nil {
		return false, err
	}
	return filter(command), nil
}

func (c config) privacyFilter() (func(string) bool, error) {
	var patterns []*regexp.Regexp
	for _, pattern := range c.History.IgnorePatterns {
		match, err := compileIgnoreGlob(pattern)
		if err != nil {
			return nil, fmt.Errorf("invalid ignore pattern %q: %w", pattern, err)
		}
		patterns = append(patterns, match)
	}
	_, disabled := os.LookupEnv("FMN_DISABLE")
	return func(command string) bool {
		if disabled || strings.TrimSpace(command) == "" || c.History.IgnoreLeadingSpace && strings.HasPrefix(command, " ") {
			return false
		}
		if len(patterns) != 0 {
			candidate := globBytes(command)
			for _, pattern := range patterns {
				if pattern.MatchString(candidate) {
					return false
				}
			}
		}
		return true
	}, nil
}

func logHookError(err error) {
	paths, pathErr := discoverPaths()
	if pathErr != nil {
		return
	}
	parent := filepath.Dir(paths.diagnostics)
	if os.MkdirAll(parent, 0o700) != nil || os.Chmod(parent, 0o700) != nil {
		return
	}
	if info, statErr := os.Stat(paths.diagnostics); statErr == nil && info.Size() >= 64*1024 {
		_ = os.Rename(paths.diagnostics, paths.diagnostics+".old")
	}
	if file, openErr := os.OpenFile(paths.diagnostics, os.O_WRONLY|os.O_APPEND|os.O_CREATE, 0o600); openErr == nil {
		defer file.Close()
		if file.Chmod(0o600) == nil {
			fmt.Fprintln(file, err)
		}
	}
}

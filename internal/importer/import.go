package importer

import (
	"crypto/sha256"
	"encoding/binary"
	"fmt"
	"math"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"unicode/utf8"

	"github.com/pelletier/go-toml/v2"
	"github.com/z0mbix/fmn/internal/history"
)

type Result struct {
	Records  []history.ImportRecord
	Warnings []string
}

func DefaultPath(source string) (string, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	data := envOr("XDG_DATA_HOME", filepath.Join(home, ".local", "share"))
	switch source {
	case "bash":
		return filepath.Join(home, ".bash_history"), nil
	case "zsh":
		return filepath.Join(home, ".zsh_history"), nil
	case "fish":
		return filepath.Join(data, "fish", "fish_history"), nil
	case "atuin":
		if path, ok := os.LookupEnv("ATUIN_DB_PATH"); ok {
			return path, nil
		}
		configDir := envOr("ATUIN_CONFIG_DIR", filepath.Join(envOr("XDG_CONFIG_HOME", filepath.Join(home, ".config")), "atuin"))
		config := map[string]any{}
		if contents, err := os.ReadFile(filepath.Join(configDir, "config.toml")); err == nil {
			if err := toml.Unmarshal(contents, &config); err != nil {
				config = nil
			}
		}
		if path, ok := config["db_path"].(string); ok {
			return expandHome(path, home), nil
		}
		dataDir := filepath.Join(data, "atuin")
		if path, ok := config["data_dir"].(string); ok {
			dataDir = expandHome(path, home)
		}
		return filepath.Join(envOr("ATUIN_DATA_DIR", dataDir), "history.db"), nil
	default:
		return "", fmt.Errorf("unsupported import source %q", source)
	}
}

func envOr(key, fallback string) string {
	if value, ok := os.LookupEnv(key); ok {
		return value
	}
	return fallback
}

func expandHome(path, home string) string {
	if rest, ok := strings.CutPrefix(path, "~/"); ok {
		return filepath.Join(home, rest)
	}
	return path
}

func Load(source, path string) (Result, error) {
	if source == "atuin" {
		return loadAtuin(path)
	}
	data, err := os.ReadFile(path)
	if err != nil {
		return Result{}, err
	}
	if !utf8.Valid(data) {
		return Result{}, fmt.Errorf("history file %q is not valid UTF-8", path)
	}
	return Parse(source, string(data))
}

func Parse(source, input string) (Result, error) {
	lines := strings.Split(strings.TrimSuffix(strings.ReplaceAll(input, "\r\n", "\n"), "\n"), "\n")
	var result Result
	switch source {
	case "bash":
		result = parseBash(lines)
	case "zsh":
		result = parseZsh(lines)
	case "fish":
		result = parseFish(lines)
	default:
		return Result{}, fmt.Errorf("unsupported import source %q", source)
	}
	// Identity and ordinal are shared with Rust so either binary can resume
	// an import without duplicating existing entries.
	ordinals := make(map[string]uint64)
	for i := range result.Records {
		record := &result.Records[i]
		stamp, duration := int64(math.MinInt64), int64(math.MinInt64)
		if record.StartedAt != nil {
			stamp = *record.StartedAt
		}
		if record.DurationMS != nil {
			duration = *record.DurationMS
		}
		key := []byte(source + "\x00" + record.Command + "\x00")
		key = binary.BigEndian.AppendUint64(key, uint64(stamp))
		key = binary.BigEndian.AppendUint64(key, uint64(duration))
		identity := string(key)
		key = binary.BigEndian.AppendUint64(key, ordinals[identity])
		ordinals[identity]++
		record.Key = fmt.Sprintf("%x", sha256.Sum256(key))
	}
	return result, nil
}

func millis(seconds int64) *int64 {
	if seconds > math.MaxInt64/1000 || seconds < math.MinInt64/1000 {
		return nil
	}
	value := seconds * 1000
	return &value
}

func secondsField(raw string) *int64 {
	value, err := strconv.ParseInt(raw, 10, 64)
	if err != nil {
		return nil
	}
	return millis(value)
}

func bashTimestamp(line string) *int64 {
	if seconds, ok := strings.CutPrefix(line, "#"); ok {
		return secondsField(seconds)
	}
	return nil
}

func parseBash(lines []string) Result {
	var result Result
	var stamp *int64
	var command []string
	flush := func() {
		if stamp == nil {
			return
		}
		if len(command) == 0 {
			result.Warnings = append(result.Warnings, fmt.Sprintf("timestamp %d has no command", *stamp))
		} else {
			result.Records = append(result.Records, history.ImportRecord{Command: strings.Join(command, "\n"), StartedAt: stamp})
		}
		command = command[:0]
	}
	for _, line := range lines {
		if next := bashTimestamp(line); next != nil {
			flush()
			stamp = next
		} else if stamp != nil {
			command = append(command, line)
		} else if line != "" {
			result.Records = append(result.Records, history.ImportRecord{Command: line})
		}
	}
	flush()
	return result
}

func parseZsh(lines []string) Result {
	var result Result
	extended := false
	for i, line := range lines {
		if row, ok := zshExtended(line); ok {
			result.Records = append(result.Records, row)
			extended = true
		} else if extended {
			previous := &result.Records[len(result.Records)-1]
			if command, ok := strings.CutSuffix(previous.Command, "\\"); ok {
				previous.Command = command + "\n" + line
			} else if line != "" {
				result.Warnings = append(result.Warnings, fmt.Sprintf("line %d is not a valid extended-history record", i+1))
			}
		} else if line != "" {
			result.Records = append(result.Records, history.ImportRecord{Command: line})
		}
	}
	return result
}

func zshExtended(line string) (history.ImportRecord, bool) {
	rest, prefixed := strings.CutPrefix(line, ": ")
	seconds, rest, colon := strings.Cut(rest, ":")
	elapsed, command, sep := strings.Cut(rest, ";")
	if !prefixed || !colon || !sep {
		return history.ImportRecord{}, false
	}
	stamp, timeErr := strconv.ParseInt(seconds, 10, 64)
	duration, durationErr := strconv.ParseInt(elapsed, 10, 64)
	return history.ImportRecord{Command: command, StartedAt: millis(stamp), DurationMS: millis(duration)}, timeErr == nil && durationErr == nil
}

func parseFish(lines []string) Result {
	var result Result
	unescape := strings.NewReplacer(`\n`, "\n", `\\`, `\`)
	for i, line := range lines {
		if command, ok := strings.CutPrefix(line, "- cmd: "); ok {
			result.Records = append(result.Records, history.ImportRecord{Command: unescape.Replace(command)})
		} else if when, ok := strings.CutPrefix(strings.TrimSpace(line), "when: "); ok {
			if len(result.Records) == 0 {
				continue
			}
			if stamp := secondsField(when); stamp != nil {
				result.Records[len(result.Records)-1].StartedAt = stamp
			} else {
				result.Warnings = append(result.Warnings, fmt.Sprintf("line %d has an invalid fish timestamp", i+1))
			}
		} else if strings.HasPrefix(line, "- cmd:") {
			result.Warnings = append(result.Warnings, fmt.Sprintf("line %d has an empty fish command", i+1))
		}
	}
	return result
}

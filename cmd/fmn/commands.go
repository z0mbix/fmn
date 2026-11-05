package main

import (
	"bufio"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	"github.com/charmbracelet/x/term"
	"github.com/z0mbix/fmn/internal/history"
	"github.com/z0mbix/fmn/internal/importer"
)

func historyCommand(args []string) error {
	if len(args) == 0 || args[0] == "--help" || args[0] == "-h" {
		fmt.Println("Usage: fmn history <list | delete ENTRY_ID | forget COMMAND_ID | clear [--yes] | purge>")
		return nil
	}
	if args[0] == "list" {
		return listHistory(args[1:])
	}
	flags := flagsFor("history " + args[0])
	var action func(*history.Store) error
	switch args[0] {
	case "delete", "forget":
		if len(args) != 2 {
			return fmt.Errorf("history %s requires one numeric ID", args[0])
		}
		if args[1] == "--help" || args[1] == "-h" {
			fmt.Printf("Usage: fmn history %s ID\n", args[0])
			return nil
		}
		id, err := strconv.ParseInt(args[1], 10, 64)
		if err != nil {
			return fmt.Errorf("invalid history ID %q", args[1])
		}
		action = func(db *history.Store) error {
			if args[0] == "delete" {
				return db.DeleteEntry(id)
			}
			return db.ForgetCommand(id)
		}
	case "clear":
		yes := flags.Bool("yes", false, "clear without prompting")
		if err := parse(flags, args[1:]); err != nil {
			return err
		}
		if !*yes {
			if !term.IsTerminal(os.Stdin.Fd()) {
				return errors.New("history clear requires --yes when stdin is not a terminal")
			}
			fmt.Fprint(os.Stderr, "Type clear to permanently remove all fmn history: ")
			confirmation, _ := bufio.NewReader(os.Stdin).ReadString('\n')
			if strings.TrimSpace(confirmation) != "clear" {
				return nil
			}
		}
		action = (*history.Store).Clear
	case "purge":
		if err := parse(flags, args[1:]); err != nil {
			return err
		}
		action = (*history.Store).Purge
	default:
		return fmt.Errorf("unknown history command %q", args[0])
	}
	p, _, err := settings()
	if err != nil {
		return err
	}
	db, err := history.Open(p.database)
	if err != nil {
		return err
	}
	defer db.Close()
	return action(db)
}

func importHistory(args []string) error {
	flags := flagsFor("import <fish|zsh|bash|atuin>")
	path := flags.String("path", "", "path to the history file or database")
	dry := flags.Bool("dry-run", false, "report without importing")
	include := flags.Bool("include-ignored", false, "include commands filtered by privacy preferences")
	// Accept the source on either side of options, as the Rust CLI does.
	var source string
	options := make([]string, 0, len(args))
	for i := 0; i < len(args); i++ {
		arg := args[i]
		if !strings.HasPrefix(arg, "-") {
			if source != "" {
				return fmt.Errorf("unexpected argument %q", arg)
			}
			source = arg
		} else {
			options = append(options, arg)
			if (arg == "--path" || arg == "-path") && i+1 < len(args) {
				i++
				options = append(options, args[i])
			}
		}
	}
	if err := parse(flags, options); err != nil {
		return err
	}
	if source != "fish" && source != "zsh" && source != "bash" && source != "atuin" {
		return fmt.Errorf("unsupported import source %q (choose fish, zsh, bash or atuin)", source)
	}
	if *path == "" {
		value, err := importer.DefaultPath(source)
		if err != nil {
			return err
		}
		*path = value
	}
	p, cfg, err := settings()
	if err != nil {
		return err
	}
	filter, err := cfg.privacyFilter()
	if err != nil {
		return err
	}
	parsed, err := importer.Load(source, *path)
	if err != nil {
		return err
	}
	db, err := history.Open(p.database)
	if err != nil {
		return err
	}
	defer db.Close()
	imported, duplicates, ignored := 0, 0, 0
	fallback := time.Now().UnixMilli()
	for i, record := range parsed.Records {
		if !*include && !filter(record.Command) {
			ignored++
			continue
		}
		if *dry {
			exists, err := db.HasImportKey(record.Key)
			if err != nil {
				return err
			}
			if exists {
				duplicates++
			} else {
				imported++
			}
			continue
		}
		inserted, err := db.Import(record, source, fallback+int64(i))
		if err != nil {
			return err
		}
		if inserted {
			imported++
		} else {
			duplicates++
		}
	}
	label := "imported"
	if *dry {
		label = "would_import"
	}
	fmt.Printf("parsed=%d %s=%d duplicates=%d ignored=%d invalid=%d\n",
		len(parsed.Records), label, imported, duplicates, ignored, len(parsed.Warnings))
	for _, warning := range parsed.Warnings {
		fmt.Fprintln(os.Stderr, "fmn import: warning:", warning)
	}
	return nil
}

func doctor(args []string) (int, error) {
	if err := parse(flagsFor("doctor"), args); err != nil {
		return 2, err
	}
	p, _, err := settings()
	if err != nil {
		return 2, err
	}
	db, err := history.Open(p.database)
	if err != nil {
		return 2, err
	}
	defer db.Close()
	checks := db.Checks()
	perms := history.Check{Status: "ok", Name: "data permissions"}
	if info, err := os.Stat(filepath.Dir(p.database)); err != nil {
		perms.Status, perms.Detail = "error", err.Error()
	} else {
		mode := info.Mode().Perm()
		perms.Detail = fmt.Sprintf("%o", mode)
		if mode&0o077 != 0 {
			perms.Status, perms.Detail = "error", perms.Detail+"; expected no group/other access"
		}
	}
	checks = append(checks, perms)
	integration := history.Check{Status: "ok", Name: "shell integration", Detail: "FMN_SESSION_ID is set"}
	if _, ok := os.LookupEnv("FMN_SESSION_ID"); !ok {
		integration.Status, integration.Detail = "warn", "FMN_SESSION_ID is not set; source `fmn init <shell>`"
	}
	checks = append(checks, integration)
	log := history.Check{Status: "ok", Name: "hook diagnostics", Detail: "no errors recorded"}
	if info, err := os.Stat(p.diagnostics); err == nil {
		log.Detail = "empty"
		if info.Size() > 0 {
			log.Status, log.Detail = "warn", fmt.Sprintf("%d bytes; inspect %s", info.Size(), p.diagnostics)
		}
	} else if !errors.Is(err, os.ErrNotExist) {
		log.Status, log.Detail = "warn", err.Error()
	}
	checks = append(checks, log)
	code := 0
	for _, check := range checks {
		fmt.Printf("[%s] %s: %s\n", check.Status, check.Name, check.Detail)
		if check.Status == "error" {
			code = 2
		}
	}
	return code, nil
}

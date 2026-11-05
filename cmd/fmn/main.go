package main

import (
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"os"
	"os/signal"
	"syscall"
	"time"
	"unicode/utf8"

	"github.com/google/uuid"
	"github.com/z0mbix/fmn/internal/history"
	"github.com/z0mbix/fmn/internal/search"
	"github.com/z0mbix/fmn/internal/shell"
	"github.com/z0mbix/fmn/internal/terminaltext"
)

var version = "go-dev"

func main() {
	args := os.Args[1:]
	code, err := run(args)
	var interruption search.Interruption
	if errors.As(err, &interruption) {
		sig := syscall.Signal(interruption)
		signal.Reset(sig)
		if syscall.Kill(os.Getpid(), sig) == nil {
			// Allow the OS to deliver the signal after terminal teardown.
			time.Sleep(50 * time.Millisecond)
		}
		os.Exit(128 + int(sig))
	}
	if errors.Is(err, flag.ErrHelp) {
		return
	}
	if err != nil {
		if len(args) > 0 && (args[0] == "_record-start" || args[0] == "_record-finish") {
			logHookError(err)
		} else {
			fmt.Fprintln(os.Stderr, "fmn:", err)
		}
		os.Exit(2)
	}
	os.Exit(code)
}

func run(args []string) (int, error) {
	if len(args) == 0 || args[0] == "--help" || args[0] == "-h" {
		fmt.Println("Usage: fmn <doctor | init | search | import | history>")
		return 0, nil
	}
	switch args[0] {
	case "--version", "-V":
		fmt.Println("fmn", version)
	case "doctor":
		return doctor(args[1:])
	case "import":
		return 0, importHistory(args[1:])
	case "_session":
		id, err := uuid.NewRandom()
		if err != nil {
			return 2, err
		}
		_, err = fmt.Fprintln(os.Stdout, id)
		return 0, err
	case "init":
		if len(args) != 2 {
			return 2, errors.New("usage: fmn init <bash|zsh|fish>")
		}
		script, err := shell.Script(args[1])
		if err != nil {
			return 2, err
		}
		_, err = fmt.Fprint(os.Stdout, script)
		return 0, err
	case "_record-start":
		return 0, recordStart(args[1:])
	case "_record-finish":
		return 0, recordFinish(args[1:])
	case "history":
		return 0, historyCommand(args[1:])
	case "search":
		return searchHistory(args[1:])
	default:
		return 2, fmt.Errorf("unknown command %q", args[0])
	}
	return 0, nil
}

func flagsFor(name string) *flag.FlagSet {
	flags := flag.NewFlagSet(name, flag.ContinueOnError)
	flags.SetOutput(io.Discard)
	flags.Usage = func() {
		fmt.Fprintf(os.Stdout, "Usage: fmn %s [options]\n", name)
		flags.SetOutput(os.Stdout)
		flags.PrintDefaults()
		flags.SetOutput(io.Discard)
	}
	return flags
}

func parse(flags *flag.FlagSet, args []string, required ...string) error {
	if err := flags.Parse(args); err != nil {
		return err
	}
	if flags.NArg() != 0 {
		return fmt.Errorf("unexpected argument %q", flags.Arg(0))
	}
	provided := make(map[string]bool)
	flags.Visit(func(f *flag.Flag) { provided[f.Name] = true })
	for _, name := range required {
		if !provided[name] {
			return fmt.Errorf("--%s is required", name)
		}
	}
	return nil
}

func settings() (paths, config, error) {
	p, err := discoverPaths()
	if err != nil {
		return p, config{}, err
	}
	cfg, err := loadConfig(p.config)
	return p, cfg, err
}

func recordStart(args []string) error {
	flags := flagsFor("_record-start")
	name := flags.String("shell", "", "interactive shell")
	session := flags.String("session", "", "shell session ID")
	cwd := flags.String("cwd", "", "working directory")
	started := flags.Int64("started-at", time.Now().UnixMilli(), "start time in epoch milliseconds")
	if err := parse(flags, args, "shell", "session", "cwd"); err != nil {
		return err
	}
	if !shell.Supported(*name) {
		return fmt.Errorf("unsupported shell %q", *name)
	}
	p, cfg, err := settings()
	if err != nil {
		return err
	}
	data, err := io.ReadAll(os.Stdin)
	if err != nil {
		return err
	}
	if !utf8.Valid(data) {
		return errors.New("command is not valid UTF-8")
	}
	command := string(data)
	record, err := cfg.shouldRecord(command)
	if err != nil || !record {
		return err
	}
	db, err := history.Open(p.database)
	if err != nil {
		return err
	}
	defer db.Close()
	return db.RecordStart(command, *cwd, *name, *session, *started)
}

func recordFinish(args []string) error {
	flags := flagsFor("_record-finish")
	session := flags.String("session", "", "shell session ID")
	exit := flags.Int64("exit-code", 0, "command exit code")
	finished := flags.Int64("finished-at", time.Now().UnixMilli(), "finish time in epoch milliseconds")
	if err := parse(flags, args, "session", "exit-code"); err != nil {
		return err
	}
	if int64(int32(*exit)) != *exit {
		return errors.New("exit code is out of range")
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
	return db.RecordFinish(*session, *finished, int32(*exit))
}

func listHistory(args []string) error {
	flags := flagsFor("history list")
	limit := flags.Int64("limit", 100, "maximum number of entries")
	asJSON := flags.Bool("json", false, "output JSON")
	if err := parse(flags, args); err != nil {
		return err
	}
	if *limit < 0 {
		return errors.New("limit must be nonnegative")
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
	entries, err := db.Entries(*limit)
	if err != nil {
		return err
	}
	if *asJSON {
		encoder := json.NewEncoder(os.Stdout)
		encoder.SetEscapeHTML(false)
		encoder.SetIndent("", "  ")
		return encoder.Encode(entries)
	}
	for _, entry := range entries {
		if _, err := fmt.Fprintf(os.Stdout, "%d\t%d\t%s\n", entry.ID, entry.StartedAt, terminaltext.Escape(entry.Command)); err != nil {
			return err
		}
	}
	return nil
}

func searchHistory(args []string) (int, error) {
	flags := flagsFor("search")
	query := flags.String("query", "", "initial search query")
	cwd := flags.String("cwd", "", "working directory")
	widget := flags.Bool("shell-integration", false, "use the shell widget protocol")
	if err := parse(flags, args); err != nil {
		return 2, err
	}
	if *cwd == "" {
		*cwd, _ = os.Getwd()
	}
	p, cfg, err := settings()
	if err != nil {
		return 2, err
	}
	db, err := history.Open(p.database)
	if err != nil {
		return 2, err
	}
	defer db.Close()
	ui, selection := os.Stderr, os.Stdout
	if *widget {
		ui, selection = os.Stdout, os.Stderr
	}
	bindings, err := search.ParseBindings(cfg.UI.Keybindings)
	if err != nil {
		return 2, err
	}
	options := search.Options{
		Query: *query, Cwd: *cwd, Widget: *widget, NoColor: os.Getenv("NO_COLOR") != "",
		Preferences: search.Preferences{
			Theme: cfg.UI.Theme, DefaultScope: cfg.UI.DefaultScope, ShowInspector: cfg.UI.ShowInspector,
			ExecuteOnSelect: cfg.UI.ExecuteOnSelect, Bindings: bindings,
		},
		Save: func(change search.Change) error { return savePreference(p.config, change) },
	}
	result, err := search.Run(db, os.Stdin, ui, options)
	if err != nil {
		return 2, err
	}
	if !result.Accepted {
		return 1, nil
	}
	if _, err := fmt.Fprint(selection, result.Command); err != nil {
		return 2, err
	}
	if result.Execute {
		return 10, nil
	}
	return 0, nil
}

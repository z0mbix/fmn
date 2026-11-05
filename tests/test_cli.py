from concurrent.futures import ThreadPoolExecutor
from contextlib import closing
import json
import os
from pathlib import Path
import re
import sqlite3
import subprocess
import tempfile
import unittest

from support.terminal_recovery import run as terminal_run


ROOT = Path(__file__).resolve().parents[1]
BINARY = Path(os.environ.get("FMN_TEST_BIN", ROOT / "target/debug/fmn")).resolve()


class CLITests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="fmn-tests-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.config = self.root / "config.toml"
        self.database = self.root / "data" / "history.db"
        self.env = {
            **os.environ,
            "FMN_CONFIG": str(self.config),
            "FMN_DATA_DIR": str(self.database.parent),
            "XDG_DATA_HOME": str(self.root / "xdg"),
            "XDG_CONFIG_HOME": str(self.root / "config"),
            "TERM": "xterm-256color",
            "COLORTERM": "truecolor",
        }
        for name in ("FMN_DISABLE", "FMN_SESSION_ID", "NO_COLOR", "ATUIN_DB_PATH",
                     "ATUIN_CONFIG_DIR", "ATUIN_DATA_DIR"):
            self.env.pop(name, None)

    def fmn(self, *arguments, input=None, status=0):
        result = subprocess.run(
            [BINARY, *arguments], env=self.env, input="" if input is None else input, capture_output=True,
            text=True, timeout=10,
        )
        self.assertEqual(result.returncode, status, (arguments, result.stderr))
        return result

    def record(self, command, *, shell="fish", session="test", stamp=1000, exit_code=7):
        self.fmn("_record-start", "--shell", shell, "--session", session, "--cwd", str(self.root),
                 "--started-at", str(stamp), input=command)
        if exit_code is not None:
            self.fmn("_record-finish", "--session", session, "--exit-code", str(exit_code),
                     "--finished-at", str(stamp + 25))

    def entries(self, *options):
        return json.loads(self.fmn("history", "list", "--json", *options).stdout)

    def test_record_list_limit_and_escape(self):
        command = "echo café\x1b]52;c;Zm1u\x07\r\n\tend"
        self.assertRegex(self.fmn("_session").stdout, r"^[0-9a-f-]{36}\n$")
        self.record(command)
        self.record("echo newest", stamp=2000, exit_code=None)
        latest, earlier = self.entries()
        self.assertEqual(self.entries("--limit", "1"), [latest])
        self.assertIsNone(latest["exit_code"])
        self.assertEqual((earlier["command"], earlier["duration_ms"], earlier["exit_code"]),
                         (command, 25, 7))
        self.assertEqual((earlier["cwd"], earlier["shell"], earlier["session_id"], earlier["source"]),
                         (str(self.root), "fish", "test", "live"))
        plain = self.fmn("history", "list").stdout
        self.assertIn(r"café\u{1b}]52;c;Zm1u\u{7}\r\n\tend", plain)
        self.assertEqual(len(plain.splitlines()), 2)

    def test_privacy_and_diagnostics(self):
        self.config.write_text("[history]\nignore_patterns = ['secret *']\n")
        self.record("secret hide", exit_code=None)
        self.record(" hidden", exit_code=None)
        self.env["FMN_DISABLE"] = ""
        self.record("echo disabled", exit_code=None)
        del self.env["FMN_DISABLE"]
        self.record("echo visible")
        self.assertEqual([e["command"] for e in self.entries()], ["echo visible"])
        self.config.write_text("[history]\nignore_patterns = ['[']\n")
        result = self.fmn("_record-start", "--shell", "bash", "--session", "s",
                          "--cwd", "/", input="hidden", status=2)
        self.assertEqual(result.stderr, "")
        log = self.database.parent / "diagnostics.log"
        self.assertIn("invalid ignore pattern", log.read_text())
        self.assertEqual(log.stat().st_mode & 0o777, 0o600)
        self.assertEqual(self.database.parent.stat().st_mode & 0o777, 0o700)
        self.config.unlink()
        self.assertIn("[warn] hook diagnostics", self.fmn("doctor").stdout)

    def test_concurrent_first_capture_from_separate_processes(self):
        with ThreadPoolExecutor(max_workers=12) as workers:
            list(workers.map(
                lambda i: self.record("echo concurrent", session=str(i), stamp=1000 + i),
                range(12),
            ))
        entries = self.entries()
        self.assertEqual(len(entries), 12)
        self.assertEqual(len({entry["command_id"] for entry in entries}), 1)
        self.assertEqual({entry["session_id"] for entry in entries}, {str(i) for i in range(12)})
        self.assertTrue(all(entry["duration_ms"] == 25 for entry in entries))

    def test_delete_forget_clear_and_pending_checkpoint(self):
        self.record("repeat")
        self.record("repeat", stamp=2000)
        self.record("keep", stamp=3000)
        kept, newest, oldest = self.entries()
        with closing(sqlite3.connect(self.database)) as reader, reader:
            reader.execute("BEGIN")
            reader.execute("SELECT count(*) FROM entries").fetchone()
            result = self.fmn("history", "delete", str(newest["id"]), status=2)
            self.assertIn("cleanup is pending", result.stderr)
            self.assertEqual(self.entries(), [kept, oldest])
            reader.rollback()
        self.fmn("history", "purge")
        with closing(sqlite3.connect(self.database)) as db, db:
            aggregate = db.execute("SELECT first_seen_at, last_seen_at, use_count FROM commands WHERE id=?",
                                   (oldest["command_id"],)).fetchone()
        self.assertEqual(aggregate, (1000, 1000, 1))
        self.fmn("history", "forget", str(oldest["command_id"]))
        self.assertEqual(self.entries(), [kept])
        self.assertIn("requires --yes", self.fmn("history", "clear", status=2).stderr)
        self.fmn("history", "clear", "--yes")
        self.assertEqual(self.entries(), [])
        self.assertIn("[ok] database integrity", self.fmn("doctor").stdout)

    def test_native_imports_preserve_multiline_duplicates_and_filters(self):
        self.config.write_text("[history]\nignore_patterns = ['secret *']\n")
        sources = {
            "bash": "#1\necho repeat\n#1\necho repeat\n#2\nprintf first\nprintf second\n",
            "zsh": "echo plain\n: 3:2;echo extended\\\ncontinuation\n",
            "fish": "- cmd: printf first\\nsecond\n  when: 5\n- cmd: secret filtered\n  when: 6\n",
        }
        for shell, contents in sources.items():
            with self.subTest(shell=shell):
                file = self.root / shell
                file.write_text(contents)
                command = ("import", shell, "--path", str(file))
                expected = {"bash": 3, "zsh": 2, "fish": 1}[shell]
                before = self.entries()
                self.assertIn(f"would_import={expected}", self.fmn(*command, "--dry-run").stdout)
                self.assertEqual(self.entries(), before)
                self.assertIn(f"imported={expected}", self.fmn(*command).stdout)
                self.assertIn(f"duplicates={expected}", self.fmn(*command).stdout)
        self.assertIn("ignored=1", self.fmn("import", "fish", "--path", str(self.root / "fish")).stdout)
        self.assertIn("imported=1", self.fmn("import", "fish", "--include-ignored",
                                            "--path", str(self.root / "fish")).stdout)
        entries = self.entries()
        by_text = {entry["command"]: entry for entry in entries}
        self.assertEqual(len(entries), 7)
        self.assertEqual(sum(entry["command"] == "echo repeat" for entry in entries), 2)
        self.assertEqual(by_text["echo extended\ncontinuation"]["duration_ms"], 2000)
        self.assertIn("printf first\nsecond", by_text)
        self.assertIn("printf first\nprintf second", by_text)

    def test_atuin_path_and_metadata(self):
        source = self.root / "atuin.db"
        with closing(sqlite3.connect(source)) as db, db:
            db.executescript("""
                CREATE TABLE history(id TEXT, timestamp INTEGER, duration INTEGER, exit INTEGER,
                                     command TEXT, cwd TEXT, shell TEXT, session TEXT, deleted_at INTEGER);
                INSERT INTO history VALUES ('one', -1, 17000000, 7, 'atuin kept', '/repo', 'zsh', 's', NULL);
                INSERT INTO history VALUES ('two', 1, 10, 0, 'atuin deleted', '/repo', 'fish', 's', 1);
            """)
        config_dir = Path(self.env["XDG_CONFIG_HOME"]) / "atuin"
        config_dir.mkdir(parents=True)
        (config_dir / "config.toml").write_text(f"db_path = {json.dumps(str(source))}\n")
        self.assertIn("imported=1", self.fmn("import", "atuin").stdout)
        self.assertIn("duplicates=1", self.fmn("import", "atuin").stdout)
        row, = self.entries()
        self.assertEqual((row["command"], row["started_at"], row["duration_ms"], row["exit_code"],
                          row["cwd"], row["shell"], row["session_id"], row["source"]),
                         ("atuin kept", -1, 17, 7, "/repo", "zsh", "atuin:s", "import"))

    def test_init_scripts_and_noninteractive_search(self):
        for shell in ("bash", "zsh", "fish"):
            script = self.fmn("init", shell).stdout
            subprocess.run([shell, "-n"], input=script, text=True, env=self.env, check=True, timeout=10)
        self.fmn("init", "unsupported", status=2)
        self.assertIn("interactive terminal", self.fmn("search", status=2).stderr)

    def test_real_shell_hooks(self):
        for shell in ("bash", "bash-vi", "zsh", "fish"):
            with self.subTest(shell=shell):
                result = subprocess.run(
                    ["expect", ROOT / "tests/support/shell_roundtrip.exp", BINARY, shell, self.root / shell],
                    env=self.env, capture_output=True, timeout=55,
                )
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                with closing(sqlite3.connect(self.root / shell / "data/history.db")) as db:
                    captures = db.execute(
                        "SELECT e.shell, e.exit_code, e.duration_ms FROM entries e JOIN commands c "
                        "ON c.id=e.command_id WHERE c.command='false'"
                    ).fetchall()
                self.assertEqual(len(captures), 1)
                self.assertEqual(captures[0][:2], (shell.removesuffix("-vi"), 1))
                self.assertIsNotNone(captures[0][2])

    def test_selection_protocol_and_signals_restore_terminal(self):
        self.record("echo literal?")
        self.record("echo other", stamp=2000)
        self.config.write_text("[ui]\nexecute_on_select = true\n")
        for stream in ("stdout", "stderr"):
            flags = ["--shell-integration"] if stream == "stdout" else []
            arguments = [BINARY, "search", "--query", "literal?", *flags]
            with self.subTest(stream=stream):
                terminal_run("enter", stream, arguments, self.env, selection=b"echo literal?",
                             expected=10 if flags else 0)
                terminal_run("cancel", stream, arguments, self.env, expected=1)
                for scenario in ("sigterm", "repeated-sigterm", "sigint", "sighup"):
                    terminal_run(scenario, stream, arguments, self.env)
        terminal_run("resize-paste", "stderr", [BINARY, "search"], self.env,
                     selection=b"echo literal?", expected=0)

    def test_no_color_overrides_saved_theme_for_the_session(self):
        self.record("echo sample")
        config = "[ui]\ntheme = 'nord'\n"
        self.config.write_text(config)
        for value in ("", "1", "0"):
            transcript = terminal_run(
                "cancel", "stderr", [BINARY, "search"], {**self.env, "NO_COLOR": value}, expected=1
            )
            colored = re.search(rb"\x1b\[[0-9;]*(?:38|48);[25];[0-9;]*m", transcript)
            self.assertEqual(bool(colored), not value)
            self.assertEqual(self.config.read_text(), config)


if __name__ == "__main__":
    unittest.main()

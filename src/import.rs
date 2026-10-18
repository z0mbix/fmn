use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use sha2::{Digest, Sha256};

use crate::model::Shell;
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum ImportSource {
    Fish,
    Zsh,
    Bash,
    Atuin,
}

impl ImportSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fish => "fish",
            Self::Zsh => "zsh",
            Self::Bash => "bash",
            Self::Atuin => "atuin",
        }
    }

    pub const fn native_shell(self) -> Option<Shell> {
        match self {
            Self::Fish => Some(Shell::Fish),
            Self::Zsh => Some(Shell::Zsh),
            Self::Bash => Some(Shell::Bash),
            Self::Atuin => None,
        }
    }
}

impl From<Shell> for ImportSource {
    fn from(shell: Shell) -> Self {
        match shell {
            Shell::Fish => Self::Fish,
            Shell::Zsh => Self::Zsh,
            Shell::Bash => Self::Bash,
        }
    }
}

impl fmt::Display for ImportSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::str::FromStr for ImportSource {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "fish" => Ok(Self::Fish),
            "zsh" => Ok(Self::Zsh),
            "bash" => Ok(Self::Bash),
            "atuin" => Ok(Self::Atuin),
            other => Err(format!("unsupported import source {other:?}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedRecord {
    pub command: String,
    pub started_at: Option<i64>,
    pub duration_ms: Option<i64>,
    pub exit_code: Option<i32>,
    pub cwd: Option<String>,
    pub shell: Option<String>,
    pub session_id: Option<String>,
    pub import_key: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParseResult {
    pub records: Vec<ImportedRecord>,
    pub warnings: Vec<String>,
}

pub fn parse(shell: Shell, input: &str) -> ParseResult {
    let raw = match shell {
        Shell::Bash => parse_bash(input),
        Shell::Zsh => parse_zsh(input),
        Shell::Fish => parse_fish(input),
    };
    assign_keys(shell, raw)
}

pub fn load(source: ImportSource, path: &Path) -> Result<ParseResult> {
    match source.native_shell() {
        Some(shell) => Ok(parse(shell, &std::fs::read_to_string(path)?)),
        None => load_atuin(path),
    }
}

pub fn default_history_path(source: impl Into<ImportSource>) -> Option<PathBuf> {
    let source = source.into();
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    Some(match source {
        ImportSource::Bash => home.join(".bash_history"),
        ImportSource::Zsh => home.join(".zsh_history"),
        ImportSource::Fish => std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local").join("share"))
            .join("fish")
            .join("fish_history"),
        ImportSource::Atuin => std::env::var_os("ATUIN_DB_PATH")
            .map(PathBuf::from)
            .or_else(|| atuin_config_path(&home, "db_path"))
            .unwrap_or_else(|| {
                std::env::var_os("ATUIN_DATA_DIR")
                    .map(PathBuf::from)
                    .or_else(|| atuin_config_path(&home, "data_dir"))
                    .unwrap_or_else(|| {
                        std::env::var_os("XDG_DATA_HOME")
                            .map(PathBuf::from)
                            .unwrap_or_else(|| home.join(".local").join("share"))
                            .join("atuin")
                    })
                    .join("history.db")
            }),
    })
}

fn atuin_config_path(home: &Path, key: &str) -> Option<PathBuf> {
    let config_dir = std::env::var_os("ATUIN_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .map(|path| path.join("atuin"))
        })
        .unwrap_or_else(|| home.join(".config").join("atuin"));
    let contents = std::fs::read_to_string(config_dir.join("config.toml")).ok()?;
    let config = contents.parse::<toml::Table>().ok()?;
    let value = config.get(key)?.as_str()?;
    Some(expand_home(value, home))
}

fn expand_home(value: &str, home: &Path) -> PathBuf {
    value
        .strip_prefix("~/")
        .map_or_else(|| PathBuf::from(value), |rest| home.join(rest))
}

fn load_atuin(path: &Path) -> Result<ParseResult> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let columns = table_columns(&connection)?;
    for required in [
        "id",
        "timestamp",
        "duration",
        "exit",
        "command",
        "cwd",
        "session",
    ] {
        if !columns.iter().any(|column| column == required) {
            return Err(Error::UnsupportedAtuinDatabase(format!(
                "history table is missing the {required:?} column"
            )));
        }
    }

    let shell_expression = if columns.iter().any(|column| column == "shell") {
        "shell"
    } else {
        "NULL"
    };
    let active_filter = if columns.iter().any(|column| column == "deleted_at") {
        "WHERE deleted_at IS NULL"
    } else {
        ""
    };
    let query = format!(
        "SELECT id, timestamp, duration, exit, command, cwd, {shell_expression}, session
         FROM history
         {active_filter}
         ORDER BY timestamp, id"
    );
    let mut statement = connection.prepare(&query)?;
    let mut rows = statement.query([])?;
    let mut result = ParseResult::default();

    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let timestamp_ns: i64 = row.get(1)?;
        let duration_ns: i64 = row.get(2)?;
        let exit: i64 = row.get(3)?;
        let shell = row
            .get::<_, Option<String>>(6)?
            .filter(|value| !value.trim().is_empty());
        let exit_code = match i32::try_from(exit) {
            Ok(exit_code) => Some(exit_code),
            Err(_) => {
                result
                    .warnings
                    .push(format!("Atuin record {id} has an out-of-range exit code"));
                None
            }
        };
        result.records.push(ImportedRecord {
            command: row.get(4)?,
            started_at: Some(timestamp_ns.div_euclid(1_000_000)),
            duration_ms: (duration_ns >= 0).then(|| duration_ns.div_euclid(1_000_000)),
            exit_code,
            cwd: Some(row.get(5)?),
            shell,
            session_id: Some(format!("atuin:{}", row.get::<_, String>(7)?)),
            import_key: atuin_import_key(&id),
        });
    }

    Ok(result)
}

fn table_columns(connection: &Connection) -> Result<Vec<String>> {
    let mut statement = connection.prepare("PRAGMA table_info(history)")?;
    let rows = statement.query_map([], |row| row.get(1))?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn atuin_import_key(id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"atuin\0");
    hasher.update(id.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[derive(Debug)]
struct RawRecord {
    command: String,
    started_at: Option<i64>,
    duration_ms: Option<i64>,
}

#[derive(Debug, Default)]
struct RawParseResult {
    records: Vec<RawRecord>,
    warnings: Vec<String>,
}

fn parse_bash(input: &str) -> RawParseResult {
    let lines: Vec<_> = input.lines().collect();
    let has_timestamps = lines.iter().any(|line| bash_timestamp(line).is_some());
    if !has_timestamps {
        return RawParseResult {
            records: lines
                .into_iter()
                .filter(|line| !line.is_empty())
                .map(|line| RawRecord {
                    command: line.to_owned(),
                    started_at: None,
                    duration_ms: None,
                })
                .collect(),
            warnings: Vec::new(),
        };
    }

    let mut result = RawParseResult::default();
    let mut timestamp = None;
    let mut command_lines = Vec::new();
    for line in lines {
        if let Some(next_timestamp) = bash_timestamp(line) {
            flush_bash_record(&mut result, timestamp, &mut command_lines);
            timestamp = Some(next_timestamp);
        } else if timestamp.is_some() {
            command_lines.push(line);
        } else if !line.is_empty() {
            result.records.push(RawRecord {
                command: line.to_owned(),
                started_at: None,
                duration_ms: None,
            });
        }
    }
    flush_bash_record(&mut result, timestamp, &mut command_lines);
    result
}

fn flush_bash_record(
    result: &mut RawParseResult,
    timestamp: Option<i64>,
    command_lines: &mut Vec<&str>,
) {
    if let Some(started_at) = timestamp {
        if command_lines.is_empty() {
            result
                .warnings
                .push(format!("timestamp {started_at} has no command"));
        } else {
            result.records.push(RawRecord {
                command: command_lines.join("\n"),
                started_at: Some(started_at),
                duration_ms: None,
            });
        }
    }
    command_lines.clear();
}

fn bash_timestamp(line: &str) -> Option<i64> {
    let seconds = line.strip_prefix('#')?.parse::<i64>().ok()?;
    seconds.checked_mul(1_000)
}

fn parse_zsh(input: &str) -> RawParseResult {
    let mut result = RawParseResult::default();
    let mut pending: Option<RawRecord> = None;

    for (index, line) in input.lines().enumerate() {
        if let Some(record) = parse_zsh_extended(line) {
            if let Some(previous) = pending.replace(record) {
                result.records.push(previous);
            }
        } else if let Some(previous) = pending.as_mut() {
            if previous.command.ends_with('\\') {
                previous.command.pop();
                previous.command.push('\n');
                previous.command.push_str(line);
            } else if !line.is_empty() {
                result.warnings.push(format!(
                    "line {} is not a valid extended-history record",
                    index + 1
                ));
            }
        } else if !line.is_empty() {
            result.records.push(RawRecord {
                command: line.to_owned(),
                started_at: None,
                duration_ms: None,
            });
        }
    }
    if let Some(record) = pending {
        result.records.push(record);
    }
    result
}

fn parse_zsh_extended(line: &str) -> Option<RawRecord> {
    let rest = line.strip_prefix(": ")?;
    let (seconds, rest) = rest.split_once(':')?;
    let (duration_seconds, command) = rest.split_once(';')?;
    Some(RawRecord {
        command: command.to_owned(),
        started_at: seconds.parse::<i64>().ok()?.checked_mul(1_000),
        duration_ms: duration_seconds.parse::<i64>().ok()?.checked_mul(1_000),
    })
}

fn parse_fish(input: &str) -> RawParseResult {
    let mut result = RawParseResult::default();
    let mut pending: Option<RawRecord> = None;

    for (index, line) in input.lines().enumerate() {
        if let Some(command) = line.strip_prefix("- cmd: ") {
            if let Some(previous) = pending.replace(RawRecord {
                command: unescape_fish(command),
                started_at: None,
                duration_ms: None,
            }) {
                result.records.push(previous);
            }
        } else if let Some(seconds) = line.trim().strip_prefix("when: ") {
            if let Some(record) = pending.as_mut() {
                match seconds
                    .parse::<i64>()
                    .ok()
                    .and_then(|value| value.checked_mul(1_000))
                {
                    Some(started_at) => record.started_at = Some(started_at),
                    None => result
                        .warnings
                        .push(format!("line {} has an invalid fish timestamp", index + 1)),
                }
            }
        } else if line.starts_with("- cmd:") {
            result
                .warnings
                .push(format!("line {} has an empty fish command", index + 1));
        }
    }
    if let Some(record) = pending {
        result.records.push(record);
    }
    result
}

fn unescape_fish(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut characters = value.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }
        match characters.next() {
            Some('n') => output.push('\n'),
            Some('\\') => output.push('\\'),
            Some(escaped) => {
                output.push('\\');
                output.push(escaped);
            }
            None => output.push('\\'),
        }
    }
    output
}

fn assign_keys(shell: Shell, raw: RawParseResult) -> ParseResult {
    let mut occurrences: HashMap<Vec<u8>, u64> = HashMap::new();
    let records = raw
        .records
        .into_iter()
        .map(|record| {
            let mut identity = Vec::new();
            identity.extend_from_slice(shell.as_str().as_bytes());
            identity.push(0);
            identity.extend_from_slice(record.command.as_bytes());
            identity.push(0);
            identity.extend_from_slice(&record.started_at.unwrap_or(i64::MIN).to_be_bytes());
            identity.extend_from_slice(&record.duration_ms.unwrap_or(i64::MIN).to_be_bytes());
            let ordinal = occurrences.entry(identity.clone()).or_default();
            let current_ordinal = *ordinal;
            *ordinal += 1;

            let mut hasher = Sha256::new();
            hasher.update(&identity);
            hasher.update(current_ordinal.to_be_bytes());
            ImportedRecord {
                command: record.command,
                started_at: record.started_at,
                duration_ms: record.duration_ms,
                exit_code: None,
                cwd: None,
                shell: None,
                session_id: None,
                import_key: format!("{:x}", hasher.finalize()),
            }
        })
        .collect();
    ParseResult {
        records,
        warnings: raw.warnings,
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::{ImportSource, default_history_path, load, parse};
    use crate::model::Shell;

    #[test]
    fn parses_bash_without_timestamps() {
        let parsed = parse(Shell::Bash, "echo one\necho two\n");
        assert_eq!(parsed.records.len(), 2);
        assert_eq!(parsed.records[0].command, "echo one");
        assert_eq!(parsed.records[0].started_at, None);
    }

    #[test]
    fn parses_timestamped_and_multiline_bash() {
        let parsed = parse(
            Shell::Bash,
            "#1700000000\nprintf 'one\\ntwo'\ncontinued\n#1700000001\npwd\n",
        );
        assert_eq!(parsed.records.len(), 2);
        assert_eq!(parsed.records[0].command, "printf 'one\\ntwo'\ncontinued");
        assert_eq!(parsed.records[0].started_at, Some(1_700_000_000_000));
    }

    #[test]
    fn warns_on_bash_timestamp_without_command() {
        let parsed = parse(Shell::Bash, "#1700000000\n#1700000001\npwd\n");
        assert_eq!(parsed.records.len(), 1);
        assert_eq!(parsed.warnings.len(), 1);
    }

    #[test]
    fn parses_plain_and_extended_zsh() {
        let plain = parse(Shell::Zsh, "echo plain\n");
        assert_eq!(plain.records[0].command, "echo plain");
        let extended = parse(Shell::Zsh, ": 1700000000:3;echo extended\n");
        assert_eq!(extended.records[0].started_at, Some(1_700_000_000_000));
        assert_eq!(extended.records[0].duration_ms, Some(3_000));
    }

    #[test]
    fn parses_zsh_continuations() {
        let parsed = parse(Shell::Zsh, ": 1700000000:0;printf one\\\ntwo\n");
        assert_eq!(parsed.records[0].command, "printf one\ntwo");
    }

    #[test]
    fn parses_fish_commands_timestamps_and_escapes() {
        let parsed = parse(
            Shell::Fish,
            "- cmd: echo one\\ntwo\n  when: 1700000000\n  paths:\n    - /tmp\n",
        );
        assert_eq!(parsed.records.len(), 1);
        assert_eq!(parsed.records[0].command, "echo one\ntwo");
        assert_eq!(parsed.records[0].started_at, Some(1_700_000_000_000));
    }

    #[test]
    fn invalid_fish_timestamp_is_a_warning() {
        let parsed = parse(Shell::Fish, "- cmd: echo\n  when: nope\n");
        assert_eq!(parsed.records.len(), 1);
        assert_eq!(parsed.warnings.len(), 1);
    }

    #[test]
    fn duplicate_occurrences_get_distinct_stable_keys() {
        let first = parse(Shell::Bash, "echo same\necho same\n");
        let second = parse(Shell::Bash, "echo same\necho same\n");
        assert_ne!(first.records[0].import_key, first.records[1].import_key);
        assert_eq!(first.records[0].import_key, second.records[0].import_key);
        assert_eq!(first.records[1].import_key, second.records[1].import_key);
    }

    #[test]
    fn default_paths_are_shell_specific() {
        assert!(
            default_history_path(Shell::Bash)
                .unwrap()
                .ends_with(".bash_history")
        );
        assert!(
            default_history_path(Shell::Zsh)
                .unwrap()
                .ends_with(".zsh_history")
        );
        assert!(
            default_history_path(Shell::Fish)
                .unwrap()
                .ends_with("fish/fish_history")
        );
        assert!(
            default_history_path(ImportSource::Atuin)
                .unwrap()
                .ends_with("atuin/history.db")
        );
    }

    #[test]
    fn import_source_round_trips() {
        for source in [
            ImportSource::Fish,
            ImportSource::Zsh,
            ImportSource::Bash,
            ImportSource::Atuin,
        ] {
            assert_eq!(source.as_str().parse::<ImportSource>(), Ok(source));
        }
        assert!("sqlite".parse::<ImportSource>().is_err());
    }

    #[test]
    fn loads_atuin_metadata_and_skips_deleted_records() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("history.db");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE history (
                    id TEXT PRIMARY KEY,
                    timestamp INTEGER NOT NULL,
                    duration INTEGER NOT NULL,
                    exit INTEGER NOT NULL,
                    command TEXT NOT NULL,
                    cwd TEXT NOT NULL,
                    session TEXT NOT NULL,
                    hostname TEXT NOT NULL,
                    deleted_at INTEGER,
                    shell TEXT
                );
                INSERT INTO history VALUES (
                    'active', 1700000000123456789, 1250000000, 7,
                    'cargo test', '/repo', 'session-one', 'host:user', NULL, 'fish'
                );
                INSERT INTO history VALUES (
                    'deleted', 1700000001000000000, 1000000, 0,
                    'secret old command', '/repo', 'session-two', 'host:user', 1, 'zsh'
                );",
            )
            .unwrap();
        drop(connection);

        let parsed = load(ImportSource::Atuin, &path).unwrap();
        assert!(parsed.warnings.is_empty());
        assert_eq!(parsed.records.len(), 1);
        let record = &parsed.records[0];
        assert_eq!(record.command, "cargo test");
        assert_eq!(record.started_at, Some(1_700_000_000_123));
        assert_eq!(record.duration_ms, Some(1_250));
        assert_eq!(record.exit_code, Some(7));
        assert_eq!(record.cwd.as_deref(), Some("/repo"));
        assert_eq!(record.shell.as_deref(), Some("fish"));
        assert_eq!(record.session_id.as_deref(), Some("atuin:session-one"));
    }

    #[test]
    fn loads_older_atuin_schema_and_unknown_duration() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("history.db");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE history (
                    id TEXT PRIMARY KEY,
                    timestamp INTEGER NOT NULL,
                    duration INTEGER NOT NULL,
                    exit INTEGER NOT NULL,
                    command TEXT NOT NULL,
                    cwd TEXT NOT NULL,
                    session TEXT NOT NULL,
                    hostname TEXT NOT NULL
                );
                INSERT INTO history VALUES (
                    'old', 1700000000000000000, -1, 0,
                    'echo old', '/', 'old-session', 'host:user'
                );",
            )
            .unwrap();
        drop(connection);

        let parsed = load(ImportSource::Atuin, &path).unwrap();
        assert_eq!(parsed.records.len(), 1);
        assert_eq!(parsed.records[0].duration_ms, None);
        assert_eq!(parsed.records[0].shell, None);
    }

    #[test]
    fn atuin_ids_produce_stable_distinct_import_keys() {
        assert_eq!(
            super::atuin_import_key("one"),
            super::atuin_import_key("one")
        );
        assert_ne!(
            super::atuin_import_key("one"),
            super::atuin_import_key("two")
        );
    }

    #[test]
    fn rejects_sqlite_databases_without_atuin_history_schema() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("not-atuin.db");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch("CREATE TABLE something_else (id INTEGER PRIMARY KEY);")
            .unwrap();
        drop(connection);

        let error = load(ImportSource::Atuin, &path).unwrap_err();
        assert!(matches!(error, crate::Error::UnsupportedAtuinDatabase(_)));
    }
}

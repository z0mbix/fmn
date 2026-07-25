use std::collections::HashMap;
use std::path::PathBuf;

use sha2::{Digest, Sha256};

use crate::model::Shell;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedRecord {
    pub command: String,
    pub started_at: Option<i64>,
    pub duration_ms: Option<i64>,
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

pub fn default_history_path(shell: Shell) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    Some(match shell {
        Shell::Bash => home.join(".bash_history"),
        Shell::Zsh => home.join(".zsh_history"),
        Shell::Fish => std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local").join("share"))
            .join("fish")
            .join("fish_history"),
    })
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
    use super::{default_history_path, parse};
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
    }
}

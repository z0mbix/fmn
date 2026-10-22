use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Fish,
    Zsh,
    Bash,
}

impl Shell {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fish => "fish",
            Self::Zsh => "zsh",
            Self::Bash => "bash",
        }
    }
}

impl fmt::Display for Shell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for Shell {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "fish" => Ok(Self::Fish),
            "zsh" => Ok(Self::Zsh),
            "bash" => Ok(Self::Bash),
            other => Err(format!("unsupported shell {other:?}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: i64,
    pub command_id: i64,
    pub command: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub duration_ms: Option<i64>,
    pub exit_code: Option<i32>,
    pub cwd: String,
    pub shell: String,
    pub session_id: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSummary {
    pub id: i64,
    pub command: String,
    pub first_seen_at: i64,
    pub last_seen_at: i64,
    pub use_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchCandidate {
    pub id: i64,
    pub command: String,
    pub first_seen_at: i64,
    pub last_seen_at: i64,
    pub use_count: i64,
    pub cwd_use_count: i64,
    pub cwd_last_seen_at: Option<i64>,
    pub latest_cwd: String,
    pub latest_exit_code: Option<i32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct HistoryStats {
    pub total: i64,
    pub unique: i64,
    pub completed: i64,
    pub failed: i64,
    pub average_duration_ms: Option<f64>,
    pub activity: [u64; 30],
    pub top_commands: Vec<CommandStats>,
    pub failed_commands: Vec<CommandStats>,
    pub slow_commands: Vec<CommandStats>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommandStats {
    pub command: String,
    pub count: i64,
    pub failures: i64,
    pub average_duration_ms: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::Shell;

    #[test]
    fn shell_round_trips() {
        for shell in [Shell::Fish, Shell::Zsh, Shell::Bash] {
            assert_eq!(shell.as_str().parse::<Shell>(), Ok(shell));
        }
    }

    #[test]
    fn rejects_unknown_shell() {
        assert!("powershell".parse::<Shell>().is_err());
    }
}

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("could not determine the home directory")]
    HomeDirectoryUnavailable,
    #[error("invalid configuration in {path}: {source}")]
    InvalidConfig {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("invalid ignore pattern {pattern:?}: {source}")]
    InvalidIgnorePattern {
        pattern: String,
        source: globset::Error,
    },
    #[error("entry {0} does not exist")]
    EntryNotFound(i64),
    #[error("command {0} does not exist")]
    CommandNotFound(i64),
    #[error(
        "WAL cleanup is pending because another connection is using history; close other searches and run `fmn history purge`"
    )]
    HistoryPurgePending,
    #[error(
        "history schema {found} is newer than the supported version {supported}; use a newer fmn release"
    )]
    UnsupportedSchema { found: i64, supported: i64 },
    #[error("unsupported Atuin database: {0}")]
    UnsupportedAtuinDatabase(String),
}

pub type Result<T> = std::result::Result<T, Error>;

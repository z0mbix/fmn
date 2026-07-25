use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use crate::Result;
use crate::paths::AppPaths;
use crate::storage::Store;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub status: Status,
    pub name: &'static str,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub checks: Vec<Check>,
}

impl Report {
    pub fn is_healthy(&self) -> bool {
        !self
            .checks
            .iter()
            .any(|check| check.status == Status::Error)
    }
}

pub fn inspect(paths: &AppPaths, store: &Store) -> Report {
    let mut checks = Vec::new();

    checks.push(match store.integrity_check() {
        Ok(result) if result == "ok" => ok("database integrity", "ok"),
        Ok(result) => error("database integrity", result),
        Err(failure) => error("database integrity", failure.to_string()),
    });
    checks.push(match store.schema_version() {
        Ok(1) => ok("schema version", "1"),
        Ok(version) => error("schema version", format!("unsupported version {version}")),
        Err(failure) => error("schema version", failure.to_string()),
    });
    checks.push(match store.journal_mode() {
        Ok(mode) if mode.eq_ignore_ascii_case("wal") => ok("journal mode", mode),
        Ok(mode) => warning("journal mode", format!("{mode}; expected wal")),
        Err(failure) => error("journal mode", failure.to_string()),
    });
    checks.push(match store.foreign_keys_enabled() {
        Ok(true) => ok("foreign keys", "enabled"),
        Ok(false) => error("foreign keys", "disabled"),
        Err(failure) => error("foreign keys", failure.to_string()),
    });
    checks.push(check_permissions(&paths.data_dir));
    checks.push(if std::env::var_os("FMN_SESSION_ID").is_some() {
        ok("shell integration", "FMN_SESSION_ID is set")
    } else {
        warning(
            "shell integration",
            "FMN_SESSION_ID is not set; source `fmn init <shell>`",
        )
    });
    checks.push(match fs::metadata(&paths.diagnostics) {
        Ok(metadata) if metadata.len() > 0 => warning(
            "hook diagnostics",
            format!(
                "{} bytes; inspect {}",
                metadata.len(),
                paths.diagnostics.display()
            ),
        ),
        Ok(_) => ok("hook diagnostics", "empty"),
        Err(failure) if failure.kind() == std::io::ErrorKind::NotFound => {
            ok("hook diagnostics", "no errors recorded")
        }
        Err(failure) => warning("hook diagnostics", failure.to_string()),
    });

    Report { checks }
}

pub fn append_diagnostic(paths: &AppPaths, message: &str) -> Result<()> {
    paths.ensure_data_dir()?;
    if fs::metadata(&paths.diagnostics)
        .map(|metadata| metadata.len() >= 64 * 1024)
        .unwrap_or(false)
    {
        let old = paths.diagnostics.with_extension("log.old");
        let _ = fs::remove_file(&old);
        fs::rename(&paths.diagnostics, old)?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&paths.diagnostics)?;
    writeln!(file, "{message}")?;
    set_private_file(&paths.diagnostics)?;
    Ok(())
}

fn check_permissions(data_dir: &Path) -> Check {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        match fs::metadata(data_dir) {
            Ok(metadata) => {
                let mode = metadata.permissions().mode() & 0o777;
                if mode & 0o077 == 0 {
                    ok("data permissions", format!("{mode:o}"))
                } else {
                    error(
                        "data permissions",
                        format!("{mode:o}; expected no group/other access"),
                    )
                }
            }
            Err(failure) => error("data permissions", failure.to_string()),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = data_dir;
        ok("data permissions", "platform managed")
    }
}

#[cfg(unix)]
fn set_private_file(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_file(_path: &Path) -> Result<()> {
    Ok(())
}

fn ok(name: &'static str, detail: impl Into<String>) -> Check {
    Check {
        status: Status::Ok,
        name,
        detail: detail.into(),
    }
}

fn warning(name: &'static str, detail: impl Into<String>) -> Check {
    Check {
        status: Status::Warning,
        name,
        detail: detail.into(),
    }
}

fn error(name: &'static str, detail: impl Into<String>) -> Check {
    Check {
        status: Status::Error,
        name,
        detail: detail.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::{Status, append_diagnostic, inspect};
    use crate::paths::AppPaths;
    use crate::storage::Store;

    #[test]
    fn healthy_database_passes_core_checks() {
        let root = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_parts(root.path().join("config"), root.path().join("data"));
        paths.ensure_data_dir().unwrap();
        let store = Store::open(&paths.database).unwrap();
        let report = inspect(&paths, &store);
        assert!(report.is_healthy());
        assert!(
            report
                .checks
                .iter()
                .any(|check| check.name == "database integrity" && check.status == Status::Ok)
        );
    }

    #[test]
    fn diagnostics_are_private_and_reported() {
        #[cfg(unix)]
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_parts(root.path().join("config"), root.path().join("data"));
        append_diagnostic(&paths, "failure one").unwrap();
        let contents = std::fs::read_to_string(&paths.diagnostics).unwrap();
        assert_eq!(contents, "failure one\n");
        #[cfg(unix)]
        assert_eq!(
            std::fs::metadata(&paths.diagnostics)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

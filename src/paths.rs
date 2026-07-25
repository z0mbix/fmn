use std::env;
use std::path::{Path, PathBuf};

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    pub config_file: PathBuf,
    pub data_dir: PathBuf,
    pub database: PathBuf,
    pub diagnostics: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Result<Self> {
        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or(Error::HomeDirectoryUnavailable)?;

        let config_file = match env::var_os("FMN_CONFIG") {
            Some(path) => PathBuf::from(path),
            None => {
                let base = env::var_os("XDG_CONFIG_HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| home.join(".config"));
                base.join("fmn").join("config.toml")
            }
        };

        let data_dir = match env::var_os("FMN_DATA_DIR") {
            Some(path) => PathBuf::from(path),
            None => {
                let base = env::var_os("XDG_DATA_HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| home.join(".local").join("share"));
                base.join("fmn")
            }
        };

        Ok(Self::from_parts(config_file, data_dir))
    }

    pub fn from_parts(config_file: PathBuf, data_dir: PathBuf) -> Self {
        Self {
            config_file,
            database: data_dir.join("history.db"),
            diagnostics: data_dir.join("diagnostics.log"),
            data_dir,
        }
    }

    pub fn ensure_data_dir(&self) -> Result<()> {
        std::fs::create_dir_all(&self.data_dir)?;
        set_private_directory(&self.data_dir)?;
        Ok(())
    }
}

#[cfg(unix)]
fn set_private_directory(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_directory(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::AppPaths;
    use std::path::PathBuf;

    #[test]
    fn derives_database_and_diagnostics_paths() {
        let paths = AppPaths::from_parts(
            PathBuf::from("/config/fmn.toml"),
            PathBuf::from("/data/fmn"),
        );
        assert_eq!(paths.database, PathBuf::from("/data/fmn/history.db"));
        assert_eq!(
            paths.diagnostics,
            PathBuf::from("/data/fmn/diagnostics.log")
        );
    }

    #[cfg(unix)]
    #[test]
    fn creates_private_data_directory() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let paths =
            AppPaths::from_parts(root.path().join("config"), root.path().join("nested/data"));
        paths.ensure_data_dir().unwrap();
        let mode = std::fs::metadata(&paths.data_dir)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700);
    }
}

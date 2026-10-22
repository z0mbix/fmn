use std::process::Command;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SearchScope {
    #[default]
    Global,
    Directory,
    Repository,
    Session,
}

impl SearchScope {
    pub const ALL: [Self; 4] = [
        Self::Global,
        Self::Directory,
        Self::Repository,
        Self::Session,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Global => "All directories",
            Self::Directory => "This directory",
            Self::Repository => "This repository",
            Self::Session => "This terminal session",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SearchContext {
    pub cwd: String,
    pub repository: Option<String>,
    pub session: Option<String>,
}

impl SearchContext {
    pub fn detect(cwd: String) -> Self {
        let repository = Command::new("git")
            .args(["-C", &cwd, "rev-parse", "--show-toplevel"])
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map(|path| path.trim_end_matches(['\r', '\n']).to_owned())
            .filter(|path| !path.is_empty())
            .map(|root| {
                // Shells capture logical $PWD paths. Git reports physical paths
                // (notably /private/var on macOS), so retain the matching logical
                // ancestor for history recorded through that path.
                std::path::Path::new(&cwd)
                    .ancestors()
                    .find(|ancestor| {
                        ancestor
                            .canonicalize()
                            .ok()
                            .is_some_and(|path| path == std::path::Path::new(&root))
                    })
                    .map_or(root.clone(), |ancestor| {
                        ancestor.to_string_lossy().into_owned()
                    })
            });
        let session = std::env::var("FMN_SESSION_ID")
            .ok()
            .filter(|id| !id.is_empty());
        Self {
            cwd,
            repository,
            session,
        }
    }

    pub fn available(&self, scope: SearchScope) -> bool {
        match scope {
            SearchScope::Global | SearchScope::Directory => true,
            SearchScope::Repository => self.repository.is_some(),
            SearchScope::Session => self.session.is_some(),
        }
    }

    pub fn effective(&self, scope: SearchScope) -> SearchScope {
        if self.available(scope) {
            scope
        } else {
            SearchScope::Global
        }
    }

    pub fn next(&self, scope: SearchScope, backwards: bool) -> SearchScope {
        let index = SearchScope::ALL
            .iter()
            .position(|item| *item == scope)
            .unwrap_or(0);
        for step in 1..=SearchScope::ALL.len() {
            let next = if backwards {
                index + 4 - step
            } else {
                index + step
            } % 4;
            let scope = SearchScope::ALL[next];
            if self.available(scope) {
                return scope;
            }
        }
        SearchScope::Global
    }

    // Only fixed SQL fragments reach query construction. Values are always bound.
    pub(crate) fn filter(&self, scope: SearchScope) -> (&'static str, &str, String) {
        match scope {
            SearchScope::Global => ("?1 IS NOT NULL AND ?2 IS NOT NULL", "", String::new()),
            SearchScope::Directory => ("e.cwd = ?1 AND ?2 IS NOT NULL", &self.cwd, String::new()),
            SearchScope::Repository => self.repository.as_deref().map_or(
                ("0 AND ?1 IS NOT NULL AND ?2 IS NOT NULL", "", String::new()),
                |root| {
                    let physical = std::path::Path::new(root).canonicalize().ok()
                        .map_or_else(|| root.to_owned(), |path| path.to_string_lossy().into_owned());
                    ("(e.cwd = ?1 OR substr(e.cwd, 1, length(rtrim(?1, '/')) + 1) = rtrim(?1, '/') || '/'
                       OR e.cwd = ?2 OR substr(e.cwd, 1, length(rtrim(?2, '/')) + 1) = rtrim(?2, '/') || '/')", root, physical)
                }),
            SearchScope::Session => self.session.as_deref().map_or(
                ("0 AND ?1 IS NOT NULL AND ?2 IS NOT NULL", "", String::new()),
                |session| ("e.session_id = ?1 AND e.source = 'live' AND ?2 IS NOT NULL", session, String::new())),
        }
    }
}

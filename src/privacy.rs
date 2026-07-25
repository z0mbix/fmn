use globset::{Glob, GlobSet, GlobSetBuilder};

use crate::config::HistoryConfig;
use crate::{Error, Result};

#[derive(Debug)]
pub struct PrivacyFilter {
    ignore_leading_space: bool,
    patterns: GlobSet,
}

impl PrivacyFilter {
    pub fn new(config: &HistoryConfig) -> Result<Self> {
        let mut builder = GlobSetBuilder::new();
        for pattern in &config.ignore_patterns {
            let glob = Glob::new(pattern).map_err(|source| Error::InvalidIgnorePattern {
                pattern: pattern.clone(),
                source,
            })?;
            builder.add(glob);
        }
        Ok(Self {
            ignore_leading_space: config.ignore_leading_space,
            patterns: builder
                .build()
                .expect("validated glob patterns should build"),
        })
    }

    pub fn should_record(&self, command: &str) -> bool {
        if std::env::var_os("FMN_DISABLE").is_some() {
            return false;
        }
        if command.trim().is_empty() {
            return false;
        }
        if self.ignore_leading_space && command.starts_with(' ') {
            return false;
        }
        !self.patterns.is_match(command)
    }
}

#[cfg(test)]
mod tests {
    use super::PrivacyFilter;
    use crate::config::HistoryConfig;

    fn filter(patterns: &[&str]) -> PrivacyFilter {
        PrivacyFilter::new(&HistoryConfig {
            ignore_leading_space: true,
            ignore_patterns: patterns.iter().map(ToString::to_string).collect(),
        })
        .unwrap()
    }

    #[test]
    fn rejects_empty_and_leading_space_commands() {
        let filter = filter(&[]);
        assert!(!filter.should_record(""));
        assert!(!filter.should_record(" \t"));
        assert!(!filter.should_record(" echo secret"));
        assert!(filter.should_record("echo ok"));
    }

    #[test]
    fn applies_full_command_globs() {
        let filter = filter(&["secret *", "token=*"]);
        assert!(!filter.should_record("secret show"));
        assert!(!filter.should_record("token=abc"));
        assert!(filter.should_record("echo secret show"));
    }

    #[test]
    fn rejects_invalid_glob() {
        assert!(
            PrivacyFilter::new(&HistoryConfig {
                ignore_leading_space: true,
                ignore_patterns: vec!["[".into()],
            })
            .is_err()
        );
    }
}

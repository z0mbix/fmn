use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use crate::model::SearchCandidate;

#[derive(Debug, Clone, PartialEq)]
pub struct RankedCandidate {
    pub candidate: SearchCandidate,
    pub score: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RankedMatch {
    pub index: usize,
    pub score: f64,
}

#[derive(Debug)]
pub struct Ranker {
    matcher: Matcher,
    utf32_buf: Vec<char>,
}

impl Default for Ranker {
    fn default() -> Self {
        Self {
            matcher: Matcher::new(Config::DEFAULT.match_paths()),
            utf32_buf: Vec::new(),
        }
    }
}

impl Ranker {
    pub fn rank_into(
        &mut self,
        candidates: &[SearchCandidate],
        query: &str,
        current_dir_only: bool,
        now_millis: i64,
        output: &mut Vec<RankedMatch>,
    ) {
        let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
        output.clear();
        let mut max_fuzzy = 1_f64;
        let mut max_count = 1_i64;
        for (index, candidate) in candidates.iter().enumerate() {
            if current_dir_only && candidate.cwd_use_count == 0 {
                continue;
            }
            let fuzzy = if query.is_empty() {
                Some(1)
            } else {
                pattern.score(
                    Utf32Str::new(&candidate.command, &mut self.utf32_buf),
                    &mut self.matcher,
                )
            };
            if let Some(fuzzy) = fuzzy {
                max_fuzzy = max_fuzzy.max(fuzzy as f64);
                max_count = max_count.max(candidate.use_count);
                output.push(RankedMatch {
                    index,
                    score: fuzzy as f64,
                });
            }
        }
        let frequency_scale = (1.0 + max_count as f64).ln();
        for item in output.iter_mut() {
            let candidate = &candidates[item.index];
            let age_days =
                now_millis.saturating_sub(candidate.last_seen_at).max(0) as f64 / 86_400_000.0;
            item.score = 0.70 * item.score / max_fuzzy
                + 0.15 * 2_f64.powf(-age_days / 30.0)
                + 0.10 * (1.0 + candidate.use_count as f64).ln() / frequency_scale
                + 0.05 * f64::from(candidate.cwd_use_count > 0);
        }
        output.sort_unstable_by(|left, right| {
            let left_candidate = &candidates[left.index];
            let right_candidate = &candidates[right.index];
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| {
                    right_candidate
                        .last_seen_at
                        .cmp(&left_candidate.last_seen_at)
                })
                .then_with(|| left_candidate.id.cmp(&right_candidate.id))
        });
    }
}

pub fn rank(
    candidates: &[SearchCandidate],
    query: &str,
    current_dir_only: bool,
    now_millis: i64,
) -> Vec<RankedCandidate> {
    let mut output = Vec::new();
    Ranker::default().rank_into(candidates, query, current_dir_only, now_millis, &mut output);
    output
        .into_iter()
        .map(|item| RankedCandidate {
            candidate: candidates[item.index].clone(),
            score: item.score,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::rank;
    use crate::model::SearchCandidate;

    fn candidate(
        id: i64,
        command: &str,
        last_seen_at: i64,
        use_count: i64,
        cwd_use_count: i64,
    ) -> SearchCandidate {
        SearchCandidate {
            id,
            command: command.into(),
            first_seen_at: last_seen_at,
            last_seen_at,
            use_count,
            cwd_use_count,
            cwd_last_seen_at: (cwd_use_count > 0).then_some(last_seen_at),
            latest_cwd: "/tmp".into(),
            latest_exit_code: Some(0),
        }
    }

    #[test]
    fn fuzzy_match_filters_and_orders() {
        let candidates = vec![
            candidate(1, "git checkout main", 100, 1, 0),
            candidate(2, "cargo test", 100, 1, 0),
            candidate(3, "git commit", 100, 1, 0),
        ];
        let ranked = rank(&candidates, "gch", false, 100);
        assert_eq!(ranked[0].candidate.command, "git checkout main");
        assert!(
            !ranked
                .iter()
                .any(|item| item.candidate.command == "cargo test")
        );
    }

    #[test]
    fn current_directory_filter_is_strict() {
        let candidates = vec![
            candidate(1, "global", 100, 1, 0),
            candidate(2, "local", 90, 1, 2),
        ];
        let ranked = rank(&candidates, "", true, 100);
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].candidate.command, "local");
    }

    #[test]
    fn cwd_frequency_and_recency_contribute() {
        let candidates = vec![
            candidate(1, "same command one", 0, 1, 0),
            candidate(2, "same command two", 2_592_000_000, 20, 4),
        ];
        let ranked = rank(&candidates, "same command", false, 2_592_000_000);
        assert_eq!(ranked[0].candidate.id, 2);
    }

    #[test]
    fn ties_are_stable_by_id() {
        let candidates = vec![
            candidate(2, "same", 100, 1, 0),
            candidate(1, "same", 100, 1, 0),
        ];
        let ranked = rank(&candidates, "", false, 100);
        assert_eq!(
            ranked
                .iter()
                .map(|item| item.candidate.id)
                .collect::<Vec<_>>(),
            [1, 2]
        );
    }

    #[test]
    fn reused_ranker_handles_new_queries_scopes_and_empty_results() {
        let candidates = vec![
            candidate(1, "git status", 100, 1, 1),
            candidate(2, "cargo test", 100, 1, 0),
        ];
        let mut ranker = super::Ranker::default();
        let mut matches = Vec::new();
        for (query, local, expected) in [
            ("git", false, vec![1]),
            ("missing", false, vec![]),
            ("cargo", false, vec![2]),
            ("cargo", true, vec![]),
            ("", false, vec![1, 2]),
        ] {
            ranker.rank_into(&candidates, query, local, 100, &mut matches);
            assert_eq!(
                matches
                    .iter()
                    .map(|item| candidates[item.index].id)
                    .collect::<Vec<_>>(),
                expected
            );
        }
    }
}

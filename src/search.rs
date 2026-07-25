use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use crate::model::SearchCandidate;

#[derive(Debug, Clone, PartialEq)]
pub struct RankedCandidate {
    pub candidate: SearchCandidate,
    pub score: f64,
}

pub fn rank(
    candidates: &[SearchCandidate],
    query: &str,
    current_dir_only: bool,
    now_millis: i64,
) -> Vec<RankedCandidate> {
    let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let mut utf32_buf = Vec::new();

    let mut scored: Vec<(SearchCandidate, u32)> = candidates
        .iter()
        .filter(|candidate| !current_dir_only || candidate.cwd_use_count > 0)
        .filter_map(|candidate| {
            let fuzzy = if query.is_empty() {
                1
            } else {
                pattern.score(
                    Utf32Str::new(&candidate.command, &mut utf32_buf),
                    &mut matcher,
                )?
            };
            Some((candidate.clone(), fuzzy))
        })
        .collect();

    let max_fuzzy = scored.iter().map(|(_, score)| *score).max().unwrap_or(1) as f64;
    let max_count = scored
        .iter()
        .map(|(candidate, _)| candidate.use_count)
        .max()
        .unwrap_or(1) as f64;

    let mut ranked: Vec<_> = scored
        .drain(..)
        .map(|(candidate, fuzzy)| {
            let fuzzy_score = fuzzy as f64 / max_fuzzy.max(1.0);
            let age_days =
                now_millis.saturating_sub(candidate.last_seen_at).max(0) as f64 / 86_400_000.0;
            let recency_score = 2_f64.powf(-age_days / 30.0);
            let frequency_score = (1.0 + candidate.use_count as f64).ln() / (1.0 + max_count).ln();
            let cwd_score = if candidate.cwd_use_count > 0 {
                1.0
            } else {
                0.0
            };
            let score = 0.70 * fuzzy_score
                + 0.15 * recency_score
                + 0.10 * frequency_score
                + 0.05 * cwd_score;
            RankedCandidate { candidate, score }
        })
        .collect();

    ranked.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| {
                right
                    .candidate
                    .last_seen_at
                    .cmp(&left.candidate.last_seen_at)
            })
            .then_with(|| left.candidate.id.cmp(&right.candidate.id))
    });
    ranked
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
}

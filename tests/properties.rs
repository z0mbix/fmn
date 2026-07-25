use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use fmn::config::HistoryConfig;
use fmn::import;
use fmn::model::{SearchCandidate, Shell};
use fmn::privacy::PrivacyFilter;
use fmn::search;
use fmn::tui::App;
use proptest::prelude::*;

proptest! {
    #[test]
    fn imports_are_deterministic_and_never_panic(
        input in proptest::collection::vec(any::<char>(), 0..512)
            .prop_map(|characters| characters.into_iter().collect::<String>()),
        shell_index in 0usize..3,
    ) {
        let shell = [Shell::Fish, Shell::Zsh, Shell::Bash][shell_index];
        let first = import::parse(shell, &input);
        let second = import::parse(shell, &input);
        prop_assert_eq!(first, second);
    }

    #[test]
    fn whitespace_commands_are_never_recorded(
        spaces in proptest::collection::vec(prop_oneof![Just(' '), Just('\t'), Just('\n')], 0..64),
    ) {
        let command: String = spaces.into_iter().collect();
        let filter = PrivacyFilter::new(&HistoryConfig::default()).unwrap();
        prop_assert!(!filter.should_record(&command));
    }

    #[test]
    fn ranking_is_deterministic_and_finite(
        query in "[a-zA-Z0-9 ]{0,16}",
        commands in proptest::collection::vec("[a-zA-Z0-9 /_-]{1,40}", 0..64),
    ) {
        let candidates: Vec<_> = commands
            .into_iter()
            .enumerate()
            .map(|(index, command)| SearchCandidate {
                id: index as i64,
                command,
                first_seen_at: index as i64,
                last_seen_at: index as i64,
                use_count: 1 + index as i64,
                cwd_use_count: (index % 2) as i64,
                cwd_last_seen_at: (index % 2 == 1).then_some(index as i64),
                latest_cwd: "/tmp".into(),
                latest_exit_code: Some(0),
            })
            .collect();
        let first = search::rank(&candidates, &query, false, 10_000);
        let second = search::rank(&candidates, &query, false, 10_000);
        prop_assert_eq!(&first, &second);
        prop_assert!(first.iter().all(|item| item.score.is_finite()));
    }

    #[test]
    fn arbitrary_tui_key_sequences_do_not_panic(
        keys in proptest::collection::vec((0u8..8, any::<char>()), 0..256),
    ) {
        let candidate = SearchCandidate {
            id: 1,
            command: "echo test".into(),
            first_seen_at: 1,
            last_seen_at: 1,
            use_count: 1,
            cwd_use_count: 1,
            cwd_last_seen_at: Some(1),
            latest_cwd: "/tmp".into(),
            latest_exit_code: Some(0),
        };
        let mut app = App::new(vec![candidate], String::new(), 1);
        for (kind, character) in keys {
            let key = match kind {
                0 => KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE),
                1 => KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
                2 => KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
                3 => KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
                4 => KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
                5 => KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
                6 => KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                _ => KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL),
            };
            let _ = app.handle_key(key);
        }
    }
}

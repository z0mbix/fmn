#![no_main]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use fmn::model::SearchCandidate;
use fmn::tui::App;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let candidate = SearchCandidate {
        id: 1,
        command: "echo fuzz".into(),
        first_seen_at: 1,
        last_seen_at: 1,
        use_count: 1,
        cwd_use_count: 1,
        cwd_last_seen_at: Some(1),
        latest_cwd: "/tmp".into(),
        latest_exit_code: Some(0),
    };
    let mut app = App::new(vec![candidate], String::new(), 1);
    for byte in data {
        let key = match byte % 8 {
            0 => KeyEvent::new(KeyCode::Char(char::from(*byte)), KeyModifiers::NONE),
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
});


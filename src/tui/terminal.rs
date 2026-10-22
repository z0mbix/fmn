use std::io::{self, Write};

use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::{Error, Result};

pub(super) struct TuiTerminal<W: Write> {
    pub(super) terminal: Terminal<CrosstermBackend<W>>,
    // Dropped after terminal restoration, including during setup failures and
    // panic unwinding. Repeated signals cannot interrupt cleanup.
    signals: ShutdownSignals,
}

impl<W: Write> TuiTerminal<W> {
    pub(super) fn enter(writer: W) -> Result<Self> {
        let signals = ShutdownSignals::enter()?;
        let terminal = Terminal::new(CrosstermBackend::new(writer))?;
        let mut session = Self { terminal, signals };
        enable_raw_mode().map_err(|error| {
            Error::Io(io::Error::new(
                error.kind(),
                format!("could not enable terminal raw mode: {error}"),
            ))
        })?;
        execute!(
            session.terminal.backend_mut(),
            EnterAlternateScreen,
            EnableMouseCapture,
            EnableBracketedPaste
        )?;
        session.terminal.hide_cursor()?;
        Ok(session)
    }

    pub(super) fn interrupted(&self) -> bool {
        self.signals.interrupted()
    }
}

impl<W: Write> Drop for TuiTerminal<W> {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            DisableBracketedPaste,
            DisableMouseCapture,
            LeaveAlternateScreen
        );
        let _ = self.terminal.show_cursor();
    }
}

#[cfg(unix)]
use shutdown::ShutdownSignals;

#[cfg(unix)]
mod shutdown {
    use std::io;
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use signal_hook::consts::{SIGHUP, SIGINT, SIGQUIT, SIGTERM};
    use signal_hook::low_level::{emulate_default_handler, register};

    const INACTIVE: usize = 0;
    const ACTIVE: usize = usize::MAX;
    // One atomic transition orders signal delivery against guard teardown:
    // inactive, active, or the first pending signal number.
    static STATE: AtomicUsize = AtomicUsize::new(INACTIVE);
    // Unregistering signal-hook actions does not restore the default handler.
    // Keep these installed and emulate the default action while inactive.
    static HANDLERS: OnceLock<io::Result<()>> = OnceLock::new();

    pub(super) struct ShutdownSignals;

    impl ShutdownSignals {
        pub(super) fn enter() -> io::Result<Self> {
            HANDLERS
                .get_or_init(|| {
                    for signal in [SIGHUP, SIGINT, SIGQUIT, SIGTERM] {
                        // SAFETY: The handler only uses lock-free atomics and
                        // signal-hook's async-signal-safe default handler.
                        unsafe { register(signal, move || on_signal(signal))? };
                    }
                    Ok(())
                })
                .as_ref()
                .map_err(|error| io::Error::new(error.kind(), error.to_string()))?;
            STATE
                .compare_exchange(INACTIVE, ACTIVE, Ordering::SeqCst, Ordering::SeqCst)
                .map_err(|_| io::Error::other("a terminal session is already active"))?;
            Ok(Self)
        }

        pub(super) fn interrupted(&self) -> bool {
            STATE.load(Ordering::SeqCst) != ACTIVE
        }
    }

    fn on_signal(signal: i32) {
        match STATE.compare_exchange(ACTIVE, signal as usize, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => {}
            Err(INACTIVE) => {
                let _ = emulate_default_handler(signal);
            }
            // Keep the first signal and defer repeats until cleanup finishes.
            Err(_) => {}
        }
    }

    impl Drop for ShutdownSignals {
        fn drop(&mut self) {
            let signal = STATE.swap(INACTIVE, Ordering::SeqCst);
            if signal != ACTIVE && signal != INACTIVE {
                // Restore the usual signal exit status only after the terminal
                // is usable again. Never return a selected shell command.
                let _ = emulate_default_handler(signal as i32);
            }
        }
    }
}

#[cfg(not(unix))]
struct ShutdownSignals;

#[cfg(not(unix))]
impl ShutdownSignals {
    fn enter() -> io::Result<Self> {
        Ok(Self)
    }

    fn interrupted(&self) -> bool {
        false
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::cell::Cell;
    use std::io::{self, Read, Write};
    use std::process::Command;
    use std::rc::Rc;

    use super::TuiTerminal;

    fn wait_for_driver() {
        // Synchronize independently of terminal input.
        let descriptor = std::env::var("FMN_TERMINAL_TEST_CONTROL").unwrap();
        std::fs::File::open(format!("/dev/fd/{descriptor}"))
            .unwrap()
            .read_exact(&mut [0])
            .unwrap();
    }

    #[derive(Clone, Copy)]
    enum FlushAction {
        Panic,
        Pause,
    }

    struct TestWriter {
        writer: Box<dyn Write>,
        action: Rc<Cell<Option<FlushAction>>>,
    }

    impl Write for TestWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.writer.write(bytes)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.writer.flush()?;
            match self.action.take() {
                Some(FlushAction::Panic) => {
                    wait_for_driver();
                    panic!("fmn terminal recovery test panic during setup");
                }
                Some(FlushAction::Pause) => {
                    self.writer.write_all(b"FMN_CLEANUP_PAUSED")?;
                    self.writer.flush()?;
                    wait_for_driver();
                }
                None => {}
            }
            Ok(())
        }
    }

    // Run only in an isolated child of the real-PTY driver. This keeps panic
    // injection out of the shipped CLI and never touches the test runner's TTY.
    #[test]
    fn lifecycle_child() {
        let Ok(scenario) = std::env::var("FMN_TERMINAL_TEST_SCENARIO") else {
            return;
        };
        let output = std::env::var("FMN_TERMINAL_TEST_OUTPUT").unwrap();
        let writer: Box<dyn Write> = if output == "stderr" {
            Box::new(io::stderr())
        } else {
            Box::new(io::stdout())
        };

        let action = Rc::new(Cell::new(
            (scenario == "setup-panic").then_some(FlushAction::Panic),
        ));
        let mut terminal = TuiTerminal::enter(TestWriter {
            writer,
            action: action.clone(),
        })
        .unwrap();
        if scenario == "panic" {
            terminal
                .terminal
                .draw(|_| {
                    wait_for_driver();
                    panic!("fmn terminal recovery test panic during rendering");
                })
                .unwrap();
        } else if scenario == "cleanup-sigterm" {
            wait_for_driver();
            action.set(Some(FlushAction::Pause));
            drop(terminal);
            unreachable!("pending signal must terminate after cleanup");
        } else {
            assert_eq!(scenario, "after-close");
            wait_for_driver();
            drop(terminal);
            // Reusing the guard must work, and a later signal must retain its
            // default action rather than being swallowed by an empty handler.
            drop(TuiTerminal::enter(io::stdout()).unwrap());
            println!("FMN_SESSION_CLOSED");
            io::stdout().flush().unwrap();
            loop {
                std::thread::park();
            }
        }
    }

    fn check_lifecycle(scenario: &str, output_stream: &str) {
        let output = Command::new("python3")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/support/terminal_recovery.py"
            ))
            .args([scenario, "both"])
            .arg(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "tui::terminal::tests::lifecycle_child",
                "--nocapture",
            ])
            .env("FMN_TERMINAL_TEST_SCENARIO", scenario)
            .env("FMN_TERMINAL_TEST_OUTPUT", output_stream)
            .env("RUST_BACKTRACE", "0")
            .output()
            .expect("Python 3 must be installed");
        assert!(
            output.status.success(),
            "{scenario} ({output_stream}):\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn panic_restores_terminal_during_setup_and_rendering() {
        for output_stream in ["stdout", "stderr"] {
            check_lifecycle("panic", output_stream);
            check_lifecycle("setup-panic", output_stream);
        }
    }

    #[test]
    fn sigterm_keeps_default_action_after_terminal_closes() {
        for output_stream in ["stdout", "stderr"] {
            check_lifecycle("after-close", output_stream);
        }
    }

    #[test]
    fn repeated_sigterm_during_cleanup_restores_cursor() {
        for output_stream in ["stdout", "stderr"] {
            check_lifecycle("cleanup-sigterm", output_stream);
        }
    }
}

#![no_main]

use fmn::import;
use fmn::model::Shell;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let input = String::from_utf8_lossy(data);
    for shell in [Shell::Fish, Shell::Zsh, Shell::Bash] {
        let _ = import::parse(shell, &input);
    }
});


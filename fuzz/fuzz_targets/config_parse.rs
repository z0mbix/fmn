#![no_main]

use fmn::config::Config;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(input) = std::str::from_utf8(data) {
        let _ = toml::from_str::<Config>(input);
    }
});


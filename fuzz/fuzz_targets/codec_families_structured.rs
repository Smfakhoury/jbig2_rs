#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../../tests/support/codec_scenarios.rs"]
mod codec_scenarios;

fuzz_target!(|input: &[u8]| codec_scenarios::exercise(input));

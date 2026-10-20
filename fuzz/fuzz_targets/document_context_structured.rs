#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../../tests/support/context_scenarios.rs"]
mod scenarios;

fuzz_target!(|input: &[u8]| scenarios::exercise(input));

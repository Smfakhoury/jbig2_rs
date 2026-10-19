#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: (&[u8], u8, u8)| {
    jbig2_rs::runtime_access::fuzz_mmr(input.0, input.1, input.2);
});

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: (&[u8], u16)| {
    jbig2_rs::runtime_access::fuzz_arithmetic(input.0, input.1);
});

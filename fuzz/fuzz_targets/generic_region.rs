#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: (&[u8], bool)| {
    jbig2_rs::runtime_access::fuzz_generic_region(input.0, input.1);
});

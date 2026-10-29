#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    let _ = jbig2_decoder::inspect_file_header(input);
});

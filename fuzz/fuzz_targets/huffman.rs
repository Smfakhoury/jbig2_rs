#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    jbig2_decoder::runtime_access::fuzz_huffman(input);
});

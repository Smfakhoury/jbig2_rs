#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    let decoder = jbig2_rs::Decoder::default();
    let _ = decoder.inspect(input);
    let _ = decoder.inspect_embedded(input);
    let _ = decoder.decode(input);
    let _ = decoder.decode_embedded(input);
    if let Ok(globals) = decoder.parse_globals(input) {
        let _ = decoder.inspect_embedded_with_globals(input, &globals);
        let _ = decoder.decode_embedded_with_globals(input, &globals);
    }
});

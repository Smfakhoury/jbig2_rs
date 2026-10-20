#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    let mut limits = jbig2_rs::Limits::default();
    limits.max_width = 1024;
    limits.max_height = 1024;
    limits.max_pixels_per_page = 1 << 18;
    limits.max_decoder_managed_memory_bytes = 1 << 23;
    limits.max_stream_bytes = 1 << 16;
    limits.max_pages = 16;
    limits.max_segments = 256;
    limits.max_symbols = 256;
    limits.max_references_per_segment = 64;
    limits.max_total_references = 1024;
    limits.max_reference_graph_depth = 32;
    limits.max_nesting_depth = 32;
    limits.max_work_units = 1 << 20;
    limits.max_cumulative_decoded_bitmap_bytes = 1 << 23;
    limits.max_retained_state_bytes = 1 << 22;
    let decoder = jbig2_rs::Decoder::new(limits);
    let _ = decoder.inspect(input);
    let _ = decoder.inspect_embedded(input);
    let _ = decoder.decode(input);
    let _ = decoder.decode_embedded(input);
    if let Ok(globals) = decoder.parse_globals(input) {
        let _ = decoder.inspect_embedded_with_globals(input, &globals);
        let _ = decoder.decode_embedded_with_globals(input, &globals);
    }
});

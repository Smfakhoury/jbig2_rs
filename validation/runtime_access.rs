//! Validation-only access to private codec primitives.

use crate::arithmetic::{ArithmeticContext, ArithmeticDecoder};
use crate::generic_region::decode_generic_region_segment;
use crate::huffman::HuffmanTable;
use crate::mmr::{decode_mmr, EofbPolicy};
use crate::reader::bit_reader;
use crate::Limits;

/// Exercises bounded arithmetic decisions.
pub fn fuzz_arithmetic(input: &[u8], decisions: u16) {
    let Ok(mut decoder) = ArithmeticDecoder::new(input) else {
        return;
    };
    let mut context = ArithmeticContext::new();
    for _ in 0..decisions.min(4096) {
        if decoder.decode(&mut context).is_err() {
            break;
        }
    }
}

/// Parses and exercises one custom Huffman table.
pub fn fuzz_huffman(input: &[u8]) {
    let limits = strict_limits();
    let Ok(table) = HuffmanTable::from_encoded(input, &limits) else {
        return;
    };
    let mut reader = bit_reader(input);
    let _ = table.decode(&mut reader);
}

/// Exercises one bounded MMR bitmap.
pub fn fuzz_mmr(input: &[u8], width: u8, height: u8) {
    let width = u32::from(width % 64);
    let height = u32::from(height % 64);
    let _ = decode_mmr(input, width, height, &strict_limits(), EofbPolicy::Optional);
}

/// Exercises one generic-region segment payload.
pub fn fuzz_generic_region(input: &[u8], unknown_length: bool) {
    let _ = decode_generic_region_segment(input, unknown_length, &strict_limits());
}

fn strict_limits() -> Limits {
    Limits {
        max_width: 64,
        max_height: 64,
        max_pixels_per_page: 4096,
        max_decoder_managed_memory_bytes: 1 << 20,
        max_stream_bytes: 1 << 20,
        max_work_units: 1 << 20,
        max_cumulative_decoded_bitmap_bytes: 1 << 20,
        max_retained_state_bytes: 1 << 20,
        ..Limits::default()
    }
}

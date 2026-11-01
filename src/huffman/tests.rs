#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

mod benchmarks;
mod cache_map;
mod canonical_decoding;
mod fallback_state;
mod table_validation;
mod wide_arithmetic;

use super::*;

const ALL_STANDARD_TABLES: [StandardHuffmanTable; 15] = [
    StandardHuffmanTable::A,
    StandardHuffmanTable::B,
    StandardHuffmanTable::C,
    StandardHuffmanTable::D,
    StandardHuffmanTable::E,
    StandardHuffmanTable::F,
    StandardHuffmanTable::G,
    StandardHuffmanTable::H,
    StandardHuffmanTable::I,
    StandardHuffmanTable::J,
    StandardHuffmanTable::K,
    StandardHuffmanTable::L,
    StandardHuffmanTable::M,
    StandardHuffmanTable::N,
    StandardHuffmanTable::O,
];

fn bits(bit_string: &str) -> Vec<u8> {
    let mut bytes = vec![0_u8; bit_string.len().div_ceil(8)];
    for (position, bit) in bit_string.bytes().enumerate() {
        if bit == b'1' {
            bytes[position / 8] |= 1 << (7 - position % 8);
        }
    }
    bytes
}

fn decode_bits(table: &HuffmanTable, bit_string: &str) -> HuffmanValue {
    let bytes = bits(bit_string);
    let mut reader = bit_reader(&bytes);
    table.decode(&mut reader).expect("valid encoded value")
}

fn assert_decoders_equal(table: &HuffmanTable, bytes: &[u8], bit_position: usize) {
    let mut optimized = bit_reader(bytes);
    optimized.bit_position = bit_position;
    let mut canonical = optimized;
    let optimized_result = table.decode(&mut optimized);
    let canonical_result = table.decode_canonical(&mut canonical);
    assert_eq!(
        format!("{optimized_result:?}"),
        format!("{canonical_result:?}"),
        "bytes={bytes:02x?}, bit_position={bit_position}"
    );
    assert_eq!(
        optimized.bit_position, canonical.bit_position,
        "bytes={bytes:02x?}, bit_position={bit_position}"
    );
}

fn force_prefix_lookup(table: &mut HuffmanTable) {
    table.prefix_lookup = build_prefix_lookup(
        &table.length_count,
        &table.first_code,
        &table.first_line,
        &table.ordered_lines,
        usize::from(table.max_prefix_len),
    )
    .expect("prefix lookup");
    table.prefer_prefix_lookup = true;
}

fn append_code(output: &mut String, code: WideCode, length: u8) {
    for position in (0..usize::from(length)).rev() {
        let limb = position / 64;
        let bit = position % 64;
        output.push(if code.limbs[limb] >> bit & 1 == 0 {
            '0'
        } else {
            '1'
        });
    }
}

fn encoded_line(table: &HuffmanTable, index: usize, offset: u32) -> String {
    let (code, length) = table.code_for_line(index).expect("active line");
    let line = table.lines[index];
    let mut encoded = String::new();
    append_code(&mut encoded, code, length);
    for position in (0..line.range_len).rev() {
        encoded.push(if offset >> position & 1 == 0 {
            '0'
        } else {
            '1'
        });
    }
    encoded
}

fn code_string(table: &HuffmanTable, index: usize) -> String {
    let (code, length) = table.code_for_line(index).expect("active line");
    let mut result = String::new();
    append_code(&mut result, code, length);
    result
}

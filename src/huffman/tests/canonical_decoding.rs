#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

use super::*;

#[test]
fn annex_b_table_a_codes_and_endpoints() {
    let table =
        HuffmanTable::standard(StandardHuffmanTable::A, &Limits::default()).expect("table A");
    assert_eq!(decode_bits(&table, "00000"), HuffmanValue::Integer(0));
    assert_eq!(decode_bits(&table, "01111"), HuffmanValue::Integer(15));
    assert_eq!(decode_bits(&table, "1000000000"), HuffmanValue::Integer(16));
    assert_eq!(
        decode_bits(&table, "1011111111"),
        HuffmanValue::Integer(271)
    );
    assert_eq!(
        decode_bits(&table, "1100000000000000000"),
        HuffmanValue::Integer(272)
    );
    assert_eq!(
        decode_bits(&table, "11100000000000000000000000000000000"),
        HuffmanValue::Integer(65_808)
    );
}

#[test]
fn annex_b_table_b_oob_and_fixed_values() {
    let table =
        HuffmanTable::standard(StandardHuffmanTable::B, &Limits::default()).expect("table B");
    assert_eq!(decode_bits(&table, "0"), HuffmanValue::Integer(0));
    assert_eq!(decode_bits(&table, "10"), HuffmanValue::Integer(1));
    assert_eq!(decode_bits(&table, "110"), HuffmanValue::Integer(2));
    assert_eq!(decode_bits(&table, "1110000"), HuffmanValue::Integer(3));
    assert_eq!(decode_bits(&table, "111111"), HuffmanValue::OutOfBand);
}

#[test]
fn every_line_of_every_standard_table_decodes() {
    for standard in ALL_STANDARD_TABLES {
        let mut table =
            HuffmanTable::standard(standard, &Limits::default()).expect("standard table");
        force_prefix_lookup(&mut table);
        for (index, line) in table.lines.iter().copied().enumerate() {
            if line.prefix_len == 0 {
                continue;
            }
            if line.kind == LineKind::OutOfBand {
                let encoded = encoded_line(&table, index, 0);
                assert_eq!(decode_bits(&table, &encoded), HuffmanValue::OutOfBand);
                continue;
            }

            let encoded = encoded_line(&table, index, 0);
            assert_eq!(
                decode_bits(&table, &encoded),
                HuffmanValue::Integer(
                    i32::try_from(line.range_low).expect("zero-offset value in range")
                )
            );

            let maximum_offset = match line.kind {
                LineKind::Normal => {
                    if line.range_len == 32 {
                        u32::MAX
                    } else {
                        (1_u32 << line.range_len) - 1
                    }
                }

                LineKind::Lower => {
                    u32::try_from(line.range_low - i64::from(i32::MIN)).expect("lower endpoint")
                }
                LineKind::Upper => {
                    u32::try_from(i64::from(i32::MAX) - line.range_low).expect("upper endpoint")
                }
                LineKind::OutOfBand => 0,
            };
            let expected = if line.kind == LineKind::Lower {
                line.range_low - i64::from(maximum_offset)
            } else {
                line.range_low + i64::from(maximum_offset)
            };
            let encoded = encoded_line(&table, index, maximum_offset);
            assert_eq!(
                decode_bits(&table, &encoded),
                HuffmanValue::Integer(i32::try_from(expected).expect("endpoint in range"))
            );
        }
    }
}

#[test]
fn prefix_lookup_matches_canonical_for_all_standard_prefixes() {
    for standard in ALL_STANDARD_TABLES {
        let table = HuffmanTable::standard(standard, &Limits::default()).expect("standard table");
        for prefix in 0_u32..=u32::from(u16::MAX) {
            let prefix = u16::try_from(prefix).expect("bounded prefix");
            let bytes = [
                u8::try_from(prefix >> 8).expect("high byte"),
                u8::try_from(prefix & 0xff).expect("low byte"),
                0x5a,
                0xa5,
                0x3c,
                0xc3,
            ];
            assert_decoders_equal(&table, &bytes, 0);
        }
        for byte_count in 0..=1 {
            let bytes = [0xa5];
            for bit_position in 0..=byte_count * 8 {
                assert_decoders_equal(&table, &bytes[..byte_count], bit_position);
            }
        }
    }
}

#[test]
fn randomized_custom_prefix_lookup_matches_canonical() {
    let mut random = 0x9e37_79b9_u32;
    for _ in 0..128 {
        random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let leaf_count = usize::try_from(random % 63 + 2).expect("small leaf count");
        let mut lengths = vec![1_u8, 1_u8];
        while lengths.len() < leaf_count {
            random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let index = usize::try_from(random).expect("u32 fits usize") % lengths.len();
            let child_length = lengths[index].checked_add(1).expect("bounded depth");
            lengths[index] = child_length;
            lengths.push(child_length);
        }

        let mut lines = Vec::with_capacity(lengths.len());
        let mut range_low = -10_000_i32;
        for length in lengths {
            random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let range_len = u8::try_from(random & 3).expect("two-bit range length");
            lines.push(HuffmanLine::normal(range_low, length, range_len));
            range_low = range_low
                .checked_add(1_i32 << range_len)
                .expect("small generated ranges");
        }
        let mut table = HuffmanTable::new(&lines, &Limits::default()).expect("custom table");
        force_prefix_lookup(&mut table);

        for _ in 0..512 {
            let mut bytes = [0_u8; 8];
            for byte in &mut bytes {
                random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                *byte = u8::try_from(random >> 24).expect("high byte");
            }
            let bit_position = usize::try_from(random & 7).expect("three-bit position");
            assert_decoders_equal(&table, &bytes, bit_position);
        }
    }
}

#[test]
fn standard_prefixes_match_every_annex_b_table() {
    let expected: [&[&str]; 15] = [
        &["0", "10", "110", "111"],
        &["0", "10", "110", "1110", "11110", "111110", "111111"],
        &[
            "11111110", "0", "10", "110", "1110", "11110", "11111111", "1111110", "111110",
        ],
        &["0", "10", "110", "1110", "11110", "11111"],
        &[
            "1111110", "0", "10", "110", "1110", "11110", "1111111", "111110",
        ],
        &[
            "11100", "1000", "1001", "1010", "11101", "11110", "1011", "00", "010", "011", "1100",
            "1101", "111110", "111111",
        ],
        &[
            "1000", "000", "1001", "11010", "11011", "1010", "1011", "11100", "11101", "1100",
            "001", "010", "011", "11110", "11111",
        ],
        &[
            "11111100",
            "111111100",
            "11111101",
            "111111101",
            "1111100",
            "1010",
            "00",
            "11010",
            "111010",
            "100",
            "111011",
            "1011",
            "1100",
            "11011",
            "11100",
            "111100",
            "1111101",
            "111101",
            "111111110",
            "111111111",
            "01",
        ],
        &[
            "11111100",
            "111111100",
            "11111101",
            "111111101",
            "1111100",
            "1010",
            "010",
            "011",
            "11010",
            "111010",
            "100",
            "111011",
            "1011",
            "1100",
            "11011",
            "11100",
            "111100",
            "1111101",
            "111101",
            "111111110",
            "111111111",
            "00",
        ],
        &[
            "1111010", "11111100", "1111011", "11000", "00", "11001", "110110", "1111100",
            "11111101", "01", "11010", "110111", "111000", "111001", "111010", "111011", "111100",
            "1111101", "11111110", "11111111", "10",
        ],
        &[
            "0", "10", "1100", "1101", "11100", "11101", "111100", "1111010", "1111011", "1111100",
            "1111101", "1111110", "1111111",
        ],
        &[
            "0", "10", "110", "11100", "11101", "111100", "1111010", "1111011", "1111100",
            "1111101", "1111110", "11111110", "11111111",
        ],
        &[
            "0", "100", "1100", "11100", "1101", "101", "111010", "111011", "111100", "111101",
            "111110", "1111110", "1111111",
        ],
        &["100", "101", "0", "110", "111"],
        &[
            "1111100", "111100", "11100", "1100", "100", "0", "101", "1101", "11101", "111101",
            "1111101", "1111110", "1111111",
        ],
    ];

    for (table_number, standard) in ALL_STANDARD_TABLES.into_iter().enumerate() {
        let table = HuffmanTable::standard(standard, &Limits::default()).expect("standard table");
        let expected_codes = expected[table_number];
        assert_eq!(table.lines.len(), expected_codes.len());
        for (index, expected_code) in expected_codes.iter().enumerate() {
            assert_eq!(code_string(&table, index), *expected_code);
        }
    }
}

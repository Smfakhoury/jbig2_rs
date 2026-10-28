#![cfg(test)]

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
#[ignore = "performance microbenchmark"]
#[allow(clippy::cast_precision_loss)]
fn benchmark_fixed_width_prefix_lookup_against_canonical() {
    use std::hint::black_box;
    use std::time::Instant;

    let lines = (0..=u8::MAX)
        .map(|value| HuffmanLine::normal(i32::from(value), 8, 0))
        .collect::<Vec<_>>();
    let table = HuffmanTable::new(&lines, &Limits::default()).expect("fixed-width table");
    let bytes = (0..=u8::MAX).cycle().take(65_536).collect::<Vec<_>>();
    let mut optimized_samples = Vec::new();
    let mut canonical_samples = Vec::new();
    for round in 0..9 {
        for canonical in [round % 2 != 0, round % 2 == 0] {
            let start = Instant::now();
            let mut reader = bit_reader(&bytes);
            for _ in 0..bytes.len() {
                let value = if canonical {
                    table.decode_canonical(&mut reader)
                } else {
                    table.decode(&mut reader)
                }
                .expect("complete fixed-width value");
                black_box(value);
            }
            let elapsed = start.elapsed().as_nanos();
            if canonical {
                canonical_samples.push(elapsed);
            } else {
                optimized_samples.push(elapsed);
            }
        }
    }
    optimized_samples.sort_unstable();
    canonical_samples.sort_unstable();
    let optimized = optimized_samples[optimized_samples.len() / 2];
    let canonical = canonical_samples[canonical_samples.len() / 2];
    let gain = (canonical - optimized) as f64 * 100.0 / canonical as f64;
    eprintln!("huffman-prefix: canonical={canonical}ns optimized={optimized}ns gain={gain:.2}%");
}

#[test]
#[ignore = "performance microbenchmark"]
#[allow(clippy::cast_precision_loss)]
fn benchmark_symbol_id_source_without_line_staging() {
    use std::hint::black_box;
    use std::time::Instant;

    let lengths = vec![12_u8; 4_096];
    let limits = Limits::default();
    let mut specialized_samples = Vec::new();
    let mut staged_samples = Vec::new();
    for round in 0..9 {
        for staged in [round % 2 != 0, round % 2 == 0] {
            let start = Instant::now();
            for _ in 0..32 {
                if staged {
                    let lines = lengths
                        .iter()
                        .copied()
                        .enumerate()
                        .map(|(index, length)| {
                            HuffmanLine::normal(
                                i32::try_from(index).expect("small index"),
                                length,
                                0,
                            )
                        })
                        .collect::<Vec<_>>();
                    black_box(HuffmanTable::new(&lines, &limits).expect("staged table"));
                } else {
                    black_box(
                        HuffmanTable::new_symbol_ids(&lengths, &limits).expect("specialized table"),
                    );
                }
            }
            let elapsed = start.elapsed().as_nanos();
            if staged {
                staged_samples.push(elapsed);
            } else {
                specialized_samples.push(elapsed);
            }
        }
    }
    specialized_samples.sort_unstable();
    staged_samples.sort_unstable();
    let specialized = specialized_samples[specialized_samples.len() / 2];
    let staged = staged_samples[staged_samples.len() / 2];
    let gain = (staged - specialized) as f64 * 100.0 / staged as f64;
    eprintln!("symbol-id-table: staged={staged}ns specialized={specialized}ns gain={gain:.2}%");
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

#[test]
fn wide_zero_constructor_matches_default_and_array_initialization() {
    assert_eq!(WideCode::zero().limbs, [0; 4]);
    assert_eq!(WideCode::zero(), WideCode::default());
    assert_eq!(
        [WideCode::zero(); MAX_PREFIX_LEN + 1],
        [WideCode::default(); MAX_PREFIX_LEN + 1]
    );
}

#[test]
fn raw_canonical_assignment_matches_numeric_histogram_and_stable_order() {
    for line_count in 1..=5_u32 {
        for mut pattern in 0..6_usize.pow(line_count) {
            let mut lines = Vec::new();
            for index in 0..line_count {
                let length = u8::try_from(pattern % 6).expect("small length");
                pattern /= 6;
                lines.push(HuffmanLine::normal(
                    i32::try_from(index).expect("small index"),
                    length,
                    0,
                ));
            }
            let mut counts = [0_u32; MAX_PREFIX_LEN + 1];
            let mut expected_order: Vec<_> = (0..lines.len())
                .filter(|&index| lines[index].prefix_len != 0)
                .collect();
            expected_order.sort_by_key(|&index| (lines[index].prefix_len, index));
            for &index in &expected_order {
                counts[usize::from(lines[index].prefix_len)] += 1;
            }
            let mut first = [0_u64; 6];
            let mut fits = !expected_order.is_empty();
            for length in 1..6 {
                first[length] = 2 * (first[length - 1] + u64::from(counts[length - 1]));
                fits &= first[length] + u64::from(counts[length]) <= 1 << length;
            }
            let result = HuffmanTable::new(&lines, &Limits::default());
            if !fits {
                assert!(matches!(
                    result,
                    Err(HuffmanError::InvalidTable(
                        HuffmanTableError::NoCodes | HuffmanTableError::Oversubscribed
                    ))
                ));
                continue;
            }
            let table = result.expect("numeric prefix allocation fits");
            assert_eq!(table.lines, lines);
            assert_eq!(table.length_count, counts);
            assert_eq!(
                table.ordered_lines,
                expected_order
                    .iter()
                    .map(|&i| u32::try_from(i).unwrap())
                    .collect::<Vec<_>>()
            );
            let maximum = usize::from(table.max_prefix_len);
            assert_eq!(
                maximum,
                usize::from(lines[*expected_order.last().unwrap()].prefix_len)
            );
            for length in 1..=maximum {
                assert_eq!(table.first_code[length].limbs, [first[length], 0, 0, 0]);
                let start = expected_order
                    .iter()
                    .take_while(|&&index| usize::from(lines[index].prefix_len) < length)
                    .count();
                if counts[length] != 0 {
                    assert_eq!(usize::try_from(table.first_line[length]).unwrap(), start);
                }
                for (rank, &index) in expected_order
                    .iter()
                    .skip(start)
                    .take(usize::try_from(counts[length]).unwrap())
                    .enumerate()
                {
                    assert_eq!(
                        table.code_for_line(index),
                        Some((
                            WideCode {
                                limbs: [first[length] + u64::try_from(rank).unwrap(), 0, 0, 0]
                            },
                            lines[index].prefix_len
                        ))
                    );
                }
            }
            for (index, line) in lines.iter().enumerate() {
                if line.prefix_len == 0 {
                    assert!(table.code_for_line(index).is_none());
                }
            }
        }
    }
}

#[test]
fn canonical_assignment_follows_table_order() {
    let lines = [
        HuffmanLine::normal(0, 2, 0),
        HuffmanLine::normal(1, 1, 0),
        HuffmanLine::normal(2, 3, 0),
        HuffmanLine::normal(3, 3, 0),
    ];
    let table = HuffmanTable::new(&lines, &Limits::default()).expect("valid table");
    assert_eq!(decode_bits(&table, "0"), HuffmanValue::Integer(1));
    assert_eq!(decode_bits(&table, "10"), HuffmanValue::Integer(0));
    assert_eq!(decode_bits(&table, "110"), HuffmanValue::Integer(2));
    assert_eq!(decode_bits(&table, "111"), HuffmanValue::Integer(3));
}

#[test]
fn symbol_length_source_matches_materialized_line_oracle() {
    let lengths = [2_u8, 2, 3, 3, 3, 3];
    let lines = lengths
        .iter()
        .copied()
        .enumerate()
        .map(|(index, length)| {
            HuffmanLine::normal(i32::try_from(index).expect("small index"), length, 0)
        })
        .collect::<Vec<_>>();
    let specialized =
        HuffmanTable::new_symbol_ids(&lengths, &Limits::default()).expect("specialized table");
    let materialized = HuffmanTable::new(&lines, &Limits::default()).expect("materialized table");
    assert_eq!(specialized.lines.capacity(), materialized.lines.capacity());
    for prefix in 0_u16..=u16::MAX {
        let bytes = prefix.to_be_bytes();
        let mut specialized_reader = bit_reader(&bytes);
        let mut materialized_reader = bit_reader(&bytes);
        assert_eq!(
            format!("{:?}", specialized.decode(&mut specialized_reader)),
            format!("{:?}", materialized.decode(&mut materialized_reader))
        );
        assert_eq!(
            specialized_reader.bit_position,
            materialized_reader.bit_position
        );
    }
}

#[test]
fn parses_annex_b_example_table() {
    let encoded = [
        0x42, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01, 0x10, 0x49, 0x23, 0x81, 0x80,
    ];
    let table =
        HuffmanTable::from_encoded(&encoded, &Limits::default()).expect("Annex B example table");
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
fn parses_oob_custom_table() {
    // HTOOB=1, HTPS=2, HTRS=1, low=0, high=1.
    // Lines are (prefix=1, range=0), lower=0, upper=2, OOB=2.
    let encoded = [0x03, 0, 0, 0, 0, 0, 0, 0, 1, 0b0100_0101, 0];
    let table = HuffmanTable::from_encoded(&encoded, &Limits::default()).expect("custom table");
    assert_eq!(decode_bits(&table, "0"), HuffmanValue::Integer(0));
    assert_eq!(decode_bits(&table, "11"), HuffmanValue::OutOfBand);
}

#[test]
fn rejects_reserved_flags_and_nonzero_padding() {
    let reserved = [0x80, 0, 0, 0, 0, 0, 0, 0, 1];
    assert!(matches!(
        HuffmanTable::from_encoded(&reserved, &Limits::default()),
        Err(HuffmanError::InvalidTable(
            HuffmanTableError::ReservedFlags(0x80)
        ))
    ));

    let padding = [0, 0, 0, 0, 0, 0, 0, 0, 1, 0b1011_1111];
    assert!(matches!(
        HuffmanTable::from_encoded(&padding, &Limits::default()),
        Err(HuffmanError::InvalidTable(
            HuffmanTableError::NonZeroPadding
        ))
    ));
}

#[test]
fn rejects_invalid_bounds_partition_and_range_length() {
    let bounds = [0, 0, 0, 0, 1, 0, 0, 0, 1];
    assert!(matches!(
        HuffmanTable::from_encoded(&bounds, &Limits::default()),
        Err(HuffmanError::InvalidTable(
            HuffmanTableError::InvalidBounds { low: 1, high: 1 }
        ))
    ));

    // HTRS=6 and the first line has range length 33.
    let long_range = [0x50, 0, 0, 0, 0, 0, 0, 0, 1, 0b1100_0010];
    assert!(matches!(
        HuffmanTable::from_encoded(&long_range, &Limits::default()),
        Err(HuffmanError::InvalidTable(
            HuffmanTableError::InvalidRangeLength(33)
        ))
    ));

    // A two-value interval cannot be partitioned by a four-value line.
    let partition = [0x10, 0, 0, 0, 0, 0, 0, 0, 2, 0b1100_0000];
    assert!(matches!(
        HuffmanTable::from_encoded(&partition, &Limits::default()),
        Err(HuffmanError::InvalidTable(
            HuffmanTableError::InvalidRangePartition
        ))
    ));
}

#[test]
fn raw_wide_code_arithmetic_matches_independent_bit_serial_oracles() {
    for low in [0, 1, 1_u64 << 63, u64::MAX] {
        for middle in [0, 1, u64::MAX] {
            for high in [0, 1, u64::MAX] {
                for top in [0, 1, 1_u64 << 63, u64::MAX] {
                    let value = WideCode {
                        limbs: [low, middle, high, top],
                    };
                    for increment in [0, 1, 2, u32::MAX] {
                        let mut expected = [0_u64; 4];
                        let mut carry = false;
                        for bit in 0..256 {
                            let a = value.limbs[bit / 64] & (1 << (bit % 64)) != 0;
                            let b = bit < 32 && increment & (1 << bit) != 0;
                            if a ^ b ^ carry {
                                expected[bit / 64] |= 1 << (bit % 64);
                            }
                            carry = u8::from(a) + u8::from(b) + u8::from(carry) >= 2;
                        }
                        assert_eq!(
                            value.checked_add_small(increment),
                            if carry {
                                None
                            } else {
                                Some(WideCode { limbs: expected })
                            }
                        );
                    }
                    let expected = [
                        low.wrapping_mul(2),
                        middle.wrapping_mul(2) | u64::from(low >= 1 << 63),
                        high.wrapping_mul(2) | u64::from(middle >= 1 << 63),
                        top.wrapping_mul(2) | u64::from(high >= 1 << 63),
                    ];
                    assert_eq!(
                        value.checked_shift_left_one(),
                        if top >= 1 << 63 {
                            None
                        } else {
                            Some(WideCode { limbs: expected })
                        }
                    );
                    for bit in [0, 1] {
                        let mut appended = expected;
                        appended[0] |= u64::from(bit);
                        assert_eq!(
                            value.append_bit(bit),
                            if top >= 1 << 63 {
                                None
                            } else {
                                Some(WideCode { limbs: appended })
                            }
                        );
                    }
                }
            }
        }
    }
    for left in [0, 1, 2, 1_u64 << 63, u64::MAX] {
        for right in [0, 1, 2, 1_u64 << 63, u64::MAX] {
            for borrow in [false, true] {
                let subtrahend = u128::from(right) + u128::from(borrow);
                let difference = u128::from(left) + (1_u128 << 64) - subtrahend;
                assert_eq!(
                    subtract_limb(left, right, borrow),
                    (
                        u64::try_from(difference & u128::from(u64::MAX)).unwrap(),
                        u128::from(left) < subtrahend,
                    )
                );
            }
        }
    }
}

#[test]
fn raw_wide_distances_match_bit_serial_subtraction_and_exclusive_bounds() {
    let values = [
        WideCode {
            limbs: [0, 0, 0, 0],
        },
        WideCode {
            limbs: [1, 0, 0, 0],
        },
        WideCode {
            limbs: [u64::from(u32::MAX), 0, 0, 0],
        },
        WideCode {
            limbs: [u64::from(u32::MAX) + 1, 0, 0, 0],
        },
        WideCode {
            limbs: [u64::MAX, 0, 0, 0],
        },
        WideCode {
            limbs: [0, 1, 0, 0],
        },
        WideCode {
            limbs: [u64::MAX, u64::MAX, 0, 0],
        },
        WideCode {
            limbs: [0, 0, 1, 0],
        },
        WideCode {
            limbs: [0, 0, 0, 1],
        },
        WideCode {
            limbs: [u64::MAX; 4],
        },
    ];
    for upper in values {
        for lower in values {
            let mut difference = [false; 256];
            let mut borrow = false;
            for (bit, output) in difference.iter_mut().enumerate() {
                let a = upper.limbs[bit / 64] & (1 << (bit % 64)) != 0;
                let b = lower.limbs[bit / 64] & (1 << (bit % 64)) != 0;
                *output = a ^ b ^ borrow;
                borrow = u8::from(a) < u8::from(b) + u8::from(borrow);
            }
            let small = if borrow || difference[32..].iter().any(|bit| *bit) {
                None
            } else {
                Some(
                    difference[..32]
                        .iter()
                        .enumerate()
                        .fold(0_u32, |value, (index, bit)| {
                            value | (u32::from(*bit) << index)
                        }),
                )
            };
            for bound in [0, 1, 2, u32::MAX] {
                assert_eq!(
                    upper.small_distance_from(lower, bound),
                    small.filter(|value| *value < bound)
                );
            }
        }
    }
}

#[test]
fn raw_normal_ranges_match_wide_endpoint_and_contiguity_rules() {
    for low in [
        i64::MIN,
        i64::from(i32::MIN) - 1,
        i64::from(i32::MIN),
        -1,
        0,
        i64::from(i32::MAX) - 1,
        i64::from(i32::MAX),
        i64::from(i32::MAX) + 1,
        i64::MAX,
    ] {
        for range_len in [0, 1, 2, 31, 32, 33, u8::MAX] {
            let lines = [HuffmanLine {
                prefix_len: 1,
                range_len,
                range_low: low,
                kind: LineKind::Normal,
            }];
            let result = validate_lines(HuffmanLines::Borrowed(&lines));
            if range_len > 32 {
                assert!(matches!(result, Err(HuffmanError::InvalidTable(
                        HuffmanTableError::InvalidRangeLength(actual)
                    )) if actual == range_len));
                continue;
            }
            let end = i128::from(low) + (1_i128 << range_len) - 1;
            if end > i128::from(i64::MAX) {
                assert!(matches!(
                    result,
                    Err(HuffmanError::Decode(DecodeError::Malformed(
                        MalformedError::IntegerOverflow
                    )))
                ));
            } else if i32::try_from(low).is_err() || i32::try_from(end).is_err() {
                assert!(matches!(
                    result,
                    Err(HuffmanError::InvalidTable(
                        HuffmanTableError::InvalidRangePartition
                    ))
                ));
            } else {
                result.unwrap();
                for displacement in [-1, 0, 1] {
                    let mut adjacent = lines.to_vec();
                    adjacent.push(HuffmanLine {
                        prefix_len: 1,
                        range_len: 0,
                        range_low: i64::try_from(end + 1 + displacement).unwrap(),
                        kind: LineKind::Normal,
                    });
                    let result = validate_lines(HuffmanLines::Borrowed(&adjacent));
                    let valid = displacement == 0 && end < i128::from(i32::MAX);
                    assert_eq!(result.is_ok(), valid, "{adjacent:?}");
                    if !valid {
                        assert!(matches!(
                            result,
                            Err(HuffmanError::InvalidTable(
                                HuffmanTableError::InvalidRangePartition
                            ))
                        ));
                    }
                }
            }
        }
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn raw_special_line_metadata_rejects_order_duplicates_and_invalid_fields() {
    let normal = HuffmanLine::normal(0, 2, 0);
    let lower = HuffmanLine::lower(-1, 2);
    let upper = HuffmanLine::upper(1, 2);
    let oob = HuffmanLine::out_of_band(2);
    let cases = [
        (vec![], Some(HuffmanTableError::NoCodes)),
        (
            vec![lower, upper, oob],
            Some(HuffmanTableError::InvalidRangePartition),
        ),
        (vec![normal, lower, upper, oob], None),
        (vec![normal, lower], None),
        (vec![normal, upper], None),
        (vec![normal, oob], None),
        (
            vec![normal, lower, normal],
            Some(HuffmanTableError::InvalidLineOrder),
        ),
        (
            vec![normal, upper, lower],
            Some(HuffmanTableError::InvalidLineOrder),
        ),
        (
            vec![normal, oob, lower],
            Some(HuffmanTableError::InvalidLineOrder),
        ),
        (
            vec![normal, oob, upper],
            Some(HuffmanTableError::InvalidLineOrder),
        ),
        (
            vec![normal, lower, lower],
            Some(HuffmanTableError::DuplicateSpecialLine),
        ),
        (
            vec![normal, upper, upper],
            Some(HuffmanTableError::DuplicateSpecialLine),
        ),
        (
            vec![normal, oob, oob],
            Some(HuffmanTableError::DuplicateSpecialLine),
        ),
        (
            vec![
                normal,
                HuffmanLine {
                    range_len: 0,
                    ..lower
                },
            ],
            Some(HuffmanTableError::InvalidLineOrder),
        ),
        (
            vec![
                normal,
                HuffmanLine {
                    range_len: 0,
                    ..upper
                },
            ],
            Some(HuffmanTableError::InvalidLineOrder),
        ),
        (
            vec![
                normal,
                HuffmanLine {
                    range_len: 1,
                    ..oob
                },
            ],
            Some(HuffmanTableError::InvalidSpecialLine),
        ),
        (
            vec![normal, HuffmanLine::lower(-2, 2)],
            Some(HuffmanTableError::InvalidSpecialLine),
        ),
        (
            vec![normal, HuffmanLine::upper(2, 2)],
            Some(HuffmanTableError::InvalidSpecialLine),
        ),
        (
            vec![
                normal,
                HuffmanLine {
                    range_low: i64::MIN,
                    ..upper
                },
            ],
            Some(HuffmanTableError::InvalidSpecialLine),
        ),
        (
            vec![
                HuffmanLine::normal(i32::MAX, 2, 0),
                HuffmanLine {
                    range_low: i64::from(i32::MAX) + 1,
                    ..upper
                },
            ],
            Some(HuffmanTableError::InvalidSpecialLine),
        ),
        (
            vec![
                HuffmanLine::normal(i32::MIN, 2, 0),
                HuffmanLine::lower(i64::from(i32::MIN) - 1, 2),
            ],
            Some(HuffmanTableError::InvalidSpecialLine),
        ),
        (
            vec![
                HuffmanLine::normal(i32::MIN, 2, 0),
                HuffmanLine::lower(i64::from(i32::MIN) - 1, 0),
            ],
            None,
        ),
    ];
    for (lines, expected) in cases {
        match (validate_lines(HuffmanLines::Borrowed(&lines)), expected) {
            (Ok(()), None) => {}
            (Err(HuffmanError::InvalidTable(actual)), Some(expected)) => {
                assert_eq!(actual, expected, "{lines:?}");
            }
            (actual, expected) => panic!("{lines:?}: got {actual:?}, expected {expected:?}"),
        }
    }
}

#[test]
fn raw_table_memory_limits_match_wide_sizes_and_error_priority() {
    for line_count in [0, 1, 15, 256, u32::MAX as usize, usize::MAX] {
        for prefix_lookup in [false, true] {
            let bytes = size_of::<HuffmanTable>() as u128
                + line_count as u128 * (size_of::<HuffmanLine>() + size_of::<u32>()) as u128
                + if prefix_lookup { 256 * 2 } else { 0 };
            for symbol_limit in [0, 15, u32::MAX] {
                for memory_limit in [0, u64::try_from(bytes).unwrap_or(u64::MAX), u64::MAX] {
                    let limits = Limits {
                        max_symbols: symbol_limit,
                        max_decoder_managed_memory_bytes: memory_limit,
                        ..Limits::default()
                    };
                    let result = check_table_memory(line_count, prefix_lookup, &limits);
                    if line_count as u128 > u128::from(symbol_limit) {
                        assert!(matches!(result, Err(HuffmanError::Decode(
                                DecodeError::LimitExceeded(LimitExceeded {
                                    kind: LimitKind::Symbols, limit, actual,
                                })
                            )) if limit == u64::from(symbol_limit) && u128::from(actual) == line_count as u128));
                    } else if bytes > usize::MAX as u128 || bytes > u128::from(u64::MAX) {
                        assert!(matches!(
                            result,
                            Err(HuffmanError::Decode(DecodeError::Malformed(
                                MalformedError::IntegerOverflow
                            )))
                        ));
                    } else if bytes > u128::from(memory_limit) {
                        assert!(matches!(result, Err(HuffmanError::Decode(
                                DecodeError::LimitExceeded(LimitExceeded {
                                    kind: LimitKind::Memory, limit, actual,
                                })
                            )) if limit == memory_limit && u128::from(actual) == bytes));
                    } else {
                        result.unwrap();
                    }
                }
            }
        }
    }
}

#[test]
fn rejects_oversubscribed_and_structurally_invalid_tables() {
    let oversubscribed = [
        HuffmanLine::normal(0, 1, 0),
        HuffmanLine::normal(1, 1, 0),
        HuffmanLine::normal(2, 1, 0),
    ];
    assert!(matches!(
        HuffmanTable::new(&oversubscribed, &Limits::default()),
        Err(HuffmanError::InvalidTable(
            HuffmanTableError::Oversubscribed
        ))
    ));

    let gap = [HuffmanLine::normal(0, 1, 0), HuffmanLine::normal(2, 1, 0)];
    assert!(matches!(
        HuffmanTable::new(&gap, &Limits::default()),
        Err(HuffmanError::InvalidTable(
            HuffmanTableError::InvalidRangePartition
        ))
    ));

    let no_codes = [HuffmanLine::normal(0, 0, 0)];
    assert!(matches!(
        HuffmanTable::new(&no_codes, &Limits::default()),
        Err(HuffmanError::InvalidTable(HuffmanTableError::NoCodes))
    ));
}

#[test]
fn invalid_prefix_and_eof_are_oob_safe() {
    let lines = [HuffmanLine::normal(0, 1, 0), HuffmanLine::normal(1, 3, 0)];
    let table = HuffmanTable::new(&lines, &Limits::default()).expect("valid sparse table");

    let mut invalid_reader = bit_reader(&[0b1100_0000]);
    assert!(matches!(
        table.decode(&mut invalid_reader),
        Err(HuffmanError::InvalidTable(HuffmanTableError::InvalidCode))
    ));

    let mut eof_reader = bit_reader(&[0b0000_0001]);
    eof_reader.bit_position = 7;
    assert!(matches!(
        table.decode(&mut eof_reader),
        Err(HuffmanError::Decode(DecodeError::Malformed(
            MalformedError::UnexpectedEof {
                offset: 8,
                needed: 1,
                remaining: 0
            }
        )))
    ));
    assert_eq!(eof_reader.bit_position, 8);
}

#[test]
fn open_ranges_reject_values_outside_i32() {
    let table =
        HuffmanTable::standard(StandardHuffmanTable::A, &Limits::default()).expect("table A");
    let encoded = format!("111{:032b}", u32::MAX);
    let bytes = bits(&encoded);
    let mut reader = bit_reader(&bytes);
    assert!(matches!(
        table.decode(&mut reader),
        Err(HuffmanError::Decode(DecodeError::Malformed(
            MalformedError::IntegerOverflow
        )))
    ));
}

#[test]
fn honors_memory_and_stream_limits() {
    let limits = Limits {
        max_decoder_managed_memory_bytes: 1,
        ..Limits::default()
    };
    assert!(matches!(
        HuffmanTable::standard(StandardHuffmanTable::A, &limits),
        Err(HuffmanError::Decode(DecodeError::LimitExceeded(
            LimitExceeded {
                kind: LimitKind::Memory,
                ..
            }
        )))
    ));

    let limits = Limits {
        max_stream_bytes: 8,
        ..Limits::default()
    };
    let encoded = [0_u8; 9];
    assert!(matches!(
        HuffmanTable::from_encoded(&encoded, &limits),
        Err(HuffmanError::Decode(DecodeError::LimitExceeded(
            LimitExceeded {
                kind: LimitKind::StreamBytes,
                limit: 8,
                actual: 9
            }
        )))
    ));
}

#[test]
fn wide_code_fit_matches_independent_significant_bit_counts() {
    let mut cases = vec![
        WideCode::default(),
        WideCode {
            limbs: [u64::MAX; 4],
        },
        WideCode {
            limbs: [0xaaaa_aaaa_aaaa_aaaa, 0x5555_5555_5555_5555, 0, 1],
        },
    ];
    for position in 0..256 {
        let mut limbs = [0; 4];
        limbs[position / 64] = 1_u64 << (position % 64);
        cases.push(WideCode { limbs });
    }
    for code in cases {
        let significant_bits = code
            .limbs
            .iter()
            .enumerate()
            .filter(|(_, limb)| **limb != 0)
            .map(|(index, limb)| {
                index * 64 + usize::try_from(u64::BITS - limb.leading_zeros()).expect("bits")
            })
            .max()
            .unwrap_or(0);
        for width in 0..=257 {
            assert_eq!(
                code.fits_in_bits(width),
                significant_bits <= width,
                "{code:?}, width={width}"
            );
        }
    }
}

#[test]
fn canonical_width_boundaries_reject_oversubscription_and_preserve_valid_codes() {
    for boundary in [1_u8, 63, 64, 65, 127, 128, 129, 191, 192, 193, 255] {
        let mut lines: Vec<_> = (1..boundary)
            .map(|length| HuffmanLine::normal(i32::from(length - 1), length, 0))
            .collect();
        for offset in 0..2 {
            lines.push(HuffmanLine::normal(
                i32::from(boundary - 1) + offset,
                boundary,
                0,
            ));
            let table = HuffmanTable::new(&lines, &Limits::default()).expect("valid tree");
            let last = lines.len() - 1;
            assert_eq!(
                decode_bits(&table, &encoded_line(&table, last, 0)),
                HuffmanValue::Integer(i32::try_from(last).expect("small line index"))
            );
        }
        lines.push(HuffmanLine::normal(i32::from(boundary) + 1, boundary, 0));
        assert!(
            matches!(
                HuffmanTable::new(&lines, &Limits::default()),
                Err(HuffmanError::InvalidTable(
                    HuffmanTableError::Oversubscribed
                ))
            ),
            "oversubscribed {boundary}-bit tree was accepted"
        );
    }
}

#[test]
fn supports_prefixes_longer_than_machine_words() {
    let mut lines = Vec::new();
    for value in 0..254_i32 {
        lines.push(HuffmanLine::normal(
            value,
            u8::try_from(value + 1).expect("length"),
            0,
        ));
    }
    lines.push(HuffmanLine::normal(254, 255, 0));
    lines.push(HuffmanLine::normal(255, 255, 0));
    let table = HuffmanTable::new(&lines, &Limits::default()).expect("255-bit table");
    let encoded = encoded_line(&table, 255, 0);
    assert_eq!(encoded.len(), 255);
    assert_eq!(decode_bits(&table, &encoded), HuffmanValue::Integer(255));
}

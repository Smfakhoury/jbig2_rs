#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

use super::*;

#[test]
#[allow(clippy::too_many_lines)]
fn horizontal_pairs_match_numeric_endpoints_and_previous_inline_procedure() {
    let mut pairs = 0;
    let mut comparisons = 0;
    for black in [false, true] {
        let (first_table, second_table) = if black {
            (&BLACK_TERMINATING, &WHITE_TERMINATING)
        } else {
            (&WHITE_TERMINATING, &BLACK_TERMINATING)
        };
        for first_terminal in first_table {
            for second_terminal in second_table {
                for start in [0, 1, 7] {
                    for (first_makeup, second_makeup) in [(0, 0), (64, 2560), (2560, 64)] {
                        let mut codes = Vec::new();
                        if first_makeup != 0 {
                            codes.push(
                                (if black {
                                    BLACK_MAKEUP.iter()
                                } else {
                                    WHITE_MAKEUP.iter()
                                })
                                .chain(COMMON_MAKEUP.iter())
                                .copied()
                                .find(|code| code.value == first_makeup)
                                .unwrap(),
                            );
                        }
                        codes.push(*first_terminal);
                        if second_makeup != 0 {
                            codes.push(
                                COMMON_MAKEUP
                                    .iter()
                                    .chain(if black {
                                        WHITE_MAKEUP.iter()
                                    } else {
                                        BLACK_MAKEUP.iter()
                                    })
                                    .copied()
                                    .find(|code| code.value == second_makeup)
                                    .unwrap(),
                            );
                        }
                        codes.push(*second_terminal);
                        let first = u32::from(first_makeup) + u32::from(first_terminal.value);
                        let second = u32::from(second_makeup) + u32::from(second_terminal.value);
                        let total = first + second;
                        let (bytes, bits) = run_sequence_bytes(&codes, start);
                        pairs += 1;
                        for x in [0, u32::MAX - total - 1] {
                            for spare in 0..=1 {
                                let width = x + total + spare;
                                let mut reader = bit_reader(&bytes);
                                reader.bit_position = start;
                                let mut previous = bit_reader(&bytes);
                                previous.bit_position = start;
                                let actual = decode_horizontal_runs(&mut reader, black, x, width)
                                    .map_err(horizontal_error);
                                let expected =
                                    previous_horizontal_runs(&mut previous, black, x, width)
                                        .map_err(horizontal_error);
                                assert_eq!(actual, expected);
                                if total == 0 {
                                    assert_eq!(
                                        actual,
                                        Err(MalformedError::InvalidMmr(MmrErrorKind::NonProgress))
                                    );
                                } else {
                                    assert_eq!(actual.unwrap(), (x + first, x + total));
                                }
                                assert_eq!(reader.bit_position, start + bits);
                                assert_eq!(reader.bit_position, previous.bit_position);
                                assert_eq!(reader.bytes, previous.bytes);
                                comparisons += 1;
                            }
                        }
                        if total != 0 {
                            let mut reader = bit_reader(&bytes);
                            reader.bit_position = start;
                            let mut previous = bit_reader(&bytes);
                            previous.bit_position = start;
                            let error = decode_horizontal_runs(&mut reader, black, 0, total - 1)
                                .unwrap_err();
                            let old_error =
                                previous_horizontal_runs(&mut previous, black, 0, total - 1)
                                    .unwrap_err();
                            assert_eq!(horizontal_error(error), horizontal_error(old_error));
                            let mut sum = 0;
                            let mut consumed = start;
                            for code in &codes {
                                sum += u32::from(code.value);
                                consumed += usize::from(code.length);
                                if sum >= total {
                                    break;
                                }
                            }
                            assert_eq!(reader.bit_position, consumed);
                            assert_eq!(reader.bit_position, previous.bit_position);
                            assert_eq!(reader.bytes, previous.bytes);
                            comparisons += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(pairs, 73_728);
    assert_eq!(comparisons, 368_634);
}

#[test]
fn horizontal_pair_partial_failures_preserve_previous_error_fields_and_cursor() {
    let inputs: &[&[u8]] = &[
        &[],
        &[0],
        &[0xff],
        &[0x35],
        &[0x35, 0],
        &[0x35, 0x01],
        &[0x03, 0xc0],
        &[0xde, 0xcd, 0x40],
    ];
    let mut errors = 0;
    let mut cases = 0;
    for bytes in inputs {
        for start in 0..=bytes.len() * 8 {
            for black in [false, true] {
                for (x, width) in [
                    (0, 0),
                    (0, 1),
                    (0, 63),
                    (0, 64),
                    (0, 4096),
                    (u32::MAX, u32::MAX),
                    (1, 0),
                ] {
                    let mut reader = bit_reader(bytes);
                    reader.bit_position = start;
                    let mut previous = bit_reader(bytes);
                    previous.bit_position = start;
                    let actual = decode_horizontal_runs(&mut reader, black, x, width)
                        .map_err(horizontal_error);
                    let expected = previous_horizontal_runs(&mut previous, black, x, width)
                        .map_err(horizontal_error);
                    errors += usize::from(actual.is_err());
                    cases += 1;
                    assert_eq!(actual, expected);
                    assert_eq!(reader.bit_position, previous.bit_position);
                    assert_eq!(reader.bytes, previous.bytes);
                    if x > width {
                        assert_eq!(reader.bit_position, start);
                    }
                }
            }
        }
    }
    assert_eq!(cases, 1456);
    assert!(errors > 0);
}

#[test]
fn makeup_terminal_sequences_have_exact_sums_extent_and_overshoot_errors() {
    let mut cases = 0;
    for black in [false, true] {
        let terminating = if black {
            &BLACK_TERMINATING
        } else {
            &WHITE_TERMINATING
        };
        let makeup = if black { &BLACK_MAKEUP } else { &WHITE_MAKEUP };
        let choices = [64, 128, 1728, 2560].map(|value| {
            *makeup
                .iter()
                .chain(&COMMON_MAKEUP)
                .find(|code| code.value == value)
                .expect("project makeup entry")
        });
        for count in 0..=3 {
            for pattern in 0..4_usize.pow(count) {
                let mut prefix = Vec::new();
                let mut selected = pattern;
                for _ in 0..count {
                    prefix.push(choices[selected % 4]);
                    selected /= 4;
                }
                for terminal in terminating {
                    let mut codes = prefix.clone();
                    codes.push(*terminal);
                    let expected: u32 = codes.iter().map(|code| u32::from(code.value)).sum();
                    for start in 0..8 {
                        let (encoded, bits) = run_sequence_bytes(&codes, start);
                        for remaining in [expected, expected + 1] {
                            let mut reader = BitReader {
                                bytes: &encoded,
                                bit_position: start,
                            };
                            assert_eq!(
                                decode_run(&mut reader, black, remaining).unwrap(),
                                expected
                            );
                            assert_eq!(reader.bit_position(), start + bits);
                            assert_eq!(reader.bytes.as_ptr(), encoded.as_ptr());
                        }
                        if expected > 0 {
                            let remaining = expected - 1;
                            let mut sum = 0;
                            let mut consumed = 0;
                            for code in &codes {
                                sum += u32::from(code.value);
                                consumed += usize::from(code.length);
                                if sum > remaining {
                                    break;
                                }
                            }
                            let mut reader = BitReader {
                                bytes: &encoded,
                                bit_position: start,
                            };
                            assert_kind(
                                decode_run(&mut reader, black, remaining).unwrap_err(),
                                MmrErrorKind::RunExceedsRow,
                            );
                            assert_eq!(reader.bit_position(), start + consumed);
                            assert_eq!(reader.bytes.as_ptr(), encoded.as_ptr());
                        }
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases, 87_040);
}

#[test]
fn makeup_without_terminal_reports_exact_physical_eof_after_the_code() {
    for black in [false, true] {
        let makeup = if black { &BLACK_MAKEUP } else { &WHITE_MAKEUP };
        for value in [64, 128, 1728, 2560] {
            let code = *makeup
                .iter()
                .chain(&COMMON_MAKEUP)
                .find(|code| code.value == value)
                .unwrap();
            let start = (8 - usize::from(code.length) % 8) % 8;
            let (encoded, count) = run_sequence_bytes(&[code], start);
            let end = start + count;
            let bytes = &encoded[..end / 8];
            let mut reader = BitReader {
                bytes,
                bit_position: start,
            };
            let MmrError::Reader(error) =
                decode_run(&mut reader, black, u32::from(value)).unwrap_err()
            else {
                panic!("terminating code is required")
            };
            assert_eq!(
                error,
                MalformedError::UnexpectedEof {
                    offset: end as u64,
                    needed: 1,
                    remaining: 0,
                }
            );
            assert_eq!(reader.bit_position(), end);
            assert_eq!(reader.bytes.as_ptr(), bytes.as_ptr());
        }
    }
}

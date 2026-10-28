#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

use super::*;

#[test]
fn recorded_span_cache_tracks_scalar_pixels_across_zero_and_colored_runs() {
    let mut sequences = 0;
    let mut prefixes = 0;
    for width in 0..=12 {
        for first in 0..=width {
            for second in first..=width {
                for colors in 0..8_u8 {
                    for row in 0..2 {
                        let mut bitmap = Bitmap::blank(width, 2, &Limits::default()).unwrap();
                        let mut transitions = Vec::new();
                        let mut emitted = false;
                        let mut expected = vec![false; usize::try_from(width).unwrap()];
                        let mut start = 0;
                        for (index, end) in [first, second, width].into_iter().enumerate() {
                            let black = colors & (1 << index) != 0;
                            for x in start..end {
                                expected[usize::try_from(x).unwrap()] = black;
                            }
                            fill_recorded_span(
                                &mut bitmap,
                                row,
                                start,
                                end,
                                black,
                                width,
                                &mut transitions,
                                &mut emitted,
                            )
                            .unwrap();
                            let mut expected_transitions = Vec::new();
                            let mut last = false;
                            for x in 0..end {
                                let pixel = expected[usize::try_from(x).unwrap()];
                                if pixel != last {
                                    expected_transitions.push(x);
                                    last = pixel;
                                }
                            }
                            assert_eq!(transitions, expected_transitions);
                            assert_eq!(emitted, last);
                            for x in 0..width {
                                assert_eq!(
                                    bitmap.pixel(x, row),
                                    Some(expected[usize::try_from(x).unwrap()])
                                );
                                assert_eq!(bitmap.pixel(x, 1 - row), Some(false));
                                let cache_black = transitions
                                    .iter()
                                    .filter(|&&position| position <= x)
                                    .count()
                                    % 2
                                    == 1;
                                assert_eq!(
                                    x < end && cache_black,
                                    expected[usize::try_from(x).unwrap()]
                                );
                            }
                            prefixes += 1;
                            start = end;
                        }
                        finish_transition_row(&mut transitions, width);
                        assert_eq!(&transitions[transitions.len() - 2..], &[width, width]);
                        for x in 0..width {
                            let cache_black = transitions
                                .iter()
                                .filter(|&&position| position <= x)
                                .count()
                                % 2
                                == 1;
                            assert_eq!(cache_black, expected[usize::try_from(x).unwrap()]);
                        }
                        sequences += 1;
                    }
                }
            }
        }
    }
    assert_eq!(sequences, 7280);
    assert_eq!(prefixes, 21840);
}

#[test]
fn decoded_reference_cache_matches_the_last_physical_row() {
    type RasterCase<'a> = (&'a [u8], u32, usize, [[bool; 8]; 2]);
    let island = [false, false, true, true, false, false, false, false];
    let cases: [RasterCase<'_>; 4] = [
        (&[0x80], 1, 1, [[false; 8]; 2]),
        (&[0x26, 0xA2, 0x80], 1, 17, [[true; 8]; 2]),
        (&[0x26, 0xA2, 0xE0], 2, 19, [[true; 8]; 2]),
        (&[0x2F, 0xF8], 2, 13, [island; 2]),
    ];
    for (data, height, expected_bits, expected) in cases {
        let width = 8;
        let mut scratch = MmrScratch::default();
        let (bitmap, reader) =
            decode_bitmap(data, width, height, &Limits::default(), &mut scratch).unwrap();
        assert_eq!(reader.bit_position, expected_bits);
        let cache = if height % 2 == 0 {
            &scratch.transitions
        } else {
            &scratch.row_transitions
        };
        assert_eq!(&cache[cache.len() - 2..], &[width, width]);
        for row in 0..height {
            for x in 0..width {
                assert_eq!(
                    bitmap.pixel(x, row),
                    Some(expected[usize::try_from(row).unwrap()][usize::try_from(x).unwrap()])
                );
            }
        }
        for x in 0..width {
            let black = cache.iter().filter(|&&position| position <= x).count() % 2 == 1;
            assert_eq!(bitmap.pixel(x, height - 1), Some(black));
        }
    }
}

fn previous_horizontal_runs(
    reader: &mut BitReader<'_>,
    black: bool,
    x: u32,
    width: u32,
) -> Result<(u32, u32), MmrError> {
    let remaining = width.checked_sub(x).ok_or(MmrError::InvalidTransition)?;
    let first = decode_run(reader, black, remaining)?;
    let after_first = x.checked_add(first).ok_or(MmrError::IntegerOverflow)?;
    let second_remaining = width
        .checked_sub(after_first)
        .ok_or(MmrError::RunExceedsRow)?;
    let second = decode_run(reader, !black, second_remaining)?;
    let end = after_first
        .checked_add(second)
        .ok_or(MmrError::IntegerOverflow)?;
    if end > width {
        return Err(MmrError::RunExceedsRow);
    }
    if end == x {
        return Err(MmrError::NonProgress);
    }
    Ok((after_first, end))
}

fn horizontal_error(error: MmrError) -> MalformedError {
    let DecodeError::Malformed(error) = DecodeError::from(error) else {
        panic!("horizontal runs produce typed malformed errors");
    };
    error
}

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

fn run_sequence_bytes(codes: &[RunCode], start: usize) -> ([u8; 8], usize) {
    let mut packed = 0_u64;
    let mut count = 0;
    for code in codes {
        packed = (packed << code.length) | u64::from(code.bits);
        count += usize::from(code.length);
    }
    assert!(start + count < 64);
    let leading = if start == 0 {
        0
    } else {
        (0xa5_u64 << 56) & (u64::MAX << (64 - start))
    };
    (
        (leading | (packed << (64 - start - count)) | 1).to_be_bytes(),
        count,
    )
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

fn prefix_window_bytes(window: u16, width: u8, start: usize) -> [u8; 8] {
    let value = u64::from(window) << (64 - start - usize::from(width));
    let leading = if start == 0 {
        0
    } else {
        (0xa5_u64 << 56) & (u64::MAX << (64 - start))
    };
    (value | leading | 0x5a).to_be_bytes()
}

#[test]
fn every_mode_window_and_alignment_matches_scalar_prefix_lookup() {
    for window in 0..(1_u16 << MAX_MODE_CODE_BITS) {
        let expected = MODE_CODES
            .iter()
            .filter(|code| window >> (MAX_MODE_CODE_BITS - code.length) == code.bits)
            .min_by_key(|code| code.length);
        for start in 0..8 {
            let encoded = prefix_window_bytes(window, MAX_MODE_CODE_BITS, start);
            let mut reader = BitReader {
                bytes: &encoded,
                bit_position: start,
            };
            if let Some(code) = expected {
                assert_eq!(decode_mode(&mut reader).unwrap(), code.mode);
                assert_eq!(reader.bit_position(), start + usize::from(code.length));
                let packed = u64::from_be_bytes(encoded);
                let bits = (packed >> (64 - reader.bit_position())) & ((1_u64 << code.length) - 1);
                assert_eq!(bits, u64::from(code.bits));
            } else {
                assert_kind(
                    decode_mode(&mut reader).unwrap_err(),
                    MmrErrorKind::InvalidMode { bit_offset: start },
                );
                assert_eq!(
                    reader.bit_position(),
                    start + usize::from(MAX_MODE_CODE_BITS)
                );
            }
            assert_eq!(reader.bytes.as_ptr(), encoded.as_ptr());
        }
    }
}

#[test]
fn every_run_window_colour_and_alignment_matches_scalar_prefix_lookup() {
    for black in [false, true] {
        let terminating = if black {
            &BLACK_TERMINATING
        } else {
            &WHITE_TERMINATING
        };
        let makeup = if black { &BLACK_MAKEUP } else { &WHITE_MAKEUP };
        for window in 0..(1_u16 << MAX_RUN_CODE_BITS) {
            let expected = terminating
                .iter()
                .chain(makeup)
                .chain(&COMMON_MAKEUP)
                .filter(|code| window >> (MAX_RUN_CODE_BITS - code.length) == code.bits)
                .min_by_key(|code| code.length);
            for start in 0..8 {
                let encoded = prefix_window_bytes(window, MAX_RUN_CODE_BITS, start);
                let mut reader = BitReader {
                    bytes: &encoded,
                    bit_position: start,
                };
                if let Some(code) = expected {
                    let actual = decode_run_code(&mut reader, black).unwrap();
                    assert_eq!(
                        (actual.bits, actual.length, actual.value, actual.terminal),
                        (code.bits, code.length, code.value, code.terminal)
                    );
                    assert_eq!(reader.bit_position(), start + usize::from(code.length));
                    let packed = u64::from_be_bytes(encoded);
                    let bits =
                        (packed >> (64 - reader.bit_position())) & ((1_u64 << actual.length) - 1);
                    assert_eq!(bits, u64::from(actual.bits));
                } else {
                    assert_kind(
                        decode_run_code(&mut reader, black).unwrap_err(),
                        MmrErrorKind::InvalidRunCode {
                            bit_offset: start,
                            black,
                        },
                    );
                    assert_eq!(
                        reader.bit_position(),
                        start + usize::from(MAX_RUN_CODE_BITS)
                    );
                }
                assert_eq!(reader.bytes.as_ptr(), encoded.as_ptr());
            }
        }
    }
}

#[test]
fn code_constructors_preserve_raw_fields_and_terminal_sense() {
    for bits in [0, 1, u16::MAX] {
        for length in [0, 1, MAX_RUN_CODE_BITS, u8::MAX] {
            for mode in [
                Mode::Pass,
                Mode::Horizontal,
                Mode::Extension,
                Mode::Vertical(i8::MIN),
                Mode::Vertical(0),
                Mode::Vertical(i8::MAX),
            ] {
                let code = ModeCode::new(bits, length, mode);
                assert_eq!((code.bits, code.length, code.mode), (bits, length, mode));
            }
            for value in [0, 63, 64, u16::MAX] {
                let terminal = RunCode::terminal(value, bits, length);
                let makeup = RunCode::makeup(value, bits, length);
                assert_eq!(
                    (
                        terminal.bits,
                        terminal.length,
                        terminal.value,
                        terminal.terminal
                    ),
                    (bits, length, value, true)
                );
                assert_eq!(
                    (makeup.bits, makeup.length, makeup.value, makeup.terminal),
                    (bits, length, value, false)
                );
            }
        }
    }
}

#[test]
fn sequential_prefix_checks_equal_prior_closure_at_every_u16_value() {
    for prefix in 0..=u16::MAX {
        for bit in [0, 1, 2, u16::MAX] {
            let prior = prefix
                .checked_mul(2)
                .and_then(|value| value.checked_add(bit));
            let expanded = append_code_bit(prefix, bit).ok();
            assert_eq!(expanded, prior);
        }
    }
}

fn span_test_bitmap(width: u32) -> Bitmap {
    let mut bitmap = Bitmap::blank(width, 3, &Limits::default()).expect("bounded bitmap");
    for (index, byte) in bitmap.data.iter_mut().enumerate() {
        *byte = u8::try_from(index % 256)
            .expect("bounded pattern")
            .wrapping_mul(37)
            .wrapping_add(11);
    }
    bitmap
}

#[test]
fn optimized_span_dispatch_matches_scalar_pixels_and_full_storage() {
    for width in [0, 1, 7, 8, 9, 63, 64, 65, 71, 72, 73, 127, 128, 129] {
        let original = span_test_bitmap(width);
        for row in 0..3 {
            for start in 0..=width {
                for end in start..=width {
                    for black in [false, true] {
                        let mut actual = original.clone();
                        let mut expected = original.clone();
                        if black {
                            for x in start..end {
                                let index = usize::try_from(row).expect("row") * expected.stride
                                    + usize::try_from(x / 8).expect("byte");
                                expected.data[index] |= 0x80 >> (x % 8);
                            }
                        }
                        fill_span(&mut actual, row, start, end, black).expect("valid span");
                        assert_eq!(actual, expected, "{width}/{row}/{start}/{end}/{black}");
                    }
                }
            }
        }
    }
}

#[test]
fn recorded_span_matches_scalar_storage_and_exact_transition_updates() {
    let mut cases = 0;
    for width in [0, 1, 7, 8, 9, 17] {
        let original = span_test_bitmap(width);
        for row in 0..3 {
            for start in 0..=width {
                for end in start..=width {
                    for black in [false, true] {
                        for before_black in [false, true] {
                            for prefix_shape in 0..3 {
                                let before: Vec<_> = match prefix_shape {
                                    0 => Vec::new(),
                                    1 => (0..start).collect(),
                                    _ => (0..start).flat_map(|x| [x, x]).collect(),
                                };
                                let mut expected_transitions = before.clone();
                                if start < end && before_black != black {
                                    expected_transitions.push(start);
                                }
                                let mut expected = original.clone();
                                if black {
                                    for x in start..end {
                                        let index = usize::try_from(row).unwrap() * expected.stride
                                            + usize::try_from(x / 8).unwrap();
                                        expected.data[index] |= 0x80 >> (x % 8);
                                    }
                                }
                                let mut actual = original.clone();
                                let mut transitions = before;
                                let mut emitted_black = before_black;
                                fill_recorded_span(
                                    &mut actual,
                                    row,
                                    start,
                                    end,
                                    black,
                                    width,
                                    &mut transitions,
                                    &mut emitted_black,
                                )
                                .unwrap();
                                assert_eq!(actual, expected);
                                assert_eq!(transitions, expected_transitions);
                                assert_eq!(
                                    emitted_black,
                                    if start == end { before_black } else { black },
                                );
                                cases += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 11_196);
}

#[test]
fn transition_span_complete_output_matrix_preserves_empty_and_same_colour_frames() {
    let mut cases = 0;
    for width in 0..=9 {
        for start in 0..=width {
            for end in start..=width {
                for mask in 0..(1_u32 << start) {
                    for duplicate in [false, true] {
                        let mut before = Vec::new();
                        for x in 0..start {
                            if mask & (1 << x) != 0 {
                                before.push(x);
                                if duplicate {
                                    before.push(x);
                                }
                            }
                        }
                        for before_black in [false, true] {
                            for black in [false, true] {
                                let mut expected = before.clone();
                                if start < end && before_black != black {
                                    expected.push(start);
                                }
                                let mut transitions = before.clone();
                                let mut emitted_black = before_black;
                                record_transition_span(
                                    &mut transitions,
                                    &mut emitted_black,
                                    start,
                                    end,
                                    width,
                                    black,
                                );
                                assert_eq!(transitions, expected);
                                assert_eq!(
                                    emitted_black,
                                    if start == end { before_black } else { black },
                                );
                                cases += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 32_136);
}

#[test]
fn invalid_span_dispatch_preserves_exact_error_and_all_bytes() {
    for width in [0, 1, 8, 65, 129] {
        let original = span_test_bitmap(width);
        for (row, start, end) in [
            (3, 0, 0),
            (u32::MAX, 0, width),
            (0, 1, 0),
            (0, 0, width + 1),
            (0, u32::MAX, u32::MAX),
        ] {
            for black in [false, true] {
                let mut bitmap = original.clone();
                assert!(matches!(
                    fill_span(&mut bitmap, row, start, end, black),
                    Err(MmrError::InvalidTransition)
                ));
                assert_eq!(bitmap, original);
            }
        }
    }
}

fn assert_incremental_transitions_match_scan(bitmap: &Bitmap) {
    for row in 0..bitmap.height() {
        let mut scanned = Vec::new();
        collect_transitions(bitmap, row, bitmap.width(), &mut scanned)
            .expect("oracle scan must cover a valid bitmap row");

        let mut incremental = Vec::new();
        incremental
            .try_reserve(usize::try_from(bitmap.width()).expect("test width fits usize") + 2)
            .expect("test transition storage must allocate");
        let mut emitted_black = false;
        for x in 0..bitmap.width() {
            let black = bitmap.pixel(x, row).expect("test pixel is in bounds");
            record_transition_span(
                &mut incremental,
                &mut emitted_black,
                x,
                x + 1,
                bitmap.width(),
                black,
            );
        }
        finish_transition_row(&mut incremental, bitmap.width());
        assert_eq!(incremental, scanned, "transition mismatch on row {row}");
    }
}

fn decode(data: &[u8], width: u32, height: u32) -> Bitmap {
    decode_mmr(
        data,
        width,
        height,
        &Limits::default(),
        EofbPolicy::Optional,
    )
    .expect("independently derived MMR vector must decode")
}

fn assert_row(bitmap: &Bitmap, row: u32, expected: &[bool]) {
    assert_eq!(
        usize::try_from(bitmap.width()).expect("test width fits usize"),
        expected.len()
    );
    for (x, value) in expected.iter().copied().enumerate() {
        assert_eq!(
            bitmap.pixel(u32::try_from(x).expect("test x fits u32"), row,),
            Some(value),
            "pixel mismatch at ({x}, {row})"
        );
    }
}

#[test]
fn all_white_uses_vertical_zero() {
    let bitmap = decode(&[0x80], 8, 1);
    assert_row(&bitmap, 0, &[false; 8]);
}

#[test]
fn all_black_uses_zero_white_horizontal_run() {
    let bitmap = decode(&[0x26, 0xA2, 0x80], 8, 1);
    assert_row(&bitmap, 0, &[true; 8]);
}

#[test]
fn horizontal_white_makeup_run_decodes() {
    let bitmap = decode(&[0x3B, 0x35, 0x0D, 0xC0], 64, 1);
    assert_row(&bitmap, 0, &[false; 64]);
}

#[test]
fn horizontal_black_makeup_run_decodes() {
    let bitmap = decode(&[0x26, 0xA0, 0x78, 0x6E], 64, 1);
    assert_row(&bitmap, 0, &[true; 64]);
}

#[test]
fn identical_black_rows_update_the_reference_line() {
    let bitmap = decode(&[0x26, 0xA2, 0xE0], 8, 2);
    assert_row(&bitmap, 0, &[true; 8]);
    assert_row(&bitmap, 1, &[true; 8]);
}

#[test]
fn identical_islands_use_three_vertical_zero_codes() {
    let bitmap = decode(&[0x2F, 0xF8], 8, 2);
    let island = [false, false, true, true, false, false, false, false];
    assert_row(&bitmap, 0, &island);
    assert_row(&bitmap, 1, &island);
}

#[test]
fn island_shift_right_uses_vertical_right_one() {
    let bitmap = decode(&[0x2F, 0xDB, 0x80], 8, 2);
    assert_row(
        &bitmap,
        0,
        &[false, false, true, true, false, false, false, false],
    );
    assert_row(
        &bitmap,
        1,
        &[false, false, false, true, true, false, false, false],
    );
}

#[test]
fn island_shift_left_uses_vertical_left_one() {
    let bitmap = decode(&[0x2F, 0xD2, 0x80], 8, 2);
    assert_row(
        &bitmap,
        0,
        &[false, false, true, true, false, false, false, false],
    );
    assert_row(
        &bitmap,
        1,
        &[false, true, true, false, false, false, false, false],
    );
}

#[test]
fn pass_skips_reference_island() {
    let bitmap = decode(&[0x2F, 0xC6], 8, 2);
    assert_row(
        &bitmap,
        0,
        &[false, false, true, true, false, false, false, false],
    );
    assert_row(&bitmap, 1, &[false; 8]);
}

#[test]
fn incremental_transitions_match_scan_for_normative_modes() {
    for (data, width, height) in [
        (&[0x80][..], 8, 1),
        (&[0x26, 0xA2, 0x80][..], 8, 1),
        (&[0x2F, 0xF8][..], 8, 2),
        (&[0x2F, 0xDB, 0x80][..], 8, 2),
        (&[0x2F, 0xD2, 0x80][..], 8, 2),
        (&[0x2F, 0xC6][..], 8, 2),
    ] {
        assert_incremental_transitions_match_scan(&decode(data, width, height));
    }
}

#[test]
fn zero_length_spans_and_same_x_transitions_match_scan() {
    let mut bitmap = Bitmap::blank(5, 1, &Limits::default()).expect("test bitmap must allocate");
    for x in 0..3 {
        bitmap
            .set_pixel(x, 0, true)
            .expect("test pixel is in bounds");
    }

    let mut incremental = Vec::with_capacity(7);
    let mut emitted_black = false;
    record_transition_span(&mut incremental, &mut emitted_black, 0, 0, 5, true);
    record_transition_span(&mut incremental, &mut emitted_black, 0, 0, 5, false);
    record_transition_span(&mut incremental, &mut emitted_black, 0, 3, 5, true);
    record_transition_span(&mut incremental, &mut emitted_black, 3, 3, 5, false);
    record_transition_span(&mut incremental, &mut emitted_black, 3, 3, 5, true);
    record_transition_span(&mut incremental, &mut emitted_black, 3, 5, 5, false);
    finish_transition_row(&mut incremental, 5);

    let mut scanned = Vec::new();
    collect_transitions(&bitmap, 0, 5, &mut scanned).expect("oracle scan must succeed");
    assert_eq!(incremental, scanned);
    assert_eq!(incremental, [0, 3, 5, 5]);
}

#[test]
fn incremental_transitions_match_scan_for_random_small_bitmaps() {
    let mut state = 0xD1B5_4A32_u32;
    for width in 1..=17 {
        for height in 1..=7 {
            let mut bitmap = Bitmap::blank(width, height, &Limits::default())
                .expect("small test bitmap must allocate");
            for row in 0..height {
                for x in 0..width {
                    state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    if state & 0x8000_0000 != 0 {
                        bitmap
                            .set_pixel(x, row, true)
                            .expect("test pixel is in bounds");
                    }
                }
            }
            assert_incremental_transitions_match_scan(&bitmap);
        }
    }
}

#[test]
fn reference_search_matches_scalar_suffix_and_cursor_for_all_small_sorted_vectors() {
    let mut vectors = 0;
    let mut full_cases = 0;
    let mut cursor_cases = 0;
    for width in 0..=5_u32 {
        for interior_count in 0..=4 {
            for mut encoded in 0..(width + 1).pow(interior_count) {
                let mut transitions = Vec::new();
                for _ in 0..interior_count {
                    transitions.push(encoded % (width + 1));
                    encoded /= width + 1;
                }
                if transitions.windows(2).any(|pair| pair[0] > pair[1]) {
                    continue;
                }
                transitions.push(width);
                transitions.push(width);
                let search_end = transitions.len() - 1;
                vectors += 1;
                for x in 0..=width {
                    for black in [false, true] {
                        for initial in [false, true] {
                            let eligible = |position| position > x || (initial && position == x);
                            let pair_from = |start| {
                                (start..search_end)
                                    .find(|index| {
                                        eligible(transitions[*index]) && (*index % 2 == 0) != black
                                    })
                                    .map_or((width, width), |index| {
                                        (transitions[index], transitions[index + 1])
                                    })
                            };
                            let full =
                                reference_elements(&transitions, x, black, width, initial).unwrap();
                            assert_eq!(full, pair_from(0));
                            full_cases += 1;
                            for before_cursor in 0..transitions.len() {
                                let expected_cursor = (before_cursor..search_end)
                                    .find(|index| eligible(transitions[*index]))
                                    .unwrap_or(search_end);
                                let mut cursor = before_cursor;
                                let actual = reference_elements_from(
                                    &transitions,
                                    x,
                                    black,
                                    width,
                                    initial,
                                    &mut cursor,
                                )
                                .unwrap();
                                assert_eq!(actual, pair_from(before_cursor));
                                assert_eq!(cursor, expected_cursor);
                                assert!(x <= actual.0 && actual.0 <= actual.1 && actual.1 <= width);
                                assert!(before_cursor <= cursor && cursor < transitions.len());
                                if (0..before_cursor).all(|index| !eligible(transitions[index])) {
                                    assert_eq!(actual, full);
                                }
                                cursor_cases += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(vectors, 461);
    assert_eq!(full_cases, 9240);
    assert_eq!(cursor_cases, 49_364);
}

#[test]
fn monotone_reference_cursor_matches_full_scan_exactly() {
    for width in 1..=9_u32 {
        let interior = width.saturating_sub(1);
        for mask in 0..(1_u32 << interior) {
            let mut transitions = Vec::new();
            for position in 0..width {
                if mask & (1 << position) != 0 {
                    transitions.push(position);
                }
            }
            transitions.push(width);
            transitions.push(width);

            let mut cursor = 0;
            let mut initial = true;
            for x in 0..width {
                for black in [false, true, false] {
                    let expected = reference_elements(&transitions, x, black, width, initial)
                        .expect("full scan accepts valid transitions");
                    let actual = reference_elements_from(
                        &transitions,
                        x,
                        black,
                        width,
                        initial,
                        &mut cursor,
                    )
                    .expect("cursor accepts valid transitions");
                    assert_eq!(
                        actual, expected,
                        "width={width}, mask={mask:#x}, x={x}, black={black}, initial={initial}"
                    );
                    initial = false;
                }
            }
        }
    }
}

#[test]
fn required_eofb_can_start_off_byte_boundary() {
    let bitmap = decode_mmr(
        &[0x80, 0x08, 0x00, 0x80],
        8,
        1,
        &Limits::default(),
        EofbPolicy::Required,
    )
    .expect("EOFB vector must decode");
    assert_row(&bitmap, 0, &[false; 8]);
}

#[test]
fn required_eofb_rejects_omission() {
    let error = decode_mmr(&[0x80], 8, 1, &Limits::default(), EofbPolicy::Required)
        .expect_err("required EOFB must not be omitted");
    assert!(matches!(error, MmrError::MissingEofb));
}

#[test]
fn prefix_decoder_consumes_two_concatenated_eofb_planes() {
    let stream = [
        0x80, 0x08, 0x00, 0x80, // all-white plane and EOFB
        0x26, 0xA2, 0x80, 0x08, 0x00, 0x80, // all-black plane and EOFB
    ];

    let (white, white_bytes) =
        decode_mmr_prefix(&stream, 8, 1, &Limits::default()).expect("first prefix must decode");
    assert_eq!(white_bytes, 4);
    assert_row(&white, 0, &[false; 8]);

    let remainder = stream
        .get(white_bytes..)
        .expect("consumed byte count is within the test stream");
    let (black, black_bytes) =
        decode_mmr_prefix(remainder, 8, 1, &Limits::default()).expect("second prefix must decode");
    assert_eq!(black_bytes, 6);
    assert_eq!(white_bytes + black_bytes, stream.len());
    assert_row(&black, 0, &[true; 8]);
}

#[test]
fn prefix_decoder_rejects_truncated_eofb() {
    let error = decode_mmr_prefix(&[0x80, 0x08, 0x00], 8, 1, &Limits::default())
        .expect_err("23 post-bitmap bits cannot contain EOFB");
    assert!(matches!(error, MmrError::MissingEofb));
}

#[test]
fn prefix_decoder_rejects_nonzero_alignment_padding() {
    let error = decode_mmr_prefix(&[0x80, 0x08, 0x00, 0x81], 8, 1, &Limits::default())
        .expect_err("bits after EOFB and before the byte boundary must be zero");
    assert!(matches!(error, MmrError::NonZeroEofbPadding));
}

#[test]
fn extension_mode_is_rejected() {
    let error = decode_mmr(
        &[0x02, 0x00],
        8,
        1,
        &Limits::default(),
        EofbPolicy::Optional,
    )
    .expect_err("T.6 extension mode is forbidden by T.88");
    assert!(matches!(error, MmrError::ExtensionMode { .. }));
}

#[test]
fn truncated_horizontal_run_is_rejected() {
    let error = decode_mmr(&[0x26], 8, 1, &Limits::default(), EofbPolicy::Optional)
        .expect_err("truncated horizontal run must fail");
    assert!(matches!(
        error,
        MmrError::Reader(MalformedError::UnexpectedEof { .. })
    ));
}

#[test]
fn run_overshoot_is_rejected() {
    let error = decode_mmr(&[0x33], 4, 1, &Limits::default(), EofbPolicy::Optional)
        .expect_err("white run 8 must not fit a width-four row");
    assert!(matches!(error, MmrError::RunExceedsRow));
}

#[test]
fn work_limit_is_enforced_before_decoding() {
    let limits = Limits {
        max_work_units: 15,
        ..Limits::default()
    };
    let error = decode_mmr(&[0x80], 8, 1, &limits, EofbPolicy::Optional)
        .expect_err("two raster passes already exceed the work limit");
    assert!(matches!(
        error,
        MmrError::Bitmap(DecodeError::LimitExceeded(LimitExceeded {
            kind: LimitKind::Work,
            limit: 15,
            actual: 16
        }))
    ));
}

#[test]
fn both_transition_vectors_are_counted_as_managed_memory() {
    let limits = Limits {
        max_decoder_managed_memory_bytes: 80,
        ..Limits::default()
    };
    let error = decode_mmr(&[0x80], 8, 1, &limits, EofbPolicy::Optional)
        .expect_err("bitmap plus two width+2 transition vectors need 81 bytes");
    assert!(matches!(
        error,
        MmrError::Bitmap(DecodeError::LimitExceeded(LimitExceeded {
            kind: LimitKind::Memory,
            limit: 80,
            actual: 81
        }))
    ));
}

#[test]
fn optional_eofb_accepts_bounded_remainder() {
    let bitmap = decode_mmr(
        &[0x80, 0xFF],
        8,
        1,
        &Limits::default(),
        EofbPolicy::Optional,
    )
    .expect("known-length remainder may be skipped");
    assert_row(&bitmap, 0, &[false; 8]);
}

#[test]
fn all_normative_run_codes_are_unique_per_colour() {
    fn assert_unique(codes: &[RunCode]) {
        for (index, left) in codes.iter().enumerate() {
            for right in &codes[index + 1..] {
                assert!(
                    left.length != right.length || left.bits != right.bits,
                    "duplicate run code"
                );
            }
        }
    }

    let mut white = Vec::new();
    white.extend_from_slice(&WHITE_TERMINATING);
    white.extend_from_slice(&WHITE_MAKEUP);
    white.extend_from_slice(&COMMON_MAKEUP);
    assert_unique(&white);

    let mut black = Vec::new();
    black.extend_from_slice(&BLACK_TERMINATING);
    black.extend_from_slice(&BLACK_MAKEUP);
    black.extend_from_slice(&COMMON_MAKEUP);
    assert_unique(&black);
}

#[test]
fn every_mode_table_entry_decodes() {
    for expected in MODE_CODES {
        let shift = u32::from(16_u8 - expected.length);
        let encoded = expected
            .bits
            .checked_shl(shift)
            .expect("mode code fits two test bytes")
            .to_be_bytes();
        let mut reader = bit_reader(&encoded);
        assert_eq!(
            decode_mode(&mut reader).expect("table mode must decode"),
            expected.mode
        );
        assert_eq!(reader.bit_position(), usize::from(expected.length));
    }
}

#[test]
fn every_run_table_entry_decodes_for_its_colour() {
    fn assert_codes_decode(codes: &[RunCode], black: bool) {
        for expected in codes {
            let shift = u32::from(16_u8 - expected.length);
            let encoded = expected
                .bits
                .checked_shl(shift)
                .expect("run code fits two test bytes")
                .to_be_bytes();
            let mut reader = bit_reader(&encoded);
            let actual = decode_run_code(&mut reader, black).expect("table run must decode");
            assert_eq!(actual.bits, expected.bits);
            assert_eq!(actual.length, expected.length);
            assert_eq!(actual.value, expected.value);
            assert_eq!(actual.terminal, expected.terminal);
            assert_eq!(reader.bit_position(), usize::from(expected.length));
        }
    }

    assert_codes_decode(&WHITE_TERMINATING, false);
    assert_codes_decode(&WHITE_MAKEUP, false);
    assert_codes_decode(&COMMON_MAKEUP, false);
    assert_codes_decode(&BLACK_TERMINATING, true);
    assert_codes_decode(&BLACK_MAKEUP, true);
    assert_codes_decode(&COMMON_MAKEUP, true);
}

fn capacity_charge(scratch: &MmrScratch) -> u64 {
    u64::try_from(
        (scratch.transitions.capacity() + scratch.row_transitions.capacity())
            * core::mem::size_of::<u32>(),
    )
    .unwrap()
}

fn fresh_capacity_charge(width: u32) -> u64 {
    let mut first = Vec::<u32>::new();
    let mut second = Vec::<u32>::new();
    let count = usize::try_from(width).unwrap() + 2;
    first.try_reserve(count).unwrap();
    second.try_reserve(count).unwrap();
    u64::try_from((first.capacity() + second.capacity()) * core::mem::size_of::<u32>()).unwrap()
}

fn assert_limit(error: MmrError, kind: LimitKind, limit: u64, actual: u64) {
    let DecodeError::LimitExceeded(found) = DecodeError::from(error) else {
        panic!("expected an unchanged resource-limit error");
    };
    assert_eq!(
        found,
        LimitExceeded {
            kind,
            limit,
            actual
        }
    );
}

fn assert_kind(error: MmrError, expected: MmrErrorKind) {
    let DecodeError::Malformed(found) = DecodeError::from(error) else {
        panic!("expected a typed malformed MMR error");
    };
    assert_eq!(found, MalformedError::InvalidMmr(expected));
}

fn assert_cleared(scratch: &MmrScratch, charge: u64) {
    assert!(scratch.transitions.is_empty());
    assert!(scratch.row_transitions.is_empty());
    assert_eq!(capacity_charge(scratch), charge);
    assert_eq!(scratch.retained_bytes(), charge);
}

#[test]
fn scratch_clear_retains_and_release_drops_both_vectors_idempotently() {
    let bitmap = Bitmap::blank(128, 1, &Limits::default()).unwrap();
    let mut scratch = MmrScratch::default();
    prepare_transition_storage(128, &bitmap, &Limits::default(), &mut scratch).unwrap();
    let charge = capacity_charge(&scratch);
    assert_eq!(charge, fresh_capacity_charge(128));
    assert_eq!(scratch.reserve_events, 2);
    scratch.transitions.extend_from_slice(&[0, 128, 128]);
    scratch.row_transitions.extend_from_slice(&[64, 128, 128]);
    let capacities = (
        scratch.transitions.capacity(),
        scratch.row_transitions.capacity(),
    );
    scratch.clear();
    scratch.clear();
    assert_cleared(&scratch, charge);
    assert_eq!(
        capacities,
        (
            scratch.transitions.capacity(),
            scratch.row_transitions.capacity()
        )
    );
    assert_eq!(scratch.reserve_events, 2);
    scratch.release();
    scratch.release();
    assert_cleared(&scratch, 0);
    assert_eq!(scratch.transitions.capacity(), 0);
    assert_eq!(scratch.row_transitions.capacity(), 0);
    assert_eq!(scratch.reserve_events, 0);
}

#[test]
fn reused_scratch_recovers_after_decode_and_framing_errors_then_shrinks() {
    let mut scratch = MmrScratch::default();
    let limits = Limits::default();
    let wide =
        decode_mmr_reused(&[0x80], 128, 1, &limits, EofbPolicy::Optional, &mut scratch).unwrap();
    assert_eq!(wide.bytes(), &[0; 16]);
    let charge = capacity_charge(&scratch);
    assert_eq!(charge, fresh_capacity_charge(128));
    assert_cleared(&scratch, charge);
    for (data, expected) in [
        (&[0x02][..], MmrErrorKind::ExtensionMode { bit_offset: 0 }),
        (&[0x00][..], MmrErrorKind::InvalidMode { bit_offset: 0 }),
    ] {
        let error =
            decode_mmr_reused(data, 8, 1, &limits, EofbPolicy::Optional, &mut scratch).unwrap_err();
        assert_kind(error, expected);
        assert_cleared(&scratch, charge);
        assert_eq!(scratch.reserve_events, 2);
    }
    let error =
        decode_mmr_reused(&[0x80], 8, 1, &limits, EofbPolicy::Required, &mut scratch).unwrap_err();
    assert_kind(error, MmrErrorKind::MissingEofb);
    assert_cleared(&scratch, charge);
    let below_requested = Limits {
        max_decoder_managed_memory_bytes: 80,
        ..limits
    };
    let error = decode_mmr_reused(
        &[0x80],
        8,
        1,
        &below_requested,
        EofbPolicy::Optional,
        &mut scratch,
    )
    .unwrap_err();
    assert_limit(error, LimitKind::Memory, 80, 81);
    // The requested-size rejection precedes any retained-capacity release.
    assert_cleared(&scratch, charge);
    assert_eq!(scratch.reserve_events, 2);
    let tight = Limits {
        max_decoder_managed_memory_bytes: 1 + fresh_capacity_charge(8),
        ..limits
    };
    assert!(charge + 1 > tight.max_decoder_managed_memory_bytes);
    let bitmap = decode_mmr_reused(
        &[0x26, 0xA2, 0x80],
        8,
        1,
        &tight,
        EofbPolicy::Optional,
        &mut scratch,
    )
    .unwrap();
    assert_eq!(bitmap.bytes(), &[0xFF]);
    assert_cleared(&scratch, fresh_capacity_charge(8));
    // A retained-over-budget release resets the counter before two fresh reserves.
    assert_eq!(scratch.reserve_events, 2);
}

#[test]
fn requested_and_actual_capacity_limits_are_distinct_inclusive_and_recoverable() {
    let width = 1;
    let requested = 1 + u64::from(width + 2) * 2 * 4;
    let actual = 1 + fresh_capacity_charge(width);
    assert!(
        requested < actual,
        "the minimum u32 Vec capacity rounds three elements up"
    );
    let mut scratch = MmrScratch::default();
    for (limit, expected_actual) in [
        (requested - 1, requested),
        (requested, actual),
        (actual - 1, actual),
    ] {
        let limits = Limits {
            max_decoder_managed_memory_bytes: limit,
            ..Limits::default()
        };
        let error = decode_mmr_reused(
            &[0x80],
            width,
            1,
            &limits,
            EofbPolicy::Optional,
            &mut scratch,
        )
        .unwrap_err();
        assert_limit(error, LimitKind::Memory, limit, expected_actual);
        assert_cleared(&scratch, 0);
        assert_eq!(scratch.reserve_events, 0);
    }
    let limits = Limits {
        max_decoder_managed_memory_bytes: actual,
        ..Limits::default()
    };
    let bitmap = decode_mmr_reused(
        &[0x80],
        width,
        1,
        &limits,
        EofbPolicy::Optional,
        &mut scratch,
    )
    .unwrap();
    assert_eq!((bitmap.width, bitmap.height, bitmap.stride), (1, 1, 1));
    assert_eq!(bitmap.bytes(), &[0]);
    assert_cleared(&scratch, actual - 1);
    assert_eq!(scratch.reserve_events, 2);
}

#[test]
fn stream_and_each_mode_work_boundary_preserve_retained_scratch_and_recover() {
    let mut scratch = MmrScratch::default();
    let bitmap = decode_mmr_reused(
        &[0xC0],
        8,
        2,
        &Limits::default(),
        EofbPolicy::Optional,
        &mut scratch,
    )
    .unwrap();
    assert_eq!(bitmap.bytes(), &[0, 0]);
    let charge = capacity_charge(&scratch);
    for (stream, work, kind, limit, actual) in [
        (0, 0, LimitKind::StreamBytes, 0, 1),
        (1, 31, LimitKind::Work, 31, 32),
        (1, 32, LimitKind::Work, 32, 33),
        (1, 33, LimitKind::Work, 33, 34),
    ] {
        let limits = Limits {
            max_stream_bytes: stream,
            max_work_units: work,
            ..Limits::default()
        };
        let error = decode_mmr_reused(&[0xC0], 8, 2, &limits, EofbPolicy::Optional, &mut scratch)
            .unwrap_err();
        assert_limit(error, kind, limit, actual);
        assert_cleared(&scratch, charge);
        assert_eq!(scratch.reserve_events, 2);
        let exact = Limits {
            max_stream_bytes: 1,
            max_work_units: 34,
            ..Limits::default()
        };
        let recovered =
            decode_mmr_reused(&[0xC0], 8, 2, &exact, EofbPolicy::Optional, &mut scratch).unwrap();
        assert_eq!(recovered.bytes(), &[0, 0]);
        assert_cleared(&scratch, charge);
    }
}

#[test]
fn framing_at_every_alignment_has_exact_consumption_and_padding_errors() {
    let cases: &[(u8, &[u8])] = &[
        (0, &[0x00, 0x10, 0x01]),
        (1, &[0x00, 0x08, 0x00, 0x80]),
        (2, &[0x00, 0x04, 0x00, 0x40]),
        (3, &[0x00, 0x02, 0x00, 0x20]),
        (4, &[0x00, 0x01, 0x00, 0x10]),
        (5, &[0x00, 0x00, 0x80, 0x08]),
        (6, &[0x00, 0x00, 0x40, 0x04]),
        (7, &[0x00, 0x00, 0x20, 0x02]),
    ];
    for &(offset, bytes) in cases {
        let mut reader = bit_reader(bytes);
        reader.read_bits(offset).unwrap();
        finish_block(&mut reader, EofbPolicy::Required).unwrap();
        assert_eq!(reader.bit_position(), bytes.len() * 8);
        assert_eq!(reader.bytes, bytes);

        let mut with_suffix = bytes.to_vec();
        with_suffix.extend_from_slice(&[0xA5, 0x5A]);
        let mut prefix = bit_reader(&with_suffix);
        prefix.read_bits(offset).unwrap();
        finish_required_prefix(&mut prefix).unwrap();
        assert_eq!(prefix.bit_position(), bytes.len() * 8);
        assert_eq!(&prefix.bytes[prefix.bit_position() / 8..], &[0xA5, 0x5A]);
        let mut bounded = bit_reader(&with_suffix);
        bounded.read_bits(offset).unwrap();
        assert_kind(
            finish_block(&mut bounded, EofbPolicy::Required).unwrap_err(),
            MmrErrorKind::TrailingData,
        );
        assert_eq!(bounded.bit_position(), usize::from(offset) + 24);
        let mut optional = bit_reader(&with_suffix);
        optional.read_bits(offset).unwrap();
        finish_block(&mut optional, EofbPolicy::Optional).unwrap();
        assert_eq!(optional.bit_position(), usize::from(offset));

        if offset != 0 {
            let mut nonzero = bytes.to_vec();
            *nonzero.last_mut().unwrap() |= 1;
            for prefix_mode in [false, true] {
                let mut reader = bit_reader(&nonzero);
                reader.read_bits(offset).unwrap();
                let error = if prefix_mode {
                    finish_required_prefix(&mut reader)
                } else {
                    finish_block(&mut reader, EofbPolicy::Required)
                }
                .unwrap_err();
                assert_kind(error, MmrErrorKind::NonZeroPadding);
                assert_eq!(reader.bit_position(), nonzero.len() * 8);
                assert_eq!(reader.bytes, nonzero);
            }
        }
    }
}

#[test]
fn missing_and_invalid_eofb_have_distinct_exact_reader_effects() {
    for prefix_mode in [false, true] {
        for bytes in [&[][..], &[0x00][..], &[0x00, 0x10][..]] {
            let mut reader = bit_reader(bytes);
            let error = if prefix_mode {
                finish_required_prefix(&mut reader)
            } else {
                finish_block(&mut reader, EofbPolicy::Required)
            }
            .unwrap_err();
            assert_kind(error, MmrErrorKind::MissingEofb);
            assert_eq!(reader.bit_position(), 0);
            assert_eq!(reader.bytes, bytes);
        }
        let mut reader = bit_reader(&[0x00, 0x10, 0x00, 0xA5]);
        let error = if prefix_mode {
            finish_required_prefix(&mut reader)
        } else {
            finish_block(&mut reader, EofbPolicy::Required)
        }
        .unwrap_err();
        assert_kind(error, MmrErrorKind::InvalidEofb);
        assert_eq!(reader.bit_position(), 24);
        assert_eq!(reader.bytes, &[0x00, 0x10, 0x00, 0xA5]);
    }
    let (bitmap, consumed) =
        decode_mmr_prefix(&[0x00, 0x10, 0x01, 0xA5], 0, 0, &Limits::default()).unwrap();
    assert_eq!((bitmap.width, bitmap.height, bitmap.stride), (0, 0, 0));
    assert_eq!(bitmap.bytes(), &[]);
    assert_eq!(consumed, 3);
}

#[test]
fn invalid_mode_and_run_codes_preserve_start_offsets_and_exact_eof_states() {
    let mut mode = bit_reader(&[0x80, 0x00]);
    assert_eq!(mode.read_bits(3), Ok(4));
    assert_kind(
        decode_mode(&mut mode).unwrap_err(),
        MmrErrorKind::InvalidMode { bit_offset: 3 },
    );
    assert_eq!(mode.bit_position(), 10);
    let mut truncated = bit_reader(&[0x00]);
    truncated.read_bits(2).unwrap();
    let DecodeError::Malformed(error) = DecodeError::from(decode_mode(&mut truncated).unwrap_err())
    else {
        panic!("mode truncation is a reader error");
    };
    assert_eq!(
        error,
        MalformedError::UnexpectedEof {
            offset: 8,
            needed: 1,
            remaining: 0,
        }
    );
    assert_eq!(truncated.bit_position(), 8);
    for black in [false, true] {
        let mut invalid = bit_reader(&[0x80, 0x00, 0x00]);
        invalid.read_bits(3).unwrap();
        assert_kind(
            decode_run_code(&mut invalid, black).unwrap_err(),
            MmrErrorKind::InvalidRunCode {
                bit_offset: 3,
                black,
            },
        );
        assert_eq!(invalid.bit_position(), 16);
        assert_eq!(invalid.bytes, &[0x80, 0x00, 0x00]);
        let mut truncated = bit_reader(&[0x80, 0x00]);
        truncated.read_bits(4).unwrap();
        let MmrError::Reader(error) = decode_run_code(&mut truncated, black).unwrap_err() else {
            panic!("run truncation is a reader error");
        };
        assert_eq!(
            error,
            MalformedError::UnexpectedEof {
                offset: 16,
                needed: 1,
                remaining: 0,
            }
        );
        assert_eq!(truncated.bit_position(), 16);
    }
}

#[test]
fn static_vertical_two_and_three_cases_have_literal_pixels_and_bounds() {
    for (data, expected) in [
        (&[0x0A][..], &[0x03][..]),
        (&[0x05][..], &[0x07][..]),
        (&[0x26, 0xA2, 0x87][..], &[0xFF, 0x3F][..]),
        (&[0x26, 0xA2, 0x83, 0x80][..], &[0xFF, 0x1F][..]),
    ] {
        let height = u32::try_from(expected.len()).unwrap();
        let bitmap = decode(data, 8, height);
        assert_eq!((bitmap.width, bitmap.height, bitmap.stride), (8, height, 1));
        assert_eq!(bitmap.bytes(), expected);
    }
    for (data, width, height) in [
        (&[0x0C][..], 8, 1),
        (&[0x06][..], 8, 1),
        (&[0x08][..], 1, 1),
        (&[0x04][..], 2, 1),
        (&[0x26, 0xA2, 0x84][..], 8, 2),
        (&[0x26, 0xA2, 0x82][..], 8, 2),
    ] {
        assert_kind(
            decode_mmr(
                data,
                width,
                height,
                &Limits::default(),
                EofbPolicy::Required,
            )
            .unwrap_err(),
            MmrErrorKind::InvalidTransition,
        );
    }
}

#[test]
fn horizontal_zero_pair_is_nonprogress_and_run_bounds_stop_at_the_code() {
    assert_kind(
        decode_mmr(
            &[0x26, 0xA1, 0xB8],
            8,
            1,
            &Limits::default(),
            EofbPolicy::Optional,
        )
        .unwrap_err(),
        MmrErrorKind::NonProgress,
    );
    for (bytes, black, consumed) in [(&[0x98][..], false, 5), (&[0x14][..], true, 6)] {
        let mut reader = bit_reader(bytes);
        assert_kind(
            decode_run(&mut reader, black, 7).unwrap_err(),
            MmrErrorKind::RunExceedsRow,
        );
        assert_eq!(reader.bit_position(), consumed);
        assert_eq!(reader.bytes, bytes);
        let mut exact = bit_reader(bytes);
        assert_eq!(decode_run(&mut exact, black, 8).unwrap(), 8);
        assert_eq!(exact.bit_position(), consumed);
    }
}

#[test]
fn makeup_runs_require_a_terminal_and_enforce_the_exact_remaining_width() {
    for (bytes, black, value, consumed) in [
        (&[0xD9, 0xA8][..], false, 64, 13),
        (&[0x03, 0xC3, 0x70][..], true, 64, 20),
        (&[0xDE, 0xCD, 0x40][..], false, 128, 18),
        (&[0x03, 0xC0, 0xF0, 0xDC][..], true, 128, 30),
    ] {
        let mut exact = bit_reader(bytes);
        assert_eq!(decode_run(&mut exact, black, value).unwrap(), value);
        assert_eq!(exact.bit_position(), consumed);
        assert_eq!(exact.bytes, bytes);
        let mut short = bit_reader(bytes);
        assert_kind(
            decode_run(&mut short, black, 63).unwrap_err(),
            MmrErrorKind::RunExceedsRow,
        );
        assert_eq!(short.bit_position(), if black { 10 } else { 5 });
    }
    for (bytes, black) in [(&[0xD8][..], false), (&[0x03, 0xC0][..], true)] {
        let mut reader = bit_reader(bytes);
        let MmrError::Reader(error) = decode_run(&mut reader, black, 64).unwrap_err() else {
            panic!("make-up alone does not terminate a run");
        };
        assert_eq!(
            error,
            MalformedError::UnexpectedEof {
                offset: (bytes.len() * 8) as u64,
                needed: 1,
                remaining: 0,
            }
        );
        assert_eq!(reader.bit_position(), bytes.len() * 8);
        assert_eq!(reader.bytes, bytes);
    }
}

#[test]
fn defensive_reference_errors_preserve_inputs_and_exact_cursor_effects() {
    for transitions in [&[][..], &[8][..]] {
        assert_kind(
            reference_elements(transitions, 0, false, 8, true).unwrap_err(),
            MmrErrorKind::InvalidTransition,
        );
    }
    let mut cursor = 0;
    assert_kind(
        reference_elements_from(&[], 0, false, 8, true, &mut cursor).unwrap_err(),
        MmrErrorKind::InvalidTransition,
    );
    assert_eq!(cursor, 0);
    cursor = 2;
    assert_kind(
        reference_elements_from(&[8, 8], 0, false, 8, true, &mut cursor).unwrap_err(),
        MmrErrorKind::InvalidTransition,
    );
    assert_eq!(cursor, 2);
    let transitions = [0, 2, 1, 8, 8];
    cursor = 0;
    assert_kind(
        reference_elements_from(&transitions, 2, false, 8, true, &mut cursor).unwrap_err(),
        MmrErrorKind::InvalidTransition,
    );
    assert_eq!(cursor, 1);
    assert_eq!(transitions, [0, 2, 1, 8, 8]);
}

#[test]
fn error_conversion_preserves_all_classifications_and_payloads() {
    for (error, expected) in [
        (
            MmrError::InvalidMode { bit_offset: 37 },
            MmrErrorKind::InvalidMode { bit_offset: 37 },
        ),
        (
            MmrError::ExtensionMode { bit_offset: 41 },
            MmrErrorKind::ExtensionMode { bit_offset: 41 },
        ),
        (
            MmrError::InvalidRunCode {
                bit_offset: 43,
                black: true,
            },
            MmrErrorKind::InvalidRunCode {
                bit_offset: 43,
                black: true,
            },
        ),
        (
            MmrError::InvalidRunCode {
                bit_offset: 47,
                black: false,
            },
            MmrErrorKind::InvalidRunCode {
                bit_offset: 47,
                black: false,
            },
        ),
        (MmrError::RunExceedsRow, MmrErrorKind::RunExceedsRow),
        (MmrError::InvalidTransition, MmrErrorKind::InvalidTransition),
        (MmrError::NonProgress, MmrErrorKind::NonProgress),
        (MmrError::TooManyModes, MmrErrorKind::TooManyModes),
        (MmrError::MissingEofb, MmrErrorKind::MissingEofb),
        (MmrError::InvalidEofb, MmrErrorKind::InvalidEofb),
        (MmrError::TrailingData, MmrErrorKind::TrailingData),
        (MmrError::NonZeroEofbPadding, MmrErrorKind::NonZeroPadding),
    ] {
        assert_kind(error, expected);
    }
    let DecodeError::Allocation { requested_bytes } = DecodeError::from(MmrError::Allocation {
        requested_bytes: 4096,
    }) else {
        panic!("allocation size must survive conversion");
    };
    assert_eq!(requested_bytes, 4096);
    let DecodeError::Malformed(error) = DecodeError::from(MmrError::IntegerOverflow) else {
        panic!("integer overflow must survive conversion");
    };
    assert_eq!(error, MalformedError::IntegerOverflow);
    let DecodeError::Malformed(error) = DecodeError::from(MmrError::Bitmap(
        DecodeError::Malformed(MalformedError::InvalidBitCount(33)),
    )) else {
        panic!("bitmap errors must not be reclassified");
    };
    assert_eq!(error, MalformedError::InvalidBitCount(33));
}

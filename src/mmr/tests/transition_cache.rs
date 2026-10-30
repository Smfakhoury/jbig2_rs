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

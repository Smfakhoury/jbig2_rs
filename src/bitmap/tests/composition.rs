#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

use super::*;

#[test]
fn byte_operators_match_all_independent_boolean_truth_tables() {
    for (operator, table) in OPERATOR_TRUTHS {
        for destination in 0..=u8::MAX {
            for source in 0..=u8::MAX {
                let mut expected = 0;
                for bit in 0..8 {
                    let index = 2 * usize::from((destination >> bit) & 1)
                        + usize::from((source >> bit) & 1);
                    if table[index] {
                        expected |= 1 << bit;
                    }
                }
                assert_eq!(operator.apply_byte(destination, source), expected);
            }
        }
    }
}

#[test]
fn word_operators_match_independent_truth_tables_at_every_bit() {
    let mut values = vec![0, u64::MAX, 0x0123_4567_89ab_cdef, 0xaaaa_aaaa_aaaa_aaaa];
    for bit in 0..64 {
        values.push(1_u64 << bit);
        values.push(!(1_u64 << bit));
    }
    for (operator, table) in OPERATOR_TRUTHS {
        for &destination in &values {
            for &source in &values {
                let mut expected = 0;
                for bit in 0..64 {
                    let index = 2 * usize::try_from((destination >> bit) & 1).unwrap()
                        + usize::try_from((source >> bit) & 1).unwrap();
                    if table[index] {
                        expected |= 1_u64 << bit;
                    }
                }
                assert_eq!(operator.apply_chunk(destination, source), expected);
            }
        }
    }
}

#[test]
fn composition_clips_and_applies_operators() {
    let limits = Limits::default();
    let source = Bitmap::filled(2, 2, true, &limits).expect("source should fit");
    let mut destination = Bitmap::blank(2, 2, &limits).expect("destination should fit");
    destination
        .compose(&source, 1, 1, CombinationOperator::Or)
        .expect("composition should succeed");
    assert_eq!(destination.pixel(0, 0), Some(false));
    assert_eq!(destination.pixel(1, 1), Some(true));

    destination
        .compose(&source, 0, 0, CombinationOperator::Xor)
        .expect("composition should succeed");
    assert_eq!(destination.pixel(0, 0), Some(true));
    assert_eq!(destination.pixel(1, 1), Some(false));
}

#[test]
fn every_combination_operator_matches_boolean_definition() {
    let operators = [
        CombinationOperator::Or,
        CombinationOperator::And,
        CombinationOperator::Xor,
        CombinationOperator::Xnor,
        CombinationOperator::Replace,
    ];
    for operator in operators {
        for destination in [false, true] {
            for source in [false, true] {
                let expected = match operator {
                    CombinationOperator::Or => destination || source,
                    CombinationOperator::And => destination && source,
                    CombinationOperator::Xor => destination != source,
                    CombinationOperator::Xnor => destination == source,
                    CombinationOperator::Replace => source,
                };
                assert_eq!(operator.apply(destination, source), expected);
            }
        }
    }
}

#[test]
fn completely_clipped_compositions_preserve_every_stored_byte() {
    for (source_width, source_height) in [(0, 3), (9, 0), (9, 3)] {
        let source = patterned_bitmap(source_width, source_height, 17);
        for (width, height) in [(0, 3), (17, 0), (17, 3)] {
            let original = patterned_bitmap(width, height, 29);
            for (x, y) in [
                (0, 0),
                (width, 0),
                (0, height),
                (u32::MAX, 0),
                (0, u32::MAX),
                (u32::MAX, u32::MAX),
            ] {
                if source_width != 0 && source_height != 0 && x < width && y < height {
                    continue;
                }
                for operator in [
                    CombinationOperator::Or,
                    CombinationOperator::And,
                    CombinationOperator::Xor,
                    CombinationOperator::Xnor,
                    CombinationOperator::Replace,
                ] {
                    let mut actual = original.clone();
                    actual.compose(&source, x, y, operator).unwrap();
                    assert_eq!(actual, original);
                }
            }
        }
    }
}

#[test]
fn clipped_composition_word_edges_match_scalar_pixels_and_full_storage() {
    for width in [7, 8, 9, 63, 64, 65, 127, 128, 129] {
        let source = patterned_bitmap(width + 1, 4, 17);
        let original = patterned_bitmap(width, 3, 29);
        for x in [0, 1, 7, 8, 9, 63, 64, 65, width - 1, width, u32::MAX] {
            for y in [0, 1, 2, 3, u32::MAX] {
                for operator in [
                    CombinationOperator::Or,
                    CombinationOperator::And,
                    CombinationOperator::Xor,
                    CombinationOperator::Xnor,
                    CombinationOperator::Replace,
                ] {
                    let mut expected = original.clone();
                    scalar_compose(&mut expected, &source, x, y, operator).unwrap();
                    let mut actual = original.clone();
                    actual.compose(&source, x, y, operator).unwrap();
                    assert_eq!(
                        actual, expected,
                        "width {width}, x {x}, y {y}, {operator:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn compose_writes_only_the_clipped_destination() {
    let limits = Limits::default();
    let source = Bitmap::filled(3, 2, true, &limits).expect("source fits");
    let mut destination = Bitmap::blank(10, 3, &limits).expect("destination fits");
    destination
        .compose(&source, 8, 1, CombinationOperator::Replace)
        .expect("clipped composition succeeds");

    for y in 0..3 {
        for x in 0..10 {
            assert_eq!(
                destination.pixel(x, y),
                Some((y == 1 || y == 2) && (x == 8 || x == 9)),
            );
        }
    }
    for row in 0..3 {
        assert_eq!(destination.bytes()[row * 2 + 1] & 0x3f, 0);
    }

    let before = destination.bytes().to_vec();
    destination
        .compose(&source, 10, 0, CombinationOperator::Or)
        .expect("fully clipped composition succeeds");
    assert_eq!(destination.bytes(), before);
}

#[test]
fn chunk_composition_exhaustively_matches_scalar_oracle() {
    let operators = [
        CombinationOperator::Or,
        CombinationOperator::And,
        CombinationOperator::Xor,
        CombinationOperator::Xnor,
        CombinationOperator::Replace,
    ];
    for destination_width in 0_u32..=18 {
        for source_width in 0_u32..=18 {
            let destination_seed = u64::from(destination_width) << 32 | u64::from(source_width);
            let source = patterned_bitmap(source_width, 2, !destination_seed);
            for x in 0..=destination_width.saturating_add(1) {
                for operator in operators {
                    let mut expected = patterned_bitmap(destination_width, 3, destination_seed);
                    let mut actual = expected.clone();
                    scalar_compose(&mut expected, &source, x, 1, operator)
                        .expect("scalar composition succeeds");
                    actual
                        .compose(&source, x, 1, operator)
                        .expect("chunk composition succeeds");
                    assert_eq!(
                        actual, expected,
                        "destination width {destination_width}, source width {source_width}, \
                         x {x}, operator {operator:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn row_batch_chunking_matches_independent_ordered_bit_updates() {
    let first = patterned_bitmap(161, 4, 0x1739_a56c);
    let second = patterned_bitmap(161, 4, 0xc62e_408b);
    let original = patterned_bitmap(177, 5, 0x9281_fde3);
    let first_before = first.bytes().to_vec();
    let second_before = second.bytes().to_vec();
    let mut cases = 0;
    for source_x in 0..8 {
        for destination_x in 0..8 {
            for width in [0, 1, 7, 8, 9, 63, 64, 65, 127, 128, 129] {
                for height in [0, 1, 3] {
                    for (operator, truths) in OPERATOR_TRUTHS {
                        let blits = [
                            RowBlit {
                                source: &first,
                                source_x,
                                destination_x,
                                width,
                            },
                            RowBlit {
                                source: &second,
                                source_x: 7 - source_x,
                                destination_x: destination_x + 3,
                                width: width.min(61),
                            },
                        ];
                        let mut expected = original.bytes().to_vec();
                        for row in 0..height {
                            for blit in &blits {
                                for column in 0..blit.width {
                                    let sx = blit.source_x + column;
                                    let dx = blit.destination_x + column;
                                    let source_index = usize::try_from(row + 1).unwrap()
                                        * blit.source.stride
                                        + usize::try_from(sx / 8).unwrap();
                                    let destination_index = usize::try_from(row + 2).unwrap()
                                        * original.stride
                                        + usize::try_from(dx / 8).unwrap();
                                    let incoming =
                                        blit.source.bytes()[source_index] & (0x80 >> (sx % 8)) != 0;
                                    let current =
                                        expected[destination_index] & (0x80 >> (dx % 8)) != 0;
                                    let value =
                                        truths[usize::from(current) * 2 + usize::from(incoming)];
                                    let mask = 0x80 >> (dx % 8);
                                    if value {
                                        expected[destination_index] |= mask;
                                    } else {
                                        expected[destination_index] &= !mask;
                                    }
                                }
                            }
                        }
                        let mut actual = original.clone();
                        actual
                            .compose_row_batch_chunked(&blits, 1, 2, height, operator)
                            .unwrap();
                        assert_eq!(actual.width(), 177);
                        assert_eq!(actual.height(), 5);
                        assert_eq!(actual.stride(), 23);
                        assert_eq!(actual.bytes(), expected);
                        assert_eq!(first.bytes(), first_before);
                        assert_eq!(second.bytes(), second_before);
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases, 10_560);
}

#[test]
fn zero_height_row_batch_never_reads_blit_coordinates() {
    let source = patterned_bitmap(1, 1, 7);
    let original = patterned_bitmap(9, 2, 13);
    for (operator, _) in OPERATOR_TRUTHS {
        let mut actual = original.clone();
        actual
            .compose_row_batch_chunked(
                &[RowBlit {
                    source: &source,
                    source_x: u32::MAX,
                    destination_x: u32::MAX,
                    width: u32::MAX,
                }],
                u32::MAX,
                u32::MAX,
                0,
                operator,
            )
            .unwrap();
        assert_eq!(actual, original);
    }
}

#[test]
fn zero_width_unaligned_row_blit_preserves_all_storage() {
    let source = patterned_bitmap(1, 1, 7);
    let original = patterned_bitmap(9, 1, 13);
    for (operator, _) in OPERATOR_TRUTHS {
        let mut actual = original.clone();
        let result = actual.compose_row_batch_chunked(
            &[RowBlit {
                source: &source,
                source_x: 1,
                destination_x: 1,
                width: 0,
            }],
            0,
            0,
            1,
            operator,
        );
        assert!(result.is_ok(), "zero-width unaligned blit: {result:?}");
        assert_eq!(actual, original);
    }
}

#[test]
fn chunk_composition_randomized_matches_scalar_oracle() {
    let operators = [
        CombinationOperator::Or,
        CombinationOperator::And,
        CombinationOperator::Xor,
        CombinationOperator::Xnor,
        CombinationOperator::Replace,
    ];
    let mut state = 0x4d59_5df4_d0f3_3173_u64;
    for case in 0..2_000_u64 {
        state = state
            .wrapping_mul(2_862_933_555_777_941_757)
            .wrapping_add(3_037_000_493);
        let destination_width =
            u32::try_from(state & u64::from(u32::MAX)).expect("masked value fits") % 521;
        let destination_height =
            u32::try_from((state >> 16) & u64::from(u32::MAX)).expect("masked value fits") % 9;
        state = state.rotate_left(29) ^ case;
        let source_width =
            u32::try_from(state & u64::from(u32::MAX)).expect("masked value fits") % 521;
        let source_height =
            u32::try_from((state >> 20) & u64::from(u32::MAX)).expect("masked value fits") % 9;
        let x = ((state >> 32) as u32) % destination_width.saturating_add(3).max(1);
        let y = ((state >> 44) as u32) % destination_height.saturating_add(3).max(1);
        let operator = operators
            [usize::try_from(state % operators.len() as u64).expect("operator index fits")];
        let source = patterned_bitmap(source_width, source_height, state ^ 0xa5a5_a5a5);
        let mut expected =
            patterned_bitmap(destination_width, destination_height, state ^ 0x5a5a_5a5a);
        let mut actual = expected.clone();
        let expected_result = scalar_compose(&mut expected, &source, x, y, operator);
        let actual_result = actual.compose(&source, x, y, operator);
        assert_eq!(actual_result, expected_result, "case {case}");
        assert_eq!(actual, expected, "case {case}");
    }
}

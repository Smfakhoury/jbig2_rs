#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

use super::access::copy_row_bytes;
use super::chunks::{
    read_chunk, read_shifted_byte, read_shifted_chunk, read_shifted_tail, write_chunk,
};
use super::storage::checked_bitmap_layout;
use super::*;
use crate::{DecodeError, LimitExceeded, LimitKind, Limits, MalformedError};
use std::hint::black_box;
use std::time::{Duration, Instant};

fn scalar_compose(
    destination: &mut Bitmap,
    source: &Bitmap,
    x: u32,
    y: u32,
    operator: CombinationOperator,
) -> Result<(), MalformedError> {
    let width = source.width.min(destination.width.saturating_sub(x));
    let height = source.height.min(destination.height.saturating_sub(y));
    for source_y in 0..height {
        let destination_y = y
            .checked_add(source_y)
            .ok_or(MalformedError::IntegerOverflow)?;
        for source_x in 0..width {
            let destination_x = x
                .checked_add(source_x)
                .ok_or(MalformedError::IntegerOverflow)?;
            let source_value = source
                .pixel(source_x, source_y)
                .ok_or(MalformedError::IntegerOverflow)?;
            let destination_value = destination
                .pixel(destination_x, destination_y)
                .ok_or(MalformedError::IntegerOverflow)?;
            destination.set_pixel(
                destination_x,
                destination_y,
                operator.apply(destination_value, source_value),
            )?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn scalar_compose_region(
    destination: &mut Bitmap,
    source: &Bitmap,
    source_x: u32,
    source_y: u32,
    destination_x: u32,
    destination_y: u32,
    width: u32,
    height: u32,
    operator: CombinationOperator,
) -> Result<(), MalformedError> {
    if source_x
        .checked_add(width)
        .is_none_or(|end| end > source.width)
        || source_y
            .checked_add(height)
            .is_none_or(|end| end > source.height)
        || destination_x
            .checked_add(width)
            .is_none_or(|end| end > destination.width)
        || destination_y
            .checked_add(height)
            .is_none_or(|end| end > destination.height)
    {
        return Err(MalformedError::IntegerOverflow);
    }
    for row in 0..height {
        for column in 0..width {
            let sx = source_x + column;
            let sy = source_y + row;
            let dx = destination_x + column;
            let dy = destination_y + row;
            let incoming = source
                .pixel(sx, sy)
                .ok_or(MalformedError::IntegerOverflow)?;
            let current = destination
                .pixel(dx, dy)
                .ok_or(MalformedError::IntegerOverflow)?;
            destination.set_pixel(dx, dy, operator.apply(current, incoming))?;
        }
    }
    Ok(())
}

fn scalar_fill_black_span(
    bitmap: &mut Bitmap,
    row: u32,
    start: u32,
    end: u32,
) -> Result<(), MalformedError> {
    if start > end || end > bitmap.width || row >= bitmap.height {
        return Err(MalformedError::IntegerOverflow);
    }
    for x in start..end {
        bitmap.set_pixel(x, row, true)?;
    }
    Ok(())
}

fn scalar_copy_row(
    bitmap: &mut Bitmap,
    source: u32,
    destination: u32,
) -> Result<(), MalformedError> {
    if source >= bitmap.height || destination >= bitmap.height {
        return Err(MalformedError::IntegerOverflow);
    }
    if source == destination {
        return Ok(());
    }
    let source_start =
        usize::try_from(source).map_err(|_error| MalformedError::IntegerOverflow)? * bitmap.stride;
    let destination_start = usize::try_from(destination)
        .map_err(|_error| MalformedError::IntegerOverflow)?
        * bitmap.stride;
    for index in 0..bitmap.stride {
        bitmap.data[destination_start + index] = bitmap.data[source_start + index];
    }
    Ok(())
}

fn patterned_bitmap(width: u32, height: u32, mut state: u64) -> Bitmap {
    let mut bitmap = Bitmap::blank(width, height, &Limits::default()).expect("bitmap fits");
    for y in 0..height {
        for x in 0..width {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            if state >> 63 != 0 {
                bitmap.set_pixel(x, y, true).expect("pixel is in bounds");
            }
        }
    }
    bitmap
}

const OPERATOR_TRUTHS: [(CombinationOperator, [bool; 4]); 5] = [
    (CombinationOperator::Or, [false, true, true, true]),
    (CombinationOperator::And, [false, false, false, true]),
    (CombinationOperator::Xor, [false, true, true, false]),
    (CombinationOperator::Xnor, [true, false, false, true]),
    (CombinationOperator::Replace, [false, true, false, true]),
];

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
fn blank_bitmap_has_expected_layout() {
    let bitmap = Bitmap::blank(9, 2, &Limits::default()).expect("bitmap should fit");
    assert_eq!(bitmap.width(), 9);
    assert_eq!(bitmap.height(), 2);
    assert_eq!(bitmap.stride(), 2);
    assert_eq!(bitmap.bytes(), &[0, 0, 0, 0]);
    assert_eq!(bitmap.pixel(8, 1), Some(false));
    assert_eq!(bitmap.pixel(9, 1), None);
}

#[test]
fn bitmap_enforces_pixel_limit_before_allocation() {
    let limits = Limits {
        max_pixels_per_page: 8,
        ..Limits::default()
    };
    let error = Bitmap::blank(9, 1, &limits).expect_err("bitmap must exceed the limit");
    assert!(matches!(
        error,
        DecodeError::LimitExceeded(LimitExceeded {
            kind: LimitKind::PagePixels,
            limit: 8,
            actual: 9
        })
    ));
}

#[test]
fn filled_bitmap_clears_padding_bits() {
    let bitmap = Bitmap::filled(9, 2, true, &Limits::default()).expect("bitmap should fit");
    assert_eq!(bitmap.bytes(), &[0xFF, 0x80, 0xFF, 0x80]);
    assert_eq!(bitmap.pixel(8, 1), Some(true));
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
fn filled_layout_and_padding_are_deterministic() {
    let limits = Limits::default();
    let mut cases = 0;
    for width in 0..=128 {
        for height in 0..=4 {
            for value in [false, true] {
                let bitmap = Bitmap::filled(width, height, value, &limits).expect("bitmap fits");
                let expected_stride = usize::try_from(width.div_ceil(8)).expect("test width fits");
                assert_eq!(bitmap.stride(), expected_stride);
                assert_eq!(
                    bitmap.bytes().len(),
                    expected_stride * usize::try_from(height).unwrap()
                );
                for y in 0..height {
                    for x in 0..width {
                        assert_eq!(bitmap.pixel(x, y), Some(value));
                    }
                    for column in 0..expected_stride {
                        let mut expected = 0_u8;
                        for bit in 0..8 {
                            expected = expected * 2
                                + u8::from(
                                    value && column * 8 + bit < usize::try_from(width).unwrap(),
                                );
                        }
                        assert_eq!(
                            bitmap.bytes()[usize::try_from(y).unwrap() * expected_stride + column],
                            expected,
                            "{width}x{height}, row{y} byte{column}, value{value}",
                        );
                    }
                }
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 1290);
}

#[test]
fn set_pixel_is_local_and_rejects_out_of_bounds() {
    let limits = Limits::default();
    let mut bitmap = Bitmap::blank(9, 2, &limits).expect("bitmap fits");
    bitmap.set_pixel(7, 0, true).expect("pixel is in bounds");
    assert_eq!(bitmap.bytes(), &[0x01, 0, 0, 0]);
    assert_eq!(bitmap.pixel(7, 0), Some(true));
    assert_eq!(bitmap.pixel(6, 0), Some(false));
    assert_eq!(bitmap.pixel(8, 0), Some(false));

    let before = bitmap.bytes().to_vec();
    assert!(matches!(
        bitmap.set_pixel(9, 0, true),
        Err(MalformedError::IntegerOverflow)
    ));
    assert_eq!(bitmap.bytes(), before);
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
fn shifted_reads_accept_exactly_available_valid_bit_spans() {
    for length in 0..=18 {
        let bytes: Vec<u8> = (0..length)
            .map(|index| u8::try_from((index * 37 + 19) % 256).unwrap())
            .collect();
        let scalar = |start: usize, shift: u32, count: u32| -> Option<u64> {
            if shift >= 8 {
                return None;
            }
            let first = start
                .checked_mul(8)?
                .checked_add(usize::try_from(shift).unwrap())?;
            let mut result = 0;
            for bit in 0..count {
                let position = first.checked_add(usize::try_from(bit).unwrap())?;
                let byte = bytes.get(position / 8)?;
                result = (result << 1) | u64::from((byte >> (7 - position % 8)) & 1);
            }
            Some(result)
        };
        for start in (0..=length).chain([usize::MAX]) {
            for shift in 0..=8 {
                assert_eq!(
                    read_shifted_byte(&bytes, start, shift).map(u64::from),
                    scalar(start, shift, 8)
                );
                assert_eq!(
                    read_shifted_chunk(&bytes, start, shift),
                    scalar(start, shift, 64)
                );
                for count in 1..=8 {
                    assert_eq!(
                        read_shifted_tail(&bytes, start, shift, count)
                            .map(|value| u64::from(value >> (8 - count))),
                        scalar(start, shift, count),
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
fn checked_layout_enforces_each_bitmap_limit() {
    let width_limits = Limits {
        max_width: 8,
        ..Limits::default()
    };
    assert!(matches!(
        checked_bitmap_layout(9, 1, &width_limits),
        Err(DecodeError::LimitExceeded(LimitExceeded {
            kind: LimitKind::Width,
            limit: 8,
            actual: 9,
        }))
    ));

    let memory_limits = Limits {
        max_decoder_managed_memory_bytes: 1,
        ..Limits::default()
    };
    assert!(matches!(
        checked_bitmap_layout(9, 1, &memory_limits),
        Err(DecodeError::LimitExceeded(LimitExceeded {
            kind: LimitKind::Memory,
            limit: 1,
            actual: 2,
        }))
    ));
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

#[test]
fn zero_area_region_requests_succeed_without_changing_storage() {
    let source = patterned_bitmap(17, 3, 17);
    let original = patterned_bitmap(25, 4, 29);
    for operator in [
        CombinationOperator::Or,
        CombinationOperator::And,
        CombinationOperator::Xor,
        CombinationOperator::Xnor,
        CombinationOperator::Replace,
    ] {
        for (sx, sy, dx, dy, width, height) in [
            (17, 0, 25, 0, 0, 3),
            (0, 3, 0, 4, 17, 0),
            (17, 3, 25, 4, 0, 0),
            (0, 0, 0, 0, 0, 0),
        ] {
            let mut destination = original.clone();
            destination
                .compose_region(&source, sx, sy, dx, dy, width, height, operator)
                .expect("valid zero-area request");
            assert_eq!(destination, original);
        }
    }
}

#[test]
fn invalid_region_requests_preserve_exact_error_and_all_storage() {
    let source = patterned_bitmap(17, 3, 17);
    let original = patterned_bitmap(25, 4, 29);
    for operator in [
        CombinationOperator::Or,
        CombinationOperator::And,
        CombinationOperator::Xor,
        CombinationOperator::Xnor,
        CombinationOperator::Replace,
    ] {
        for (sx, sy, dx, dy, width, height) in [
            (u32::MAX, 0, 0, 0, 1, 1),
            (0, u32::MAX, 0, 0, 1, 1),
            (0, 0, u32::MAX, 0, 1, 1),
            (0, 0, 0, u32::MAX, 1, 1),
            (18, 0, 0, 0, 0, 1),
            (0, 4, 0, 0, 1, 0),
            (0, 0, 26, 0, 0, 1),
            (0, 0, 0, 5, 1, 0),
            (17, 0, 0, 0, 1, 1),
            (0, 3, 0, 0, 1, 1),
            (0, 0, 25, 0, 1, 1),
            (0, 0, 0, 4, 1, 1),
        ] {
            let mut destination = original.clone();
            assert!(matches!(
                destination.compose_region(&source, sx, sy, dx, dy, width, height, operator),
                Err(MalformedError::IntegerOverflow)
            ));
            assert_eq!(destination, original);
        }
    }
}

#[test]
fn source_offset_region_composition_exhaustively_matches_scalar_oracle() {
    let operators = [
        CombinationOperator::Or,
        CombinationOperator::And,
        CombinationOperator::Xor,
        CombinationOperator::Xnor,
        CombinationOperator::Replace,
    ];
    for source_width in 0_u32..=18 {
        for destination_width in 0_u32..=18 {
            let source = patterned_bitmap(source_width, 3, u64::from(source_width));
            let original = patterned_bitmap(destination_width, 4, !u64::from(destination_width));
            for source_x in 0..=source_width {
                for destination_x in 0..=destination_width {
                    let max_width =
                        (source_width - source_x).min(destination_width - destination_x);
                    for width in 0..=max_width {
                        for operator in operators {
                            let mut expected = original.clone();
                            let mut actual = original.clone();
                            scalar_compose_region(
                                &mut expected,
                                &source,
                                source_x,
                                1,
                                destination_x,
                                1,
                                width,
                                2,
                                operator,
                            )
                            .expect("scalar region composition succeeds");
                            actual
                                .compose_region(
                                    &source,
                                    source_x,
                                    1,
                                    destination_x,
                                    1,
                                    width,
                                    2,
                                    operator,
                                )
                                .expect("chunk region composition succeeds");
                            assert_eq!(
                                actual, expected,
                                "source width {source_width}, destination width \
                                 {destination_width}, source x {source_x}, destination x \
                                 {destination_x}, width {width}, operator {operator:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn source_offset_region_composition_randomized_matches_scalar_oracle() {
    let operators = [
        CombinationOperator::Or,
        CombinationOperator::And,
        CombinationOperator::Xor,
        CombinationOperator::Xnor,
        CombinationOperator::Replace,
    ];
    let mut state = 0xd1b5_4a32_d192_ed03_u64;
    for case in 0..2_000_u64 {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let source_width =
            u32::try_from(state & u64::from(u16::MAX)).expect("masked value fits") % 521;
        let destination_width =
            u32::try_from((state >> 16) & u64::from(u16::MAX)).expect("masked value fits") % 521;
        let source_height = u32::try_from((state >> 32) & 15).expect("masked value fits");
        let destination_height = u32::try_from((state >> 40) & 15).expect("masked value fits");
        let source_x = if source_width == 0 {
            0
        } else {
            u32::try_from((state >> 8) & u64::from(u32::MAX)).expect("masked value fits")
                % (source_width + 1)
        };
        let destination_x = if destination_width == 0 {
            0
        } else {
            u32::try_from((state >> 24) & u64::from(u32::MAX)).expect("masked value fits")
                % (destination_width + 1)
        };
        let source_y = if source_height == 0 {
            0
        } else {
            u32::try_from((state >> 36) & u64::from(u32::MAX)).expect("masked value fits")
                % (source_height + 1)
        };
        let destination_y = if destination_height == 0 {
            0
        } else {
            u32::try_from((state >> 44) & u64::from(u32::MAX)).expect("masked value fits")
                % (destination_height + 1)
        };
        let width = u32::try_from((state >> 12) & u64::from(u32::MAX)).expect("masked value fits")
            % ((source_width - source_x).min(destination_width - destination_x) + 1);
        let height = u32::try_from((state >> 20) & u64::from(u32::MAX)).expect("masked value fits")
            % ((source_height - source_y).min(destination_height - destination_y) + 1);
        let operator =
            operators[usize::try_from(case % operators.len() as u64).expect("operator index fits")];
        let source = patterned_bitmap(source_width, source_height, state);
        let mut expected = patterned_bitmap(destination_width, destination_height, !state);
        let mut actual = expected.clone();
        scalar_compose_region(
            &mut expected,
            &source,
            source_x,
            source_y,
            destination_x,
            destination_y,
            width,
            height,
            operator,
        )
        .expect("scalar region composition succeeds");
        actual
            .compose_region(
                &source,
                source_x,
                source_y,
                destination_x,
                destination_y,
                width,
                height,
                operator,
            )
            .expect("chunk region composition succeeds");
        assert_eq!(actual, expected, "case {case}");
    }
}

#[test]
fn prefix_copy_preserves_suffix_source_and_error_state() {
    for (source_height, destination_height) in [
        (0, 0),
        (0, 9),
        (9, 0),
        (3, 7),
        (7, 3),
        (66, 129),
        (129, 66),
        (257, 1024),
    ] {
        let source = patterned_bitmap(8, source_height, 17);
        let original = patterned_bitmap(8, destination_height, 29);
        let immutable = source.clone();
        let limit = source.data.len().min(original.data.len());
        for count in 0..=limit {
            let mut expected = original.clone();
            expected.data[..count].copy_from_slice(&source.data[..count]);
            let mut actual = original.clone();
            actual.copy_prefix_from(&source, count).unwrap();
            assert_eq!(
                actual, expected,
                "source {source_height}, destination {destination_height}, count {count}"
            );
            assert_eq!(source, immutable);
        }
        for count in [
            limit + 1,
            source.data.len().max(original.data.len()) + 1,
            usize::MAX,
        ] {
            let mut actual = original.clone();
            assert!(matches!(
                actual.copy_prefix_from(&source, count),
                Err(MalformedError::IntegerOverflow)
            ));
            assert_eq!(actual, original);
            assert_eq!(source, immutable);
        }
    }
}

#[test]
fn blank_storage_is_zero_for_empty_and_partial_rows() {
    for width in [0, 1, 7, 8, 9, 63, 64, 65] {
        for height in [0, 1, 3] {
            let bitmap = Bitmap::blank(width, height, &Limits::default()).unwrap();
            assert_eq!((bitmap.width(), bitmap.height()), (width, height));
            assert!(bitmap.bytes().iter().all(|byte| *byte == 0));
            for y in 0..height {
                for x in 0..width {
                    assert_eq!(bitmap.pixel(x, y), Some(false));
                }
            }
        }
    }
}

#[test]
fn chunk_values_match_independent_big_endian_oracles() {
    let mut values = vec![0, u64::MAX, 0x0123_4567_89ab_cdef];
    for offset in 0..8 {
        for byte in 0..=u8::MAX {
            values.push(u64::from(byte) << (offset * 8));
        }
    }
    for value in values {
        let encoded = value.to_be_bytes();
        for start in 0..=9 {
            let mut bytes = vec![0xa5; 25];
            let mut expected = bytes.clone();
            expected[start..start + 8].copy_from_slice(&encoded);
            assert!(write_chunk(&mut bytes, start, value));
            assert_eq!(bytes, expected);
            assert_eq!(read_chunk(&bytes, start), Some(value));
            assert_eq!(
                read_chunk(&expected, start),
                Some(u64::from_be_bytes(encoded)),
            );
        }
    }
}

#[test]
fn chunk_access_accepts_exactly_complete_ranges_and_preserves_failed_writes() {
    for length in 0..=25 {
        let original: Vec<u8> = (0..length)
            .map(|index| u8::try_from((index * 37 + 19) % 256).expect("byte"))
            .collect();
        for start in (0..=length).chain([usize::MAX - 7, usize::MAX]) {
            let range = start
                .checked_add(8)
                .filter(|&end| end <= length)
                .map(|end| start..end);
            let expected_read = range.as_ref().map(|range| {
                u64::from_be_bytes(
                    original[range.clone()]
                        .try_into()
                        .expect("exactly eight bytes"),
                )
            });
            assert_eq!(read_chunk(&original, start), expected_read);
            for value in [0, u64::MAX, 0x0123_4567_89ab_cdef] {
                let mut bytes = original.clone();
                let mut expected = original.clone();
                if let Some(range) = &range {
                    expected[range.clone()].copy_from_slice(&value.to_be_bytes());
                }
                assert_eq!(write_chunk(&mut bytes, start, value), range.is_some());
                assert_eq!(bytes, expected);
            }
        }
    }
}

#[test]
fn row_copy_bytes_and_chunk_access_have_exact_boundaries() {
    for length in [0, 1, 2, 8, 64, 65, 66, 127, 128, 129, 257] {
        let source: Vec<u8> = (0..length)
            .map(|index| u8::try_from(index % 251).unwrap())
            .collect();
        let mut destination = vec![0xa5; length];
        copy_row_bytes(&mut destination, &source);
        assert_eq!(destination, source);
    }
    let mut bytes = vec![0x81; 8];
    let original = bytes.clone();
    for start in [1, usize::MAX - 7, usize::MAX] {
        assert_eq!(read_chunk(&bytes, start), None);
        assert!(!write_chunk(&mut bytes, start, u64::MAX));
        assert_eq!(bytes, original);
    }
    assert_eq!(read_shifted_byte(&bytes, 0, 8), None);
    assert_eq!(read_shifted_tail(&bytes, 0, 1, 0), None);
    assert_eq!(read_shifted_tail(&bytes, 0, 1, 9), None);
    assert_eq!(read_shifted_chunk(&bytes, 0, 8), None);
}

#[test]
fn row_copy_rejects_invalid_indices_and_storage_without_mutation() {
    for (source, destination) in [(2, 0), (0, 2), (2, 2)] {
        let mut bitmap = Bitmap::blank(8, 2, &Limits::default()).unwrap();
        let original = bitmap.clone();
        assert!(matches!(
            bitmap.copy_row(source, destination),
            Err(MalformedError::IntegerOverflow)
        ));
        assert_eq!(bitmap, original);
    }
    for (stride, source, destination) in [
        (usize::MAX, 2, 0),
        (usize::MAX, 0, 2),
        (usize::MAX, 1, 0),
        (usize::MAX, 0, 1),
        (1, 1, 0),
        (1, 0, 1),
    ] {
        let mut bitmap = Bitmap {
            width: 8,
            height: 3,
            stride,
            data: vec![0xa5],
        };
        let original = bitmap.clone();
        assert!(matches!(
            bitmap.copy_row(source, destination),
            Err(MalformedError::IntegerOverflow)
        ));
        assert_eq!(bitmap, original);
    }
}

#[test]
fn black_spans_preserve_arbitrary_padding_and_bounds_errors() {
    for width in [1_u32, 7, 9, 65, 73, 129, 137] {
        for seed in [0_u8, 0xFF, 0xA5, 0x5A] {
            let mut original = patterned_bitmap(width, 3, u64::from(width));
            original.data.fill(seed);
            for row in 0..3 {
                for start in 0..=width {
                    for end in start..=width {
                        let mut actual = original.clone();
                        let mut expected = original.clone();
                        actual.fill_black_span(row, start, end).unwrap();
                        scalar_fill_black_span(&mut expected, row, start, end).unwrap();
                        assert_eq!(actual, expected, "{width}: {row}, {start}..{end}, {seed}");
                    }
                }
            }
            for (row, start, end) in [(3, 0, 0), (0, 1, 0), (0, 0, width + 1)] {
                let mut actual = original.clone();
                assert_eq!(
                    actual.fill_black_span(row, start, end),
                    Err(MalformedError::IntegerOverflow),
                );
                assert_eq!(actual, original);
            }
        }
    }
}

#[test]
fn chunk_span_fill_and_row_copy_match_scalar_oracles() {
    for width in 0_u32..=130 {
        let original = patterned_bitmap(width, 4, u64::from(width));
        for row in 0_u32..4 {
            for start in 0..=width {
                for end in start..=width {
                    let mut expected = original.clone();
                    let mut actual = original.clone();
                    scalar_fill_black_span(&mut expected, row, start, end)
                        .expect("scalar fill succeeds");
                    actual
                        .fill_black_span(row, start, end)
                        .expect("chunk fill succeeds");
                    assert_eq!(actual, expected, "width {width}, row {row}, {start}..{end}");
                }
            }
            for destination in 0_u32..4 {
                let mut expected = original.clone();
                let mut actual = original.clone();
                scalar_copy_row(&mut expected, row, destination).expect("scalar copy succeeds");
                actual
                    .copy_row(row, destination)
                    .expect("chunk copy succeeds");
                assert_eq!(
                    actual, expected,
                    "width {width}, source {row}, destination {destination}"
                );
            }
            let source = patterned_bitmap(width, 4, !u64::from(width));
            for byte_count in 0..=original.data.len() {
                let mut expected = original.clone();
                let mut actual = original.clone();
                expected.data[..byte_count].copy_from_slice(&source.data[..byte_count]);
                actual
                    .copy_prefix_from(&source, byte_count)
                    .expect("chunk prefix copy succeeds");
                assert_eq!(actual, expected, "width {width}, byte count {byte_count}");
            }
        }
    }
}

fn median_duration(mut values: Vec<Duration>) -> Duration {
    values.sort_unstable();
    values[values.len() / 2]
}

#[test]
#[ignore = "microbenchmark; run explicitly with --release --ignored --nocapture"]
fn benchmark_chunk_composition_against_fa117f0_scalar_oracle() {
    let operators = [
        CombinationOperator::Or,
        CombinationOperator::And,
        CombinationOperator::Xor,
        CombinationOperator::Xnor,
        CombinationOperator::Replace,
    ];
    let source = patterned_bitmap(1024, 512, 0x1234_5678_9abc_def0);
    let original = patterned_bitmap(1032, 512, 0xfedc_ba98_7654_3210);
    let mut chunk_times = Vec::new();
    let mut scalar_times = Vec::new();
    for sample in 0..11 {
        let mut chunk = original.clone();
        let mut scalar = original.clone();
        if sample % 2 == 0 {
            let start = Instant::now();
            for x in [0, 3] {
                for operator in operators {
                    chunk
                        .compose(&source, x, 0, operator)
                        .expect("chunk composition succeeds");
                }
            }
            chunk_times.push(start.elapsed());
            black_box(chunk.bytes());

            let start = Instant::now();
            for x in [0, 3] {
                for operator in operators {
                    scalar_compose(&mut scalar, &source, x, 0, operator)
                        .expect("scalar composition succeeds");
                }
            }
            scalar_times.push(start.elapsed());
            black_box(scalar.bytes());
        } else {
            let start = Instant::now();
            for x in [0, 3] {
                for operator in operators {
                    scalar_compose(&mut scalar, &source, x, 0, operator)
                        .expect("scalar composition succeeds");
                }
            }
            scalar_times.push(start.elapsed());
            black_box(scalar.bytes());

            let start = Instant::now();
            for x in [0, 3] {
                for operator in operators {
                    chunk
                        .compose(&source, x, 0, operator)
                        .expect("chunk composition succeeds");
                }
            }
            chunk_times.push(start.elapsed());
            black_box(chunk.bytes());
        }
        assert_eq!(chunk, scalar);
    }
    let chunk = median_duration(chunk_times);
    let scalar = median_duration(scalar_times);
    println!(
        "composition scalar_fa117f0_median_ns={} chunk_median_ns={} speedup={:.3}",
        scalar.as_nanos(),
        chunk.as_nanos(),
        scalar.as_secs_f64() / chunk.as_secs_f64(),
    );
}

#[test]
#[ignore = "microbenchmark; run explicitly with --release --ignored --nocapture"]
fn benchmark_source_offset_composition_against_6333205_scalar_oracle() {
    let operators = [
        CombinationOperator::Or,
        CombinationOperator::And,
        CombinationOperator::Xor,
        CombinationOperator::Xnor,
        CombinationOperator::Replace,
    ];
    let source = patterned_bitmap(1032, 512, 0x0ddc_0ffe_e15e_beef);
    let original = patterned_bitmap(1040, 512, 0x5ca1_ab1e_0123_4567);
    let mut chunk_times = Vec::new();
    let mut scalar_times = Vec::new();
    for sample in 0..11 {
        let mut chunk = original.clone();
        let mut scalar = original.clone();
        if sample % 2 == 0 {
            let start = Instant::now();
            for operator in operators {
                chunk
                    .compose_region(&source, 3, 0, 5, 0, 1024, 512, operator)
                    .expect("chunk region composition succeeds");
            }
            chunk_times.push(start.elapsed());
            black_box(chunk.bytes());

            let start = Instant::now();
            for operator in operators {
                scalar_compose_region(&mut scalar, &source, 3, 0, 5, 0, 1024, 512, operator)
                    .expect("scalar region composition succeeds");
            }
            scalar_times.push(start.elapsed());
            black_box(scalar.bytes());
        } else {
            let start = Instant::now();
            for operator in operators {
                scalar_compose_region(&mut scalar, &source, 3, 0, 5, 0, 1024, 512, operator)
                    .expect("scalar region composition succeeds");
            }
            scalar_times.push(start.elapsed());
            black_box(scalar.bytes());

            let start = Instant::now();
            for operator in operators {
                chunk
                    .compose_region(&source, 3, 0, 5, 0, 1024, 512, operator)
                    .expect("chunk region composition succeeds");
            }
            chunk_times.push(start.elapsed());
            black_box(chunk.bytes());
        }
        assert_eq!(chunk, scalar);
    }
    let chunk = median_duration(chunk_times);
    let scalar = median_duration(scalar_times);
    println!(
        "source-offset-composition scalar_6333205_median_ns={} chunk_median_ns={} \
         speedup={:.3}",
        scalar.as_nanos(),
        chunk.as_nanos(),
        scalar.as_secs_f64() / chunk.as_secs_f64(),
    );
}

#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

use super::bitmap::{bitmap_from_packed_bytes, clone_bitmap};
use super::codec::{decode_refined_bitmap, expand_symbol_code_lengths};
use super::dictionary::{assemble_exported_symbols, decode_export_runs};
use super::headers::parse_text_huffman_flags;
use super::placement::{flush_placement_batch, negative_magnitude, prepare_placement};
use super::tables::{
    associate_dictionary_tables, associate_text_tables, bind_tables, find_table_binding,
    materialize_huffman_table, same_table_parameter, select_dictionary_tables, select_text_tables,
    wrap_custom_tables,
};
use super::*;
use std::hint::black_box;
use std::time::{Duration, Instant};

fn arithmetic_codec_fixture(bytes: &[u8], iaid_count: usize, salt: usize) -> ArithmeticCodec<'_> {
    ArithmeticCodec {
        decoder: ArithmeticDecoder::new(bytes).expect("arithmetic constructor"),
        integers: IntegerContexts {
            slots: std::array::from_fn(|slot| {
                std::array::from_fn(|index| {
                    ArithmeticContext::from_state(
                        u8::try_from((index * 17 + slot + salt) % 47).unwrap(),
                        u8::try_from((index / 2 + slot + salt) % 2).unwrap(),
                    )
                    .unwrap()
                })
            }),
        },
        iaid: (0..iaid_count)
            .map(|index| {
                ArithmeticContext::from_state(
                    u8::try_from((index * 17 + salt) % 47).unwrap(),
                    u8::try_from((index / 2 + salt) % 2).unwrap(),
                )
                .unwrap()
            })
            .collect(),
        refinement: None,
        refinement_work: 73,
    }
}

fn assert_arithmetic_forwarding<T: std::fmt::Debug + PartialEq>(
    actual: Result<T, SymbolTextError>,
    expected: Result<T, SymbolTextError>,
) -> bool {
    match (actual, expected) {
        (Ok(actual), Ok(expected)) => {
            assert_eq!(actual, expected);
            true
        }
        (
            Err(SymbolTextError::Decode(DecodeError::Malformed(actual))),
            Err(SymbolTextError::Decode(DecodeError::Malformed(expected))),
        ) => {
            assert_eq!(actual, expected);
            false
        }
        (actual, expected) => {
            panic!("wrong arithmetic forwarding: {actual:?}, expected {expected:?}")
        }
    }
}

fn assert_arithmetic_cursor_and_continuation(
    actual: &mut ArithmeticDecoder<'_>,
    expected: &mut ArithmeticDecoder<'_>,
    success: bool,
) {
    assert_eq!(actual.byte_position(), expected.byte_position());
    assert_eq!(actual.marker(), expected.marker());
    if success {
        let mut actual_context = ArithmeticContext::new();
        let mut expected_context = ArithmeticContext::new();
        for _ in 0..64 {
            let signal = actual.decode_signal(&mut actual_context);
            assert_eq!(signal, expected.decode_signal(&mut expected_context));
            assert_eq!(actual_context, expected_context);
            assert_eq!(actual.byte_position(), expected.byte_position());
            assert_eq!(actual.marker(), expected.marker());
            if signal > 1 {
                break;
            }
        }
    }
}

#[test]
fn arithmetic_integer_caller_matches_direct_procedure_and_preserves_other_banks() {
    let fields = [
        IntegerField::AggregationInstances,
        IntegerField::HeightDelta,
        IntegerField::WidthDelta,
        IntegerField::ExportRun,
        IntegerField::StripDelta,
        IntegerField::FirstS,
        IntegerField::SubsequentS,
        IntegerField::WithinStripT,
        IntegerField::RefinementWidth,
        IntegerField::RefinementHeight,
        IntegerField::RefinementX,
        IntegerField::RefinementY,
        IntegerField::RefinementSize,
        IntegerField::RefinementIndicator,
        IntegerField::DictionaryBitmapSize,
    ];
    let samples: [&[u8]; 6] = [
        &[0, 0],
        &[0xff, 0x90],
        &[0; 64],
        &[0x13; 64],
        &[0xaa; 64],
        &[0xff, 0x8f, 0, 0],
    ];
    let mut outcomes = [0; 2];
    for (slot, field) in fields.into_iter().enumerate() {
        assert_eq!(field.slot(), slot);
        for bytes in samples {
            for salt in [0, 1, 23, 46] {
                let mut actual = arithmetic_codec_fixture(bytes, 7, salt);
                let before = actual.integers.slots;
                let before_iaid = actual.iaid.clone();
                let mut expected_bank = before[slot];
                let mut expected_decoder = ArithmeticDecoder::new(bytes).unwrap();
                let expected = expected_decoder
                    .decode_integer(&mut expected_bank)
                    .map(|value| match value {
                        ArithmeticInteger::Value(value) => EntropyInteger::Value(value),
                        ArithmeticInteger::OutOfBand => EntropyInteger::OutOfBand,
                    })
                    .map_err(SymbolTextError::from);
                let success = assert_arithmetic_forwarding(actual.decode_integer(field), expected);
                outcomes[usize::from(success)] += 1;
                assert_eq!(actual.integers.slots[slot], expected_bank);
                for (index, bank) in before.iter().enumerate() {
                    if index != slot {
                        assert_eq!(actual.integers.slots[index], *bank);
                    }
                }
                assert_eq!(actual.iaid, before_iaid);
                assert!(actual.refinement.is_none());
                assert_eq!(actual.refinement_work, 73);
                assert_arithmetic_cursor_and_continuation(
                    &mut actual.decoder,
                    &mut expected_decoder,
                    success,
                );
            }
        }
    }
    assert_eq!(outcomes.iter().sum::<usize>(), 360);
    assert!(outcomes.into_iter().all(|count| count > 0));
}

#[test]
fn arithmetic_iaid_caller_forwards_every_length_and_preserves_integer_banks() {
    let samples: [&[u8]; 6] = [
        &[0, 0],
        &[0xff, 0x90],
        &[0; 64],
        &[0x13; 64],
        &[0xaa; 64],
        &[0xff, 0x8f, 0, 0],
    ];
    let mut outcomes = [0; 2];
    for length in 0..=u8::MAX {
        for count in [0, 1, 2, 3, 4, 16, 32, 35] {
            for bytes in samples {
                let mut actual = arithmetic_codec_fixture(bytes, count, usize::from(length) % 47);
                let before = actual.integers.slots;
                let mut expected_bank = actual.iaid.clone();
                let mut expected_decoder = ArithmeticDecoder::new(bytes).unwrap();
                let expected = expected_decoder
                    .decode_iaid(length, &mut expected_bank)
                    .map_err(SymbolTextError::from);
                let request = SymbolIdRequest {
                    available_symbols: 0,
                    code_length: length,
                    huffman: false,
                };
                let success =
                    assert_arithmetic_forwarding(actual.decode_symbol_id(request), expected);
                outcomes[usize::from(success)] += 1;
                assert_eq!(actual.iaid, expected_bank);
                assert_eq!(actual.integers.slots, before);
                assert!(actual.refinement.is_none());
                assert_eq!(actual.refinement_work, 73);
                assert_arithmetic_cursor_and_continuation(
                    &mut actual.decoder,
                    &mut expected_decoder,
                    success,
                );
            }
        }
    }
    assert_eq!(outcomes.iter().sum::<usize>(), 12_288);
    assert!(outcomes.into_iter().all(|count| count > 0));
}

#[test]
fn symbol_code_length_is_minimal_at_every_u32_power_boundary() {
    let boundaries = (0..u32::BITS).flat_map(|shift| {
        let power = 1_u32 << shift;
        [power - 1, power, power + 1]
    });
    for count in (0..=65_536).chain(boundaries).chain([u32::MAX]) {
        let length = symbol_id_code_length(count).expect("bounded code length");
        let expected = u8::try_from(u32::BITS - count.saturating_sub(1).leading_zeros())
            .expect("at most 32 bits");
        assert_eq!(length, expected, "symbol count {count}");
        assert!((1_u64 << length) >= u64::from(count.max(1)));
        if length != 0 {
            assert!((1_u64 << (length - 1)) < u64::from(count));
        }
    }
}

#[test]
fn packed_symbol_storage_matches_pixels_and_rejects_each_dirty_row() {
    let limits = Limits::default();
    for width in 0_u32..=33 {
        for height in [0_u32, 1, 3] {
            let stride = usize::try_from(width.div_ceil(8)).unwrap();
            let length = stride * usize::try_from(height).unwrap();
            for seed in [0_usize, 0x55, 0xaa, 0xff] {
                let mut bytes: Vec<_> = (0..length)
                    .map(|index| u8::try_from((index * 73 + seed) % 256).unwrap())
                    .collect();
                if width % 8 != 0 {
                    for row in bytes.chunks_exact_mut(stride) {
                        row[stride - 1] &= 0xff << (8 - width % 8);
                    }
                }
                let expected = bytes.clone();
                bytes.extend_from_slice(&[0xff, 0x93]);
                let input = bytes.clone();
                let bitmap = bitmap_from_packed_bytes(width, height, &bytes, &limits)
                    .expect("packed storage");
                assert_eq!(
                    (bitmap.width, bitmap.height, bitmap.stride),
                    (width, height, stride)
                );
                assert_eq!(bitmap.data, expected);
                assert_eq!(bytes, input);
                for y in 0..height {
                    for x in 0..width {
                        let index =
                            usize::try_from(y).unwrap() * stride + usize::try_from(x / 8).unwrap();
                        assert_eq!(
                            bitmap.pixel(x, y),
                            Some(expected[index] & (0x80 >> (x % 8)) != 0),
                        );
                    }
                }
                for cut in 0..length {
                    assert!(matches!(
                        bitmap_from_packed_bytes(width, height, &bytes[..cut], &limits),
                        Err(SymbolTextError::Decode(DecodeError::Malformed(
                            MalformedError::UnexpectedEof {
                                offset: 0, needed, remaining,
                            }
                        ))) if needed == length as u64 && remaining == cut as u64
                    ));
                }
                if width % 8 != 0 {
                    for y in 0..usize::try_from(height).unwrap() {
                        let mut dirty = expected.clone();
                        dirty[(y + 1) * stride - 1] |= 1;
                        assert!(matches!(
                            bitmap_from_packed_bytes(width, height, &dirty, &limits),
                            Err(SymbolTextError::InvalidState(
                                "raw collective bitmap has nonzero padding"
                            ))
                        ));
                    }
                }
            }
        }
    }
}

#[test]
fn packed_symbol_limits_are_inclusive_and_precede_payload_checks() {
    let inclusive = Limits {
        max_width: 9,
        max_height: 3,
        max_pixels_per_page: 27,
        max_decoder_managed_memory_bytes: 6,
        ..Limits::default()
    };
    let bytes = [0xff, 0x80, 0x33, 0, 0, 0];
    assert_eq!(
        bitmap_from_packed_bytes(9, 3, &bytes, &inclusive)
            .expect("inclusive limits")
            .data,
        bytes,
    );
    let failures = [
        (LimitKind::Width, 8, 9),
        (LimitKind::Height, 2, 3),
        (LimitKind::PagePixels, 26, 27),
        (LimitKind::Memory, 5, 6),
    ];
    for rejected in 1_u8..16 {
        let limits = Limits {
            max_width: 9 - u32::from(rejected & 1 != 0),
            max_height: 3 - u32::from(rejected & 2 != 0),
            max_pixels_per_page: 27 - u64::from(rejected & 4 != 0),
            max_decoder_managed_memory_bytes: 6 - u64::from(rejected & 8 != 0),
            ..inclusive
        };
        let expected = failures
            .iter()
            .enumerate()
            .find(|(index, _)| rejected & (1 << index) != 0)
            .unwrap()
            .1;
        assert!(matches!(
            bitmap_from_packed_bytes(9, 3, &[], &limits),
            Err(SymbolTextError::Decode(DecodeError::LimitExceeded(
                LimitExceeded { kind, limit, actual }
            ))) if (kind, limit, actual) == *expected
        ));
    }
    let limits = Limits {
        max_width: u32::MAX,
        max_height: u32::MAX,
        max_pixels_per_page: u64::MAX,
        max_decoder_managed_memory_bytes: 0,
        ..Limits::default()
    };
    let needed = u64::from(u32::MAX).div_ceil(8) * u64::from(u32::MAX);
    assert!(matches!(
        bitmap_from_packed_bytes(u32::MAX, u32::MAX, &[], &limits),
        Err(SymbolTextError::Decode(DecodeError::LimitExceeded(
            LimitExceeded { kind: LimitKind::Memory, limit: 0, actual }
        ))) if actual == needed
    ));
}

#[test]
#[allow(clippy::too_many_lines)]
fn export_run_errors_preserve_classification_work_and_unread_input() {
    let cases = [
        (
            vec![EntropyInteger::Value(1)],
            1,
            2,
            10,
            0,
            1,
            SymbolTextError::InvalidCount("export count exceeds candidate count"),
        ),
        (
            vec![EntropyInteger::Value(-1)],
            3,
            0,
            10,
            1,
            0,
            SymbolTextError::InvalidEntropyValue("negative export run"),
        ),
        (
            vec![EntropyInteger::Value(i64::from(u32::MAX) + 1)],
            3,
            0,
            10,
            1,
            0,
            SymbolTextError::InvalidEntropyValue("negative export run"),
        ),
        (
            vec![EntropyInteger::OutOfBand],
            3,
            0,
            10,
            1,
            0,
            SymbolTextError::UnexpectedOutOfBand("export run"),
        ),
        (
            vec![EntropyInteger::Value(4)],
            3,
            0,
            10,
            1,
            0,
            SymbolTextError::InvalidCount("export run exceeds candidate count"),
        ),
        (
            vec![EntropyInteger::Value(0), EntropyInteger::Value(2)],
            3,
            1,
            10,
            2,
            0,
            SymbolTextError::InvalidCount("too many exported symbols"),
        ),
        (
            vec![EntropyInteger::Value(3)],
            3,
            1,
            10,
            1,
            0,
            SymbolTextError::InvalidCount("exported-symbol popcount mismatch"),
        ),
        (
            vec![
                EntropyInteger::Value(1),
                EntropyInteger::Value(i64::from(u32::MAX)),
            ],
            u32::MAX,
            0,
            10,
            2,
            0,
            SymbolTextError::Decode(DecodeError::Malformed(MalformedError::IntegerOverflow)),
        ),
        (
            vec![],
            1,
            0,
            10,
            1,
            0,
            SymbolTextError::InvalidState("missing scripted integer"),
        ),
        (
            vec![EntropyInteger::Value(1)],
            1,
            0,
            0,
            1,
            1,
            SymbolTextError::Decode(DecodeError::LimitExceeded(LimitExceeded {
                kind: LimitKind::Work,
                limit: 0,
                actual: 1,
            })),
        ),
    ];
    for (integers, candidates, expected_exports, max_work_units, work, unread, expected) in cases {
        let mut codec = SymbolCodec::Scripted(ScriptedCodec {
            integers: integers.into(),
            symbol_id: 0,
        });
        let mut resources = ResourceTracker {
            work_units: 0,
            decoded_bitmap_bytes: 17,
        };
        let limits = Limits {
            max_work_units,
            ..Limits::default()
        };
        let error = decode_export_runs(
            &mut codec,
            candidates,
            expected_exports,
            &limits,
            &mut resources,
        )
        .expect_err("invalid export run");
        match (error, expected) {
            (SymbolTextError::InvalidCount(actual), SymbolTextError::InvalidCount(expected))
            | (
                SymbolTextError::InvalidEntropyValue(actual),
                SymbolTextError::InvalidEntropyValue(expected),
            )
            | (
                SymbolTextError::UnexpectedOutOfBand(actual),
                SymbolTextError::UnexpectedOutOfBand(expected),
            )
            | (SymbolTextError::InvalidState(actual), SymbolTextError::InvalidState(expected)) => {
                assert_eq!(actual, expected);
            }
            (
                SymbolTextError::Decode(DecodeError::Malformed(actual)),
                SymbolTextError::Decode(DecodeError::Malformed(expected)),
            ) => assert_eq!(actual, expected),
            (
                SymbolTextError::Decode(DecodeError::LimitExceeded(actual)),
                SymbolTextError::Decode(DecodeError::LimitExceeded(expected)),
            ) => assert_eq!(actual, expected),
            (actual, expected) => {
                panic!("wrong export error: {actual:?}, expected {expected:?}")
            }
        }
        assert_eq!(
            resources,
            ResourceTracker {
                work_units: work,
                decoded_bitmap_bytes: 17
            }
        );
        let SymbolCodec::Scripted(scripted) = codec else {
            panic!("scripted codec")
        };
        assert_eq!(scripted.integers.len(), unread);
    }
}

#[test]
fn export_runs_match_every_small_selection_with_exact_work_and_input_frames() {
    for count in 0_u32..=9 {
        for selected in 0_u32..(1 << count) {
            let expected: Vec<_> = (0..count)
                .filter(|index| selected & (1 << index) != 0)
                .collect();
            let mut integers = VecDeque::new();
            let mut flag = false;
            let mut run = 0;
            for index in 0..count {
                let next = selected & (1 << index) != 0;
                if next != flag {
                    integers.push_back(EntropyInteger::Value(run));
                    flag = next;
                    run = 0;
                }
                run += 1;
            }
            if count != 0 {
                integers.push_back(EntropyInteger::Value(run));
            }
            let work = u64::try_from(integers.len()).unwrap() + 5;
            integers.push_back(EntropyInteger::Value(-73));
            let mut codec = SymbolCodec::Scripted(ScriptedCodec {
                integers,
                symbol_id: 0,
            });
            let mut resources = ResourceTracker {
                work_units: 5,
                decoded_bitmap_bytes: 17,
            };
            let limits = Limits {
                max_work_units: work,
                ..Limits::default()
            };
            let result = decode_export_runs(
                &mut codec,
                count,
                selected.count_ones(),
                &limits,
                &mut resources,
            )
            .expect("scalar selection");
            assert_eq!(result, expected);
            assert_eq!(
                resources,
                ResourceTracker {
                    work_units: work,
                    decoded_bitmap_bytes: 17
                }
            );
            let SymbolCodec::Scripted(scripted) = codec else {
                panic!("scripted codec")
            };
            assert_eq!(
                scripted.integers,
                VecDeque::from([EntropyInteger::Value(-73)])
            );
        }
    }
}

#[test]
fn lossless_byte_and_boolean_bridges_match_standard_conversions() {
    for value in 0..=u8::MAX {
        assert_eq!(widen_u8_i64(value), i64::from(value));
        assert_eq!(widen_u8_i32(value), i32::from(value));
    }
    for value in [false, true] {
        assert_eq!(widen_bool_i64(value), i64::from(value));
    }
}

#[test]
fn full_u32_widening_matches_standard_conversion() {
    for value in [
        0,
        1,
        255,
        256,
        65_535,
        65_536,
        0x7fff_ffff,
        1_u32 << 31,
        u32::MAX - 1,
        u32::MAX,
    ] {
        assert_eq!(widen_u32_i64(value), i64::from(value));
    }
    let mut value = 0x1020_3040_u32;
    for _ in 0..4096 {
        value = value.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        assert_eq!(widen_u32_i64(value), i64::from(value));
    }
}

fn scalar_compose_placement_oracle(
    destination: &mut Bitmap,
    source: &Bitmap,
    placement: Placement,
    operator: CombinationOperator,
    limits: &Limits,
    resources: &mut ResourceTracker,
) -> Result<(), SymbolTextError> {
    let source_x = if placement.x < 0 {
        negative_magnitude(placement.x)?
    } else {
        0
    };
    let source_y = if placement.y < 0 {
        negative_magnitude(placement.y)?
    } else {
        0
    };
    let destination_x = placement.x.max(0);
    let destination_y = placement.y.max(0);
    let overlap_width = (i64::from(source.width()) - source_x)
        .min(i64::from(destination.width()) - destination_x)
        .max(0);
    let overlap_height = (i64::from(source.height()) - source_y)
        .min(i64::from(destination.height()) - destination_y)
        .max(0);
    resources.charge_work(
        u64::try_from(overlap_width)
            .map_err(|_| MalformedError::IntegerOverflow)?
            .checked_mul(
                u64::try_from(overlap_height).map_err(|_| MalformedError::IntegerOverflow)?,
            )
            .ok_or(MalformedError::IntegerOverflow)?,
        limits,
    )?;
    for row in 0..overlap_height {
        for column in 0..overlap_width {
            let sx =
                u32::try_from(source_x + column).map_err(|_| MalformedError::IntegerOverflow)?;
            let sy = u32::try_from(source_y + row).map_err(|_| MalformedError::IntegerOverflow)?;
            let dx = u32::try_from(destination_x + column)
                .map_err(|_| MalformedError::IntegerOverflow)?;
            let dy =
                u32::try_from(destination_y + row).map_err(|_| MalformedError::IntegerOverflow)?;
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

fn batched_compose_placement(
    destination: &mut Bitmap,
    placements: &[(&Bitmap, Placement)],
    operator: CombinationOperator,
    limits: &Limits,
    resources: &mut ResourceTracker,
) -> Result<(), SymbolTextError> {
    let mut batch = PlacementBatch::new();
    batch.reserve(placements.len(), limits)?;
    for &(source, placement) in placements {
        if let Some(prepared) =
            prepare_placement(destination, source, placement, limits, resources)?
        {
            if !batch.compatible(prepared) {
                flush_placement_batch(destination, &mut batch, operator)?;
            }
            batch.push(prepared, destination)?;
        }
    }
    flush_placement_batch(destination, &mut batch, operator)
}

fn median_duration(mut values: Vec<Duration>) -> Duration {
    values.sort_unstable();
    values[values.len() / 2]
}

fn text_config(corner: ReferenceCorner, transposed: bool, strip_size: u8) -> TextRegionConfig {
    TextRegionConfig {
        huffman: false,
        refinement: false,
        strip_size,
        reference_corner: corner,
        transposed,
        operator: CombinationOperator::Or,
        default_pixel: false,
        s_offset: 0,
        refinement_template: 0,
        huffman_choices: None,
    }
}

fn assert_resource_guard_result(
    actual: Result<(), SymbolTextError>,
    expected: Result<(), Option<LimitExceeded>>,
) {
    match actual {
        Ok(()) => assert_eq!(expected, Ok(())),
        Err(SymbolTextError::Decode(DecodeError::Malformed(MalformedError::IntegerOverflow))) => {
            assert_eq!(expected, Err(None));
        }
        Err(SymbolTextError::Decode(DecodeError::LimitExceeded(error))) => {
            assert_eq!(expected, Err(Some(error)));
        }
        other => panic!("unexpected resource guard result: {other:?}"),
    }
}

#[test]
fn work_charges_match_wide_results_and_attempted_charge_failure_state() {
    let values = [0, 1, 2, u64::MAX / 2, u64::MAX - 1, u64::MAX];
    let mut cases = 0;
    for initial in values {
        for units in values {
            for limit in values {
                let mut tracker = ResourceTracker {
                    work_units: initial,
                    decoded_bitmap_bytes: 37,
                };
                let limits = Limits {
                    max_work_units: limit,
                    ..Limits::default()
                };
                let total = u128::from(initial) + u128::from(units);
                let expected = if total > u128::from(u64::MAX) {
                    Err(None)
                } else if total > u128::from(limit) {
                    Err(Some(LimitExceeded {
                        kind: LimitKind::Work,
                        limit,
                        actual: u64::try_from(total).unwrap(),
                    }))
                } else {
                    Ok(())
                };
                assert_resource_guard_result(tracker.charge_work(units, &limits), expected);
                let charged = if total <= u128::from(u64::MAX) {
                    u64::try_from(total).unwrap()
                } else {
                    initial
                };
                assert_eq!(tracker.work_units, charged);
                assert_eq!(tracker.decoded_bitmap_bytes, 37);
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 216);
}

#[test]
#[allow(clippy::too_many_lines)]
fn bitmap_charges_match_wide_priority_and_attempted_charge_failure_state() {
    let mut cases = 0;
    for width in [0_u32, 1, 8, 9, u32::MAX] {
        for height in [0_u32, 1, 2, u32::MAX] {
            let pixels = u128::from(width) * u128::from(height);
            let bytes = u128::from(width).div_ceil(8) * u128::from(height);
            for initial in [0, 1, u64::MAX / 2, u64::MAX] {
                let total = u128::from(initial) + bytes;
                let boundary = u64::try_from(total.min(u128::from(u64::MAX))).unwrap();
                for max_width in [width.saturating_sub(1), width, u32::MAX] {
                    for max_height in [height.saturating_sub(1), height, u32::MAX] {
                        let pixels = u64::try_from(pixels).unwrap();
                        for max_pixels in [pixels.saturating_sub(1), pixels, u64::MAX] {
                            for limit in [0, 1, boundary, u64::MAX] {
                                let limits = Limits {
                                    max_width,
                                    max_height,
                                    max_pixels_per_page: max_pixels,
                                    max_cumulative_decoded_bitmap_bytes: limit,
                                    ..Limits::default()
                                };
                                let dimension_error = if width > max_width {
                                    Some(LimitExceeded {
                                        kind: LimitKind::Width,
                                        limit: u64::from(max_width),
                                        actual: u64::from(width),
                                    })
                                } else if height > max_height {
                                    Some(LimitExceeded {
                                        kind: LimitKind::Height,
                                        limit: u64::from(max_height),
                                        actual: u64::from(height),
                                    })
                                } else if pixels > max_pixels {
                                    Some(LimitExceeded {
                                        kind: LimitKind::PagePixels,
                                        limit: max_pixels,
                                        actual: pixels,
                                    })
                                } else {
                                    None
                                };
                                let expected = if let Some(error) = dimension_error {
                                    Err(Some(error))
                                } else if total > u128::from(u64::MAX) {
                                    Err(None)
                                } else if total > u128::from(limit) {
                                    Err(Some(LimitExceeded {
                                        kind: LimitKind::DecodedBitmapBytes,
                                        limit,
                                        actual: u64::try_from(total).unwrap(),
                                    }))
                                } else {
                                    Ok(())
                                };
                                let mut tracker = ResourceTracker {
                                    work_units: 37,
                                    decoded_bitmap_bytes: initial,
                                };
                                assert_resource_guard_result(
                                    tracker.charge_bitmap(width, height, &limits),
                                    expected,
                                );
                                let charged =
                                    if dimension_error.is_none() && total <= u128::from(u64::MAX) {
                                        u64::try_from(total).unwrap()
                                    } else {
                                        initial
                                    };
                                assert_eq!(tracker.work_units, 37);
                                assert_eq!(tracker.decoded_bitmap_bytes, charged);
                                cases += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 8_640);
}

fn wide_placement_geometry(
    destination: &Bitmap,
    source: &Bitmap,
    placement: Placement,
) -> Option<(u32, u32, u32, u32, u32, u32)> {
    let x = i128::from(placement.x);
    let y = i128::from(placement.y);
    let left = x.max(0);
    let top = y.max(0);
    let right = (x + i128::from(source.width())).min(i128::from(destination.width()));
    let bottom = (y + i128::from(source.height())).min(i128::from(destination.height()));
    if right <= left || bottom <= top {
        None
    } else {
        Some((
            u32::try_from(left - x).unwrap(),
            u32::try_from(top - y).unwrap(),
            u32::try_from(left).unwrap(),
            u32::try_from(top).unwrap(),
            u32::try_from(right - left).unwrap(),
            u32::try_from(bottom - top).unwrap(),
        ))
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn placement_preflight_matches_wide_geometry_and_resource_failure_frames() {
    let limits = Limits::default();
    let sources = [
        Bitmap::blank(0, 2, &limits).unwrap(),
        Bitmap::filled(1, 1, true, &limits).unwrap(),
        Bitmap::filled(9, 3, true, &limits).unwrap(),
        Bitmap::filled(17, 3, true, &limits).unwrap(),
    ];
    let mut cases = 0;
    for source in &sources {
        let source_before = source.bytes().to_vec();
        for width in [0, 1, 8, 19] {
            for height in [0, 1, 4] {
                let destination = Bitmap::filled(width, height, true, &limits).unwrap();
                let before = destination.bytes().to_vec();
                for x in [
                    i64::MIN,
                    i64::MIN + 1,
                    -32,
                    -17,
                    -9,
                    -1,
                    0,
                    1,
                    8,
                    18,
                    19,
                    32,
                    i64::MAX,
                ] {
                    for y in [i64::MIN, i64::MIN + 1, -4, -1, 0, 1, 3, 4, i64::MAX] {
                        let placement = Placement { x, y };
                        let geometry = wide_placement_geometry(&destination, source, placement);
                        let units = geometry.map_or(0, |g| u64::from(g.4) * u64::from(g.5));
                        for (initial, limit) in [(0, 0), (1, 1), (u64::MAX, u64::MAX), (7, 10_000)]
                        {
                            let limited = Limits {
                                max_work_units: limit,
                                ..limits
                            };
                            let mut resources = ResourceTracker {
                                work_units: initial,
                                decoded_bitmap_bytes: 37,
                            };
                            let total = u128::from(initial) + u128::from(units);
                            let coordinate_error = x == i64::MIN || y == i64::MIN;
                            let expected = if coordinate_error || total > u128::from(u64::MAX) {
                                Err(None)
                            } else if total > u128::from(limit) {
                                Err(Some(LimitExceeded {
                                    kind: LimitKind::Work,
                                    limit,
                                    actual: u64::try_from(total).unwrap(),
                                }))
                            } else {
                                Ok(())
                            };
                            let actual = prepare_placement(
                                &destination,
                                source,
                                placement,
                                &limited,
                                &mut resources,
                            );
                            if expected.is_ok() {
                                match (actual.as_ref().unwrap(), geometry) {
                                    (None, None) => {}
                                    (Some(prepared), Some((sx, sy, dx, dy, w, h))) => {
                                        assert!(std::ptr::eq(prepared.blit.source, source));
                                        assert_eq!(prepared.blit.source_x, sx);
                                        assert_eq!(prepared.source_y, sy);
                                        assert_eq!(prepared.blit.destination_x, dx);
                                        assert_eq!(prepared.destination_y, dy);
                                        assert_eq!(prepared.blit.width, w);
                                        assert_eq!(prepared.height, h);
                                    }
                                    other => panic!("wrong clipping shape: {other:?}"),
                                }
                            }
                            assert_resource_guard_result(actual.map(|_| ()), expected);
                            let charged = if coordinate_error || total > u128::from(u64::MAX) {
                                initial
                            } else {
                                u64::try_from(total).unwrap()
                            };
                            assert_eq!(resources.work_units, charged);
                            assert_eq!(resources.decoded_bitmap_bytes, 37);
                            assert_eq!(source.bytes(), source_before);
                            assert_eq!(destination.bytes(), before);
                            cases += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 22_464);
}

#[test]
fn placement_batch_reservation_clamps_counts_and_preserves_pending_state() {
    for count in [0, 1, 15, 16, 17, usize::MAX] {
        let requested = u64::try_from(count.min(16)).unwrap()
            * u64::try_from(core::mem::size_of::<RowBlit<'_>>()).unwrap();
        for limit in [requested.saturating_sub(1), requested, requested + 1] {
            let limits = Limits {
                max_decoder_managed_memory_bytes: limit,
                ..Limits::default()
            };
            let mut batch = PlacementBatch::new();
            batch.source_y = 3;
            batch.destination_y = 5;
            batch.height = 7;
            batch.source_shift = 2;
            batch.destination_shift = 6;
            let expected = if requested > limit {
                Err(Some(LimitExceeded {
                    kind: LimitKind::Memory,
                    limit,
                    actual: requested,
                }))
            } else {
                Ok(())
            };
            assert_resource_guard_result(batch.reserve(count, &limits), expected);
            assert!(batch.blits.is_empty());
            assert_eq!(
                (
                    batch.source_y,
                    batch.destination_y,
                    batch.height,
                    batch.source_shift,
                    batch.destination_shift
                ),
                (3, 5, 7, 2, 6),
            );
        }
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn placement_batch_compatibility_uses_group_geometry_modulo_alignment_and_cap() {
    let limits = Limits::default();
    let source = Bitmap::filled(80, 6, true, &limits).unwrap();
    let destination = Bitmap::blank(96, 8, &limits).unwrap();
    let base = PreparedPlacement {
        blit: RowBlit {
            source: &source,
            source_x: 3,
            destination_x: 11,
            width: 13,
        },
        source_y: 1,
        destination_y: 2,
        height: 3,
    };
    let mut cases = 0;
    for count in 0..=16 {
        for variant in 0..9 {
            let mut batch = PlacementBatch::new();
            for _ in 0..count {
                batch.push(base, &destination).unwrap();
            }
            let mut prepared = base;
            match variant {
                0 => {}
                1 => prepared.blit.source_x += 8,
                2 => prepared.blit.destination_x += 8,
                3 => prepared.blit.width = 1,
                4 => prepared.source_y += 1,
                5 => prepared.destination_y += 1,
                6 => prepared.height += 1,
                7 => prepared.blit.source_x += 1,
                8 => prepared.blit.destination_x += 1,
                _ => unreachable!(),
            }
            let expected = count == 0 || (count < 16 && variant < 4);
            let before = batch.blits.clone();
            let metadata = (
                batch.source_y,
                batch.destination_y,
                batch.height,
                batch.source_shift,
                batch.destination_shift,
            );
            assert_eq!(batch.compatible(prepared), expected);
            let result = batch.push(prepared, &destination);
            if expected {
                result.unwrap();
                assert_eq!(batch.blits.len(), count + 1);
                let last = batch.blits.last().unwrap();
                assert!(std::ptr::eq(last.source, std::ptr::from_ref(&source)));
                assert_eq!(
                    (last.source_x, last.destination_x, last.width),
                    (
                        prepared.blit.source_x,
                        prepared.blit.destination_x,
                        prepared.blit.width
                    )
                );
            } else {
                assert!(matches!(
                    result,
                    Err(SymbolTextError::InvalidState(
                        "incompatible text placement batch"
                    ))
                ));
                assert_eq!(batch.blits.len(), count);
            }
            for (actual, old) in batch.blits.iter().zip(&before) {
                assert!(std::ptr::eq(actual.source, old.source));
                assert_eq!(
                    (actual.source_x, actual.destination_x, actual.width),
                    (old.source_x, old.destination_x, old.width)
                );
            }
            if count != 0 || !expected {
                assert_eq!(
                    (
                        batch.source_y,
                        batch.destination_y,
                        batch.height,
                        batch.source_shift,
                        batch.destination_shift
                    ),
                    metadata,
                );
            } else {
                assert_eq!(
                    (
                        batch.source_y,
                        batch.destination_y,
                        batch.height,
                        batch.source_shift,
                        batch.destination_shift
                    ),
                    (
                        prepared.source_y,
                        prepared.destination_y,
                        prepared.height,
                        prepared.blit.source_x % 8,
                        prepared.blit.destination_x % 8
                    ),
                );
            }
            cases += 1;
        }
    }
    assert_eq!(cases, 153);
}

#[test]
#[allow(clippy::too_many_lines)]
fn placement_batch_flush_matches_independent_bit_truths_and_preserves_sources() {
    let limits = Limits::default();
    let mut sources = [
        Bitmap::blank(89, 5, &limits).unwrap(),
        Bitmap::blank(89, 5, &limits).unwrap(),
    ];
    for (salt, source) in sources.iter_mut().enumerate() {
        for y in 0..5 {
            for x in 0..89 {
                source
                    .set_pixel(
                        x,
                        y,
                        (x * 17 + y * 11 + u32::try_from(salt).unwrap()) % 7 < 3,
                    )
                    .unwrap();
            }
        }
    }
    let source_before: Vec<_> = sources.iter().map(|s| s.bytes().to_vec()).collect();
    let original = Bitmap::filled(137, 6, true, &limits).unwrap();
    let truths = [
        (CombinationOperator::Or, [false, true, true, true]),
        (CombinationOperator::And, [false, false, false, true]),
        (CombinationOperator::Xor, [false, true, true, false]),
        (CombinationOperator::Xnor, [true, false, false, true]),
        (CombinationOperator::Replace, [false, true, false, true]),
    ];
    let mut cases = 0;
    for count in [0, 1, 2, 16] {
        for width in [0, 1, 8, 9, 63, 64, 65] {
            for (operator, table) in truths {
                let mut batch = PlacementBatch::new();
                batch.reserve(count, &limits).unwrap();
                let mut expected = original.bytes().to_vec();
                for index in 0..count {
                    let source = &sources[index % 2];
                    let sx = u32::try_from(index % 2).unwrap() * 8 + 3;
                    let dx = u32::try_from(index % 7).unwrap() * 8 + 3;
                    batch
                        .push(
                            PreparedPlacement {
                                blit: RowBlit {
                                    source,
                                    source_x: sx,
                                    destination_x: dx,
                                    width,
                                },
                                source_y: 1,
                                destination_y: 2,
                                height: 3,
                            },
                            &original,
                        )
                        .unwrap();
                    for row in 0..3 {
                        for column in 0..width {
                            let source_x = sx + column;
                            let destination_x = dx + column;
                            let si = usize::try_from(row + 1).unwrap() * source.stride()
                                + usize::try_from(source_x / 8).unwrap();
                            let di = usize::try_from(row + 2).unwrap() * original.stride()
                                + usize::try_from(destination_x / 8).unwrap();
                            let incoming = source.bytes()[si] & (0x80 >> (source_x % 8)) != 0;
                            let mask = 0x80 >> (destination_x % 8);
                            let current = expected[di] & mask != 0;
                            if table[usize::from(current) * 2 + usize::from(incoming)] {
                                expected[di] |= mask;
                            } else {
                                expected[di] &= !mask;
                            }
                        }
                    }
                }
                let metadata = (
                    batch.source_y,
                    batch.destination_y,
                    batch.height,
                    batch.source_shift,
                    batch.destination_shift,
                );
                let mut actual = original.clone();
                flush_placement_batch(&mut actual, &mut batch, operator).unwrap();
                assert_eq!(actual.bytes(), expected);
                assert_eq!(
                    (actual.width(), actual.height(), actual.stride()),
                    (137, 6, 18)
                );
                assert!(batch.blits.is_empty());
                assert_eq!(
                    (
                        batch.source_y,
                        batch.destination_y,
                        batch.height,
                        batch.source_shift,
                        batch.destination_shift
                    ),
                    metadata,
                );
                flush_placement_batch(&mut actual, &mut batch, operator).unwrap();
                assert_eq!(actual.bytes(), expected);
                for (source, before) in sources.iter().zip(&source_before) {
                    assert_eq!(source.bytes(), before);
                }
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 140);
}

#[test]
fn text_state_initialization_matches_wide_checked_scaling_and_work_priority() {
    let values = [
        i64::MIN,
        i64::MIN + 1,
        -17,
        -1,
        0,
        1,
        17,
        i64::MAX / 2,
        i64::MAX,
    ];
    let mut cases = 0;
    for strip_size in [1, 2, 4, 8] {
        let config = text_config(ReferenceCorner::TopLeft, false, strip_size);
        for initial in values {
            for instances in [0, 1, u32::MAX] {
                for limit in [0, 1, u64::MAX] {
                    let limits = Limits {
                        max_work_units: limit,
                        ..Limits::default()
                    };
                    let scaled = i128::from(initial) * i128::from(strip_size);
                    let fits = i64::try_from(scaled).is_ok() && i64::try_from(-scaled).is_ok();
                    let result = TextRegionState::new(config, instances, initial, &limits);
                    let expected = if u64::from(instances) > limit {
                        Err(Some(LimitExceeded {
                            kind: LimitKind::Work,
                            limit,
                            actual: u64::from(instances),
                        }))
                    } else if !fits {
                        Err(None)
                    } else {
                        Ok(())
                    };
                    if expected.is_ok() {
                        let state = result.as_ref().unwrap();
                        assert_eq!(state.config, config);
                        assert_eq!(state.declared_instances, instances);
                        assert_eq!(state.decoded_instances, 0);
                        assert_eq!(i128::from(state.strip_t), -scaled);
                        assert_eq!(state.first_s, 0);
                        assert!(state.strip.is_none());
                    }
                    assert_resource_guard_result(result.map(|_| ()), expected);
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 324);
}

#[test]
fn begin_strip_preserves_exact_partial_updates_on_checked_arithmetic_errors() {
    let values = [i64::MIN, -1, 0, 1, i64::MAX];
    let mut cases = 0;
    for strip_size in [1, 2, 4, 8] {
        for old_t in values {
            for old_s in values {
                for dt in values {
                    for ds in values {
                        let mut state = TextRegionState {
                            config: text_config(ReferenceCorner::TopLeft, false, strip_size),
                            declared_instances: 2,
                            decoded_instances: 0,
                            strip_t: old_t,
                            first_s: old_s,
                            strip: None,
                        };
                        let scaled = i128::from(dt) * i128::from(strip_size);
                        let next_t = i128::from(old_t) + scaled;
                        let next_s = i128::from(old_s) + i128::from(ds);
                        let t_written =
                            i64::try_from(scaled).is_ok() && i64::try_from(next_t).is_ok();
                        let accepted = t_written && i64::try_from(next_s).is_ok();
                        let result = state.begin_strip(dt, ds);
                        assert_resource_guard_result(
                            result,
                            if accepted { Ok(()) } else { Err(None) },
                        );
                        assert_eq!(
                            state.strip_t,
                            if t_written {
                                i64::try_from(next_t).unwrap()
                            } else {
                                old_t
                            },
                        );
                        assert_eq!(
                            state.first_s,
                            if accepted {
                                i64::try_from(next_s).unwrap()
                            } else {
                                old_s
                            },
                        );
                        assert_eq!(state.decoded_instances, 0);
                        if accepted {
                            let strip = state.strip.unwrap();
                            assert_eq!(i128::from(strip.cursor_s), next_s);
                            assert!(!strip.has_instance);
                        } else {
                            assert!(state.strip.is_none());
                        }
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases, 2_500);
}

#[test]
#[allow(clippy::too_many_lines)]
fn current_text_placement_matches_wide_coordinates_and_partial_failure_state() {
    let values = [i64::MIN, i64::MIN + 1, -1, 0, 1, i64::MAX - 1, i64::MAX];
    let corners = [
        (ReferenceCorner::TopLeft, false, false),
        (ReferenceCorner::TopRight, true, false),
        (ReferenceCorner::BottomLeft, false, true),
        (ReferenceCorner::BottomRight, true, true),
    ];
    let mut cases = 0;
    for transposed in [false, true] {
        for (corner, right, bottom) in corners {
            for strip_size in [1, 2, 4, 8] {
                for (width, height) in [(0, 1), (1, 0), (1, 1), (2, 3), (u32::MAX, u32::MAX)] {
                    for cursor in values {
                        for strip_t in values {
                            for within in [0, strip_size - 1, strip_size, u8::MAX] {
                                for initial in [0, u64::MAX] {
                                    for limit in [0, u64::MAX] {
                                        let config = text_config(corner, transposed, strip_size);
                                        let mut state = TextRegionState {
                                            config,
                                            declared_instances: 1,
                                            decoded_instances: 0,
                                            strip_t,
                                            first_s: cursor,
                                            strip: Some(StripState {
                                                cursor_s: cursor,
                                                has_instance: false,
                                            }),
                                        };
                                        let mut resources = ResourceTracker {
                                            work_units: initial,
                                            decoded_bitmap_bytes: 37,
                                        };
                                        let limits = Limits {
                                            max_work_units: limit,
                                            ..Limits::default()
                                        };
                                        let dimensions_valid = width != 0 && height != 0;
                                        let within_valid = within < strip_size;
                                        let total = u128::from(initial) + 1;
                                        let resource_valid = total <= u128::from(u64::MAX)
                                            && total <= u128::from(limit);
                                        let extent =
                                            i128::from(if transposed { height } else { width }) - 1;
                                        let next_s = i128::from(cursor) + extent;
                                        let next_t = i128::from(strip_t) + i128::from(within);
                                        let axis = next_t
                                            - if transposed {
                                                if right {
                                                    i128::from(width) - 1
                                                } else {
                                                    0
                                                }
                                            } else if bottom {
                                                i128::from(height) - 1
                                            } else {
                                                0
                                            };
                                        let coordinates_valid = i64::try_from(next_s).is_ok()
                                            && i64::try_from(next_t).is_ok()
                                            && i64::try_from(axis).is_ok();
                                        let accepted = dimensions_valid
                                            && within_valid
                                            && resource_valid
                                            && coordinates_valid;
                                        let result = state.place_current(
                                            within,
                                            width,
                                            height,
                                            &limits,
                                            &mut resources,
                                        );
                                        if !dimensions_valid {
                                            assert!(matches!(
                                                result,
                                                Err(SymbolTextError::InvalidDimensions)
                                            ));
                                        } else if !within_valid {
                                            assert!(matches!(
                                                result,
                                                Err(SymbolTextError::InvalidEntropyValue(
                                                    "within-strip T coordinate"
                                                ))
                                            ));
                                        } else if !resource_valid {
                                            let expected = if total > u128::from(u64::MAX) {
                                                None
                                            } else {
                                                Some(LimitExceeded {
                                                    kind: LimitKind::Work,
                                                    limit,
                                                    actual: u64::try_from(total).unwrap(),
                                                })
                                            };
                                            assert_resource_guard_result(
                                                result.map(|_| ()),
                                                Err(expected),
                                            );
                                        } else if !coordinates_valid {
                                            assert_resource_guard_result(
                                                result.map(|_| ()),
                                                Err(None),
                                            );
                                        } else {
                                            let placement = result.unwrap();
                                            let expected = if transposed {
                                                (axis, i128::from(cursor))
                                            } else {
                                                (i128::from(cursor), axis)
                                            };
                                            assert_eq!(
                                                (i128::from(placement.x), i128::from(placement.y)),
                                                expected
                                            );
                                        }
                                        let charged = if dimensions_valid
                                            && within_valid
                                            && total <= u128::from(u64::MAX)
                                        {
                                            u64::try_from(total).unwrap()
                                        } else {
                                            initial
                                        };
                                        assert_eq!(resources.work_units, charged);
                                        assert_eq!(resources.decoded_bitmap_bytes, 37);
                                        let advanced_before =
                                            (!transposed && right) || (transposed && bottom);
                                        let arithmetic_reached =
                                            dimensions_valid && within_valid && resource_valid;
                                        let cursor_written = accepted
                                            || (arithmetic_reached
                                                && advanced_before
                                                && i64::try_from(next_s).is_ok());
                                        let strip = state.strip.unwrap();
                                        assert_eq!(
                                            strip.cursor_s,
                                            if cursor_written {
                                                i64::try_from(next_s).unwrap()
                                            } else {
                                                cursor
                                            }
                                        );
                                        assert_eq!(strip.has_instance, accepted);
                                        assert_eq!(state.decoded_instances, u32::from(accepted));
                                        assert_eq!(state.declared_instances, 1);
                                        assert_eq!(state.strip_t, strip_t);
                                        assert_eq!(state.first_s, cursor);
                                        assert_eq!(state.config, config);
                                        cases += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 125_440);
}

fn text_state_signature(
    state: &TextRegionState,
) -> (TextRegionConfig, u32, u32, i64, i64, Option<(i64, bool)>) {
    (
        state.config,
        state.declared_instances,
        state.decoded_instances,
        state.strip_t,
        state.first_s,
        state
            .strip
            .map(|strip| (strip.cursor_s, strip.has_instance)),
    )
}

#[test]
fn text_strip_order_guards_preserve_their_exact_state_and_charge_frames() {
    let limits = Limits::default();
    let config = text_config(ReferenceCorner::TopLeft, false, 2);
    let mut state = TextRegionState::new(config, 2, 0, &limits).unwrap();
    let mut resources = ResourceTracker::default();
    let before = text_state_signature(&state);
    for subsequent in [false, true] {
        let result = if subsequent {
            state.place_subsequent(0, 0, 1, 1, &limits, &mut resources)
        } else {
            state.place_first(0, 1, 1, &limits, &mut resources)
        };
        assert!(matches!(
            result,
            Err(SymbolTextError::InvalidState("no open strip"))
        ));
        assert_eq!(text_state_signature(&state), before);
        assert_eq!(resources, ResourceTracker::default());
    }
    assert!(matches!(
        state.end_strip(),
        Err(SymbolTextError::InvalidState("no open strip"))
    ));
    assert!(matches!(
        state.finish(),
        Err(SymbolTextError::InvalidCount(
            "text instance count mismatch"
        ))
    ));
    state.begin_strip(0, 0).unwrap();
    let before = text_state_signature(&state);
    assert!(matches!(
        state.begin_strip(0, 0),
        Err(SymbolTextError::InvalidState("strip already open"))
    ));
    assert_eq!(text_state_signature(&state), before);
    assert!(matches!(
        state.place_subsequent(0, 0, 1, 1, &limits, &mut resources),
        Err(SymbolTextError::InvalidState(
            "subsequent instance before first"
        ))
    ));
    assert_eq!(text_state_signature(&state), before);
    assert!(matches!(
        state.finish(),
        Err(SymbolTextError::InvalidState("text strip still open"))
    ));
    assert!(matches!(
        state.end_strip(),
        Err(SymbolTextError::InvalidState("empty text strip"))
    ));
    assert!(state.strip.is_none());
    assert_eq!(state.decoded_instances, 0);
    state.begin_strip(0, 0).unwrap();
    state.place_first(0, 1, 1, &limits, &mut resources).unwrap();
    let before = text_state_signature(&state);
    assert!(matches!(
        state.place_first(0, 1, 1, &limits, &mut resources),
        Err(SymbolTextError::InvalidState(
            "first instance already placed"
        ))
    ));
    assert_eq!(text_state_signature(&state), before);
    assert_eq!(resources.work_units, 1);
    state
        .place_subsequent(0, 0, 1, 1, &limits, &mut resources)
        .unwrap();
    let before = text_state_signature(&state);
    assert!(matches!(
        state.place_subsequent(0, 0, 1, 1, &limits, &mut resources),
        Err(SymbolTextError::InvalidCount("too many text instances"))
    ));
    assert_eq!(text_state_signature(&state), before);
    assert_eq!(resources.work_units, 2);
    state.end_strip().unwrap();
    let before = text_state_signature(&state);
    assert!(matches!(
        state.begin_strip(0, 0),
        Err(SymbolTextError::InvalidState("strip after final instance"))
    ));
    assert_eq!(text_state_signature(&state), before);
    state.finish().unwrap();
}

#[test]
fn raw_error_trait_bridges_preserve_all_symbol_variants_and_nested_fields() {
    for error in [
        SymbolTextError::InvalidFlags("raw diagnostic"),
        SymbolTextError::InvalidTableAssociation("raw diagnostic"),
        SymbolTextError::InvalidCount("raw diagnostic"),
        SymbolTextError::InvalidEntropyValue("raw diagnostic"),
        SymbolTextError::UnexpectedOutOfBand("raw diagnostic"),
        SymbolTextError::InvalidState("raw diagnostic"),
    ] {
        assert!(matches!(
            DecodeError::from(error),
            DecodeError::Malformed(MalformedError::InvalidSymbolText("raw diagnostic"))
        ));
    }
    for (error, expected) in [
        (
            SymbolTextError::InvalidDimensions,
            MalformedError::InvalidSymbolDimensions,
        ),
        (
            SymbolTextError::SymbolIdOutOfRange {
                id: u32::MAX,
                available: 1,
            },
            MalformedError::SymbolIdOutOfRange {
                id: u32::MAX,
                available: 1,
            },
        ),
        (
            SymbolTextError::InvalidSymbolLengthRun,
            MalformedError::InvalidSymbolText("invalid symbol-code length run"),
        ),
    ] {
        assert!(
            matches!(DecodeError::from(error), DecodeError::Malformed(actual)
            if actual == expected)
        );
    }
    let arithmetic: SymbolTextError = ArithmeticError::InvalidContextState {
        state_index: u8::MAX,
        mps: u8::MAX,
    }
    .into();
    assert!(matches!(
        DecodeError::from(arithmetic),
        DecodeError::Malformed(MalformedError::InvalidArithmeticContextState {
            state_index: u8::MAX,
            mps: u8::MAX,
        })
    ));
    let generic: SymbolTextError = GenericRegionError::UnexpectedArithmeticMarker {
        offset: usize::MAX,
        code: u8::MAX,
    }
    .into();
    assert!(matches!(
        DecodeError::from(generic),
        DecodeError::Malformed(MalformedError::InvalidGenericRegion(
            crate::GenericRegionErrorKind::UnexpectedArithmeticMarker {
                offset: usize::MAX,
                code: u8::MAX,
            }
        ))
    ));
    let mmr: SymbolTextError =
        MmrError::Bitmap(DecodeError::Io(crate::error::IoErrorKind::PermissionDenied)).into();
    assert!(matches!(
        DecodeError::from(mmr),
        DecodeError::Io(crate::error::IoErrorKind::PermissionDenied)
    ));
    let generic: GenericRegionError = MmrError::Bitmap(DecodeError::Allocation {
        requested_bytes: u64::MAX,
    })
    .into();
    assert!(matches!(
        generic,
        GenericRegionError::Decode(DecodeError::Allocation {
            requested_bytes: u64::MAX
        })
    ));
    let huffman: HuffmanError = DecodeError::Io(crate::error::IoErrorKind::Interrupted).into();
    assert!(matches!(
        DecodeError::from(huffman),
        DecodeError::Io(crate::error::IoErrorKind::Interrupted)
    ));
}

#[test]
fn raw_dictionary_flag_words_match_independent_field_policy() {
    for flags in 0..=u16::MAX {
        let huffman = flags & 1 != 0;
        let aggregation = flags & 2 != 0;
        let height = usize::from((flags >> 2) & 3);
        let width = usize::from((flags >> 4) & 3);
        let expected_error = if flags & 0xe000 != 0 {
            Some("reserved symbol-dictionary flags")
        } else if !huffman && flags & 0x00fc != 0 {
            Some("Huffman selections in arithmetic dictionary")
        } else if huffman && flags & 0x0c00 != 0 {
            Some("direct bitmap template in Huffman dictionary")
        } else if !aggregation && flags & 0x1080 != 0 {
            Some("aggregation fields without refinement aggregation")
        } else if huffman && !aggregation && flags & 0x0300 != 0 {
            Some("bitmap contexts in direct Huffman dictionary")
        } else if height == 2 {
            Some("invalid height-delta table selection")
        } else if width == 2 {
            Some("invalid width-delta table selection")
        } else {
            None
        };
        let result = parse_symbol_dictionary_flags(flags);
        if let Some(expected) = expected_error {
            assert!(
                matches!(result, Err(SymbolTextError::InvalidFlags(reason)) if reason == expected),
                "{flags:#06x}: {result:?}"
            );
            continue;
        }
        let config = result.unwrap();
        assert_eq!(
            config,
            SymbolDictionaryConfig {
                huffman,
                refinement_aggregation: aggregation,
                height_delta: [
                    TableChoice::Standard(StandardHuffmanTable::D),
                    TableChoice::Standard(StandardHuffmanTable::E),
                    TableChoice::Custom,
                    TableChoice::Custom,
                ][height],
                width_delta: [
                    TableChoice::Standard(StandardHuffmanTable::B),
                    TableChoice::Standard(StandardHuffmanTable::C),
                    TableChoice::Custom,
                    TableChoice::Custom,
                ][width],
                bitmap_size: if flags & 0x0040 == 0 {
                    TableChoice::Standard(StandardHuffmanTable::A)
                } else {
                    TableChoice::Custom
                },
                aggregation_instances: if flags & 0x0080 == 0 {
                    TableChoice::Standard(StandardHuffmanTable::A)
                } else {
                    TableChoice::Custom
                },
                bitmap_context_used: flags & 0x0100 != 0,
                bitmap_context_retained: flags & 0x0200 != 0,
                template: u8::try_from((flags >> 10) & 3).unwrap(),
                refinement_template: u8::from(flags & 0x1000 != 0),
            },
            "{flags:#06x}"
        );
    }
}

#[test]
fn raw_text_flag_words_preserve_geometry_and_error_precedence() {
    for flags in 0..=u16::MAX {
        for huffman_flags in [None, Some(0), Some(0x8000)] {
            let huffman = flags & 1 != 0;
            let refinement = flags & 2 != 0;
            let expected_error = if !refinement && flags & 0x8000 != 0 {
                Some("refinement template without refinement")
            } else {
                match (huffman, huffman_flags) {
                    (true, None) => Some("missing text Huffman flags"),
                    (false, Some(_)) => Some("text Huffman flags in arithmetic region"),
                    (true, Some(0x8000)) => Some("reserved text Huffman flag"),
                    _ => None,
                }
            };
            let result = parse_text_region_flags(flags, huffman_flags, false);
            if let Some(expected) = expected_error {
                assert!(
                    matches!(result, Err(SymbolTextError::InvalidFlags(reason)) if reason == expected),
                    "{flags:#06x}/{huffman_flags:?}: {result:?}"
                );
            } else {
                let config = result.unwrap();
                let signed_offset = i16::try_from((flags >> 10) & 31).unwrap();
                assert_eq!(config.huffman, huffman);
                assert_eq!(config.refinement, refinement);
                assert_eq!(config.strip_size, 1 << ((flags >> 2) & 3));
                assert_eq!(
                    config.reference_corner,
                    [
                        ReferenceCorner::BottomLeft,
                        ReferenceCorner::TopLeft,
                        ReferenceCorner::BottomRight,
                        ReferenceCorner::TopRight,
                    ][usize::from((flags >> 4) & 3)]
                );
                assert_eq!(config.transposed, flags & 0x0040 != 0);
                assert_eq!(
                    config.operator,
                    [
                        CombinationOperator::Or,
                        CombinationOperator::And,
                        CombinationOperator::Xor,
                        CombinationOperator::Xnor,
                    ][usize::from((flags >> 7) & 3)]
                );
                assert_eq!(config.default_pixel, flags & 0x0200 != 0);
                assert_eq!(
                    i16::from(config.s_offset),
                    if signed_offset >= 16 {
                        signed_offset - 32
                    } else {
                        signed_offset
                    }
                );
                assert_eq!(config.refinement_template, u8::from(flags & 0x8000 != 0));
                assert_eq!(config.huffman_choices.is_some(), huffman);
            }
            assert!(matches!(
                parse_text_region_flags(flags, huffman_flags, true),
                Err(SymbolTextError::Decode(DecodeError::Unsupported(
                    UnsupportedFeature::Colour
                )))
            ));
        }
    }
}

#[test]
fn raw_text_table_selectors_match_complete_word_domain() {
    use StandardHuffmanTable::{A, F, G, H, I, J, K, L, M, N, O};
    use TableChoice::{Custom, Standard};
    for flags in 0..=u16::MAX {
        for refinement in [false, true] {
            let selectors = [0, 6, 8, 10, 12].map(|shift| (flags >> shift) & 3);
            let valid = flags & 0x8000 == 0
                && selectors.iter().all(|selector| *selector != 2)
                && (refinement || flags & 0x7fc0 == 0);
            let result = parse_text_huffman_flags(flags, refinement);
            assert_eq!(result.is_ok(), valid, "{flags:#06x}/{refinement}");
            if let Ok(choices) = result {
                let refinement_choices = [Standard(N), Standard(O), Custom, Custom];
                assert_eq!(
                    choices,
                    TextHuffmanChoices {
                        first_s: [Standard(F), Standard(G), Custom, Custom][usize::from(flags & 3)],
                        subsequent_s: [Standard(H), Standard(I), Standard(J), Custom]
                            [usize::from((flags >> 2) & 3)],
                        strip_delta: [Standard(K), Standard(L), Standard(M), Custom]
                            [usize::from((flags >> 4) & 3)],
                        refinement_width: refinement_choices[usize::from((flags >> 6) & 3)],
                        refinement_height: refinement_choices[usize::from((flags >> 8) & 3)],
                        refinement_x: refinement_choices[usize::from((flags >> 10) & 3)],
                        refinement_y: refinement_choices[usize::from((flags >> 12) & 3)],
                        refinement_size: if flags & 0x4000 == 0 {
                            Standard(A)
                        } else {
                            Custom
                        },
                    }
                );
            }
        }
    }
}

#[test]
fn raw_table_bindings_preserve_selector_order_identity_and_oob_policy() {
    let limits = Limits::default();
    let tables = [
        Box::new(HuffmanTable::standard(StandardHuffmanTable::A, &limits).unwrap()),
        Box::new(HuffmanTable::standard(StandardHuffmanTable::B, &limits).unwrap()),
    ];
    let parameters = [
        TableParameter::TextFirstS,
        TableParameter::TextSubsequentS,
        TableParameter::TextStripDelta,
        TableParameter::TextRefinementWidth,
        TableParameter::TextRefinementHeight,
        TableParameter::TextRefinementX,
        TableParameter::TextRefinementY,
        TableParameter::TextRefinementSize,
    ];
    for mask in 0..256_u16 {
        let specifications: Vec<_> = parameters
            .iter()
            .enumerate()
            .map(|(index, parameter)| {
                (
                    *parameter,
                    if mask & (1 << index) == 0 {
                        TableChoice::Standard(StandardHuffmanTable::O)
                    } else {
                        TableChoice::Custom
                    },
                    index == 1,
                )
            })
            .collect();
        let supplied: Vec<_> = specifications
            .iter()
            .enumerate()
            .filter(|(_, (_, choice, _))| *choice == TableChoice::Custom)
            .map(|(index, (_, _, has_oob))| CustomHuffmanTable {
                table: &tables[index % 2],
                has_oob: *has_oob,
            })
            .collect();
        let bindings = bind_tables(&specifications, &supplied).unwrap();
        assert_eq!(bindings.len, parameters.len());
        for (index, parameter) in parameters.iter().enumerate() {
            assert_eq!(bindings.entries[index].unwrap().parameter, *parameter);
            let found = find_table_binding(&bindings, *parameter).unwrap();
            match found {
                BoundHuffmanTable::Standard(table) => {
                    assert_eq!(mask & (1 << index), 0);
                    assert_eq!(table, StandardHuffmanTable::O);
                }
                BoundHuffmanTable::Custom(table) => {
                    assert_ne!(mask & (1 << index), 0);
                    assert!(std::ptr::eq(table, tables[index % 2].as_ref()));
                }
            }
        }
        assert!(find_table_binding(&bindings, TableParameter::DictionaryHeight).is_none());
        if !supplied.is_empty() {
            assert!(matches!(
                bind_tables(&specifications, &supplied[..supplied.len() - 1]),
                Err(SymbolTextError::InvalidTableAssociation(
                    "custom table count does not match selectors"
                ))
            ));
            for index in 0..supplied.len() {
                let mut invalid = supplied.clone();
                invalid[index].has_oob = !invalid[index].has_oob;
                assert!(matches!(
                    bind_tables(&specifications, &invalid),
                    Err(SymbolTextError::InvalidTableAssociation(
                        "custom table OOB capability is invalid"
                    ))
                ));
            }
        }
        let mut extra = supplied.clone();
        extra.push(CustomHuffmanTable {
            table: &tables[0],
            has_oob: false,
        });
        assert!(matches!(
            bind_tables(&specifications, &extra),
            Err(SymbolTextError::InvalidTableAssociation(
                "custom table count does not match selectors"
            ))
        ));
    }
    let oversized = [(TableParameter::TextFirstS, TableChoice::Custom, false); 9];
    assert!(matches!(
        bind_tables(&oversized, &[]),
        Err(SymbolTextError::InvalidTableAssociation(
            "too many table selectors"
        ))
    ));
    for left in parameters {
        for right in parameters {
            assert_eq!(same_table_parameter(left, right), left == right);
        }
    }
}

#[test]
fn raw_standard_table_cache_covers_every_slot_and_failed_materialization() {
    let limits = Limits::default();
    let restrictive = Limits {
        max_symbols: 0,
        ..limits
    };
    let names = [
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
    let mut cache = StandardHuffmanTables::empty();
    for (index, name) in names.into_iter().enumerate() {
        assert!(cache.table(name).is_none());
        assert!(matches!(
            cache.materialize(name, &restrictive),
            Err(HuffmanError::Decode(DecodeError::LimitExceeded(
                LimitExceeded {
                    kind: LimitKind::Symbols,
                    limit: 0,
                    ..
                }
            )))
        ));
        assert!(cache.table(name).is_none());
        cache.materialize(name, &limits).unwrap();
        let first = std::ptr::from_ref(cache.table(name).unwrap());
        cache.materialize(name, &restrictive).unwrap();
        assert_eq!(std::ptr::from_ref(cache.table(name).unwrap()), first);
        for (other_index, other) in names.into_iter().enumerate() {
            assert_eq!(cache.table(other).is_some(), other_index <= index);
        }
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn raw_table_associations_reject_missing_fields_and_preserve_borrowed_tables(
) -> Result<(), SymbolTextError> {
    let limits = Limits::default();
    let plain = HuffmanTable::standard(StandardHuffmanTable::A, &limits).unwrap();
    let oob = HuffmanTable::standard(StandardHuffmanTable::B, &limits).unwrap();
    let borrowed = [&plain, &oob, &plain, &plain, &plain, &plain, &plain, &plain];
    let wrapped = wrap_custom_tables(&borrowed).unwrap();
    for (index, supplied) in wrapped.iter().enumerate() {
        assert!(std::ptr::eq(supplied.table, borrowed[index]));
        assert_eq!(supplied.has_oob, index == 1);
    }
    assert!(wrap_custom_tables(&[]).unwrap().is_empty());
    let arithmetic_dictionary = parse_symbol_dictionary_flags(0).unwrap();
    let arithmetic_text = parse_text_region_flags(0, None, false).unwrap();
    assert_eq!(
        associate_dictionary_tables(&arithmetic_dictionary, &[])
            .unwrap()
            .len,
        0
    );
    assert_eq!(associate_text_tables(&arithmetic_text, &[]).unwrap().len, 0);
    assert!(matches!(
        associate_dictionary_tables(&arithmetic_dictionary, &wrapped),
        Err(SymbolTextError::InvalidTableAssociation(
            "custom tables on arithmetic dictionary"
        ))
    ));
    assert!(matches!(
        associate_text_tables(&arithmetic_text, &wrapped),
        Err(SymbolTextError::InvalidTableAssociation(
            "custom tables on arithmetic text region"
        ))
    ));
    let empty = TableBindings {
        entries: [None; MAX_TABLE_BINDINGS],
        len: 0,
    };
    assert!(matches!(
        select_dictionary_tables(&arithmetic_dictionary, &empty),
        Err(SymbolTextError::InvalidTableAssociation(
            "dictionary choices require Huffman mode"
        ))
    ));
    let dictionary = parse_symbol_dictionary_flags(0x00ff).unwrap();
    let dictionary_bindings = associate_dictionary_tables(&dictionary, &wrapped[..4]).unwrap();
    let text = parse_text_region_flags(3, Some(0x7fff), false).unwrap();
    let text_bindings = associate_text_tables(&text, &wrapped).unwrap();
    for (bindings, dictionary_mode, errors) in [
        (
            dictionary_bindings,
            true,
            &[
                "missing height table",
                "missing width table",
                "missing bitmap-size table",
                "missing aggregation-instances table",
            ][..],
        ),
        (
            text_bindings,
            false,
            &[
                "missing FS table",
                "missing DS table",
                "missing DT table",
                "missing RDW table",
                "missing RDH table",
                "missing RDX table",
                "missing RDY table",
                "missing RSIZE table",
            ][..],
        ),
    ] {
        for (index, expected) in errors.iter().enumerate() {
            let mut incomplete = bindings;
            incomplete
                .entries
                .copy_within(index + 1..bindings.len, index);
            incomplete.len -= 1;
            incomplete.entries[incomplete.len] = None;
            let result = if dictionary_mode {
                select_dictionary_tables(&dictionary, &incomplete)
            } else {
                select_text_tables(&incomplete, true)
            };
            assert!(matches!(
                result,
                Err(SymbolTextError::InvalidTableAssociation(reason)) if reason == *expected
            ));
        }
    }
    let dictionary_choices = select_dictionary_tables(&dictionary, &dictionary_bindings).unwrap();
    let dictionary_tables =
        materialize_huffman_tables!(HuffmanChoiceSet::Dictionary(dictionary_choices), &limits)
            .unwrap();
    assert!(std::ptr::eq(
        dictionary_tables
            .table_for(IntegerField::HeightDelta)
            .unwrap(),
        std::ptr::from_ref(&plain)
    ));
    assert!(std::ptr::eq(
        dictionary_tables
            .table_for(IntegerField::WidthDelta)
            .unwrap(),
        std::ptr::from_ref(&oob)
    ));
    for field in [
        IntegerField::AggregationInstances,
        IntegerField::ExportRun,
        IntegerField::RefinementX,
        IntegerField::RefinementY,
        IntegerField::RefinementSize,
        IntegerField::DictionaryBitmapSize,
    ] {
        assert!(dictionary_tables.table_for(field).is_some());
    }
    for field in [
        IntegerField::StripDelta,
        IntegerField::FirstS,
        IntegerField::SubsequentS,
        IntegerField::RefinementWidth,
        IntegerField::RefinementHeight,
        IntegerField::WithinStripT,
        IntegerField::RefinementIndicator,
    ] {
        assert!(dictionary_tables.table_for(field).is_none());
    }
    let choices = select_text_tables(&text_bindings, true).unwrap();
    let text_tables =
        materialize_huffman_tables!(HuffmanChoiceSet::TextRegion(choices), &limits).unwrap();
    for (index, field) in [
        IntegerField::FirstS,
        IntegerField::SubsequentS,
        IntegerField::StripDelta,
        IntegerField::RefinementWidth,
        IntegerField::RefinementHeight,
        IntegerField::RefinementX,
        IntegerField::RefinementY,
        IntegerField::RefinementSize,
    ]
    .into_iter()
    .enumerate()
    {
        assert!(std::ptr::eq(
            text_tables.table_for(field).unwrap(),
            borrowed[index]
        ));
    }
    for field in [
        IntegerField::AggregationInstances,
        IntegerField::HeightDelta,
        IntegerField::WidthDelta,
        IntegerField::ExportRun,
        IntegerField::DictionaryBitmapSize,
        IntegerField::WithinStripT,
        IntegerField::RefinementIndicator,
    ] {
        assert!(text_tables.table_for(field).is_none());
    }
    Ok(())
}

#[test]
fn raw_symbol_counts_and_deltas_match_wide_integer_domains() {
    let values = [0, 1, 2, u32::MAX - 1, u32::MAX];
    for imported in values {
        for new_symbols in values {
            for exported in values {
                for maximum in values {
                    let limits = Limits {
                        max_symbols: maximum,
                        ..Limits::default()
                    };
                    let total = u64::from(imported) + u64::from(new_symbols);
                    let result = check_symbol_counts(imported, new_symbols, exported, &limits);
                    if total > u64::from(u32::MAX) {
                        assert!(matches!(
                            result,
                            Err(SymbolTextError::Decode(DecodeError::Malformed(
                                MalformedError::IntegerOverflow
                            )))
                        ));
                    } else if total > u64::from(maximum) {
                        assert!(matches!(result, Err(SymbolTextError::Decode(
                            DecodeError::LimitExceeded(LimitExceeded {
                                kind: LimitKind::Symbols, limit, actual,
                            })
                        )) if limit == u64::from(maximum) && actual == total));
                    } else if u64::from(exported) > total {
                        assert!(matches!(
                            result,
                            Err(SymbolTextError::InvalidCount(
                                "exported symbols exceed candidates"
                            ))
                        ));
                    } else {
                        assert_eq!(u64::from(result.unwrap()), total);
                    }
                }
            }
        }
    }
    for current in values {
        for delta in [
            i64::MIN,
            -i64::from(u32::MAX) - 1,
            -1,
            0,
            1,
            i64::from(u32::MAX),
            i64::MAX,
        ] {
            for maximum in values {
                let sum = i128::from(current) + i128::from(delta);
                let result = checked_unsigned_delta(current, delta, maximum);
                if sum > i128::from(i64::MAX) {
                    assert!(matches!(
                        result,
                        Err(SymbolTextError::Decode(DecodeError::Malformed(
                            MalformedError::IntegerOverflow
                        )))
                    ));
                } else if sum < 0 || sum > i128::from(maximum) {
                    assert!(matches!(result, Err(SymbolTextError::InvalidDimensions)));
                } else {
                    assert_eq!(i128::from(result.unwrap()), sum);
                }
            }
        }
    }
    for power in 0..=32 {
        let boundary = 1_u64 << power;
        for count in [boundary.saturating_sub(1), boundary, boundary + 1] {
            if let Ok(count) = u32::try_from(count) {
                let length = symbol_id_code_length(count).unwrap();
                assert!((1_u64 << length) >= u64::from(count));
                assert!(length == 0 || (1_u64 << (length - 1)) < u64::from(count));
            }
        }
    }
}

#[test]
fn validates_symbol_dictionary_flags() {
    let config = parse_symbol_dictionary_flags(1).expect("valid Huffman flags");
    assert!(config.huffman);
    assert_eq!(
        config.height_delta,
        TableChoice::Standard(StandardHuffmanTable::D)
    );
    assert!(matches!(
        parse_symbol_dictionary_flags(1 | (2 << 2)),
        Err(SymbolTextError::InvalidFlags(_))
    ));
    assert!(matches!(
        parse_symbol_dictionary_flags(0xe000),
        Err(SymbolTextError::InvalidFlags(_))
    ));
}

#[test]
fn validates_text_flags_and_signed_offset() {
    let flags = 1 | (2 << 2) | (3 << 4) | (0x1d << 10);
    let config = parse_text_region_flags(flags, Some(0), false).expect("valid text flags");
    assert_eq!(config.strip_size, 4);
    assert_eq!(config.reference_corner, ReferenceCorner::TopRight);
    assert_eq!(config.s_offset, -3);
    assert!(matches!(
        parse_text_region_flags(flags, Some(0x8000), false),
        Err(SymbolTextError::InvalidFlags(_))
    ));
}

#[test]
fn associates_custom_tables_in_normative_order_and_checks_oob() {
    let limits = Limits::default();
    let table = HuffmanTable::standard(StandardHuffmanTable::A, &limits).expect("standard table");
    let config = parse_symbol_dictionary_flags(1 | (3 << 2) | (3 << 4)).expect("valid flags");
    let supplied = [
        CustomHuffmanTable {
            table: &table,
            has_oob: false,
        },
        CustomHuffmanTable {
            table: &table,
            has_oob: true,
        },
    ];
    let bindings = associate_dictionary_tables(&config, &supplied).expect("valid associations");
    assert_eq!(
        bindings.entries[0].expect("height").parameter,
        TableParameter::DictionaryHeight
    );
    assert_eq!(
        bindings.entries[1].expect("width").parameter,
        TableParameter::DictionaryWidth
    );

    let invalid = [
        CustomHuffmanTable {
            table: &table,
            has_oob: true,
        },
        supplied[1],
    ];
    assert!(matches!(
        associate_dictionary_tables(&config, &invalid),
        Err(SymbolTextError::InvalidTableAssociation(_))
    ));
}

#[test]
fn checks_cumulative_dictionary_dimensions_and_counts() {
    let limits = Limits {
        max_width: 8,
        max_height: 8,
        ..Limits::default()
    };
    let mut resources = ResourceTracker::default();
    let mut state = DictionaryClassState::new(2, &limits).expect("state");
    assert_eq!(
        state
            .begin_height_class(2, &limits, &mut resources)
            .expect("height"),
        2
    );
    assert_eq!(
        state
            .add_symbol(2, &limits, &mut resources)
            .expect("first symbol"),
        (2, 2)
    );
    assert_eq!(
        state
            .add_symbol(1, &limits, &mut resources)
            .expect("second symbol"),
        (3, 2)
    );
    assert_eq!(state.finish_height_class().expect("finish class"), 5);
    state.finish().expect("exact symbol count");

    let mut state = DictionaryClassState::new(1, &limits).expect("state");
    assert!(matches!(
        state.begin_height_class(-1, &limits, &mut resources),
        Err(SymbolTextError::InvalidDimensions)
    ));
}

#[test]
fn decodes_export_runs_including_zero_length_toggles() {
    let limits = Limits::default();
    let mut resources = ResourceTracker::default();
    let mut codec = SymbolCodec::Scripted(ScriptedCodec {
        integers: VecDeque::from([
            EntropyInteger::Value(2),
            EntropyInteger::Value(1),
            EntropyInteger::Value(2),
        ]),
        symbol_id: 0,
    });
    let exported =
        decode_export_runs(&mut codec, 5, 1, &limits, &mut resources).expect("valid export runs");
    assert_eq!(exported, vec![2]);

    let mut resources = ResourceTracker::default();
    let mut codec = SymbolCodec::Scripted(ScriptedCodec {
        integers: VecDeque::from([
            EntropyInteger::Value(0),
            EntropyInteger::Value(0),
            EntropyInteger::Value(5),
        ]),
        symbol_id: 0,
    });
    let exported = decode_export_runs(&mut codec, 5, 0, &limits, &mut resources)
        .expect("zero runs make input progress");
    assert!(exported.is_empty());
}

#[test]
fn bounds_nonprogressing_export_runs_by_work_limit() {
    let limits = Limits {
        max_work_units: 2,
        ..Limits::default()
    };
    let mut resources = ResourceTracker::default();
    let mut codec = SymbolCodec::Scripted(ScriptedCodec {
        integers: VecDeque::from([
            EntropyInteger::Value(0),
            EntropyInteger::Value(0),
            EntropyInteger::Value(0),
        ]),
        symbol_id: 0,
    });
    let result = decode_export_runs(&mut codec, 1, 0, &limits, &mut resources);
    assert!(matches!(
        result,
        Err(SymbolTextError::Decode(DecodeError::LimitExceeded(
            LimitExceeded {
                kind: LimitKind::Work,
                ..
            }
        )))
    ));
}

#[test]
fn expands_and_validates_symbol_length_runs() {
    let limits = Limits::default();
    let mut resources = ResourceTracker::default();
    let lengths = expand_symbol_code_lengths(
        8,
        &[
            SymbolLengthRun::Literal(3),
            SymbolLengthRun::RepeatPrevious(3),
            SymbolLengthRun::Zeros(4),
        ],
        &limits,
        &mut resources,
    )
    .expect("valid runs");
    assert_eq!(lengths, vec![3, 3, 3, 3, 0, 0, 0, 0]);

    let mut resources = ResourceTracker::default();
    assert!(matches!(
        expand_symbol_code_lengths(
            4,
            &[SymbolLengthRun::RepeatPrevious(3)],
            &limits,
            &mut resources
        ),
        Err(SymbolTextError::InvalidSymbolLengthRun)
    ));
    let mut resources = ResourceTracker::default();
    assert!(matches!(
        expand_symbol_code_lengths(4, &[SymbolLengthRun::Zeros(5)], &limits, &mut resources),
        Err(SymbolTextError::InvalidSymbolLengthRun)
    ));
}

#[test]
fn builds_symbol_id_huffman_table_from_lengths() {
    let limits = Limits::default();
    let table = HuffmanTable::new_symbol_ids(&[1, 3, 3], &limits);
    assert!(table.is_ok());
    let invalid = HuffmanTable::new_symbol_ids(&[1, 1, 1], &limits);
    assert!(invalid.is_err());
}

#[test]
fn exported_symbols_match_exact_clone_oracle() {
    type Descriptor<'a> = (u32, u32, usize, &'a [u8]);
    let limits = Limits::default();
    let descriptions: [Descriptor<'_>; 6] = [
        (8, 1, 1, &[0x11]),
        (16, 2, 2, &[0x21, 0x22, 0x23, 0x24]),
        (1, 3, 1, &[0x80, 0, 0x80]),
        (9, 1, 2, &[0x81, 0x80]),
        (0, 2, 0, &[]),
        (8, 2, 1, &[0x5A, 0xA5]),
    ];
    let symbols: Vec<Bitmap> = descriptions
        .iter()
        .map(|&(width, height, stride, data)| Bitmap {
            width,
            height,
            stride,
            data: data.to_vec(),
        })
        .collect();
    let (imported, new_symbols) = symbols.split_at(3);
    let borrowed: Vec<&Bitmap> = imported.iter().collect();
    let selections: [&[u32]; 8] = [
        &[],
        &[0],
        &[2],
        &[3],
        &[5],
        &[0, 1, 2, 3, 4, 5],
        &[5, 2, 0, 5, 3, 4, 1, 0],
        &[1, 1, 1, 1],
    ];
    for codebook in [
        SymbolCodebook::Contiguous(imported),
        SymbolCodebook::Borrowed(&borrowed),
    ] {
        for selected in selections {
            let output = assemble_exported_symbols(codebook, new_symbols, selected, &limits)
                .expect("assembled raw symbols");
            assert_eq!(output.len(), selected.len());
            assert!(output.capacity() >= selected.len());
            for (offset, &id) in selected.iter().enumerate() {
                let id = usize::try_from(id).unwrap();
                let (width, height, stride, data) = descriptions[id];
                let bitmap = &output[offset];
                assert_eq!(
                    (bitmap.width, bitmap.height, bitmap.stride),
                    (width, height, stride)
                );
                assert_eq!(bitmap.data.as_slice(), data);
                if !data.is_empty() {
                    assert_ne!(bitmap.data.as_ptr(), symbols[id].data.as_ptr());
                    for earlier in &output[..offset] {
                        if !earlier.data.is_empty() {
                            assert_ne!(bitmap.data.as_ptr(), earlier.data.as_ptr());
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn sparse_new_exports_preserve_scalar_order() {
    let limits = Limits::default();
    let mut symbols = Vec::new();
    for value in 0_u8..8 {
        symbols.push(Bitmap {
            width: 8,
            height: 1,
            stride: 1,
            data: vec![value],
        });
    }
    let output = assemble_exported_symbols(
        SymbolCodebook::Contiguous(&[]),
        &symbols,
        &[0, 3, 7],
        &limits,
    )
    .expect("sparse exports");
    assert_eq!(
        output
            .iter()
            .map(|bitmap| bitmap.data[0])
            .collect::<Vec<_>>(),
        vec![0, 3, 7]
    );
}

#[test]
fn borrowed_symbol_buffer_reuses_pointer_capacity() {
    let limits = Limits::default();
    let symbols = [
        Bitmap::blank(8, 1, &limits).expect("symbol"),
        Bitmap::blank(8, 1, &limits).expect("symbol"),
    ];
    let mut borrowed = Vec::new();
    borrowed
        .try_reserve(symbols.len())
        .expect("pointer reserve");
    for symbol in &symbols {
        borrowed.push(symbol);
    }
    let pointer = borrowed.as_ptr();
    let capacity = borrowed.capacity();
    borrowed.clear();
    for symbol in &symbols {
        borrowed.push(symbol);
    }
    assert_eq!(borrowed.as_ptr(), pointer);
    assert_eq!(borrowed.capacity(), capacity);
}

#[test]
fn computes_strip_placement_and_reference_corners() {
    let limits = Limits::default();
    let mut resources = ResourceTracker::default();
    let mut state = TextRegionState::new(
        text_config(ReferenceCorner::TopRight, false, 4),
        1,
        2,
        &limits,
    )
    .expect("state");
    state.begin_strip(3, 2).expect("strip");
    let placement = state
        .place_first(2, 3, 2, &limits, &mut resources)
        .expect("placement");
    assert_eq!(placement, Placement { x: 2, y: 6 });
    state.end_strip().expect("end strip");
    state.finish().expect("finish region");

    let mut resources = ResourceTracker::default();
    let mut state = TextRegionState::new(
        text_config(ReferenceCorner::BottomLeft, true, 1),
        1,
        0,
        &limits,
    )
    .expect("state");
    state.begin_strip(4, 7).expect("strip");
    let placement = state
        .place_first(0, 3, 2, &limits, &mut resources)
        .expect("placement");
    assert_eq!(placement, Placement { x: 4, y: 7 });
}

#[test]
fn clips_negative_symbol_placement() {
    let limits = Limits::default();
    let source = Bitmap::filled(3, 2, true, &limits).expect("source");
    let mut destination = Bitmap::blank(3, 3, &limits).expect("destination");
    let mut resources = ResourceTracker::default();
    compose_placement(
        &mut destination,
        &source,
        Placement { x: -1, y: -1 },
        CombinationOperator::Or,
        &limits,
        &mut resources,
    )
    .expect("clipped composition");
    assert_eq!(destination.pixel(0, 0), Some(true));
    assert_eq!(destination.pixel(1, 0), Some(true));
    assert_eq!(destination.pixel(2, 0), Some(false));
    assert_eq!(destination.pixel(0, 1), Some(false));
}

#[test]
fn clipped_symbol_placement_matches_scalar_oracle() {
    let limits = Limits::default();
    let operators = [
        CombinationOperator::Or,
        CombinationOperator::And,
        CombinationOperator::Xor,
        CombinationOperator::Xnor,
        CombinationOperator::Replace,
    ];
    for source_width in 0..=17 {
        for destination_width in 0..=17 {
            let source = Bitmap::filled(source_width, 3, true, &limits).expect("source");
            for x in -20..=20 {
                for y in -4..=4 {
                    for operator in operators {
                        let mut expected = Bitmap::filled(destination_width, 4, false, &limits)
                            .expect("destination");
                        let mut actual = expected.clone();
                        let mut expected_resources = ResourceTracker::default();
                        let mut actual_resources = ResourceTracker::default();
                        scalar_compose_placement_oracle(
                            &mut expected,
                            &source,
                            Placement { x, y },
                            operator,
                            &limits,
                            &mut expected_resources,
                        )
                        .expect("scalar placement");
                        compose_placement(
                            &mut actual,
                            &source,
                            Placement { x, y },
                            operator,
                            &limits,
                            &mut actual_resources,
                        )
                        .expect("chunk placement");
                        assert_eq!(actual, expected);
                        assert_eq!(actual_resources, expected_resources);
                    }
                }
            }
        }
    }
}

#[test]
fn placement_batches_match_scalar_for_alignments_and_operators() {
    let limits = Limits::default();
    let operators = [
        CombinationOperator::Or,
        CombinationOperator::And,
        CombinationOperator::Xor,
        CombinationOperator::Xnor,
        CombinationOperator::Replace,
    ];
    for alignment in 0_i64..8 {
        for width in 1..=16 {
            let source = Bitmap::filled(width, 3, true, &limits).expect("source");
            let placements = [
                (&source, Placement { x: alignment, y: 1 }),
                (
                    &source,
                    Placement {
                        x: alignment + i64::from(width / 2),
                        y: 1,
                    },
                ),
                (
                    &source,
                    Placement {
                        x: alignment + i64::from(width),
                        y: 1,
                    },
                ),
            ];
            for operator in operators {
                let mut expected = Bitmap::filled(64, 6, false, &limits).expect("destination");
                let mut actual = expected.clone();
                let mut expected_resources = ResourceTracker::default();
                let mut actual_resources = ResourceTracker::default();
                for &(source, placement) in &placements {
                    scalar_compose_placement_oracle(
                        &mut expected,
                        source,
                        placement,
                        operator,
                        &limits,
                        &mut expected_resources,
                    )
                    .expect("scalar placement");
                }
                batched_compose_placement(
                    &mut actual,
                    &placements,
                    operator,
                    &limits,
                    &mut actual_resources,
                )
                .expect("batched placement");
                assert_eq!(actual, expected);
                assert_eq!(actual_resources, expected_resources);
            }
        }
    }
}

#[test]
fn empty_placement_batch_flush_preserves_all_storage_and_metadata() {
    let limits = Limits::default();
    for width in [0, 1, 7, 8, 9, 65] {
        for height in [0, 1, 3] {
            for operator in [
                CombinationOperator::Or,
                CombinationOperator::And,
                CombinationOperator::Xor,
                CombinationOperator::Xnor,
                CombinationOperator::Replace,
            ] {
                let original = Bitmap::filled(width, height, true, &limits).expect("bitmap");
                let mut destination = original.clone();
                let mut batch = PlacementBatch {
                    blits: Vec::with_capacity(MAX_PLACEMENT_BATCH),
                    source_y: u32::MAX,
                    destination_y: u32::MAX,
                    height: u32::MAX,
                    source_shift: 7,
                    destination_shift: 7,
                };
                let capacity = batch.blits.capacity();
                flush_placement_batch(&mut destination, &mut batch, operator)
                    .expect("empty flush succeeds without consulting unused geometry");
                assert_eq!(destination, original);
                assert!(batch.blits.is_empty());
                assert_eq!(batch.blits.capacity(), capacity);
                assert_eq!(
                    (
                        batch.source_y,
                        batch.destination_y,
                        batch.height,
                        batch.source_shift,
                        batch.destination_shift,
                    ),
                    (u32::MAX, u32::MAX, u32::MAX, 7, 7),
                );
            }
        }
    }
}

#[test]
fn zero_height_placement_batches_clear_pending_entries_without_mutation() {
    let limits = Limits::default();
    let source = Bitmap::filled(129, 3, true, &limits).expect("source");
    let original = Bitmap::filled(139, 4, true, &limits).expect("destination");
    for count in [1, 2, MAX_PLACEMENT_BATCH] {
        for width in [0, 1, 7, 8, 64] {
            for operator in [
                CombinationOperator::Or,
                CombinationOperator::And,
                CombinationOperator::Xor,
                CombinationOperator::Xnor,
                CombinationOperator::Replace,
            ] {
                let mut destination = original.clone();
                let mut batch = PlacementBatch {
                    blits: (0..count)
                        .map(|index| RowBlit {
                            source: &source,
                            source_x: 3,
                            destination_x: 5 + u32::try_from(index % 4).expect("bounded column")
                                * 8,
                            width,
                        })
                        .collect(),
                    source_y: source.height(),
                    destination_y: destination.height(),
                    height: 0,
                    source_shift: 3,
                    destination_shift: 5,
                };
                let capacity = batch.blits.capacity();
                for _ in 0..2 {
                    flush_placement_batch(&mut destination, &mut batch, operator)
                        .expect("zero-height and subsequent empty flush succeed");
                    assert_eq!(destination, original);
                    assert!(batch.blits.is_empty());
                    assert_eq!(batch.blits.capacity(), capacity);
                    assert_eq!(
                        (
                            batch.source_y,
                            batch.destination_y,
                            batch.height,
                            batch.source_shift,
                            batch.destination_shift,
                        ),
                        (3, 4, 0, 3, 5),
                    );
                }
            }
        }
    }
}

#[test]
fn randomized_placement_batches_match_scalar_oracle() {
    let limits = Limits::default();
    let operators = [
        CombinationOperator::Or,
        CombinationOperator::And,
        CombinationOperator::Xor,
        CombinationOperator::Xnor,
        CombinationOperator::Replace,
    ];
    let mut random = 0x51f1_5eed_u32;
    for case in 0..512 {
        let mut sources = Vec::new();
        for _ in 0..8 {
            random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let width = random % 24 + 1;
            random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let height = random % 5 + 1;
            let mut bitmap = Bitmap::blank(width, height, &limits).expect("source");
            for byte in &mut bitmap.data {
                random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                *byte = u8::try_from(random >> 24).expect("random byte");
            }
            sources.push(bitmap);
        }
        let mut placements = Vec::new();
        for source in &sources {
            random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let x = i64::from(random % 52) - 10;
            random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let y = i64::from(random % 14) - 4;
            placements.push((source, Placement { x, y }));
        }
        let operator = operators[case % operators.len()];
        let mut expected = Bitmap::blank(41, 9, &limits).expect("destination");
        let mut actual = expected.clone();
        let mut expected_resources = ResourceTracker::default();
        let mut actual_resources = ResourceTracker::default();
        for &(source, placement) in &placements {
            scalar_compose_placement_oracle(
                &mut expected,
                source,
                placement,
                operator,
                &limits,
                &mut expected_resources,
            )
            .expect("scalar placement");
        }
        batched_compose_placement(
            &mut actual,
            &placements,
            operator,
            &limits,
            &mut actual_resources,
        )
        .expect("batched placement");
        assert_eq!(actual, expected, "case {case}");
        assert_eq!(actual_resources, expected_resources, "case {case}");
    }
}

#[test]
#[ignore = "microbenchmark; run explicitly with --release --ignored --nocapture"]
fn benchmark_clipped_text_route_against_6333205_scalar_oracle() {
    let limits = Limits::default();
    let source = Bitmap::filled(1024, 512, true, &limits).expect("source");
    let original = Bitmap::blank(1024, 512, &limits).expect("destination");
    let placement = Placement { x: -3, y: -2 };
    let mut chunk_times = Vec::new();
    let mut scalar_times = Vec::new();
    for sample in 0..11 {
        let mut chunk = original.clone();
        let mut scalar = original.clone();
        let mut chunk_resources = ResourceTracker::default();
        let mut scalar_resources = ResourceTracker::default();
        if sample % 2 == 0 {
            let start = Instant::now();
            compose_placement(
                &mut chunk,
                &source,
                placement,
                CombinationOperator::Or,
                &limits,
                &mut chunk_resources,
            )
            .expect("chunk placement");
            chunk_times.push(start.elapsed());
            black_box(chunk.bytes());

            let start = Instant::now();
            scalar_compose_placement_oracle(
                &mut scalar,
                &source,
                placement,
                CombinationOperator::Or,
                &limits,
                &mut scalar_resources,
            )
            .expect("scalar placement");
            scalar_times.push(start.elapsed());
            black_box(scalar.bytes());
        } else {
            let start = Instant::now();
            scalar_compose_placement_oracle(
                &mut scalar,
                &source,
                placement,
                CombinationOperator::Or,
                &limits,
                &mut scalar_resources,
            )
            .expect("scalar placement");
            scalar_times.push(start.elapsed());
            black_box(scalar.bytes());

            let start = Instant::now();
            compose_placement(
                &mut chunk,
                &source,
                placement,
                CombinationOperator::Or,
                &limits,
                &mut chunk_resources,
            )
            .expect("chunk placement");
            chunk_times.push(start.elapsed());
            black_box(chunk.bytes());
        }
        assert_eq!(chunk, scalar);
        assert_eq!(chunk_resources, scalar_resources);
    }
    let chunk = median_duration(chunk_times);
    let scalar = median_duration(scalar_times);
    println!(
        "text scalar_6333205_median_ns={} chunk_median_ns={} speedup={:.3}",
        scalar.as_nanos(),
        chunk.as_nanos(),
        scalar.as_secs_f64() / chunk.as_secs_f64(),
    );
}

#[test]
#[ignore = "performance microbenchmark"]
fn benchmark_row_aligned_text_placement_batch() {
    let limits = Limits::default();
    let source = Bitmap::filled(8, 1, true, &limits).expect("source");
    let original = Bitmap::blank(32_768, 1, &limits).expect("destination");
    let placements = (0..4_096)
        .map(|index| {
            (
                &source,
                Placement {
                    x: i64::from(index * 8),
                    y: 0,
                },
            )
        })
        .collect::<Vec<_>>();
    let mut batched_times = Vec::new();
    let mut sequential_times = Vec::new();
    for sample in 0..11 {
        let mut batched = original.clone();
        let mut sequential = original.clone();
        let mut batched_resources = ResourceTracker::default();
        let mut sequential_resources = ResourceTracker::default();
        for batched_first in [sample % 2 == 0, sample % 2 != 0] {
            let start = Instant::now();
            if batched_first {
                batched_compose_placement(
                    &mut batched,
                    &placements,
                    CombinationOperator::Or,
                    &limits,
                    &mut batched_resources,
                )
                .expect("batched placements");
                batched_times.push(start.elapsed());
                black_box(batched.bytes());
            } else {
                for &(source, placement) in &placements {
                    compose_placement(
                        &mut sequential,
                        source,
                        placement,
                        CombinationOperator::Or,
                        &limits,
                        &mut sequential_resources,
                    )
                    .expect("sequential placement");
                }
                sequential_times.push(start.elapsed());
                black_box(sequential.bytes());
            }
        }
        assert_eq!(batched, sequential);
        assert_eq!(batched_resources, sequential_resources);
    }
    let batched = median_duration(batched_times);
    let sequential = median_duration(sequential_times);
    println!(
        "text-batch sequential_median_ns={} batched_median_ns={} speedup={:.3}",
        sequential.as_nanos(),
        batched.as_nanos(),
        sequential.as_secs_f64() / batched.as_secs_f64(),
    );
}

#[test]
fn refinement_geometry_uses_mathematical_floor() {
    let geometry =
        refinement_geometry(5, 7, -3, 1, 3, -1, &Limits::default()).expect("valid refinement");
    assert_eq!(
        geometry,
        RefinementGeometry {
            width: 2,
            height: 8,
            reference_dx: 1,
            reference_dy: -1,
        }
    );
}

#[test]
fn refinement_state_receives_checked_geometry() {
    let limits = Limits::default();
    let reference = Bitmap::blank(5, 7, &limits).expect("reference");
    let mut codec = SymbolCodec::Scripted(ScriptedCodec {
        integers: VecDeque::from([
            EntropyInteger::Value(-3),
            EntropyInteger::Value(1),
            EntropyInteger::Value(3),
            EntropyInteger::Value(-1),
            EntropyInteger::Value(4),
        ]),
        symbol_id: 0,
    });
    let mut resources = ResourceTracker::default();
    let bitmap = decode_refined_bitmap(
        &mut codec,
        &reference,
        0,
        [(0, 0); 2],
        true,
        &limits,
        &mut resources,
    )
    .expect("refinement");
    assert_eq!((bitmap.width(), bitmap.height()), (2, 8));
}

#[test]
fn rejects_unused_fixed_length_symbol_ids() {
    let error = checked_symbol_id(7, 5).expect_err("ID 7 is outside five symbols");
    assert!(matches!(
        error,
        SymbolTextError::SymbolIdOutOfRange {
            id: 7,
            available: 5
        }
    ));
}

#[test]
fn repeated_standard_table_materialization_is_reused() {
    let limits = Limits::default();
    let mut tables = StandardHuffmanTables::empty();
    tables
        .materialize(StandardHuffmanTable::A, &limits)
        .expect("first table A");
    let first = tables
        .table(StandardHuffmanTable::A)
        .expect("materialized table") as *const HuffmanTable;
    tables
        .materialize(StandardHuffmanTable::A, &limits)
        .expect("second table A");
    let second =
        tables.table(StandardHuffmanTable::A).expect("reused table") as *const HuffmanTable;
    assert_eq!(first, second);
}

#[test]
fn concrete_empty_dictionary_and_arithmetic_text_segments_decode() {
    const ANNEX_H_ENCODED: [u8; 30] = [
        0x84, 0xc7, 0x3b, 0xfc, 0xe1, 0xa1, 0x43, 0x04, 0x02, 0x20, 0x00, 0x00, 0x41, 0x0d, 0xbb,
        0x86, 0xf4, 0x31, 0x7f, 0xff, 0x88, 0xff, 0x37, 0x47, 0x1a, 0xdb, 0x6a, 0xdf, 0xff, 0xac,
    ];
    let limits = Limits::default();
    let custom: [&HuffmanTable; 0] = [];

    let mut arithmetic_dictionary = vec![0x00, 0x00];
    arithmetic_dictionary.extend_from_slice(&[3, 0xff, 0xfd, 0xff, 2, 0xfe, 0xfe, 0xfe]);
    arithmetic_dictionary.extend_from_slice(&0_u32.to_be_bytes());
    arithmetic_dictionary.extend_from_slice(&0_u32.to_be_bytes());
    arithmetic_dictionary.extend_from_slice(&[0xff, 0xac]);
    let arithmetic_output = decode_symbol_dictionary(&SymbolDictionaryInput {
        payload: &arithmetic_dictionary,
        imported_symbols: SymbolCodebook::Contiguous(&[]),
        custom_tables: &custom,
        used_contexts: None,
        limits: &limits,
    })
    .expect("empty arithmetic dictionary decodes");
    assert!(arithmetic_output.exported_symbols.is_empty());
    assert_eq!(
        arithmetic_output.consumed_bytes,
        arithmetic_dictionary.len()
    );

    let mut huffman_dictionary = vec![0x00, 0x01];
    huffman_dictionary.extend_from_slice(&0_u32.to_be_bytes());
    huffman_dictionary.extend_from_slice(&0_u32.to_be_bytes());
    let huffman_output = decode_symbol_dictionary(&SymbolDictionaryInput {
        payload: &huffman_dictionary,
        imported_symbols: SymbolCodebook::Contiguous(&[]),
        custom_tables: &custom,
        used_contexts: None,
        limits: &limits,
    })
    .expect("empty Huffman dictionary decodes");
    assert!(huffman_output.exported_symbols.is_empty());
    assert_eq!(huffman_output.consumed_bytes, huffman_dictionary.len());

    let mut arithmetic_text = vec![0_u8; 17];
    arithmetic_text[3] = 1;
    arithmetic_text[7] = 1;
    arithmetic_text.extend_from_slice(&0_u16.to_be_bytes());
    arithmetic_text.extend_from_slice(&0_u32.to_be_bytes());
    arithmetic_text.extend_from_slice(&ANNEX_H_ENCODED);
    let arithmetic_text_output = decode_text_region(&TextRegionInput {
        payload: &arithmetic_text,
        symbols: SymbolCodebook::Contiguous(&[]),
        custom_tables: &custom,
        limits: &limits,
    })
    .expect("empty arithmetic text region decodes");
    assert_eq!(arithmetic_text_output.bitmap.bytes(), &[0]);
}

#[test]
#[ignore = "performance microbenchmark"]
#[allow(clippy::cast_precision_loss)]
fn benchmark_reused_standard_huffman_materialization() {
    use std::hint::black_box;
    use std::time::Instant;

    fn choices() -> HuffmanFieldChoices<'static> {
        let mut choices = HuffmanFieldChoices::empty();
        choices.first_s = Some(BoundHuffmanTable::Standard(StandardHuffmanTable::A));
        choices.subsequent_s = Some(BoundHuffmanTable::Standard(StandardHuffmanTable::A));
        choices.strip_delta = Some(BoundHuffmanTable::Standard(StandardHuffmanTable::A));
        choices.refinement_width = Some(BoundHuffmanTable::Standard(StandardHuffmanTable::O));
        choices.refinement_height = Some(BoundHuffmanTable::Standard(StandardHuffmanTable::O));
        choices.refinement_x = Some(BoundHuffmanTable::Standard(StandardHuffmanTable::O));
        choices.refinement_y = Some(BoundHuffmanTable::Standard(StandardHuffmanTable::O));
        choices.refinement_size = Some(BoundHuffmanTable::Standard(StandardHuffmanTable::A));
        choices
    }

    let limits = Limits::default();
    let mut reused_samples = Vec::new();
    let mut repeated_samples = Vec::new();
    for round in 0..9 {
        for repeated in [round % 2 != 0, round % 2 == 0] {
            let start = Instant::now();
            for _ in 0..64 {
                if repeated {
                    let tables = vec![
                        Box::new(
                            HuffmanTable::standard(StandardHuffmanTable::A, &limits)
                                .expect("table A"),
                        ),
                        Box::new(
                            HuffmanTable::standard(StandardHuffmanTable::A, &limits)
                                .expect("table A"),
                        ),
                        Box::new(
                            HuffmanTable::standard(StandardHuffmanTable::A, &limits)
                                .expect("table A"),
                        ),
                        Box::new(
                            HuffmanTable::standard(StandardHuffmanTable::O, &limits)
                                .expect("table O"),
                        ),
                        Box::new(
                            HuffmanTable::standard(StandardHuffmanTable::O, &limits)
                                .expect("table O"),
                        ),
                        Box::new(
                            HuffmanTable::standard(StandardHuffmanTable::O, &limits)
                                .expect("table O"),
                        ),
                        Box::new(
                            HuffmanTable::standard(StandardHuffmanTable::O, &limits)
                                .expect("table O"),
                        ),
                        Box::new(
                            HuffmanTable::standard(StandardHuffmanTable::A, &limits)
                                .expect("table A"),
                        ),
                    ];
                    black_box(tables);
                } else {
                    let tables = (|| -> Result<_, SymbolTextError> {
                        materialize_huffman_tables!(
                            HuffmanChoiceSet::TextRegion(choices()),
                            &limits
                        )
                    })()
                    .expect("materialized tables");
                    black_box(tables);
                }
            }
            let elapsed = start.elapsed().as_nanos();
            if repeated {
                repeated_samples.push(elapsed);
            } else {
                reused_samples.push(elapsed);
            }
        }
    }
    reused_samples.sort_unstable();
    repeated_samples.sort_unstable();
    let reused = reused_samples[reused_samples.len() / 2];
    let repeated = repeated_samples[repeated_samples.len() / 2];
    let gain = (repeated - reused) as f64 * 100.0 / repeated as f64;
    eprintln!("huffman-materialization: repeated={repeated}ns reused={reused}ns gain={gain:.2}%");
}

#[test]
#[ignore = "performance microbenchmark"]
#[allow(clippy::cast_precision_loss)]
fn benchmark_borrowed_dictionary_text_codebook() {
    let limits = Limits::default();
    let mut symbols = Vec::new();
    for value in 0_u16..2_048 {
        let mut bitmap = Bitmap::blank(128, 8, &limits).expect("symbol");
        bitmap.data.fill(u8::try_from(value & 0xff).expect("byte"));
        symbols.push(bitmap);
    }
    let mut borrowed_samples = Vec::new();
    let mut cloned_samples = Vec::new();
    let mut borrowed = Vec::new();
    borrowed
        .try_reserve(symbols.len())
        .expect("borrowed codebook capacity");
    for round in 0..9 {
        for cloned in [round % 2 != 0, round % 2 == 0] {
            let start = Instant::now();
            for _ in 0..32 {
                if cloned {
                    let mut owned = Vec::new();
                    owned.try_reserve(symbols.len()).expect("owned codebook");
                    for symbol in &symbols {
                        owned.push(clone_bitmap(symbol, &limits).expect("symbol clone"));
                    }
                    black_box(owned);
                } else {
                    borrowed.clear();
                    for symbol in &symbols {
                        borrowed.push(symbol);
                    }
                    black_box(&borrowed);
                }
            }
            let elapsed = start.elapsed().as_nanos();
            if cloned {
                cloned_samples.push(elapsed);
            } else {
                borrowed_samples.push(elapsed);
            }
        }
    }
    borrowed_samples.sort_unstable();
    cloned_samples.sort_unstable();
    let borrowed = borrowed_samples[borrowed_samples.len() / 2];
    let cloned = cloned_samples[cloned_samples.len() / 2];
    let gain = (cloned - borrowed) as f64 * 100.0 / cloned as f64;
    eprintln!("symbol-codebook: cloned={cloned}ns borrowed={borrowed}ns gain={gain:.2}%");
}

#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

use super::*;

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

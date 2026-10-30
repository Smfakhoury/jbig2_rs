#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

use super::*;

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

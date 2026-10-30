#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

use super::*;

#[test]
fn verified_decode_helpers_enforce_boundaries() {
    assert_eq!(
        checked_page_write_state(None, None, 1, 1, 0, 0, 8, 8).expect("first write is accepted"),
        Some(0)
    );
    assert!(checked_page_write_state(Some(3), None, 1, 1, 0, 3, 8, 8).is_err());
    assert_eq!(
        checked_seal_stripe_state(Some(3), 3).expect("stripe closes at its maximum"),
        None
    );
    assert!(checked_seal_stripe_state(Some(4), 3).is_err());
    assert_eq!(clipped_coordinate(u32::MAX, 1, u32::MAX), None);
    assert_eq!(clipped_coordinate(3, 4, 8), Some(7));
    assert_eq!(structural_work_totals(5, 2, 3).expect("work sum fits"), 10);
    assert!(structural_work_totals(u64::MAX, 1, 0).is_err());
}

#[test]
fn runtime_budgets_match_wide_arithmetic_and_preserve_rejected_state() {
    let values = [0, 1, 7, u64::MAX / 2, u64::MAX - 1, u64::MAX];
    for initial in values {
        for limit in values {
            let limits = Limits {
                max_work_units: limit,
                max_retained_state_bytes: limit,
                ..Limits::default()
            };
            let initialized = DecodeWork::new(initial, &limits);
            if let Ok(work) = &initialized {
                assert_eq!(work.used, initial);
            }
            assert_wide_budget_result(
                initialized.map(|_| ()),
                u128::from(initial),
                limit,
                crate::LimitKind::Work,
            );
            for amount in values {
                let total = u128::from(initial) + u128::from(amount);
                let accepted = total <= u128::from(limit);
                let mut work = DecodeWork { used: initial };
                assert_wide_budget_result(
                    work.charge(amount, &limits),
                    total,
                    limit,
                    crate::LimitKind::Work,
                );
                let expected = if accepted {
                    u64::try_from(total).unwrap()
                } else {
                    initial
                };
                assert_eq!(work.used, expected);
                let mut retained = RetainedBudget { bytes: initial };
                assert_wide_budget_result(
                    retained.retain(amount, &limits),
                    total,
                    limit,
                    crate::LimitKind::RetainedState,
                );
                assert_eq!(retained.bytes, expected);

                let mut retained = RetainedBudget { bytes: initial };
                let released = retained.release(amount);
                if amount <= initial {
                    released.unwrap();
                    assert_eq!(retained.bytes, initial - amount);
                } else {
                    assert!(matches!(
                        released,
                        Err(DecodeError::Malformed(MalformedError::IntegerOverflow))
                    ));
                    assert_eq!(retained.bytes, initial);
                }
                for second in values {
                    let pair_total = total + u128::from(second);
                    let work = DecodeWork { used: initial };
                    assert_eq!(
                        work.can_charge_pair(amount, second, &limits),
                        pair_total <= u128::from(limit),
                    );
                    assert_eq!(work.used, initial);
                }
            }
        }
    }
}

#[test]
fn referred_bitmap_resources_release_only_selected_existing_keys_once() {
    let limits = Limits::default();
    let references = [2, 1, 2, 3];
    let descriptors = [(2, 2_u32), (1, 1), (9, 3)];
    for mask in 0..16_u8 {
        let mut segment = symbol_reference_segment(&references);
        segment.header.retention_flags.values = std::iter::once(1)
            .chain((0..4).map(|i| (mask >> i) & 1))
            .collect();
        let expected: Vec<_> = descriptors
            .iter()
            .filter(|(number, _)| {
                !references
                    .iter()
                    .enumerate()
                    .any(|(i, referred)| referred == number && mask & (1 << i) == 0)
            })
            .map(|(number, _)| *number)
            .collect();
        let expected_bytes: u64 = descriptors
            .iter()
            .filter(|(number, _)| expected.contains(number))
            .map(|(_, height)| u64::from(*height))
            .sum();
        let mut buffers = Vec::new();
        let mut patterns = Vec::new();
        let mut symbols = Vec::new();
        for (number, height) in descriptors {
            let region = crate::segment_data::RegionInfo {
                width: 8,
                height,
                x: 0,
                y: 0,
                operator: CombinationOperator::Replace,
            };
            buffers.push((
                number,
                RegionBuffer {
                    region,
                    bitmap: Bitmap::blank(8, height, &limits).unwrap(),
                },
            ));
            patterns.push((
                number,
                vec![
                    Bitmap::blank(8, height, &limits).unwrap(),
                    Bitmap::blank(0, 1, &limits).unwrap(),
                ],
            ));
            symbols.push((
                number,
                SymbolResource {
                    exported: vec![
                        Bitmap::blank(8, height, &limits).unwrap(),
                        Bitmap::blank(0, 1, &limits).unwrap(),
                    ],
                    contexts: None,
                },
            ));
        }
        let mut buffer_budget = RetainedBudget { bytes: 6 };
        let mut pattern_budget = RetainedBudget { bytes: 6 };
        let mut symbol_budget = RetainedBudget { bytes: 6 };
        for _ in 0..2 {
            release_referred_buffers(&segment, &mut buffers, &mut buffer_budget).unwrap();
            release_referred_patterns(&segment, &mut patterns, &mut pattern_budget).unwrap();
            release_referred_symbols(&segment, &mut symbols, &mut symbol_budget).unwrap();
            assert_eq!(buffer_budget.bytes, expected_bytes);
            assert_eq!(pattern_budget.bytes, expected_bytes);
            assert_eq!(symbol_budget.bytes, expected_bytes);
            assert_eq!(
                buffers.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
                expected
            );
            assert_eq!(
                patterns.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
                expected
            );
            assert_eq!(
                symbols.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
                expected
            );
        }
    }
}

#[test]
fn raw_bitmap_snapshot_matches_literal_scalar_clipping_and_padding() {
    let limits = Limits::default();
    let bytes = [0xa5_u8, 0x3c];
    let mut page = Bitmap::blank(8, 2, &limits).unwrap();
    for y in 0..2 {
        for x in 0..8 {
            page.set_pixel(x, y, bytes[y as usize] & (0x80 >> x) != 0)
                .unwrap();
        }
    }
    for x in 0..=10 {
        for y in 0..=4 {
            for width in [0, 1, 3, 8, 10] {
                for height in 0..=3 {
                    let region = crate::segment_data::RegionInfo {
                        width,
                        height,
                        x,
                        y,
                        operator: CombinationOperator::Replace,
                    };
                    let snapshot = snapshot_region(&page, &region, &limits).unwrap();
                    let stride = usize::try_from(width.div_ceil(8)).unwrap();
                    let mut expected = vec![0; stride * usize::try_from(height).unwrap()];
                    for target_y in 0..height {
                        for target_x in 0..width {
                            let source_x = x + target_x;
                            let source_y = y + target_y;
                            if source_x < 8
                                && source_y < 2
                                && bytes[source_y as usize] & (0x80 >> source_x) != 0
                            {
                                let index = usize::try_from(target_y).unwrap() * stride
                                    + usize::try_from(target_x / 8).unwrap();
                                expected[index] |= 0x80 >> (target_x % 8);
                            }
                        }
                    }
                    assert_eq!(snapshot.width(), width);
                    assert_eq!(snapshot.height(), height);
                    assert_eq!(snapshot.stride(), stride);
                    assert_eq!(snapshot.bytes(), expected);
                    assert_eq!(page.bytes(), bytes);
                }
            }
        }
    }
}

#[test]
fn raw_page_write_bounds_match_wide_intersection_and_error_priority() {
    let coordinates = [0, 1, 2, u32::MAX - 1, u32::MAX];
    let mut cases = 0;
    for (page_width, page_height) in [(0, 0), (1, 1), (9, 3), (u32::MAX, u32::MAX)] {
        for width in coordinates {
            for height in coordinates {
                for x in coordinates {
                    for y in coordinates {
                        for sealed in [None, Some(0), Some(1), Some(u32::MAX)] {
                            for open in [None, Some(0), Some(2), Some(u32::MAX)] {
                                let bottom = u64::from(y) + u64::from(height);
                                let expected = if width == 0
                                    || height == 0
                                    || x >= page_width
                                    || y >= page_height
                                {
                                    Ok(open)
                                } else if sealed.is_some_and(|row| y <= row) {
                                    Err(MalformedError::InvalidPageState(
                                        "region modifies a sealed stripe",
                                    ))
                                } else if bottom > u64::from(u32::MAX) {
                                    Err(MalformedError::IntegerOverflow)
                                } else {
                                    let last =
                                        u32::try_from(bottom.min(u64::from(page_height)) - 1)
                                            .unwrap();
                                    Ok(Some(open.map_or(last, |old| old.max(last))))
                                };
                                match checked_page_write_state(
                                    sealed,
                                    open,
                                    width,
                                    height,
                                    x,
                                    y,
                                    page_width,
                                    page_height,
                                ) {
                                    Ok(next) => assert_eq!(expected, Ok(next)),
                                    Err(DecodeError::Malformed(error)) => {
                                        assert_eq!(expected, Err(error));
                                    }
                                    other => panic!("unexpected page-write result: {other:?}"),
                                }
                                cases += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 40_000);
}

#[test]
fn raw_stripe_sealing_preserves_pixels_and_rejected_page_state() {
    let limits = Limits::default();
    for sealed in [None, Some(0), Some(u32::MAX)] {
        for open in [None, Some(0), Some(2), Some(u32::MAX)] {
            for row in [0, 1, 2, u32::MAX] {
                let mut page =
                    runtime_test_page(9, 3, true, false, CombinationOperator::Replace, &limits);
                page.last_stripe_end = sealed;
                page.open_stripe_max_row = open;
                let before = page.bitmap.bytes().to_vec();
                let result = seal_stripe_on_page(&mut page, row);
                if open.is_some_and(|maximum| maximum > row) {
                    assert!(matches!(
                        result,
                        Err(DecodeError::Malformed(MalformedError::InvalidPageState(
                            "region preceding stripe extends below its end row"
                        )))
                    ));
                    assert_eq!(page.last_stripe_end, sealed);
                    assert_eq!(page.open_stripe_max_row, open);
                } else {
                    result.unwrap();
                    assert_eq!(page.last_stripe_end, Some(row));
                    assert_eq!(page.open_stripe_max_row, None);
                }
                assert_eq!(page.bitmap.width(), 9);
                assert_eq!(page.bitmap.height(), 3);
                assert_eq!(page.bitmap.bytes(), before);
                assert!(!page.pristine);
            }
        }
    }
}

#[test]
fn duplicate_buffer_does_not_change_retained_accounting() {
    let limits = Limits::default();
    let region = crate::segment_data::RegionInfo {
        width: 8,
        height: 1,
        x: 0,
        y: 0,
        operator: crate::bitmap::CombinationOperator::Replace,
    };
    let mut buffers = Vec::new();
    let mut retained = RetainedBudget::new();
    insert_buffer(
        &mut buffers,
        7,
        region,
        Bitmap::blank(8, 1, &limits).expect("bitmap allocates"),
        &mut retained,
        &limits,
    )
    .expect("first buffer inserts");
    assert_eq!(retained.bytes, 1);

    assert!(matches!(
        insert_buffer(
            &mut buffers,
            7,
            region,
            Bitmap::blank(8, 1, &limits).expect("bitmap allocates"),
            &mut retained,
            &limits,
        ),
        Err(DecodeError::Malformed(
            MalformedError::DuplicateSegmentNumber(7)
        ))
    ));
    assert_eq!(retained.bytes, 1);
    assert_eq!(buffers.len(), 1);
}

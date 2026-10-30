#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

use super::*;

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

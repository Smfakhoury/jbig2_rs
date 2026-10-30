#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

use super::*;

#[test]
fn numeric_preflight_quotas_match_wide_results_and_priority() {
    let counts = [
        0_usize,
        1,
        2,
        7,
        8,
        9,
        15,
        16,
        17,
        31,
        32,
        33,
        usize::MAX / 2,
        usize::MAX - 1,
        usize::MAX,
    ];
    let scalar_limits = [0_u64, 1, 7, 8, u64::MAX / 2, u64::MAX - 1, u64::MAX];
    let count_limits = [0_u32, 1, 7, 8, u32::MAX];
    let mut cases = 0;
    for count in counts {
        for limit in scalar_limits {
            let limits = Limits {
                max_stream_bytes: limit,
                max_work_units: limit,
                ..Limits::default()
            };
            assert_eq!(
                quota_view(enforce_stream_limit(count, &limits)),
                wide_quota(
                    count as u128,
                    u128::from(u64::MAX),
                    limit,
                    LimitKind::StreamBytes
                ),
                "stream {count}/{limit}",
            );
            assert_eq!(
                quota_view(enforce_input_work(count, &limits)),
                wide_quota(count as u128, u128::from(u64::MAX), limit, LimitKind::Work),
                "work {count}/{limit}",
            );
            cases += 2;
        }
        for limit in count_limits {
            let limits = Limits {
                max_pages: limit,
                max_segments: limit,
                ..Limits::default()
            };
            let actual = count as u128 + 1;
            let maximum = (usize::MAX as u128).min(u128::from(u64::MAX));
            assert_eq!(
                quota_view(enforce_segment_limit(count, &limits)),
                wide_quota(actual, maximum, u64::from(limit), LimitKind::Segments),
                "segments {count}/{limit}",
            );
            assert_eq!(
                quota_view(enforce_page_limit(count, &limits)),
                wide_quota(actual, maximum, u64::from(limit), LimitKind::Pages),
                "pages {count}/{limit}",
            );
            cases += 2;
        }
    }
    assert_eq!(cases, 360);
}

#[test]
#[allow(clippy::too_many_lines)]
fn reference_graph_retention_and_budget_precedence_match_scalar_policy() {
    let mut graphs = 0;
    let mut cases = 0;
    for first in [&[][..], &[0][..]] {
        for second in [&[][..], &[0][..], &[1][..], &[0, 1][..]] {
            let references: [&[u32]; 3] = [&[], first, second];
            let mut last = [None; 3];
            let mut depths = [1_u32; 3];
            for (owner, referred) in references.iter().enumerate() {
                for number in *referred {
                    let index = usize::try_from(*number).unwrap();
                    last[index] = Some(owner);
                    depths[owner] = depths[owner].max(depths[index] + 1);
                }
            }
            let total =
                u64::try_from(references.iter().map(|refs| refs.len()).sum::<usize>()).unwrap();
            let maximum_depth = *depths.iter().max().unwrap();
            for flags0 in 0..2_u8 {
                for flags1 in 0..(1_u8 << (first.len() + 1)) {
                    for flags2 in 0..(1_u8 << (second.len() + 1)) {
                        let flags = [flags0, flags1, flags2];
                        let segments = [
                            reference_validation_segment(0, &[], flags0),
                            reference_validation_segment(1, first, flags1),
                            reference_validation_segment(2, second, flags2),
                        ];
                        let headers: Vec<_> = segments.iter().map(|s| s.header.clone()).collect();
                        let mut retention_error = None;
                        for owner in 0..3 {
                            if last[owner].is_some() && flags[owner] & 1 == 0 {
                                let number = u64::try_from(owner).unwrap();
                                retention_error = Some(("retention", number, number));
                                break;
                            }
                            for (index, number) in references[owner].iter().enumerate() {
                                if last[usize::try_from(*number).unwrap()] != Some(owner)
                                    && flags[owner] & (1 << (index + 1)) == 0
                                {
                                    retention_error = Some((
                                        "retention",
                                        u64::try_from(owner).unwrap(),
                                        u64::from(*number),
                                    ));
                                    break;
                                }
                            }
                            if retention_error.is_some() {
                                break;
                            }
                        }
                        for reference_limit in [total.saturating_sub(1), total, total + 1] {
                            for depth_limit in 0..=maximum_depth + 1 {
                                let expected = if reference_limit < total {
                                    Some(("references", reference_limit, total))
                                } else if retention_error.is_some() {
                                    retention_error
                                } else {
                                    depths
                                        .iter()
                                        .find(|depth| **depth > depth_limit)
                                        .map(|depth| {
                                            ("depth", u64::from(depth_limit), u64::from(*depth))
                                        })
                                };
                                let limits = Limits {
                                    max_total_references: reference_limit,
                                    max_reference_graph_depth: depth_limit,
                                    ..Limits::default()
                                };
                                match validate_references(&segments, None, &limits) {
                                    Ok((types, actual_depths)) => {
                                        assert_eq!(expected, None);
                                        assert_eq!(
                                            types,
                                            vec![
                                                (0, SegmentType::SymbolDictionary),
                                                (1, SegmentType::SymbolDictionary),
                                                (2, SegmentType::SymbolDictionary),
                                            ],
                                        );
                                        assert_eq!(
                                            actual_depths,
                                            vec![(0, depths[0]), (1, depths[1]), (2, depths[2])],
                                        );
                                    }
                                    Err(DecodeError::LimitExceeded(error)) => {
                                        let kind = match error.kind {
                                            LimitKind::TotalReferences => "references",
                                            LimitKind::ReferenceDepth => "depth",
                                            other => {
                                                panic!("unexpected graph limit: {other:?}")
                                            }
                                        };
                                        assert_eq!(
                                            Some((kind, error.limit, error.actual)),
                                            expected,
                                        );
                                    }
                                    Err(DecodeError::Malformed(
                                        MalformedError::InvalidRetentionFlags { segment, referred },
                                    )) => {
                                        assert_eq!(
                                            Some((
                                                "retention",
                                                u64::from(segment),
                                                u64::from(referred)
                                            )),
                                            expected,
                                        );
                                    }
                                    other => panic!("unexpected graph result: {other:?}"),
                                }
                                assert_eq!(
                                    segments
                                        .iter()
                                        .map(|s| s.header.clone())
                                        .collect::<Vec<_>>(),
                                    headers,
                                );
                                cases += 1;
                            }
                        }
                        graphs += 1;
                    }
                }
            }
        }
    }
    assert_eq!(graphs, 216);
    assert_eq!(cases, 2856);
    let mut malformed = reference_validation_segment(0, &[], 1);
    malformed.header.retention_flags.values.clear();
    assert!(matches!(
        validate_retention_flags(&[malformed], &[]),
        Err(DecodeError::Malformed(
            MalformedError::InvalidRetentionFlags {
                segment: 0,
                referred: 0,
            }
        ))
    ));
}

#[test]
fn reference_depth_orders_raw_metadata_and_reports_missing_or_overflowed_ancestors() {
    let limits = Limits::default();
    let segments = [
        reference_validation_segment(2, &[1], 3),
        reference_validation_segment(0, &[], 1),
        reference_validation_segment(1, &[0], 3),
    ];
    let indexes = vec![(2, 0), (0, 1), (1, 2)];
    assert_eq!(
        validate_reference_depth(&segments, &indexes, None, &limits).unwrap(),
        vec![(0, 1), (1, 2), (2, 3)],
    );

    let segments = [reference_validation_segment(1, &[0], 3)];
    let ordered = [(1, 0)];
    let indexes = vec![(1, 0)];
    for globals in [
        None,
        Some(GlobalSegments {
            types: vec![(0, SegmentType::SymbolDictionary)],
            depths: Vec::new(),
            bytes: Vec::new(),
        }),
    ] {
        let mut depths = Vec::new();
        assert!(matches!(
            validate_reference_depth_ordered(
                &ordered,
                &segments,
                &indexes,
                globals.as_ref(),
                &limits,
                &mut depths,
            ),
            Err(DecodeError::Malformed(
                MalformedError::MissingReferredSegment {
                    segment: 1,
                    referred: 0,
                }
            ))
        ));
        assert!(depths.is_empty());
    }
    let globals = GlobalSegments {
        types: vec![(0, SegmentType::SymbolDictionary)],
        depths: vec![(0, u32::MAX)],
        bytes: Vec::new(),
    };
    let mut depths = Vec::new();
    assert!(matches!(
        validate_reference_depth_ordered(
            &ordered,
            &segments,
            &indexes,
            Some(&globals),
            &limits,
            &mut depths,
        ),
        Err(DecodeError::Malformed(MalformedError::IntegerOverflow))
    ));
    assert!(depths.is_empty());
}

#[test]
fn raw_reference_duplicates_and_cross_page_links_preserve_exact_diagnostics() {
    let limits = Limits::default();
    let segments = [
        reference_validation_segment(0, &[], 1),
        reference_validation_segment(0, &[], 1),
    ];
    assert!(matches!(
        validate_references(&segments, None, &limits),
        Err(DecodeError::Malformed(
            MalformedError::DuplicateSegmentNumber(0)
        ))
    ));
    let globals = GlobalSegments {
        types: vec![(0, SegmentType::SymbolDictionary)],
        depths: vec![(0, 1)],
        bytes: Vec::new(),
    };
    assert!(matches!(
        validate_references(&segments[..1], Some(&globals), &limits),
        Err(DecodeError::Malformed(
            MalformedError::DuplicateSegmentNumber(0)
        ))
    ));
    let mut segments = [
        reference_validation_segment(0, &[], 1),
        reference_validation_segment(1, &[0], 3),
    ];
    segments[0].header.page_association = 2;
    segments[1].header.page_association = 1;
    assert!(matches!(
        validate_reference_links(&segments, &vec![(0, 0), (1, 1)], None),
        Err(DecodeError::Malformed(
            MalformedError::InvalidPageAssociation {
                segment: 1,
                page: 1,
            }
        ))
    ));
}

#[test]
fn raw_reference_role_matrix_matches_numeric_tag_policy_locally_and_globally() {
    let kinds = [
        SegmentType::SymbolDictionary,
        SegmentType::IntermediateTextRegion,
        SegmentType::ImmediateTextRegion,
        SegmentType::ImmediateLosslessTextRegion,
        SegmentType::PatternDictionary,
        SegmentType::IntermediateHalftoneRegion,
        SegmentType::ImmediateHalftoneRegion,
        SegmentType::ImmediateLosslessHalftoneRegion,
        SegmentType::IntermediateGenericRegion,
        SegmentType::ImmediateGenericRegion,
        SegmentType::ImmediateLosslessGenericRegion,
        SegmentType::IntermediateGenericRefinementRegion,
        SegmentType::ImmediateGenericRefinementRegion,
        SegmentType::ImmediateLosslessGenericRefinementRegion,
        SegmentType::PageInformation,
        SegmentType::EndOfPage,
        SegmentType::EndOfStripe,
        SegmentType::EndOfFile,
        SegmentType::Profiles,
        SegmentType::Tables,
        SegmentType::ColourPalette,
        SegmentType::Extension,
    ];
    let mut cases = 0;
    for owner_kind in kinds {
        for target_kind in kinds {
            for count in [0_u32, 1, 2, 4, 5, 8, 9] {
                let references: Vec<_> = (0..count).collect();
                let mut owner = reference_validation_segment(30, &[], 1);
                owner.header.segment_type = owner_kind;
                owner.header.referred_segments = references;
                owner.header.retention_flags.values = vec![1; usize::try_from(count).unwrap() + 1];
                let segments: Vec<_> = (0..count)
                    .map(|number| {
                        let mut segment = reference_validation_segment(number, &[], 1);
                        segment.header.segment_type = target_kind;
                        segment
                    })
                    .collect();
                let indexes: Vec<_> = (0..count)
                    .map(|number| (number, usize::try_from(number).unwrap()))
                    .collect();
                let globals = GlobalSegments {
                    types: (0..count).map(|number| (number, target_kind)).collect(),
                    depths: Vec::new(),
                    bytes: Vec::new(),
                };
                let target = target_kind as u8;
                let expected = match owner_kind as u8 {
                    0 => count == 0 || target == 0 || (target == 53 && count <= 4),
                    4 | 6 | 7 => count == 0 || target == 0 || (target == 53 && count <= 8),
                    20 | 22 | 23 => count == 1 && target == 16,
                    40 => count == 1 && [4, 20, 36, 40].contains(&target),
                    42 | 43 => count == 0 || (count == 1 && [4, 20, 36, 40].contains(&target)),
                    62 => true,
                    _ => count == 0,
                };
                for global in [false, true] {
                    let actual = if global {
                        validate_reference_types(&owner.header, &[], &Vec::new(), Some(&globals))
                    } else {
                        validate_reference_types(&owner.header, &segments, &indexes, None)
                    };
                    if expected {
                        actual.unwrap();
                    } else {
                        assert!(matches!(
                            actual,
                            Err(DecodeError::Malformed(
                                MalformedError::InvalidSegmentReferences(30)
                            ))
                        ));
                    }
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 6_776);
    let empty = reference_validation_segment(30, &[], 1);
    assert_eq!(
        known_reference_type(&empty.header, 0, &[], &Vec::new(), None),
        None
    );
}

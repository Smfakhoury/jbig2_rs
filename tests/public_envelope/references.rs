#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

use super::*;

#[test]
fn reference_retention_uses_the_last_reference_not_the_first_or_wire_order() {
    for deferred in [false, true] {
        let first = extension(0, 0, 0, &[]);
        let mut middle = extension(1, 0, 0, &[]).references(&[0], 3);
        middle.deferred = deferred;
        let last = extension(2, 0, 0, &[]).references(&[0, 1], 0);
        for order in [
            [first.clone(), middle.clone(), last.clone()],
            [last.clone(), first.clone(), middle.clone()],
        ] {
            let globals = Decoder::default().parse_globals(&embedded(&order)).unwrap();
            assert_eq!(globals.segment_count(), 3);
        }
        middle.retention = 1;
        reject(
            &[first.clone(), middle.clone(), last.clone(), eof(3)],
            &MalformedError::InvalidRetentionFlags {
                segment: 1,
                referred: 0,
            },
        );
        let mut released = first;
        released.retention = 0;
        middle.retention = 3;
        reject(
            &[released, middle, last, eof(3)],
            &MalformedError::InvalidRetentionFlags {
                segment: 0,
                referred: 0,
            },
        );
    }
}

#[test]
fn embedded_duplicate_numbers_and_cross_page_references_are_exact() {
    let decoder = Decoder::default();
    let duplicate = embedded(&[extension(7, 0, 0, &[]), extension(7, 0, 0, &[])]);
    assert_malformed(
        decoder.inspect_embedded(&duplicate).unwrap_err(),
        &MalformedError::DuplicateSegmentNumber(7),
    );
    assert_malformed(
        decoder.parse_globals(&duplicate).unwrap_err(),
        &MalformedError::DuplicateSegmentNumber(7),
    );
    let segments = [
        page(0, 2, 8, 1),
        extension(1, 2, 0, &[]),
        page(2, 1, 8, 1),
        extension(3, 1, 0, &[]).references(&[1], 3),
        end(4, 1),
        end(5, 2),
        eof(6),
    ];
    reject(
        &segments,
        &MalformedError::InvalidPageAssociation {
            segment: 3,
            page: 1,
        },
    );
}

#[test]
fn table_body_errors_precede_reference_role_checks_without_losing_error_fields() {
    for (payload, expected) in [
        (
            Vec::new(),
            MalformedError::UnexpectedEof {
                offset: 0,
                needed: 1,
                remaining: 0,
            },
        ),
        (
            vec![0x83, 0, 0, 0, 0, 0, 0, 0, 1],
            MalformedError::InvalidHuffman(HuffmanErrorKind::ReservedFlags(0x83)),
        ),
        (
            vec![0, 0, 0, 0, 0, 0, 0, 0, 1, 0b1011_1111],
            MalformedError::InvalidHuffman(HuffmanErrorKind::NonZeroPadding),
        ),
    ] {
        let segments = [
            page(1, 1, 8, 1),
            extension(2, 0, 0, &[]),
            Segment::new(3, 53, 0, payload).retaining_references(&[2]),
            end(4, 1),
            eof(5),
        ];
        reject(&segments, &expected);
    }
}

#[test]
fn reference_roles_reject_invalid_consumers_before_page_delivery() {
    for source_kind in [0_u8, 4, 16, 36, 38, 53] {
        for consumer_kind in [
            0_u8, 4, 6, 7, 16, 20, 22, 23, 36, 38, 39, 40, 42, 43, 48, 49, 50, 52, 53, 62,
        ] {
            let source_page = u32::from(!matches!(source_kind, 0 | 16 | 53));
            let segments = [
                page(1, 1, 8, 1),
                Segment::new(2, source_kind, source_page, reference_payload(source_kind)),
                Segment::new(3, consumer_kind, 1, reference_payload(consumer_kind))
                    .retaining_references(&[2]),
                end(4, 1),
                eof(5),
            ];
            let allowed = match consumer_kind {
                0 | 4 | 6 | 7 => matches!(source_kind, 0 | 53),
                20 | 22 | 23 => source_kind == 16,
                40 | 42 | 43 => matches!(source_kind, 4 | 36),
                62 => true,
                _ => false,
            };
            if allowed {
                assert_reference_metadata(&segments);
            } else {
                reject(&segments, &MalformedError::InvalidSegmentReferences(3));
            }
        }
    }
}

#[test]
fn only_intermediate_region_roles_can_supply_explicit_refinement_references() {
    for source_kind in [4_u8, 6, 20, 22, 36, 38, 40, 42] {
        let prerequisite = match source_kind {
            20 | 22 => Segment::new(2, 16, 0, Vec::new()),
            40 => Segment::new(2, 36, 1, Vec::new()),
            _ => extension(2, 0, 0, &[]),
        };
        let source = Segment::new(3, source_kind, 1, Vec::new());
        let source = if matches!(source_kind, 20 | 22 | 40) {
            source.retaining_references(&[2])
        } else {
            source
        };
        for consumer_kind in [40_u8, 42, 43] {
            let segments = [
                page(1, 1, 8, 1),
                prerequisite.clone(),
                source.clone(),
                Segment::new(4, consumer_kind, 1, Vec::new()).retaining_references(&[3]),
                end(5, 1),
                eof(6),
            ];
            if matches!(source_kind, 4 | 20 | 36 | 40) {
                assert_reference_metadata(&segments);
            } else {
                reject(&segments, &MalformedError::InvalidSegmentReferences(4));
            }
        }
    }
}

#[test]
fn reference_table_limits_count_tables_not_dictionaries_in_both_header_forms() {
    for consumer_kind in [0_u8, 4, 6, 7] {
        for table_count in [0_u8, 1, 4, 5, 8, 9] {
            let mut segments = vec![page(1, 1, 8, 1), Segment::new(2, 0, 0, Vec::new())];
            for index in 0..table_count {
                segments.push(Segment::new(
                    u32::from(index) + 3,
                    53,
                    0,
                    reference_payload(53),
                ));
            }
            let number = u32::from(table_count) + 3;
            let references: Vec<_> = (2..table_count + 3).collect();
            segments.push(
                Segment::new(number, consumer_kind, 1, Vec::new())
                    .retaining_references(&references),
            );
            segments.extend([end(number + 1, 1), eof(number + 2)]);
            let maximum = if consumer_kind == 0 { 4 } else { 8 };
            if table_count <= maximum {
                assert_reference_metadata(&segments);
            } else {
                reject(&segments, &MalformedError::InvalidSegmentReferences(number));
            }
        }
    }
}

#[test]
fn halftone_and_refinement_reference_cardinality_is_exact() {
    for consumer_kind in [20_u8, 22, 23, 40, 42, 43] {
        for references in [&[][..], &[2][..], &[2, 3][..]] {
            let (source_kind, source_page) = if matches!(consumer_kind, 20 | 22 | 23) {
                (16, 0)
            } else {
                (36, 1)
            };
            let segments = [
                page(1, 1, 8, 1),
                Segment::new(2, source_kind, source_page, Vec::new()),
                Segment::new(3, source_kind, source_page, Vec::new()),
                Segment::new(4, consumer_kind, 1, Vec::new()).retaining_references(references),
                end(5, 1),
                eof(6),
            ];
            if references.len() == 1 || (references.is_empty() && matches!(consumer_kind, 42 | 43))
            {
                assert_reference_metadata(&segments);
            } else {
                reject(&segments, &MalformedError::InvalidSegmentReferences(4));
            }
        }
    }
}

#[test]
fn global_reference_roles_are_rechecked_without_mutating_shared_state() {
    let decoder = Decoder::default();
    for source_kind in [0_u8, 16, 53] {
        let globals = decoder
            .parse_globals(&embedded(&[Segment::new(
                2,
                source_kind,
                0,
                reference_payload(source_kind),
            )]))
            .unwrap();
        for consumer_kind in [0_u8, 4, 6, 7, 20, 22, 23, 40, 42, 43] {
            let bytes = embedded(&[
                page(3, 1, 8, 1),
                Segment::new(4, consumer_kind, 1, Vec::new()).retaining_references(&[2]),
                end(5, 1),
            ]);
            let allowed = if matches!(consumer_kind, 0 | 4 | 6 | 7) {
                matches!(source_kind, 0 | 53)
            } else {
                matches!(consumer_kind, 20 | 22 | 23) && source_kind == 16
            };
            let mut context = decoder.context();
            if allowed {
                let info = decoder
                    .inspect_embedded_with_globals(&bytes, &globals)
                    .unwrap();
                assert_info(
                    &info,
                    Organization::Sequential,
                    None,
                    3,
                    &[(1, 8, 1, 300, 600, false)],
                );
                assert_eq!(
                    context
                        .inspect_embedded_with_globals(&bytes, &globals)
                        .unwrap(),
                    info
                );
            } else {
                let expected = MalformedError::InvalidSegmentReferences(4);
                assert_malformed(
                    decoder
                        .inspect_embedded_with_globals(&bytes, &globals)
                        .unwrap_err(),
                    &expected,
                );
                assert_malformed(
                    decoder
                        .decode_embedded_with_globals(&bytes, &globals)
                        .unwrap_err(),
                    &expected,
                );
                assert_malformed(
                    context
                        .inspect_embedded_with_globals(&bytes, &globals)
                        .unwrap_err(),
                    &expected,
                );
                assert_malformed(
                    context
                        .decode_embedded_with_globals(&bytes, &globals)
                        .unwrap_err(),
                    &expected,
                );
            }
            assert_eq!(globals.segment_count(), 1);
            assert_malformed(
                context.inspect_embedded(&bytes).unwrap_err(),
                &MalformedError::MissingReferredSegment {
                    segment: 4,
                    referred: 2,
                },
            );
            let (valid, _) = fixture::generated_page();
            let recovered = context.decode(&valid).unwrap();
            assert_pixels(&recovered.pages()[0], 1, 8, 1, &[0]);
        }
    }
}

#[test]
fn nonempty_global_reference_chains_reuse_immutable_state_and_exact_depth() {
    let decoder = Decoder::default();
    let global_bytes = embedded(&[
        extension(0, 0, 0, &[]),
        extension(1, 0, 0, &[]).references(&[0], 3),
    ]);
    assert_eq!(global_bytes.len(), 31);
    let globals = decoder.parse_globals(&global_bytes).unwrap();
    assert_eq!(globals.segment_count(), 2);
    let bytes = embedded(&[
        page(2, 1, 9, 1),
        extension(3, 1, 0, &[]).references(&[1], 3),
        end(4, 1),
    ]);
    let mut limits = Limits::default();
    limits.max_reference_graph_depth = 3;
    let bounded = Decoder::new(limits);
    let mut context = bounded.context();
    for _ in 0..3 {
        let info = context
            .inspect_embedded_with_globals(&bytes, &globals)
            .unwrap();
        assert_info(
            &info,
            Organization::Sequential,
            None,
            3,
            &[(1, 9, 1, 300, 600, false)],
        );
        let document = context
            .decode_embedded_with_globals(&bytes, &globals)
            .unwrap();
        assert_pixels(&document.pages()[0], 1, 9, 1, &[0, 0]);
        assert_malformed(
            context.inspect_embedded(&bytes).unwrap_err(),
            &MalformedError::MissingReferredSegment {
                segment: 3,
                referred: 1,
            },
        );
        context.reset();
        context.release_scratch();
        assert_eq!(context.retained_scratch_bytes(), 0);
        assert_eq!(context.limits(), &limits);
    }
    limits.max_reference_graph_depth = 2;
    let bounded = Decoder::new(limits);
    assert_limit(
        bounded
            .inspect_embedded_with_globals(&bytes, &globals)
            .unwrap_err(),
        LimitKind::ReferenceDepth,
        2,
        3,
    );
    assert_limit(
        bounded
            .decode_embedded_with_globals(&bytes, &globals)
            .unwrap_err(),
        LimitKind::ReferenceDepth,
        2,
        3,
    );
    assert_eq!(globals.segment_count(), 2);
}

#[test]
fn globals_retained_bytes_are_inclusive_and_rechecked_by_the_consuming_decoder() {
    let bytes = embedded(&[extension(0, 0, 0, &[])]);
    assert_eq!(bytes.len(), 15);
    let mut limits = Limits::default();
    limits.max_retained_state_bytes = 15;
    let decoder = Decoder::new(limits);
    let globals = decoder.parse_globals(&bytes).unwrap();
    assert_eq!(globals.segment_count(), 1);
    assert!(decoder
        .decode_embedded_with_globals(&[], &globals)
        .unwrap()
        .pages()
        .is_empty());
    limits.max_retained_state_bytes = 14;
    let bounded = Decoder::new(limits);
    assert_limit(
        bounded.parse_globals(&bytes).unwrap_err(),
        LimitKind::RetainedState,
        14,
        15,
    );
    assert_limit(
        bounded
            .decode_embedded_with_globals(&[], &globals)
            .unwrap_err(),
        LimitKind::RetainedState,
        14,
        15,
    );
    let mut context = bounded.context();
    assert_limit(
        context.parse_globals(&bytes).unwrap_err(),
        LimitKind::RetainedState,
        14,
        15,
    );
    assert_eq!(context.parse_globals(&[]).unwrap().segment_count(), 0);
    let associated = embedded(&[extension(7, 257, 0, &[])]);
    assert_malformed(
        context.parse_globals(&associated).unwrap_err(),
        &MalformedError::InvalidPageAssociation {
            segment: 7,
            page: 257,
        },
    );
}

#[test]
fn public_reference_limits_accept_exact_counts_and_reject_one_less() {
    let bytes = embedded(&[
        extension(0, 0, 0, &[]),
        extension(1, 0, 0, &[]).references(&[0], 3),
        extension(2, 0, 0, &[]).references(&[0, 1], 7),
    ]);
    let mut limits = Limits::default();
    limits.max_total_references = 3;
    limits.max_references_per_segment = 2;
    limits.max_reference_graph_depth = 3;
    assert_eq!(
        Decoder::new(limits)
            .parse_globals(&bytes)
            .unwrap()
            .segment_count(),
        3
    );
    limits.max_total_references = 2;
    assert_limit(
        Decoder::new(limits).parse_globals(&bytes).unwrap_err(),
        LimitKind::TotalReferences,
        2,
        3,
    );
    limits.max_total_references = 3;
    limits.max_references_per_segment = 1;
    assert_limit(
        Decoder::new(limits).parse_globals(&bytes).unwrap_err(),
        LimitKind::ReferencesPerSegment,
        1,
        2,
    );
    limits.max_references_per_segment = 2;
    limits.max_reference_graph_depth = 2;
    assert_limit(
        Decoder::new(limits).parse_globals(&bytes).unwrap_err(),
        LimitKind::ReferenceDepth,
        2,
        3,
    );
}

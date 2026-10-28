#![allow(clippy::duplicated_attributes)]
#![cfg(test)]

use super::pages::validate_page_profiles;
use super::parsing::{
    find_unknown_generic_length, parse_known_segment_data, parse_random_access, read_payload,
};
use super::references::{
    known_reference_type, validate_reference_depth, validate_reference_depth_ordered,
    validate_reference_links, validate_reference_types, validate_retention_flags,
};
use super::*;

fn raw_page_state_info(height: Option<u32>, striped: bool, maximum: u16) -> PageInformation {
    PageInformation {
        width: 0x1234_5678,
        height,
        x_resolution: 0x0102_0304,
        y_resolution: 0x89ab_cdef,
        eventually_lossless: true,
        might_contain_refinements: true,
        default_pixel: true,
        default_operator: crate::bitmap::CombinationOperator::Xor,
        requires_auxiliary_buffers: true,
        combination_operator_override: true,
        striped,
        max_stripe_size: maximum,
    }
}

fn raw_page_state_result<T>(result: Result<T, DecodeError>) -> Result<T, MalformedError> {
    result.map_err(|error| match error {
        DecodeError::Malformed(error) => error,
        other => panic!("unexpected PageState error: {other:?}"),
    })
}

#[test]
fn raw_page_state_new_preserves_all_information_and_initial_boundaries() {
    for height in [None, Some(0), Some(1), Some(u32::MAX)] {
        for striped in [false, true] {
            for maximum in [0, 1, 0x7fff, u16::MAX] {
                let info = raw_page_state_info(height, striped, maximum);
                let state = PageState::new(info);
                assert_eq!(state.info, info);
                assert_eq!(state.last_stripe, None);
                assert!(!state.ended);
            }
        }
    }
}

#[test]
fn raw_page_state_stripes_match_wide_boundaries_and_complete_failure_frames() {
    for height in [
        None,
        Some(0),
        Some(1),
        Some(2),
        Some(0x1_0000),
        Some(u32::MAX),
    ] {
        for striped in [false, true] {
            for maximum in [0, 1, 2, 0x7fff, u16::MAX] {
                for previous in [
                    None,
                    Some(0),
                    Some(1),
                    Some(0xffff),
                    Some(u32::MAX - 1),
                    Some(u32::MAX),
                ] {
                    let mut rows = vec![0, 1, 2, 0xffff, 0x1_0000, u32::MAX - 1, u32::MAX];
                    let origin = u64::from(previous.unwrap_or(0));
                    let boundary = origin + u64::from(maximum);
                    for candidate in [
                        origin.saturating_sub(1),
                        origin,
                        origin + 1,
                        boundary.saturating_sub(1),
                        boundary,
                        boundary + 1,
                    ] {
                        if let Ok(row) = u32::try_from(candidate) {
                            rows.push(row);
                        }
                    }
                    if let Some(value) = height {
                        rows.extend([value.saturating_sub(1), value, value.saturating_add(1)]);
                    }
                    for row in rows {
                        for ended in [false, true] {
                            let info = raw_page_state_info(height, striped, maximum);
                            let mut state = PageState {
                                info,
                                last_stripe: previous,
                                ended,
                            };
                            let distance = i64::from(row) - i64::from(previous.unwrap_or(0));
                            let accepted = striped
                                && (previous.is_none() || distance > 0)
                                && distance >= 0
                                && distance <= i64::from(maximum)
                                && height.is_none_or(|value| row < value);
                            let expected = if accepted {
                                Ok(())
                            } else {
                                Err(MalformedError::InvalidStripeEndRow(row))
                            };
                            assert_eq!(
                                raw_page_state_result(state.add_stripe(row)),
                                expected,
                                "{height:?} {striped} {maximum} {previous:?} {row} {ended}"
                            );
                            assert_eq!(state.info, info);
                            assert_eq!(state.ended, ended);
                            assert_eq!(
                                state.last_stripe,
                                if accepted { Some(row) } else { previous }
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn raw_page_state_finish_preserves_frames_and_duplicate_error_priority() {
    for height in [None, Some(0), Some(1), Some(u32::MAX)] {
        for last_stripe in [None, Some(0), Some(1), Some(u32::MAX)] {
            for ended in [false, true] {
                for striped in [false, true] {
                    let info = raw_page_state_info(height, striped, 0);
                    let mut state = PageState {
                        info,
                        last_stripe,
                        ended,
                    };
                    let expected = if ended {
                        Err(MalformedError::DuplicatePageBoundary {
                            page: 0,
                            segment_type: 49,
                        })
                    } else if height.is_none() && last_stripe.is_none() {
                        Err(MalformedError::InvalidStripeEndRow(0))
                    } else {
                        Ok(())
                    };
                    assert_eq!(raw_page_state_result(state.finish()), expected);
                    assert_eq!(state.info, info);
                    assert_eq!(state.last_stripe, last_stripe);
                    assert_eq!(state.ended, ended || expected.is_ok());
                    if expected.is_ok() {
                        assert_eq!(
                            raw_page_state_result(state.finish()),
                            Err(MalformedError::DuplicatePageBoundary {
                                page: 0,
                                segment_type: 49
                            },)
                        );
                        assert_eq!(state.info, info);
                        assert_eq!(state.last_stripe, last_stripe);
                        assert!(state.ended);
                    }
                }
            }
        }
    }
}

#[test]
fn raw_page_state_public_metadata_matches_known_and_wide_inferred_heights() {
    for height in [None, Some(0), Some(1), Some(0x1234_5678), Some(u32::MAX)] {
        for last_stripe in [None, Some(0), Some(1), Some(u32::MAX - 1), Some(u32::MAX)] {
            for ended in [false, true] {
                for striped in [false, true] {
                    for number in [0, 1, 0x0102_0304, u32::MAX] {
                        let info = raw_page_state_info(height, striped, u16::MAX);
                        let state = PageState {
                            info,
                            last_stripe,
                            ended,
                        };
                        let expected_height = match (height, last_stripe) {
                            (Some(value), _) => Ok(value),
                            (None, Some(row)) => u32::try_from(u64::from(row) + 1)
                                .map_err(|_| MalformedError::IntegerOverflow),
                            (None, None) => Err(MalformedError::InvalidStripeEndRow(0)),
                        };
                        assert_eq!(
                            raw_page_state_result(state.into_public(number)),
                            expected_height.map(|value| PageInfo {
                                number,
                                width: info.width,
                                height: value,
                                x_resolution: info.x_resolution,
                                y_resolution: info.y_resolution,
                                striped,
                            })
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn raw_known_segment_dispatch_preserves_page_stripe_profile_variants_and_values() {
    let limits = Limits::default();
    let mut header = reference_validation_segment(0, &[], 1).header;
    let mut page = page_data(23);
    page[8..12].copy_from_slice(&0x0123_4567_u32.to_be_bytes());
    page[12..16].copy_from_slice(&0x89ab_cdef_u32.to_be_bytes());
    header.segment_type = SegmentType::PageInformation;
    match parse_known_segment_data(&header, &page, &limits).unwrap() {
        ParsedData::Page(info) => {
            assert_eq!(info.width, 8);
            assert_eq!(info.height, Some(23));
            assert_eq!(info.x_resolution, 0x0123_4567);
            assert_eq!(info.y_resolution, 0x89ab_cdef);
        }
        other => panic!("expected literal page metadata, got {other:?}"),
    }
    header.segment_type = SegmentType::EndOfStripe;
    assert!(matches!(
        parse_known_segment_data(&header, &0x0123_4567_u32.to_be_bytes(), &limits).unwrap(),
        ParsedData::EndOfStripe(0x0123_4567)
    ));
    header.segment_type = SegmentType::Profiles;
    let mut profiles = 3_u32.to_be_bytes().to_vec();
    for value in [0, u32::MAX, 0x0123_4567_u32] {
        profiles.extend_from_slice(&value.to_be_bytes());
    }
    match parse_known_segment_data(&header, &profiles, &limits).unwrap() {
        ParsedData::Profiles(values) => assert_eq!(values, [0, u32::MAX, 0x0123_4567]),
        other => panic!("expected literal profile metadata, got {other:?}"),
    }
    for kind in [SegmentType::EndOfPage, SegmentType::EndOfFile] {
        header.segment_type = kind;
        assert!(matches!(
            parse_known_segment_data(&header, &[], &limits).unwrap(),
            ParsedData::Other
        ));
        assert!(parse_known_segment_data(&header, &[1], &limits).is_err());
    }
    header.segment_type = SegmentType::Extension;
    assert!(matches!(
        parse_known_segment_data(&header, &0x100_u32.to_be_bytes(), &limits).unwrap(),
        ParsedData::Other
    ));
}

#[test]
fn raw_page_metadata_survives_sequential_random_and_embedded_container_composition() {
    let limits = Limits::default();
    let sequential = valid_sequential_document();
    let body = &sequential[13..];
    let mut random = SIGNATURE.to_vec();
    random.push(0);
    random.extend_from_slice(&1_u32.to_be_bytes());
    random.extend(header(0, 48, 1, 19));
    random.extend(header(1, 49, 1, 0));
    random.extend(header(2, 51, 0, 0));
    random.extend_from_slice(&page_data(8));
    let mut collections = vec![
        parse_sequential(body, &limits, true).unwrap(),
        parse_random_access(&random[13..], &limits).unwrap(),
    ];
    collections.push(parse_standalone_document(&sequential, &limits).unwrap().1);
    collections.push(parse_standalone_document(&random, &limits).unwrap().1);
    collections.push(parse_embedded_document(body, &limits, None).unwrap().1);
    for segments in collections {
        assert_eq!(segments.len(), 3);
        match &segments[0].data {
            ParsedData::Page(info) => {
                assert_eq!(info.width, 8);
                assert_eq!(info.height, Some(8));
                assert_eq!(info.x_resolution, 0);
                assert_eq!(info.y_resolution, 0);
            }
            other => panic!("expected original page metadata, got {other:?}"),
        }
        assert!(matches!(segments[1].data, ParsedData::Other));
        assert!(matches!(segments[2].data, ParsedData::Other));
    }
}

type LiteralPayloadWindow = (u32, Vec<u8>, usize, usize);

fn raw_extension_windows(numbers: &[u32], eof: bool) -> (Vec<u8>, Vec<LiteralPayloadWindow>) {
    let mut input = Vec::new();
    let mut expected = Vec::new();
    for (index, number) in numbers.iter().enumerate() {
        let mut payload = 0x100_u32.to_be_bytes().to_vec();
        payload.extend(std::iter::repeat_n(
            u8::try_from(index + 1).unwrap(),
            index + 1,
        ));
        input.extend(header(
            *number,
            62,
            0,
            u32::try_from(payload.len()).unwrap(),
        ));
        let start = input.len();
        input.extend_from_slice(&payload);
        expected.push((*number, payload, start, input.len()));
    }
    if eof {
        input.extend(header(43, 51, 0, 0));
        expected.push((43, Vec::new(), input.len(), input.len()));
    }
    (input, expected)
}

fn raw_random_extension_windows(numbers: &[u32]) -> (Vec<u8>, Vec<LiteralPayloadWindow>) {
    let (_, mut expected) = raw_extension_windows(numbers, false);
    let eof_number = numbers
        .last()
        .map_or(0, |number| number.checked_add(1).unwrap());
    expected.push((eof_number, Vec::new(), 0, 0));
    let mut input = Vec::new();
    for (index, (number, payload, _, _)) in expected.iter().enumerate() {
        input.extend(header(
            *number,
            if index < numbers.len() { 62 } else { 51 },
            0,
            u32::try_from(payload.len()).unwrap(),
        ));
    }
    for (_, payload, start, end) in &mut expected {
        *start = input.len();
        input.extend_from_slice(payload);
        *end = input.len();
    }
    (input, expected)
}

#[test]
fn raw_payload_windows_keep_exact_known_and_existing_unknown_extents() {
    for length in 0..=32_u32 {
        let mut header = reference_validation_segment(100, &[], 1).header;
        header.data_length = Some(length);
        let mut input = vec![0x11, 0x22, 0x33];
        input.extend((0..length).map(|index| u8::try_from(index * 7 % 256).unwrap()));
        input.extend_from_slice(&[0xaa, 0x55]);
        let mut reader = ByteReader::new(&input);
        reader.read_bytes(3).unwrap();
        let payload = read_payload(&mut reader, &header).unwrap();
        assert_eq!(payload, &input[3..3 + usize::try_from(length).unwrap()]);
        assert_eq!(reader.position(), 3 + usize::try_from(length).unwrap());
        assert_eq!(reader.remaining_bytes(), &[0xaa, 0x55]);
    }
    let mut existing = vec![0_u8; 18];
    existing[4..8].copy_from_slice(&3_u32.to_be_bytes());
    existing.extend_from_slice(&[0x11, 0x22, 0xff, 0xac]);
    existing.extend_from_slice(&3_u32.to_be_bytes());
    existing.extend_from_slice(&[0xaa, 0xbb]);
    let mut input = vec![0x11, 0x22, 0x33];
    input.extend_from_slice(&existing);
    let mut header = reference_validation_segment(100, &[], 1).header;
    header.segment_type = SegmentType::ImmediateGenericRegion;
    header.data_length = None;
    let mut reader = ByteReader::new(&input);
    reader.read_bytes(3).unwrap();
    assert_eq!(read_payload(&mut reader, &header).unwrap(), &existing[..26]);
    assert_eq!(reader.position(), 29);
    assert_eq!(reader.remaining_bytes(), &[0xaa, 0xbb]);
}

#[test]
fn raw_sequential_and_embedded_layouts_match_literal_windows_and_order_policy() {
    let limits = Limits::default();
    for numbers in [&[2_u32, 7, 42][..], &[42_u32, 7, 2][..]] {
        for eof in [false, true] {
            let (input, expected) = raw_extension_windows(numbers, eof);
            for ordered in [false, true] {
                let result = parse_sequential(&input, &limits, ordered);
                if ordered && numbers[0] > numbers[1] {
                    assert!(matches!(
                        result,
                        Err(DecodeError::Malformed(
                            MalformedError::NonIncreasingSegmentNumber {
                                previous: 42,
                                current: 7
                            }
                        ))
                    ));
                    continue;
                }
                let segments = result.unwrap();
                assert_eq!(segments.len(), expected.len());
                for (index, (segment, (number, payload, start, end))) in
                    segments.iter().zip(&expected).enumerate()
                {
                    assert_eq!(segment.header.number, *number);
                    assert_eq!(
                        segment.header.segment_type,
                        if index < numbers.len() {
                            SegmentType::Extension
                        } else {
                            SegmentType::EndOfFile
                        }
                    );
                    assert_eq!(segment.header.byte_length, 11);
                    assert_eq!(
                        segment.header.data_length,
                        Some(u32::try_from(payload.len()).unwrap())
                    );
                    assert_eq!(segment.payload, payload);
                    assert_eq!(segment.payload, &input[*start..*end]);
                }
            }
            let (info, embedded) = parse_embedded_document(&input, &limits, None).unwrap();
            assert_eq!(info.segment_count(), u32::try_from(expected.len()).unwrap());
            assert!(info.pages().is_empty());
            for (segment, (number, payload, _, _)) in embedded.iter().zip(&expected) {
                assert_eq!(segment.header.number, *number);
                assert_eq!(segment.payload, payload);
            }
            if eof {
                let mut trailing = input.clone();
                trailing.push(0xaa);
                assert!(matches!(
                    parse_sequential(&trailing, &limits, false),
                    Err(DecodeError::Malformed(MalformedError::TrailingData(1)))
                ));
            }
        }
    }
}

#[test]
fn standalone_sequential_body_origin_is_exact_and_unordered_headers_are_rejected() {
    let limits = Limits::default();
    let (body, expected) = raw_extension_windows(&[2, 7, 42], true);
    for known_count in [false, true] {
        let mut input = SIGNATURE.to_vec();
        input.push(if known_count { 1 } else { 3 });
        if known_count {
            input.extend_from_slice(&0_u32.to_be_bytes());
        }
        let body_start = input.len();
        input.extend_from_slice(&body);
        let (info, segments) = parse_standalone_document(&input, &limits).unwrap();
        assert_eq!(info.segment_count(), 4);
        assert!(info.pages().is_empty());
        for (segment, (number, payload, start, end)) in segments.iter().zip(&expected) {
            assert_eq!(segment.header.number, *number);
            assert_eq!(segment.payload, payload);
            assert_eq!(
                segment.payload,
                &input[body_start + *start..body_start + *end]
            );
        }
    }
    let (unordered, _) = raw_extension_windows(&[42, 7, 2], true);
    let mut input = SIGNATURE.to_vec();
    input.push(3);
    input.extend_from_slice(&unordered);
    assert!(matches!(
        parse_standalone_document(&input, &limits),
        Err(DecodeError::Malformed(
            MalformedError::NonIncreasingSegmentNumber {
                previous: 42,
                current: 7,
            }
        ))
    ));
}

#[test]
fn raw_random_access_layout_matches_separate_literal_headers_and_payloads() {
    let limits = Limits::default();
    for numbers in [
        &[][..],
        &[0][..],
        &[2, 7, 42][..],
        &[0, 255, 65_535, 16_777_215][..],
    ] {
        let (input, expected) = raw_random_extension_windows(numbers);
        let segments = parse_random_access(&input, &limits).unwrap();
        assert_eq!(segments.len(), expected.len());
        assert_eq!(expected[0].2, expected.len() * 11);
        for (index, (segment, (number, payload, start, end))) in
            segments.iter().zip(&expected).enumerate()
        {
            assert_eq!(segment.header.number, *number);
            assert_eq!(
                segment.header.segment_type,
                if index < numbers.len() {
                    SegmentType::Extension
                } else {
                    SegmentType::EndOfFile
                }
            );
            assert_eq!(segment.header.byte_length, 11);
            assert_eq!(
                segment.header.data_length,
                Some(u32::try_from(payload.len()).unwrap())
            );
            assert_eq!(segment.header.page_association, 0);
            assert!(segment.header.referred_segments.is_empty());
            assert_eq!(segment.payload, payload);
            assert_eq!(segment.payload, &input[*start..*end]);
        }
        assert_eq!(expected.last().unwrap().3, input.len());
    }
}

#[test]
fn standalone_random_access_origins_order_eof_and_trailing_bytes_match_raw_metadata() {
    let limits = Limits::default();
    let (body, expected) = raw_random_extension_windows(&[2, 7, 42]);
    for known_count in [false, true] {
        let mut input = SIGNATURE.to_vec();
        input.push(if known_count { 0 } else { 2 });
        if known_count {
            input.extend_from_slice(&0_u32.to_be_bytes());
        }
        let body_start = input.len();
        assert_eq!(body_start, if known_count { 13 } else { 9 });
        input.extend_from_slice(&body);
        let (info, segments) = parse_standalone_document(&input, &limits).unwrap();
        assert_eq!(info.organization(), Organization::RandomAccess);
        assert_eq!(info.segment_count(), 4);
        assert!(info.pages().is_empty());
        assert_eq!(segments.len(), expected.len());
        for (segment, (number, payload, start, end)) in segments.iter().zip(&expected) {
            assert_eq!(segment.header.number, *number);
            assert_eq!(segment.payload, payload);
            assert_eq!(
                segment.payload,
                &input[body_start + *start..body_start + *end]
            );
        }
    }
    for numbers in [&[42, 7, 2][..], &[2, 2][..]] {
        let (unordered, _) = raw_random_extension_windows(numbers);
        assert!(matches!(
            parse_random_access(&unordered, &limits),
            Err(DecodeError::Malformed(
                MalformedError::NonIncreasingSegmentNumber { previous, current }
            )) if previous == numbers[0] && current == numbers[1]
        ));
    }
    for missing in [Vec::new(), header(2, 62, 0, 4)] {
        assert!(matches!(
            parse_random_access(&missing, &limits),
            Err(DecodeError::Malformed(MalformedError::MissingEndOfFile))
        ));
    }
    let mut trailing = body;
    trailing.push(0xaa);
    assert!(matches!(
        parse_random_access(&trailing, &limits),
        Err(DecodeError::Malformed(MalformedError::TrailingData(1)))
    ));
    let mut early_eof = header(0, 51, 0, 0);
    early_eof.extend(header(1, 51, 0, 0));
    assert!(matches!(
        parse_random_access(&early_eof, &limits),
        Err(DecodeError::Malformed(MalformedError::TrailingData(11)))
    ));
}

#[test]
fn raw_structural_work_matches_wide_counts_and_exact_rejection_priority() {
    for count in 0..=2 {
        for references in [&[][..], &[0][..], &[0, 1][..]] {
            let segments: Vec<_> = (0..count)
                .map(|index| reference_validation_segment(index + 2, references, 1))
                .collect();
            let headers: Vec<_> = segments
                .iter()
                .map(|segment| segment.header.clone())
                .collect();
            for input in [
                0,
                1,
                8,
                usize::MAX / 2,
                usize::MAX - 3,
                usize::MAX - 1,
                usize::MAX,
            ] {
                let expected = input as u128
                    + u128::from(count)
                    + u128::from(count) * references.len() as u128;
                for limit in [0, 1, 8, u64::MAX] {
                    let limits = Limits {
                        max_work_units: limit,
                        ..Limits::default()
                    };
                    let result = enforce_work_limit(input, &segments, &limits);
                    if expected > u128::from(u64::MAX) {
                        assert!(matches!(
                            result,
                            Err(DecodeError::Malformed(MalformedError::IntegerOverflow))
                        ));
                    } else if expected > u128::from(limit) {
                        assert!(matches!(result, Err(DecodeError::LimitExceeded(
                            LimitExceeded { kind: LimitKind::Work, limit: actual_limit, actual }
                        )) if actual_limit == limit && u128::from(actual) == expected));
                    } else {
                        result.unwrap();
                    }
                    assert_eq!(
                        segments
                            .iter()
                            .map(|s| s.header.clone())
                            .collect::<Vec<_>>(),
                        headers
                    );
                }
            }
        }
    }
    for count in [0, 1, usize::MAX / 8, usize::MAX / 8 + 1, usize::MAX] {
        let bytes = count as u128 * 8;
        match allocation_size::<u64>(count) {
            Ok(actual) => assert_eq!(u128::from(actual), bytes),
            Err(MalformedError::IntegerOverflow) => {
                assert!(bytes > usize::MAX as u128 || bytes > u128::from(u64::MAX));
            }
            other => panic!("unexpected allocation arithmetic: {other:?}"),
        }
        assert_eq!(allocation_size::<()>(count).unwrap(), 0);
    }
}

#[test]
fn raw_profile_and_retention_metadata_rejects_inconsistent_tags_and_frames() {
    let mut global = reference_validation_segment(1, &[], 1);
    global.header.segment_type = SegmentType::Profiles;
    global.data = ParsedData::Other;
    assert!(matches!(
        validate_profiles(&[global]),
        Err(DecodeError::Malformed(
            MalformedError::InvalidProfilesSegment(1)
        ))
    ));
    let mut page = reference_validation_segment(2, &[], 1);
    page.header.segment_type = SegmentType::Profiles;
    page.header.page_association = 1;
    let mut profiles = Vec::new();
    assert!(matches!(
        validate_page_profiles(&[page], 0, &[17], &mut profiles),
        Err(DecodeError::Malformed(
            MalformedError::InvalidProfilesSegment(2)
        ))
    ));
    assert_eq!(profiles, vec![(1, 2)]);
    for number in [0, 1, u32::MAX] {
        for retained in [false, true] {
            for last in [number.saturating_sub(1), number, number.saturating_add(1)] {
                let mut segment = reference_validation_segment(number, &[], u8::from(retained));
                let before = segment.header.clone();
                let result =
                    validate_retention_flags(std::slice::from_ref(&segment), &[(number, last)]);
                if retained && last > number {
                    result.unwrap();
                } else {
                    assert!(matches!(result, Err(DecodeError::Malformed(
                        MalformedError::InvalidRetentionFlags { segment: owner, referred }
                    )) if owner == number && referred == number));
                }
                assert_eq!(segment.header, before);
                segment.header.retention_flags.values.clear();
                assert!(matches!(validate_retention_flags(&[segment], &[]),
                    Err(DecodeError::Malformed(MalformedError::InvalidRetentionFlags {
                        segment: owner, referred,
                    })) if owner == number && referred == number));
            }
        }
    }
}

fn quota_view(result: Result<(), DecodeError>) -> Result<Option<LimitExceeded>, MalformedError> {
    match result {
        Ok(()) => Ok(None),
        Err(DecodeError::LimitExceeded(limit)) => Ok(Some(limit)),
        Err(DecodeError::Malformed(error)) => Err(error),
        Err(error) => panic!("unexpected preflight error: {error:?}"),
    }
}

fn wide_quota(
    actual: u128,
    maximum: u128,
    limit: u64,
    kind: LimitKind,
) -> Result<Option<LimitExceeded>, MalformedError> {
    if actual > maximum {
        Err(MalformedError::IntegerOverflow)
    } else if actual > u128::from(limit) {
        Ok(Some(LimitExceeded {
            kind,
            limit,
            actual: u64::try_from(actual).unwrap(),
        }))
    } else {
        Ok(None)
    }
}

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

const SIGNATURE: [u8; 8] = [0x97, 0x4A, 0x42, 0x32, 0x0D, 0x0A, 0x1A, 0x0A];

fn header(number: u32, segment_type: u8, page: u8, data_length: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&number.to_be_bytes());
    bytes.push(segment_type);
    bytes.push(0);
    bytes.push(page);
    bytes.extend_from_slice(&data_length.to_be_bytes());
    bytes
}

fn header_with_reference(number: u32, segment_type: u8, referred: u8) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&number.to_be_bytes());
    bytes.push(segment_type);
    bytes.push(0x21);
    bytes.push(referred);
    bytes.push(0);
    bytes.extend_from_slice(&0_u32.to_be_bytes());
    bytes
}

fn reference_validation_segment(
    number: u32,
    references: &[u32],
    retention: u8,
) -> ParsedSegment<'static> {
    assert!(references.len() <= 2);
    let mut bytes = header(number, 0, 0, 0);
    bytes[5] = (u8::try_from(references.len()).unwrap() << 5) | retention;
    for (index, referred) in references.iter().enumerate() {
        bytes.insert(6 + index, u8::try_from(*referred).unwrap());
    }
    let mut reader = crate::reader::ByteReader::new(&bytes);
    let header =
        crate::segment_header::parse_segment_header(&mut reader, &Limits::default()).unwrap();
    assert_eq!(reader.position(), bytes.len());
    ParsedSegment {
        header,
        payload: &[],
        data: ParsedData::Other,
    }
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

fn page_data(height: u32) -> [u8; 19] {
    let mut data = [0_u8; 19];
    data[0..4].copy_from_slice(&8_u32.to_be_bytes());
    data[4..8].copy_from_slice(&height.to_be_bytes());
    data
}

fn direct_page_info(height: Option<u32>, striped: bool, maximum: u16) -> PageInformation {
    let mut info = parse_page_information(&page_data(4), &Limits::default()).unwrap();
    info.width = 13;
    info.height = height;
    info.x_resolution = 123;
    info.y_resolution = 456;
    info.striped = striped;
    info.max_stripe_size = maximum;
    info
}

#[test]
fn page_stripe_boundaries_match_wide_policy_and_preserve_rejected_state() {
    for height in [None, Some(1), Some(4)] {
        for striped in [false, true] {
            for maximum in [0, 1, 3, u16::MAX] {
                let info = direct_page_info(height, striped, maximum);
                for previous in [None, Some(0), Some(2), Some(u32::MAX - 1), Some(u32::MAX)] {
                    for row in [0, 1, 2, 3, u32::from(u16::MAX), u32::MAX] {
                        let mut state = PageState::new(info);
                        state.last_stripe = previous;
                        let progress = match previous {
                            Some(before) => {
                                row > before
                                    && u64::from(row) - u64::from(before) <= u64::from(maximum)
                            }
                            None => row <= u32::from(maximum),
                        };
                        let valid = striped && progress && height.is_none_or(|height| row < height);
                        match state.add_stripe(row) {
                            Ok(()) => {
                                assert!(valid);
                                assert_eq!(state.last_stripe, Some(row));
                            }
                            Err(DecodeError::Malformed(MalformedError::InvalidStripeEndRow(
                                actual,
                            ))) => {
                                assert!(!valid);
                                assert_eq!(actual, row);
                                assert_eq!(state.last_stripe, previous);
                            }
                            other => panic!("unexpected stripe result: {other:?}"),
                        }
                        assert_eq!(state.info, info);
                        assert!(!state.ended);
                    }
                }
            }
        }
    }
}

#[test]
fn page_finish_and_public_metadata_keep_exact_error_precedence_and_fields() {
    for height in [None, Some(1), Some(4)] {
        let info = direct_page_info(height, true, 3);
        for last in [None, Some(0), Some(3), Some(u32::MAX)] {
            for ended in [false, true] {
                let mut state = PageState::new(info);
                state.last_stripe = last;
                state.ended = ended;
                match state.finish() {
                    Err(DecodeError::Malformed(MalformedError::DuplicatePageBoundary {
                        page,
                        segment_type,
                    })) => {
                        assert!(ended);
                        assert_eq!((page, segment_type), (0, 49));
                    }
                    Err(DecodeError::Malformed(MalformedError::InvalidStripeEndRow(row))) => {
                        assert!(!ended && height.is_none() && last.is_none());
                        assert_eq!(row, 0);
                    }
                    Ok(()) => {
                        assert!(!ended && (height.is_some() || last.is_some()));
                        assert!(state.ended);
                    }
                    other => panic!("unexpected page finish result: {other:?}"),
                }
                assert_eq!(state.info, info);
                assert_eq!(state.last_stripe, last);
                if ended || (height.is_none() && last.is_none()) {
                    assert_eq!(state.ended, ended);
                }
            }
            let mut state = PageState::new(info);
            state.last_stripe = last;
            match state.into_public(u32::MAX) {
                Ok(page) => {
                    let expected = height
                        .map(u64::from)
                        .or_else(|| last.map(|row| u64::from(row) + 1))
                        .unwrap();
                    assert!(u32::try_from(expected).is_ok());
                    assert_eq!(
                        (page.number, page.width, u64::from(page.height)),
                        (u32::MAX, 13, expected)
                    );
                    assert_eq!(
                        (page.x_resolution, page.y_resolution, page.striped),
                        (123, 456, true)
                    );
                }
                Err(DecodeError::Malformed(MalformedError::IntegerOverflow)) => {
                    assert!(height.is_none() && last == Some(u32::MAX));
                }
                Err(DecodeError::Malformed(MalformedError::InvalidStripeEndRow(row))) => {
                    assert!(height.is_none() && last.is_none());
                    assert_eq!(row, 0);
                }
                other => panic!("unexpected page metadata result: {other:?}"),
            }
        }
    }
}

#[test]
fn inspects_sequential_document() {
    let bytes = valid_sequential_document();

    let info = Decoder::default()
        .inspect(&bytes)
        .expect("document is valid");
    assert_eq!(info.segment_count(), 3);
    assert_eq!(info.pages().len(), 1);
    assert_eq!(info.pages()[0].width(), 8);
    assert_eq!(info.pages()[0].height(), 8);
}

#[test]
fn inspects_random_access_document() {
    let mut bytes = SIGNATURE.to_vec();
    bytes.push(0x00);
    bytes.extend_from_slice(&1_u32.to_be_bytes());
    bytes.extend_from_slice(&header(0, 48, 1, 19));
    bytes.extend_from_slice(&header(1, 49, 1, 0));
    bytes.extend_from_slice(&header(2, 51, 0, 0));
    bytes.extend_from_slice(&page_data(8));

    let info = Decoder::default()
        .inspect(&bytes)
        .expect("document is valid");
    assert_eq!(info.organization(), Organization::RandomAccess);
    assert_eq!(info.segment_count(), 3);
}

#[test]
fn finalizes_unknown_height_from_stripe() {
    let mut bytes = SIGNATURE.to_vec();
    bytes.push(0x03);
    bytes.extend_from_slice(&header(0, 48, 1, 19));
    let mut page = page_data(u32::MAX);
    page[17..19].copy_from_slice(&0x800A_u16.to_be_bytes());
    bytes.extend_from_slice(&page);
    bytes.extend_from_slice(&header(1, 50, 1, 4));
    bytes.extend_from_slice(&7_u32.to_be_bytes());
    bytes.extend_from_slice(&header(2, 49, 1, 0));

    let info = Decoder::default()
        .inspect(&bytes)
        .expect("document is valid");
    assert_eq!(info.pages()[0].height(), 8);
    assert!(info.pages()[0].is_striped());
}

#[test]
fn rejects_page_count_mismatch() {
    let mut bytes = SIGNATURE.to_vec();
    bytes.push(0x01);
    bytes.extend_from_slice(&2_u32.to_be_bytes());
    bytes.extend_from_slice(&header(0, 48, 1, 19));
    bytes.extend_from_slice(&page_data(8));
    bytes.extend_from_slice(&header(1, 49, 1, 0));
    assert!(matches!(
        Decoder::default().inspect(&bytes),
        Err(DecodeError::Malformed(MalformedError::PageCountMismatch {
            declared: 2,
            actual: 1
        }))
    ));
}

#[test]
fn bounded_reader_enforces_stream_limit() {
    let limits = Limits {
        max_stream_bytes: 4,
        ..Limits::default()
    };
    let mut input = std::io::Cursor::new([0_u8; 5]);
    assert!(matches!(
        Decoder::new(limits).inspect_reader(&mut input),
        Err(DecodeError::LimitExceeded(LimitExceeded {
            kind: LimitKind::StreamBytes,
            limit: 4,
            actual: 5
        }))
    ));
}

#[test]
fn inspection_enforces_work_limit() {
    let bytes = valid_sequential_document();
    let limits = Limits {
        max_work_units: 10,
        ..Limits::default()
    };
    assert!(matches!(
        Decoder::new(limits).inspect(&bytes),
        Err(DecodeError::LimitExceeded(LimitExceeded {
            kind: LimitKind::Work,
            limit: 10,
            ..
        }))
    ));
}

#[test]
fn locates_unknown_length_generic_terminator() {
    let mut data = vec![0_u8; 18];
    data[4..8].copy_from_slice(&3_u32.to_be_bytes());
    data[17] = 0;
    data.extend_from_slice(&[0x11, 0x22, 0xFF, 0xAC]);
    data.extend_from_slice(&3_u32.to_be_bytes());
    data.extend_from_slice(&[0xAA, 0xBB]);
    assert_eq!(
        find_unknown_generic_length(&data).expect("terminator is present"),
        26
    );
}

#[test]
fn rejects_every_byte_truncation_of_minimal_document() {
    let bytes = valid_sequential_document();
    let valid_without_optional_eof = bytes.len() - header(2, 51, 0, 0).len();
    for length in 0..bytes.len() {
        if Decoder::default().inspect(&bytes[..length]).is_ok() {
            assert_eq!(
                length, valid_without_optional_eof,
                "truncation at byte {length} unexpectedly succeeded"
            );
        }
    }
}

#[test]
fn document_inspection_is_deterministic() {
    let bytes = valid_sequential_document();
    let first = Decoder::default()
        .inspect(&bytes)
        .expect("document is valid");
    let second = Decoder::default()
        .inspect(&bytes)
        .expect("document is valid");
    assert_eq!(first, second);
}

#[test]
fn enforces_total_reference_and_depth_limits() {
    let mut bytes = SIGNATURE.to_vec();
    bytes.push(0x01);
    bytes.extend_from_slice(&0_u32.to_be_bytes());
    bytes.extend_from_slice(&header(0, 0, 0, 0));
    let retention_index = SIGNATURE.len() + 1 + 4 + 5;
    bytes[retention_index] = 1;
    bytes.extend_from_slice(&header_with_reference(1, 0, 0));
    bytes.extend_from_slice(&header_with_reference(2, 0, 1));
    bytes.extend_from_slice(&header(3, 51, 0, 0));

    let depth_limits = Limits {
        max_reference_graph_depth: 2,
        ..Limits::default()
    };
    assert!(matches!(
        Decoder::new(depth_limits).inspect(&bytes),
        Err(DecodeError::LimitExceeded(LimitExceeded {
            kind: LimitKind::ReferenceDepth,
            limit: 2,
            actual: 3
        }))
    ));

    let reference_limits = Limits {
        max_total_references: 1,
        ..Limits::default()
    };
    assert!(matches!(
        Decoder::new(reference_limits).inspect(&bytes),
        Err(DecodeError::LimitExceeded(LimitExceeded {
            kind: LimitKind::TotalReferences,
            limit: 1,
            actual: 2
        }))
    ));
}

#[test]
fn rejects_reference_to_nonretained_segment() {
    let mut bytes = SIGNATURE.to_vec();
    bytes.push(0x01);
    bytes.extend_from_slice(&0_u32.to_be_bytes());
    bytes.extend_from_slice(&header(0, 0, 0, 0));
    bytes.extend_from_slice(&header_with_reference(1, 0, 0));
    assert!(matches!(
        Decoder::default().inspect(&bytes),
        Err(DecodeError::Malformed(
            MalformedError::InvalidRetentionFlags {
                segment: 0,
                referred: 0
            }
        ))
    ));
}

#[test]
fn embedded_page_resolves_reusable_globals_explicitly() {
    let global_bytes = header(0, 0, 0, 0);
    let globals = Decoder::default()
        .parse_globals(&global_bytes)
        .expect("global dictionary is valid");
    assert_eq!(globals.segment_count(), 1);

    let mut page_bytes = header(1, 48, 1, 19);
    page_bytes.extend_from_slice(&page_data(8));
    let mut text_header = header_with_reference(2, 6, 0);
    text_header[7] = 1;
    page_bytes.extend_from_slice(&text_header);
    page_bytes.extend_from_slice(&header(3, 49, 1, 0));

    assert!(matches!(
        Decoder::default().inspect_embedded(&page_bytes),
        Err(DecodeError::Malformed(
            MalformedError::MissingReferredSegment {
                segment: 2,
                referred: 0
            }
        ))
    ));
    let info = Decoder::default()
        .inspect_embedded_with_globals(&page_bytes, &globals)
        .expect("page resolves its global dictionary");
    assert_eq!(info.pages().len(), 1);
}

fn valid_sequential_document() -> Vec<u8> {
    let mut bytes = SIGNATURE.to_vec();
    bytes.push(0x01);
    bytes.extend_from_slice(&1_u32.to_be_bytes());
    bytes.extend_from_slice(&header(0, 48, 1, 19));
    bytes.extend_from_slice(&page_data(8));
    bytes.extend_from_slice(&header(1, 49, 1, 0));
    bytes.extend_from_slice(&header(2, 51, 0, 0));
    bytes
}

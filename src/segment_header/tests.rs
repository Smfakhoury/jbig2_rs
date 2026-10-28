#![cfg(test)]

use super::*;

#[test]
fn recognized_segment_type_equality_matches_all_literal_codes_and_eof() {
    let codes = [
        0, 4, 6, 7, 16, 20, 22, 23, 36, 38, 39, 40, 42, 43, 48, 49, 50, 51, 52, 53, 54, 62,
    ];
    for left in codes {
        let left_type = SegmentType::from_number(left).unwrap();
        assert_eq!(left_type == SegmentType::EndOfFile, left == 51);
        assert_eq!(matches!(left_type, SegmentType::EndOfFile), left == 51);
        for right in codes {
            let right_type = SegmentType::from_number(right).unwrap();
            assert_eq!(left_type == right_type, left == right);
            assert_eq!(left_type != right_type, left != right);
        }
    }
}

fn parse(input: &[u8]) -> Result<SegmentHeader, DecodeError> {
    parse_segment_header(&mut ByteReader::new(input), &Limits::default())
}

const DEFINED_TYPES: [(u8, SegmentType); 22] = [
    (0, SegmentType::SymbolDictionary),
    (4, SegmentType::IntermediateTextRegion),
    (6, SegmentType::ImmediateTextRegion),
    (7, SegmentType::ImmediateLosslessTextRegion),
    (16, SegmentType::PatternDictionary),
    (20, SegmentType::IntermediateHalftoneRegion),
    (22, SegmentType::ImmediateHalftoneRegion),
    (23, SegmentType::ImmediateLosslessHalftoneRegion),
    (36, SegmentType::IntermediateGenericRegion),
    (38, SegmentType::ImmediateGenericRegion),
    (39, SegmentType::ImmediateLosslessGenericRegion),
    (40, SegmentType::IntermediateGenericRefinementRegion),
    (42, SegmentType::ImmediateGenericRefinementRegion),
    (43, SegmentType::ImmediateLosslessGenericRefinementRegion),
    (48, SegmentType::PageInformation),
    (49, SegmentType::EndOfPage),
    (50, SegmentType::EndOfStripe),
    (51, SegmentType::EndOfFile),
    (52, SegmentType::Profiles),
    (53, SegmentType::Tables),
    (54, SegmentType::ColourPalette),
    (62, SegmentType::Extension),
];

fn generated_header(
    number: u32,
    flags: u8,
    page: u32,
    references: &[u32],
    retain: &[bool],
    length: u32,
) -> Vec<u8> {
    assert_eq!(retain.len(), references.len() + 1);
    let count = u32::try_from(references.len()).unwrap();
    let mut input = number.to_be_bytes().to_vec();
    input.push(flags);
    if count <= 4 {
        let bits = retain
            .iter()
            .enumerate()
            .fold(0_u8, |bits, (index, value)| {
                bits | (u8::from(*value) << index)
            });
        input.push((u8::try_from(count).unwrap() << 5) | bits);
    } else {
        input.extend_from_slice(&(0xe000_0000 | count).to_be_bytes());
        let mut bytes = vec![0_u8; retain.len().div_ceil(8)];
        for (index, value) in retain.iter().enumerate() {
            bytes[index / 8] |= u8::from(*value) << (index % 8);
        }
        input.extend_from_slice(&bytes);
    }
    for referred in references {
        if number <= 256 {
            input.push(u8::try_from(*referred).unwrap());
        } else if number <= 65_536 {
            input.extend_from_slice(&u16::try_from(*referred).unwrap().to_be_bytes());
        } else {
            input.extend_from_slice(&referred.to_be_bytes());
        }
    }
    if flags & 0x40 == 0 {
        input.push(u8::try_from(page).unwrap());
    } else {
        input.extend_from_slice(&page.to_be_bytes());
    }
    input.extend_from_slice(&length.to_be_bytes());
    input
}

#[test]
fn complete_headers_at_nonzero_cursors_match_literal_fields_and_exact_extent() {
    let mut cases = 0;
    for number in [256_u32, 257, 65_536, 65_537, u32::MAX] {
        for count in [0, 1, 4, 5, 8, 9] {
            let references: Vec<_> = (0..count)
                .map(|index| if index % 2 == 0 { 0 } else { number - 1 })
                .collect();
            let retained: Vec<_> = (0..=count)
                .map(|index| (index * 7 + count) % 3 == 1)
                .collect();
            for (code, kind) in DEFINED_TYPES {
                for deferred in [false, true] {
                    for long_page in [false, true] {
                        for length in [0, 1, 0x0102_0304, u32::MAX - 1, u32::MAX] {
                            if length == u32::MAX && kind != SegmentType::ImmediateGenericRegion {
                                continue;
                            }
                            let flags =
                                code | (u8::from(deferred) << 7) | (u8::from(long_page) << 6);
                            let page = if long_page { 0x0102_0304 } else { 0xfe };
                            let encoded = generated_header(
                                number,
                                flags,
                                page,
                                &references,
                                &retained,
                                length,
                            );
                            let mut input = vec![0x11, 0x22, 0x33];
                            input.extend_from_slice(&encoded);
                            input.extend_from_slice(&[0xaa, 0x55, 0x5a]);
                            let mut reader = ByteReader::new(&input);
                            reader.read_bytes(3).unwrap();
                            let header =
                                parse_segment_header(&mut reader, &Limits::default()).unwrap();
                            assert_eq!(header.number, number);
                            assert_eq!(header.segment_type, kind);
                            assert_eq!(header.deferred_non_retain, deferred);
                            assert_eq!(header.referred_segments, references);
                            assert_eq!(
                                header.retention_flags.values,
                                retained
                                    .iter()
                                    .map(|value| u8::from(*value))
                                    .collect::<Vec<_>>()
                            );
                            assert_eq!(header.page_association, page);
                            assert_eq!(
                                header.data_length,
                                if length == u32::MAX {
                                    None
                                } else {
                                    Some(length)
                                }
                            );
                            assert_eq!(header.byte_length, encoded.len());
                            assert_eq!(reader.position(), 3 + encoded.len());
                            assert_eq!(reader.remaining_bytes(), &[0xaa, 0x55, 0x5a]);
                            cases += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 10_680);
}

#[test]
fn every_flag_byte_has_exact_type_page_and_deferred_fields() {
    for flags in 0..=u8::MAX {
        let page = if flags & 0x40 == 0 { 255 } else { 0x0102_0304 };
        let input = generated_header(u32::MAX, flags, page, &[], &[true], 0x1020_3040);
        let expected = DEFINED_TYPES.iter().find(|(code, _)| *code == flags & 0x3f);
        let Some((_, kind)) = expected else {
            assert!(matches!(parse(&input),
                    Err(DecodeError::Malformed(MalformedError::ReservedSegmentType(code)))
                        if code == flags & 0x3f));
            continue;
        };
        let header = parse(&input).unwrap();
        assert_eq!(header.number, u32::MAX);
        assert_eq!(header.segment_type, *kind);
        assert_eq!(header.page_association, page);
        assert_eq!(header.deferred_non_retain, flags & 0x80 != 0);
        assert_eq!(header.byte_length, input.len());
        assert_eq!(header.data_length, Some(0x1020_3040));
        assert!(header.retention_flags.retain_this_segment());
    }
    for code in 64..=u8::MAX {
        assert!(matches!(SegmentType::from_number(code),
                Err(MalformedError::ReservedSegmentType(value)) if value == code));
    }
}

#[test]
fn every_short_count_byte_obeys_count_and_unused_bit_rules() {
    for first in 0..=u8::MAX {
        let count = first >> 5;
        let result = validate_short_count(first);
        if count > 4 {
            assert!(matches!(result,
                    Err(MalformedError::InvalidReferredToCountCode(value)) if value == count));
        } else {
            let allowed = (1_u16 << (count + 1)) - 1;
            if u16::from(first & 0x1f) & !allowed != 0 {
                assert!(matches!(
                    result,
                    Err(MalformedError::NonZeroUnusedRetentionFlags)
                ));
            } else {
                assert_eq!(result.unwrap(), u32::from(count));
            }
        }
    }
    for code in 0..=u8::MAX {
        let result = validate_count_code(code);
        match code {
            0..=4 | 7 => assert!(result.is_ok()),
            5 | 6 => assert!(matches!(result,
                    Err(MalformedError::InvalidReferredToCountCode(value)) if value == code)),
            _ => assert!(matches!(result, Err(MalformedError::IntegerOverflow))),
        }
    }
}

#[test]
fn reference_width_boundaries_and_retention_groups_round_trip() {
    for number in [256_u32, 257, 65_536, 65_537, u32::MAX] {
        for count in (0..=17).chain([31, 32, 33, 255, 256, 257]) {
            if count > number {
                continue;
            }
            let references: Vec<u32> = (0..count).collect();
            let retain: Vec<bool> = (0..=count).map(|index| index % 3 == 1).collect();
            for flags in [0_u8, 0x40] {
                let page = if flags == 0 { 17 } else { 0xf102_0304 };
                let input = generated_header(number, flags, page, &references, &retain, 0);
                let mut framed = vec![0xaa; 3];
                framed.extend_from_slice(&input);
                framed.extend_from_slice(&[0x55; 2]);
                let mut reader = ByteReader::new(&framed);
                reader.read_bytes(3).unwrap();
                let header = parse_segment_header(&mut reader, &Limits::default()).unwrap();
                assert_eq!(header.referred_segments, references);
                assert_eq!(header.retention_flags.retain_this_segment(), retain[0]);
                for index in 0..references.len() {
                    assert_eq!(
                        header.retention_flags.retain_referred_segment(index),
                        Some(retain[index + 1])
                    );
                }
                assert_eq!(
                    header
                        .retention_flags
                        .retain_referred_segment(references.len()),
                    None
                );
                assert_eq!(
                    header.retention_flags.retain_referred_segment(usize::MAX),
                    None
                );
                assert_eq!(header.page_association, page);
                assert_eq!(header.byte_length, input.len());
                assert_eq!(reader.position(), input.len() + 3);
                assert_eq!(reader.remaining(), 2);
            }
        }
    }
}

#[test]
fn every_two_byte_retention_mask_matches_forward_lsb_indexing() {
    for first in 0..=u8::MAX {
        for second in 0..=u8::MAX {
            let bytes = [first, second];
            let flags = RetentionFlags::from_bytes(15, &bytes).unwrap();
            assert_eq!(flags.values.len(), 16);
            for index in 0..16 {
                assert_eq!(flags.values[index], (bytes[index / 8] >> (index % 8)) & 1);
            }
        }
    }
}

#[test]
fn unused_long_retention_bits_validate_only_the_last_byte() {
    for bit_count in 1_u32..=40 {
        let used = bit_count % 8;
        let allowed = if used == 0 { 255 } else { (1_u16 << used) - 1 };
        for value in 0..=u8::MAX {
            let mut bytes = vec![0xff; usize::try_from(bit_count.div_ceil(8)).unwrap()];
            *bytes.last_mut().unwrap() = value;
            let result = validate_unused_retention_bits(bit_count, &bytes);
            if u16::from(value) & !allowed == 0 {
                assert!(result.is_ok());
            } else {
                assert!(matches!(
                    result,
                    Err(MalformedError::NonZeroUnusedRetentionFlags)
                ));
            }
        }
    }
}

#[test]
fn every_generated_header_prefix_is_a_typed_truncation() {
    for number in [256_u32, 257, 65_537] {
        for count in [0, 4, 5, 8, 16] {
            let references: Vec<u32> = (0..count).collect();
            let retain = vec![true; references.len() + 1];
            for flags in [0_u8, 0x40] {
                let input = generated_header(number, flags, 1, &references, &retain, 0);
                for length in 0..input.len() {
                    let mut reader = ByteReader::new(&input[..length]);
                    let result = parse_segment_header(&mut reader, &Limits::default());
                    assert!(matches!(result,
                            Err(DecodeError::Malformed(MalformedError::UnexpectedEof {
                                offset, needed, remaining,
                            })) if offset == u64::try_from(reader.position()).unwrap()
                                && remaining == u64::try_from(length - reader.position()).unwrap()
                                && needed > remaining),
                            "segment {number}, count {count}, flags {flags}, prefix {length}: {result:?}");
                }
            }
        }
    }
}

#[test]
fn count_and_length_boundaries_preserve_exact_error_values() {
    for count in 0..=4 {
        assert!(matches!(validate_long_count(0xe000_0000 | count),
                Err(MalformedError::NonCanonicalReferredToCount(value)) if value == count));
    }
    for count in [5, 7, 8, 15, 16, 0x1fff_ffff] {
        assert_eq!(validate_long_count(0xe000_0000 | count).unwrap(), count);
        assert_eq!(
            retention_byte_count(count).unwrap(),
            usize::try_from((count + 8) / 8).unwrap()
        );
    }
    assert!(matches!(
        RetentionFlags::from_bytes(u32::MAX, &[]),
        Err(DecodeError::Malformed(MalformedError::IntegerOverflow))
    ));
    assert!(matches!(
        read_retention_flag(&[], 0),
        Err(MalformedError::IntegerOverflow)
    ));
    assert!(matches!(
        read_retention_flag(&[1], 8),
        Err(MalformedError::IntegerOverflow)
    ));
    assert!(matches!(
        validate_unused_retention_bits(1, &[]),
        Err(MalformedError::IntegerOverflow)
    ));
    assert!(matches!(
        header_byte_length(2, 1),
        Err(MalformedError::IntegerOverflow)
    ));
    assert_eq!(header_byte_length(7, 7).unwrap(), 0);
    for (code, kind) in DEFINED_TYPES {
        for length in [0, 1, u32::MAX - 1] {
            assert_eq!(decode_data_length(kind, length).unwrap(), Some(length));
        }
        let result = decode_data_length(kind, u32::MAX);
        if code == 38 {
            assert_eq!(result.unwrap(), None);
        } else {
            assert!(
                matches!(result, Err(MalformedError::InvalidUnknownSegmentLength(value)) if value == code)
            );
        }
    }
}

#[test]
fn reference_numbers_are_strictly_lower_at_every_width_boundary() {
    for segment in [1_u32, 256, 257, 65_536, 65_537, u32::MAX] {
        assert!(require_lower_reference(segment, 0).is_ok());
        assert!(require_lower_reference(segment, segment - 1).is_ok());
    }
    for (segment, referred) in [
        (0, 0),
        (0, 1),
        (1, 1),
        (1, 2),
        (256, 256),
        (257, 257),
        (65_536, 65_536),
        (65_537, u32::MAX),
        (u32::MAX, u32::MAX),
    ] {
        assert!(matches!(require_lower_reference(segment, referred),
                Err(MalformedError::InvalidReferredSegmentNumber {
                    segment: actual_segment, referred: actual_referred,
                }) if actual_segment == segment && actual_referred == referred));
    }
}

#[test]
fn generated_reference_limits_fail_before_reading_references() {
    let limits = Limits {
        max_references_per_segment: 4,
        ..Limits::default()
    };
    for count in [5_u32, 8, 0x1fff_ffff] {
        let mut input = 65_537_u32.to_be_bytes().to_vec();
        input.push(0);
        input.extend_from_slice(&(0xe000_0000 | count).to_be_bytes());
        let mut reader = ByteReader::new(&input);
        assert!(matches!(parse_segment_header(&mut reader, &limits),
                Err(DecodeError::LimitExceeded(LimitExceeded {
                    kind: LimitKind::ReferencesPerSegment, limit: 4, actual,
                })) if actual == u64::from(count)));
        assert_eq!(reader.position(), input.len());
    }
}

#[test]
fn parses_short_form_header() {
    let input = [
        0x00, 0x00, 0x00, 0x20, 0x86, 0x6B, 0x02, 0x1E, 0x05, 0x04, 0x00, 0x00, 0x00, 0x0A,
    ];
    let header = parse(&input).expect("header is valid");
    assert_eq!(header.number, 32);
    assert_eq!(header.segment_type, SegmentType::ImmediateTextRegion);
    assert!(header.deferred_non_retain);
    assert_eq!(&header.referred_segments, &[2, 30, 5]);
    assert!(header.retention_flags.retain_this_segment());
    assert_eq!(
        header.retention_flags.retain_referred_segment(0),
        Some(true)
    );
    assert_eq!(
        header.retention_flags.retain_referred_segment(1),
        Some(false)
    );
    assert_eq!(header.page_association, 4);
    assert_eq!(header.data_length, Some(10));
    assert_eq!(header.byte_length, input.len());
}

#[test]
fn long_retention_flags_follow_wire_byte_order() {
    let flags = RetentionFlags::from_bytes(8, &[0x05, 0x01]).unwrap();
    assert_eq!(flags.values, [1, 0, 1, 0, 0, 0, 0, 0, 1]);
    assert!(flags.retain_this_segment());
    assert_eq!(flags.retain_referred_segment(1), Some(true));
    assert_eq!(flags.retain_referred_segment(7), Some(true));
    assert_eq!(flags.retain_referred_segment(8), None);
}

#[test]
fn long_retention_unused_bits_are_in_the_final_wire_byte() {
    assert!(validate_unused_retention_bits(9, &[0xff, 0x01]).is_ok());
    assert!(matches!(
        validate_unused_retention_bits(9, &[0x01, 0xff]),
        Err(MalformedError::NonZeroUnusedRetentionFlags)
    ));
}

#[test]
fn parses_long_form_header() {
    let mut input = [
        0x00, 0x00, 0x02, 0x34, 0x40, 0xE0, 0x00, 0x00, 0x09, 0x02, 0xFD, 0x01, 0x00, 0x00, 0x02,
        0x00, 0x1E, 0x00, 0x05, 0x02, 0x00, 0x02, 0x01, 0x02, 0x02, 0x02, 0x03, 0x02, 0x04, 0x00,
        0x00, 0x04, 0x01, 0x00, 0x00, 0x00, 0x00,
    ];
    // The worked example conflicts with 7.2.4; follow the explicit field rules.
    assert!(matches!(
        parse(&input),
        Err(DecodeError::Malformed(
            MalformedError::NonZeroUnusedRetentionFlags
        ))
    ));
    input.swap(9, 10);
    let header = parse(&input).expect("field-order correction is valid");
    assert_eq!(header.number, 564);
    assert_eq!(header.segment_type, SegmentType::SymbolDictionary);
    assert_eq!(
        &header.referred_segments,
        &[256, 2, 30, 5, 512, 513, 514, 515, 516]
    );
    assert!(header.retention_flags.retain_this_segment());
    assert_eq!(
        header.retention_flags.retain_referred_segment(0),
        Some(false)
    );
    assert_eq!(
        header.retention_flags.retain_referred_segment(7),
        Some(false)
    );
    assert_eq!(header.page_association, 1025);
    assert_eq!(header.data_length, Some(0));
}

#[test]
fn rejects_reserved_count_codes() {
    let input = [
        0x00, 0x00, 0x00, 0x01, 0x30, 0xA0, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    assert!(matches!(
        parse(&input),
        Err(DecodeError::Malformed(
            MalformedError::InvalidReferredToCountCode(5)
        ))
    ));
}

#[test]
fn rejects_nonzero_unused_short_retention_bits() {
    let input = [
        0x00, 0x00, 0x00, 0x01, 0x30, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    assert!(matches!(
        parse(&input),
        Err(DecodeError::Malformed(
            MalformedError::NonZeroUnusedRetentionFlags
        ))
    ));
}

#[test]
fn enforces_reference_limit_before_allocating() {
    let input = [0x00, 0x00, 0x01, 0x00, 0x30, 0xE0, 0x00, 0x00, 0x05];
    let limits = Limits {
        max_references_per_segment: 4,
        ..Limits::default()
    };
    let error = parse_segment_header(&mut ByteReader::new(&input), &limits)
        .expect_err("five references exceed the configured limit");
    assert!(matches!(
        error,
        DecodeError::LimitExceeded(LimitExceeded {
            kind: LimitKind::ReferencesPerSegment,
            limit: 4,
            actual: 5
        })
    ));
}

#[test]
fn rejects_nonpreceding_references() {
    let input = [
        0x00, 0x00, 0x00, 0x01, 0x30, 0x21, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    assert!(matches!(
        parse(&input),
        Err(DecodeError::Malformed(
            MalformedError::InvalidReferredSegmentNumber {
                segment: 1,
                referred: 1
            }
        ))
    ));
}

#[test]
fn accepts_unknown_length_only_for_immediate_generic_regions() {
    let valid = [
        0x00, 0x00, 0x00, 0x01, 0x26, 0x00, 0x01, 0xFF, 0xFF, 0xFF, 0xFF,
    ];
    assert_eq!(parse(&valid).expect("length is valid").data_length, None);

    let invalid = [
        0x00, 0x00, 0x00, 0x01, 0x30, 0x00, 0x01, 0xFF, 0xFF, 0xFF, 0xFF,
    ];
    assert!(matches!(
        parse(&invalid),
        Err(DecodeError::Malformed(
            MalformedError::InvalidUnknownSegmentLength(48)
        ))
    ));
}

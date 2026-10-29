//! Generated clause-specific header and document-count checks for T.88 5.4.2 and D.4.

use jbig2_decoder::{inspect_file_header, DecodeError, Decoder, MalformedError, Organization};

const SIGNATURE: [u8; 8] = [0x97, 0x4a, 0x42, 0x32, 0x0d, 0x0a, 0x1a, 0x0a];

fn header(flags: u8, count: u32) -> Vec<u8> {
    let mut bytes = SIGNATURE.to_vec();
    bytes.push(flags);
    if flags & 2 == 0 {
        bytes.extend_from_slice(&count.to_be_bytes());
    }
    bytes
}

#[test]
fn flag_fields_follow_d4_2_for_every_flag_byte() {
    for flags in 0..=u8::MAX {
        let input = header(flags, 0x1020_3040);
        let result = inspect_file_header(&input);
        if flags & 0xf0 != 0 {
            assert!(matches!(
                result,
                Err(DecodeError::Malformed(MalformedError::ReservedFileHeaderFlags(bits)))
                    if bits == flags & 0xf0
            ));
            continue;
        }
        let parsed = result.expect("defined flags");
        assert_eq!(
            parsed.organization(),
            if flags & 1 == 0 {
                Organization::RandomAccess
            } else {
                Organization::Sequential
            }
        );
        assert_eq!(parsed.uses_extended_templates(), flags & 4 != 0);
        assert_eq!(parsed.uses_colour_regions(), flags & 8 != 0);
        assert_eq!(
            parsed.page_count(),
            if flags & 2 == 0 {
                Some(0x1020_3040)
            } else {
                None
            }
        );
        assert_eq!(parsed.byte_length(), input.len());
        if flags & 2 != 0 {
            let mut followed_by_segments = input;
            followed_by_segments.extend_from_slice(&[0xff; 4]);
            assert_eq!(
                inspect_file_header(&followed_by_segments).expect("header before payload"),
                parsed
            );
        }
    }
}

#[test]
fn signature_mismatch_is_rejected_at_each_octet() {
    for index in 0..SIGNATURE.len() {
        let mut input = header(2, 0);
        input[index] ^= 1;
        assert!(matches!(
            inspect_file_header(&input),
            Err(DecodeError::Malformed(MalformedError::InvalidFileSignature))
        ));
    }
}

#[test]
fn every_valid_header_prefix_remains_truncated_until_complete() {
    for flags in 0..16 {
        let input = header(flags, 3);
        for length in 0..input.len() {
            assert!(
                matches!(
                    inspect_file_header(&input[..length]),
                    Err(DecodeError::Malformed(MalformedError::UnexpectedEof { .. }))
                ),
                "flags {flags:#x}, prefix {length}"
            );
        }
        assert!(inspect_file_header(&input).is_ok());
    }
}

#[test]
fn page_count_field_is_unsigned_and_most_significant_first() {
    for count in [0, 1, 0x0102_0304, u32::MAX] {
        let parsed = inspect_file_header(&header(1, count)).expect("four-byte count");
        assert_eq!(parsed.page_count(), Some(count));
        assert_eq!(parsed.byte_length(), 13);
    }
}

fn document(sequential: bool, declared: Option<u32>, page_numbers: &[u8]) -> Vec<u8> {
    let flags = u8::from(sequential) | if declared.is_none() { 2 } else { 0 };
    let mut bytes = header(flags, declared.unwrap_or(0));
    let mut payloads = Vec::new();
    let mut number = 0_u32;
    for &page in page_numbers {
        let mut payload = [0_u8; 19];
        payload[..4].copy_from_slice(&8_u32.to_be_bytes());
        payload[4..8].copy_from_slice(&1_u32.to_be_bytes());
        for (kind, data) in [(48, payload.as_slice()), (49, &[][..])] {
            bytes.extend_from_slice(&number.to_be_bytes());
            bytes.extend_from_slice(&[kind, 0, page]);
            bytes.extend_from_slice(&u32::try_from(data.len()).unwrap().to_be_bytes());
            if sequential {
                bytes.extend_from_slice(data);
            } else {
                payloads.extend_from_slice(data);
            }
            number += 1;
        }
    }
    bytes.extend_from_slice(&number.to_be_bytes());
    bytes.extend_from_slice(&[51, 0, 0]);
    bytes.extend_from_slice(&0_u32.to_be_bytes());
    bytes.extend_from_slice(&payloads);
    bytes
}

#[test]
fn accepted_document_counts_match_page_declarations_in_both_organizations() {
    let decoder = Decoder::default();
    for pages in [&[][..], &[1][..], &[3, 1, 2][..]] {
        let count = u32::try_from(pages.len()).unwrap();
        for declared in [None, Some(count)] {
            for sequential in [false, true] {
                let bytes = document(sequential, declared, pages);
                let info = decoder.inspect(&bytes).expect("consistent generated count");
                assert_eq!(
                    info.organization(),
                    if sequential {
                        Organization::Sequential
                    } else {
                        Organization::RandomAccess
                    }
                );
                assert_eq!(info.declared_page_count(), declared);
                assert_eq!(info.segment_count(), count * 2 + 1);
                assert_eq!(info.pages().len(), pages.len());
                for (index, page) in info.pages().iter().enumerate() {
                    assert_eq!(page.number(), u32::try_from(index + 1).unwrap());
                    assert_eq!((page.width(), page.height()), (8, 1));
                }
                let output = decoder.decode(&bytes).expect("blank pages decode");
                assert_eq!(output.info(), &info);
                assert_eq!(output.pages().len(), pages.len());
                assert_eq!(
                    decoder
                        .inspect_reader(&mut std::io::Cursor::new(&bytes))
                        .expect("reader count"),
                    info
                );
                assert_eq!(decoder.context().inspect(&bytes).unwrap(), info);
            }
        }
    }
}

#[test]
fn inconsistent_declared_counts_report_exact_actual_count() {
    let decoder = Decoder::default();
    for pages in [&[][..], &[1][..], &[1, 2, 3][..]] {
        let actual = u32::try_from(pages.len()).unwrap();
        for declared in [0, 1, 2, 4, u32::MAX] {
            if declared == actual {
                continue;
            }
            for sequential in [false, true] {
                let bytes = document(sequential, Some(declared), pages);
                for result in [
                    decoder.inspect(&bytes).map(|_| ()),
                    decoder.decode(&bytes).map(|_| ()),
                    decoder
                        .inspect_reader(&mut std::io::Cursor::new(&bytes))
                        .map(|_| ()),
                    decoder.context().inspect(&bytes).map(|_| ()),
                ] {
                    assert!(
                        matches!(result,
                                Err(DecodeError::Malformed(MalformedError::PageCountMismatch {
                                    declared: reported_declared,
                                    actual: reported_actual,
                                })) if reported_declared == declared && reported_actual == actual),
                        "declared {declared}, actual {actual}, sequential {sequential}: {result:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn inspection_metadata_preserves_headers_across_context_reuse() {
    let decoder = Decoder::default();
    let mut context = decoder.context();
    for pages in [&[][..], &[1][..], &[3, 1, 2][..]] {
        for declared in [None, Some(u32::try_from(pages.len()).unwrap())] {
            for sequential in [false, true] {
                for extended in [false, true] {
                    let mut bytes = document(sequential, declared, pages);
                    if extended {
                        bytes[8] |= 4;
                    }
                    let expected = decoder.inspect(&bytes).unwrap();
                    assert!(context.inspect(b"invalid header").is_err());
                    let info = context.inspect(&bytes).expect("recovered metadata");
                    let parsed_header = inspect_file_header(&bytes).unwrap();
                    assert_eq!(info.organization(), parsed_header.organization());
                    assert_eq!(info.declared_page_count(), parsed_header.page_count());
                    assert_eq!(info.pages().len(), pages.len());
                    assert_eq!(info, expected);
                }
            }
        }
    }
}

#[test]
fn embedded_inspection_metadata_has_no_standalone_declaration() {
    let decoder = Decoder::default();
    let globals = decoder.parse_globals(&[]).unwrap();
    let mut context = decoder.context();
    let empty = document(true, None, &[]);
    let header_length = inspect_file_header(&empty).unwrap().byte_length();
    let end_marker_length = empty.len() - header_length;
    for pages in [&[][..], &[1][..], &[3, 1, 2][..]] {
        let bytes = document(true, None, pages);
        let expected = decoder.inspect(&bytes).unwrap();
        let embedded = &bytes[header_length..bytes.len() - end_marker_length];
        assert!(context.inspect_embedded(&[0xff]).is_err());
        for info in [
            decoder.inspect_embedded(embedded).unwrap(),
            decoder
                .inspect_embedded_with_globals(embedded, &globals)
                .unwrap(),
            context.inspect_embedded(embedded).unwrap(),
            context
                .inspect_embedded_with_globals(embedded, &globals)
                .unwrap(),
        ] {
            assert_eq!(info.organization(), Organization::Sequential);
            assert_eq!(info.declared_page_count(), None);
            assert_eq!(
                info.segment_count(),
                u32::try_from(pages.len() * 2).unwrap()
            );
            assert_eq!(info.pages(), expected.pages());
        }
    }
}

#[test]
fn colour_header_is_not_accepted_as_document_metadata() {
    let decoder = Decoder::default();
    let mut context = decoder.context();
    for sequential in [false, true] {
        for declared in [None, Some(1)] {
            let mut bytes = document(sequential, declared, &[1]);
            bytes[8] |= 8;
            for result in [decoder.inspect(&bytes), context.inspect(&bytes)] {
                assert!(matches!(
                    result,
                    Err(DecodeError::Unsupported(
                        jbig2_decoder::UnsupportedFeature::Colour
                    ))
                ));
            }
        }
    }
}

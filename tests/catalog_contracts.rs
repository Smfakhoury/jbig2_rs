//! Exact public-API regressions for negative and metamorphic catalog claims.

use jbig2_decoder::{DecodeError, Decoder, MalformedError};

#[allow(dead_code)]
#[path = "../tools/distribution_consumer.rs"]
mod fixture;

fn empty_document(organization: u8) -> Vec<u8> {
    let mut input = b"\x97JB2\r\n\x1a\n".to_vec();
    input.push(organization);
    input.extend_from_slice(&0_u32.to_be_bytes());
    input
}

#[test]
fn duplicate_local_wire_numbers_have_exact_order_errors() {
    for organization in [0, 1] {
        let mut input = empty_document(organization);
        input.extend_from_slice(&[0; 11]);
        input.extend_from_slice(&[0; 11]);
        assert!(matches!(
            Decoder::default().inspect(&input),
            Err(DecodeError::Malformed(
                MalformedError::NonIncreasingSegmentNumber {
                    previous: 0,
                    current: 0,
                }
            ))
        ));
    }
}

#[test]
fn global_page_number_collision_is_an_exact_duplicate_error() {
    let decoder = Decoder::default();
    let globals = decoder
        .parse_globals(&[0; 11])
        .expect("generated global header");
    let (_, embedded) = fixture::generated_page();
    assert!(matches!(
        decoder.inspect_embedded_with_globals(&embedded, &globals),
        Err(DecodeError::Malformed(
            MalformedError::DuplicateSegmentNumber(0)
        ))
    ));
}

#[test]
fn missing_reference_has_exact_segment_and_referred_numbers() {
    let mut input = empty_document(1);
    input.extend_from_slice(&2_u32.to_be_bytes());
    input.extend_from_slice(&[0, 0x23, 1, 0]);
    input.extend_from_slice(&0_u32.to_be_bytes());
    assert!(matches!(
        Decoder::default().inspect(&input),
        Err(DecodeError::Malformed(
            MalformedError::MissingReferredSegment {
                segment: 2,
                referred: 1,
            }
        ))
    ));
}

#[test]
fn repeated_inspection_preserves_metadata_and_exact_error_fields() {
    let (input, _) = fixture::generated_page();
    let decoder = Decoder::default();
    let expected = decoder.inspect(&input).expect("generated metadata");
    for _ in 0..3 {
        assert_eq!(
            decoder.inspect(&input).expect("repeated metadata"),
            expected
        );
        assert!(matches!(
            decoder.inspect(b"not jbig"),
            Err(DecodeError::Malformed(MalformedError::InvalidFileSignature))
        ));
        assert!(matches!(
            decoder.inspect(&input[..7]),
            Err(DecodeError::Malformed(MalformedError::UnexpectedEof {
                offset: 0,
                needed: 8,
                remaining: 7,
            }))
        ));
    }
}

#[test]
fn every_standalone_prefix_is_malformed_or_the_exact_optional_eof_prefix() {
    let (input, _) = fixture::generated_page();
    let decoder = Decoder::default();
    let expected = decoder.inspect(&input).expect("generated metadata");
    let mut accepted = Vec::new();
    for length in 0..input.len() {
        match decoder.inspect(&input[..length]) {
            Ok(info) => {
                assert_eq!(info.pages(), expected.pages());
                assert_eq!(info.organization(), expected.organization());
                assert_eq!(info.segment_count() + 1, expected.segment_count());
                accepted.push(length);
            }
            Err(DecodeError::Malformed(_)) => {}
            other => panic!("prefix {length} has an unexpected outcome: {other:?}"),
        }
    }
    assert_eq!(accepted, [input.len() - 11]);
}

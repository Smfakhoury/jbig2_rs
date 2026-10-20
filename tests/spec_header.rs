//! Generated clause-specific header checks for T.88 5.4.2 and D.4.

use jbig2_rs::{inspect_file_header, DecodeError, MalformedError, Organization};

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

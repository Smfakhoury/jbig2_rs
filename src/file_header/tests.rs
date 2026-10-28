#![cfg(test)]

use super::*;

#[test]
fn parses_known_page_sequential_header() {
    let input = [
        0x97, 0x4A, 0x42, 0x32, 0x0D, 0x0A, 0x1A, 0x0A, 0x01, 0x00, 0x00, 0x00, 0x03,
    ];
    let header = inspect_file_header(&input).expect("header is valid");
    assert_eq!(header.organization(), Organization::Sequential);
    assert_eq!(header.page_count(), Some(3));
    assert_eq!(header.byte_length(), 13);
    assert!(!header.uses_extended_templates());
    assert!(!header.uses_colour_regions());
}

#[test]
fn parses_unknown_page_random_access_header() {
    let input = [0x97, 0x4A, 0x42, 0x32, 0x0D, 0x0A, 0x1A, 0x0A, 0x0E];
    let header = inspect_file_header(&input).expect("header is valid");
    assert_eq!(header.organization(), Organization::RandomAccess);
    assert_eq!(header.page_count(), None);
    assert_eq!(header.byte_length(), 9);
    assert!(header.uses_extended_templates());
    assert!(header.uses_colour_regions());
}

#[test]
fn rejects_reserved_flags() {
    let input = [0x97, 0x4A, 0x42, 0x32, 0x0D, 0x0A, 0x1A, 0x0A, 0x10];
    assert!(matches!(
        inspect_file_header(&input),
        Err(DecodeError::Malformed(
            MalformedError::ReservedFileHeaderFlags(0x10)
        ))
    ));
}

#[test]
fn truncation_does_not_become_valid_metadata() {
    let input = [0x97, 0x4A, 0x42, 0x32, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00];
    assert!(matches!(
        inspect_file_header(&input),
        Err(DecodeError::Malformed(MalformedError::UnexpectedEof { .. }))
    ));
}

//! Generated organization examples for the provisional P04 evidence ledger.

use jbig2_decoder::{DecodeError, Decoder, MalformedError};

fn segment_header(number: u32, kind: u8, page: u8, length: u32) -> Vec<u8> {
    let mut bytes = number.to_be_bytes().to_vec();
    bytes.extend_from_slice(&[kind, 1, page]);
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes
}

fn page_information(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = vec![0; 19];
    bytes[..4].copy_from_slice(&width.to_be_bytes());
    bytes[4..8].copy_from_slice(&height.to_be_bytes());
    bytes
}

fn standalone_header(sequential: bool, pages: u32) -> Vec<u8> {
    let mut bytes = b"\x97JB2\r\n\x1a\n".to_vec();
    bytes.push(u8::from(sequential));
    bytes.extend_from_slice(&pages.to_be_bytes());
    bytes
}

#[test]
fn sequential_payloads_are_interleaved_with_their_headers() {
    let mut bytes = standalone_header(true, 2);
    bytes.extend(segment_header(0, 48, 1, 19));
    bytes.extend(page_information(8, 3));
    bytes.extend(segment_header(1, 49, 1, 0));
    bytes.extend(segment_header(2, 48, 2, 19));
    bytes.extend(page_information(16, 5));
    bytes.extend(segment_header(3, 49, 2, 0));
    bytes.extend(segment_header(4, 51, 0, 0));

    let info = Decoder::default()
        .inspect(&bytes)
        .expect("sequential pages");
    let dimensions: Vec<_> = info
        .pages()
        .iter()
        .map(|page| (page.width(), page.height()))
        .collect();
    assert_eq!(dimensions, [(8, 3), (16, 5)]);
    assert_eq!(info.segment_count(), 5);
}

#[test]
fn random_access_payloads_follow_all_headers_in_header_order() {
    let mut bytes = standalone_header(false, 2);
    bytes.extend(segment_header(0, 48, 1, 19));
    bytes.extend(segment_header(1, 49, 1, 0));
    bytes.extend(segment_header(2, 48, 2, 19));
    bytes.extend(segment_header(3, 49, 2, 0));
    bytes.extend(segment_header(4, 51, 0, 0));
    bytes.extend(page_information(8, 3));
    bytes.extend(page_information(16, 5));

    let info = Decoder::default()
        .inspect(&bytes)
        .expect("random-access pages");
    let dimensions: Vec<_> = info
        .pages()
        .iter()
        .map(|page| (page.width(), page.height()))
        .collect();
    assert_eq!(dimensions, [(8, 3), (16, 5)]);
    assert_eq!(info.segment_count(), 5);
}

#[test]
fn random_access_requires_an_end_of_file_header() {
    let mut bytes = standalone_header(false, 1);
    bytes.extend(segment_header(0, 48, 1, 19));
    assert!(matches!(
        Decoder::default().inspect(&bytes),
        Err(DecodeError::Malformed(MalformedError::MissingEndOfFile))
    ));
}

#[test]
fn both_standalone_organizations_reject_descending_segment_numbers() {
    for sequential in [false, true] {
        let mut bytes = standalone_header(sequential, 0);
        bytes.extend(segment_header(2, 0, 0, 0));
        bytes.extend(segment_header(1, 0, 0, 0));
        assert!(matches!(
            Decoder::default().inspect(&bytes),
            Err(DecodeError::Malformed(
                MalformedError::NonIncreasingSegmentNumber {
                    previous: 2,
                    current: 1,
                }
            ))
        ));
    }
}

#[test]
fn embedded_segments_need_no_file_header_or_wire_number_order() {
    let mut bytes = segment_header(2, 48, 1, 19);
    bytes.extend(page_information(8, 3));
    bytes.extend(segment_header(1, 49, 1, 0));

    let info = Decoder::default()
        .inspect_embedded(&bytes)
        .expect("headerless embedded segments");
    assert_eq!(info.segment_count(), 2);
    assert_eq!(info.pages().len(), 1);
    assert_eq!((info.pages()[0].width(), info.pages()[0].height()), (8, 3));
}

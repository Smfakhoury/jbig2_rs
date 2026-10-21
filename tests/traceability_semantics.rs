//! Generated page/resource examples for provisional P04 records, not official vectors.

use jbig2_rs::{DecodeError, Decoder, MalformedError, UnsupportedFeature};

fn header(number: u32, kind: u8, page: u8, length: u32) -> Vec<u8> {
    let mut bytes = number.to_be_bytes().to_vec();
    bytes.extend_from_slice(&[kind, 1, page]);
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes
}

fn page_information(height: u32, flags: u8, striping: u16) -> Vec<u8> {
    let mut bytes = 8_u32.to_be_bytes().to_vec();
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&400_u32.to_be_bytes());
    bytes.extend_from_slice(&600_u32.to_be_bytes());
    bytes.push(flags);
    bytes.extend_from_slice(&striping.to_be_bytes());
    bytes
}

fn document(page: &[u8]) -> Vec<u8> {
    let mut bytes = b"\x97JB2\r\n\x1a\n\x01".to_vec();
    bytes.extend_from_slice(&1_u32.to_be_bytes());
    bytes.extend(header(0, 48, 1, 19));
    bytes.extend_from_slice(page);
    bytes
}

fn finish(bytes: &mut Vec<u8>, first_number: u32) {
    bytes.extend(header(first_number, 49, 1, 0));
    bytes.extend(header(first_number + 1, 51, 0, 0));
}

fn mmr_region(operator: u8, black: bool) -> Vec<u8> {
    let mut bytes = 8_u32.to_be_bytes().to_vec();
    bytes.extend_from_slice(&1_u32.to_be_bytes());
    bytes.extend_from_slice(&[0; 8]);
    bytes.extend_from_slice(&[operator, 1]);
    bytes.extend_from_slice(if black { &[0x26, 0xa2, 0x80] } else { &[0x80] });
    bytes
}

#[test]
fn known_dimensions_and_resolution_preserve_page_fields() {
    let mut bytes = document(&page_information(3, 0, 0));
    finish(&mut bytes, 1);
    let info = Decoder::default()
        .inspect(&bytes)
        .expect("page information");
    let page = &info.pages()[0];
    assert_eq!((page.width(), page.height()), (8, 3));
    assert_eq!((page.x_resolution(), page.y_resolution()), (400, 600));
}

#[test]
fn page_default_pixel_initializes_the_complete_page() {
    for black in [false, true] {
        let mut bytes = document(&page_information(3, u8::from(black) << 2, 0));
        finish(&mut bytes, 1);
        let decoded = Decoder::default().decode(&bytes).expect("empty page");
        assert_eq!(
            decoded.pages()[0].bitmap().bytes(),
            &[if black { 0xff } else { 0 }; 3]
        );
    }
}

#[test]
fn all_region_operator_codes_match_a_boolean_pixel_oracle() {
    for destination in [false, true] {
        for source in [false, true] {
            for operator in 0..=4 {
                let flags = 0x40 | (u8::from(destination) << 2);
                let mut bytes = document(&page_information(1, flags, 0));
                let region = mmr_region(operator, source);
                bytes.extend(header(
                    1,
                    39,
                    1,
                    u32::try_from(region.len()).expect("small region"),
                ));
                bytes.extend(region);
                finish(&mut bytes, 2);
                let decoded = Decoder::default().decode(&bytes).expect("operator page");
                let expected = match operator {
                    0 => destination || source,
                    1 => destination && source,
                    2 => destination != source,
                    3 => destination == source,
                    _ => source,
                };
                assert_eq!(
                    decoded.pages()[0].bitmap().bytes(),
                    &[if expected { 0xff } else { 0 }],
                    "destination={destination}, source={source}, operator={operator}"
                );
            }
        }
    }
}

#[test]
fn immediate_regions_only_modify_their_page_intersection() {
    let mut bytes = document(&page_information(1, 0, 0));
    let mut region = mmr_region(0, true);
    region[8..12].copy_from_slice(&7_u32.to_be_bytes());
    bytes.extend(header(
        1,
        39,
        1,
        u32::try_from(region.len()).expect("small region"),
    ));
    bytes.extend(region);
    finish(&mut bytes, 2);
    let decoded = Decoder::default().decode(&bytes).expect("clipped region");
    assert_eq!(decoded.pages()[0].bitmap().bytes(), &[1]);
}

#[test]
fn direct_region_operator_override_requires_the_page_flag() {
    for allow_override in [false, true] {
        let mut bytes = document(&page_information(1, u8::from(allow_override) << 6, 0));
        let region = mmr_region(2, true);
        bytes.extend(header(
            1,
            39,
            1,
            u32::try_from(region.len()).expect("small region"),
        ));
        bytes.extend(region);
        finish(&mut bytes, 2);
        let result = Decoder::default().decode(&bytes);
        if allow_override {
            assert_eq!(
                result.expect("allowed override").pages()[0]
                    .bitmap()
                    .bytes(),
                &[0xff]
            );
        } else {
            assert!(matches!(
                result,
                Err(DecodeError::Malformed(MalformedError::InvalidPageState(
                    "region overrides the page combination operator"
                )))
            ));
        }
    }
}

#[test]
fn intermediate_buffers_do_not_draw_on_the_page() {
    let mut bytes = document(&page_information(1, 0x20, 0));
    let region = mmr_region(0, true);
    bytes.extend(header(
        1,
        36,
        1,
        u32::try_from(region.len()).expect("small region"),
    ));
    bytes.extend(region);
    finish(&mut bytes, 2);
    let decoded = Decoder::default()
        .decode(&bytes)
        .expect("intermediate bitmap");
    assert_eq!(decoded.pages()[0].bitmap().bytes(), &[0]);
}

#[test]
fn intermediate_regions_require_page_auxiliary_permission() {
    let mut bytes = document(&page_information(1, 0, 0));
    let region = mmr_region(0, true);
    bytes.extend(header(
        1,
        36,
        1,
        u32::try_from(region.len()).expect("small region"),
    ));
    bytes.extend(region);
    finish(&mut bytes, 2);
    assert!(matches!(
        Decoder::default().decode(&bytes),
        Err(DecodeError::Malformed(MalformedError::InvalidPageState(
            "intermediate region on page that forbids auxiliary buffers"
        )))
    ));
}

#[test]
fn unknown_height_uses_the_last_inclusive_stripe_row() {
    let mut bytes = document(&page_information(u32::MAX, 0, 0x8008));
    bytes.extend(header(1, 50, 1, 4));
    bytes.extend_from_slice(&2_u32.to_be_bytes());
    finish(&mut bytes, 2);
    let decoded = Decoder::default().decode(&bytes).expect("striped page");
    assert_eq!(decoded.pages()[0].bitmap().height(), 3);
}

#[test]
fn stripe_rows_are_strictly_increasing_and_bounded() {
    for second in [1_u32, 2, 11] {
        let mut bytes = document(&page_information(u32::MAX, 0, 0x8008));
        bytes.extend(header(1, 50, 1, 4));
        bytes.extend_from_slice(&2_u32.to_be_bytes());
        bytes.extend(header(2, 50, 1, 4));
        bytes.extend_from_slice(&second.to_be_bytes());
        finish(&mut bytes, 3);
        assert!(matches!(
            Decoder::default().inspect(&bytes),
            Err(DecodeError::Malformed(MalformedError::InvalidStripeEndRow(row))) if row == second
        ));
    }
}

#[test]
fn page_independent_resources_cannot_refer_to_page_scoped_resources() {
    let mut bytes = document(&page_information(1, 0, 0));
    bytes.extend(header(1, 0, 1, 0));
    let mut global = 2_u32.to_be_bytes().to_vec();
    global.extend_from_slice(&[0, 0x23, 1, 0]);
    global.extend_from_slice(&0_u32.to_be_bytes());
    bytes.extend(global);
    finish(&mut bytes, 3);
    assert!(matches!(
        Decoder::default().inspect(&bytes),
        Err(DecodeError::Malformed(
            MalformedError::InvalidPageAssociation {
                segment: 2,
                page: 0,
            }
        ))
    ));
}

#[test]
fn region_segments_cannot_be_page_independent_resources() {
    let mut bytes = document(&page_information(1, 0, 0));
    bytes.extend(header(1, 36, 0, 0));
    finish(&mut bytes, 2);
    assert!(matches!(
        Decoder::default().inspect(&bytes),
        Err(DecodeError::Malformed(
            MalformedError::InvalidPageAssociation {
                segment: 1,
                page: 0,
            }
        ))
    ));
}

#[test]
fn palette_segment_rejection_is_a_typed_profile_policy() {
    let mut bytes = document(&page_information(1, 0, 0));
    bytes.extend(header(1, 54, 0, 0));
    finish(&mut bytes, 2);
    assert!(matches!(
        Decoder::default().inspect(&bytes),
        Err(DecodeError::Unsupported(UnsupportedFeature::Colour))
    ));
}

#[test]
fn coloured_page_rejection_is_a_typed_profile_policy() {
    let mut bytes = document(&page_information(1, 0x80, 0));
    finish(&mut bytes, 1);
    assert!(matches!(
        Decoder::default().inspect(&bytes),
        Err(DecodeError::Unsupported(UnsupportedFeature::Colour))
    ));
}

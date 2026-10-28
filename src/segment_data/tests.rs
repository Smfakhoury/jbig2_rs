#![cfg(test)]

use super::*;

#[test]
fn raw_page_information_preserves_every_literal_field_and_height_encoding() {
    let limits = Limits {
        max_width: u32::MAX,
        max_height: u32::MAX,
        max_pixels_per_page: u64::MAX,
        ..Limits::default()
    };
    for width in [0_u32, 17, u32::MAX] {
        for height in [23_u32, u32::MAX] {
            for flags in 0..=127_u8 {
                for striping in [0_u16, 1, 0x7fff, 0x8000, 0x8001, 0xffff] {
                    let mut payload = [0_u8; 19];
                    payload[..4].copy_from_slice(&width.to_be_bytes());
                    payload[4..8].copy_from_slice(&height.to_be_bytes());
                    payload[8..12].copy_from_slice(&0x0123_4567_u32.to_be_bytes());
                    payload[12..16].copy_from_slice(&0x89ab_cdef_u32.to_be_bytes());
                    payload[16] = flags;
                    payload[17..].copy_from_slice(&striping.to_be_bytes());
                    let result = parse_page_information(&payload, &limits);
                    if height == u32::MAX && striping & 0x8000 == 0 {
                        assert!(matches!(
                            result,
                            Err(DecodeError::Malformed(
                                MalformedError::UnknownHeightWithoutStriping
                            ))
                        ));
                        continue;
                    }
                    let info = result.unwrap();
                    assert_eq!(info.width, width);
                    assert_eq!(info.height, (height != u32::MAX).then_some(height));
                    assert_eq!(info.x_resolution, 0x0123_4567);
                    assert_eq!(info.y_resolution, 0x89ab_cdef);
                    assert_eq!(info.eventually_lossless, flags & 1 != 0);
                    assert_eq!(info.might_contain_refinements, flags & 2 != 0);
                    assert_eq!(info.default_pixel, flags & 4 != 0);
                    assert_eq!(
                        info.default_operator,
                        [
                            CombinationOperator::Or,
                            CombinationOperator::And,
                            CombinationOperator::Xor,
                            CombinationOperator::Xnor,
                        ][usize::from((flags >> 3) & 3)]
                    );
                    assert_eq!(info.requires_auxiliary_buffers, flags & 0x20 != 0);
                    assert_eq!(info.combination_operator_override, flags & 0x40 != 0);
                    assert_eq!(info.striped, striping & 0x8000 != 0);
                    assert_eq!(info.max_stripe_size, striping & 0x7fff);
                }
            }
        }
    }
}

#[test]
fn raw_profile_lists_preserve_literal_values_prefix_extent_and_suffix() {
    for count in [0_u32, 1, 2, 5, 17] {
        let values: Vec<u32> = (0..count)
            .map(|index| [0, u32::MAX, 0x0123_4567, 0x89ab_cdef][index as usize % 4])
            .collect();
        let mut payload = count.to_be_bytes().to_vec();
        for value in &values {
            payload.extend_from_slice(&value.to_be_bytes());
        }
        assert_eq!(parse_profiles(&payload).unwrap(), values);
        let mut bytes = vec![0xaa, 0xbb, 0xcc];
        bytes.extend_from_slice(&payload[4..]);
        bytes.extend_from_slice(&[0x55, 0x66]);
        let mut reader = ByteReader::new(&bytes);
        reader.read_bytes(3).unwrap();
        let mut profiles = vec![17, 33];
        read_profiles_into(&mut reader, count, &mut profiles).unwrap();
        assert_eq!(&profiles[..2], &[17, 33]);
        assert_eq!(&profiles[2..], values);
        assert_eq!(reader.position(), 3 + count as usize * 4);
        assert_eq!(reader.remaining_bytes(), &[0x55, 0x66]);
    }
}

#[test]
fn parses_known_height_page_information() {
    let payload = [
        0x00, 0x00, 0x03, 0x20, 0x00, 0x00, 0x02, 0x58, 0x00, 0x00, 0x0B, 0xB8, 0x00, 0x00, 0x0B,
        0xB8, 0x55, 0x00, 0x00,
    ];
    let page = parse_page_information(&payload, &Limits::default()).expect("page is valid");
    assert_eq!(page.width, 800);
    assert_eq!(page.height, Some(600));
    assert_eq!(page.x_resolution, 3000);
    assert_eq!(page.y_resolution, 3000);
    assert!(page.eventually_lossless);
    assert!(!page.might_contain_refinements);
    assert!(page.default_pixel);
    assert_eq!(page.default_operator, CombinationOperator::Xor);
    assert!(!page.requires_auxiliary_buffers);
    assert!(page.combination_operator_override);
    assert!(!page.striped);
}

#[test]
fn unknown_height_requires_striping() {
    let mut payload = [0_u8; 19];
    payload[4..8].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(matches!(
        parse_page_information(&payload, &Limits::default()),
        Err(DecodeError::Malformed(
            MalformedError::UnknownHeightWithoutStriping
        ))
    ));
    payload[17..19].copy_from_slice(&0x800A_u16.to_be_bytes());
    let page = parse_page_information(&payload, &Limits::default()).expect("page is striped");
    assert!(page.striped);
    assert_eq!(page.max_stripe_size, 10);
}

#[test]
fn rejects_coloured_pages_and_regions() {
    let mut page = [0_u8; 19];
    page[16] = 0x80;
    assert!(matches!(
        parse_page_information(&page, &Limits::default()),
        Err(DecodeError::Unsupported(UnsupportedFeature::Colour))
    ));

    let mut region = [0_u8; 17];
    region[16] = 0x08;
    assert!(matches!(
        parse_region_info(&mut ByteReader::new(&region), &Limits::default()),
        Err(DecodeError::Unsupported(UnsupportedFeature::Colour))
    ));
}

#[test]
fn fixed_size_segments_reject_wrong_lengths() {
    assert!(parse_end_of_stripe(&[0, 0, 0]).is_err());
    assert!(require_empty(49, &[0]).is_err());
    assert_eq!(
        parse_end_of_stripe(&[0, 0, 0, 7]).expect("stripe is valid"),
        7
    );
}

#[test]
fn parses_profiles_and_comment_extensions() {
    let profiles =
        parse_profiles(&[0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0, 3]).expect("profiles are valid");
    assert_eq!(profiles, [1, 3]);

    let single_byte = [0x20, 0x00, 0x00, 0x00, b'N', 0, b'V', 0, 0];
    parse_extension(&single_byte).expect("single-byte comment is valid");
    let multi_byte = [0x20, 0x00, 0x00, 0x02, 0, b'N', 0, 0, 0, b'V', 0, 0, 0, 0];
    parse_extension(&multi_byte).expect("multi-byte comment is valid");
}

#[test]
fn rejects_unknown_necessary_extensions() {
    assert!(matches!(
        parse_extension(&[0xA0, 0x00, 0x00, 0x01]),
        Err(DecodeError::Unsupported(UnsupportedFeature::Extension(
            0xA000_0001
        )))
    ));
    assert!(matches!(
        parse_extension(&[0x80, 0x00, 0x00, 0x01]),
        Err(DecodeError::Malformed(
            MalformedError::InvalidExtensionType(0x8000_0001)
        ))
    ));
}

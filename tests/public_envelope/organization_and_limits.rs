#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

use super::*;

#[test]
fn organizations_preserve_sorted_metadata_but_deliver_pages_in_stream_order() {
    let mut black = page(0, 2, 9, 2);
    black.payload[16] = 4;
    let segments = [black, end(1, 2), page(2, 1, 0, 3), end(3, 1), eof(4)];
    let expected = [(1, 0, 3, 300, 600, false), (2, 9, 2, 300, 600, false)];
    let decoder = Decoder::default();
    for sequential in [true, false] {
        for declared in [Some(2), None] {
            let bytes = standalone(sequential, declared, &segments);
            let organization = if sequential {
                Organization::Sequential
            } else {
                Organization::RandomAccess
            };
            let info = decoder.inspect_reader(&mut Cursor::new(&bytes)).unwrap();
            assert_info(&info, organization, declared, 5, &expected);
            let document = decoder.decode(&bytes).unwrap();
            assert_info(document.info(), organization, declared, 5, &expected);
            assert_eq!(document.pages().len(), 2);
            assert_pixels(&document.pages()[0], 2, 9, 2, &[0xff, 0x80, 0xff, 0x80]);
            assert_pixels(&document.pages()[1], 1, 0, 3, &[]);
            let mut output = Vec::new();
            let info = decoder
                .decode_reader_with_sink(&mut Cursor::new(bytes), &mut |page| {
                    output.push(page);
                    Ok(())
                })
                .unwrap();
            assert_info(&info, organization, declared, 5, &expected);
            assert_eq!(output.len(), 2);
            assert_pixels(&output[0], 2, 9, 2, &[0xff, 0x80, 0xff, 0x80]);
            assert_pixels(&output[1], 1, 0, 3, &[]);
        }
    }
    let document = decoder.decode_embedded(&embedded(&segments)).unwrap();
    assert_info(
        document.info(),
        Organization::Sequential,
        None,
        5,
        &expected,
    );
    assert_pixels(&document.pages()[0], 2, 9, 2, &[0xff, 0x80, 0xff, 0x80]);
    assert_pixels(&document.pages()[1], 1, 0, 3, &[]);
}

#[test]
fn empty_documents_and_zero_sized_bitmaps_have_complete_metadata() {
    let decoder = Decoder::default();
    for sequential in [true, false] {
        for declared in [Some(0), None] {
            let bytes = standalone(sequential, declared, &[eof(0)]);
            let document = decoder.decode(&bytes).unwrap();
            assert_info(
                document.info(),
                if sequential {
                    Organization::Sequential
                } else {
                    Organization::RandomAccess
                },
                declared,
                1,
                &[],
            );
            assert!(document.pages().is_empty());
        }
    }
    assert_info(
        &decoder.inspect_embedded(&[]).unwrap(),
        Organization::Sequential,
        None,
        0,
        &[],
    );
    for (width, height) in [(0, 0), (0, 3), (9, 0)] {
        let document = decoder
            .decode_embedded(&embedded(&[page(0, 1, width, height), end(1, 1)]))
            .unwrap();
        assert_info(
            document.info(),
            Organization::Sequential,
            None,
            2,
            &[(1, width, height, 300, 600, false)],
        );
        assert_eq!(document.pages().len(), 1);
        assert_pixels(&document.pages()[0], 1, width, height, &[]);
    }
}

#[test]
fn unknown_length_mmr_envelope_has_exact_terminator_and_row_count_errors() {
    let mut payload = vec![0; 18];
    payload[..4].copy_from_slice(&8_u32.to_be_bytes());
    payload[4..8].copy_from_slice(&1_u32.to_be_bytes());
    payload[17] = 1;
    for (suffix, expected) in [
        (vec![], MalformedError::MissingGenericRegionTerminator),
        (
            vec![0x80; 7],
            MalformedError::MissingGenericRegionTerminator,
        ),
        (
            vec![0x80, 0, 0, 0, 0, 0, 2],
            MalformedError::GenericRegionRowCount {
                declared: 1,
                actual: 2,
            },
        ),
    ] {
        let mut region = Segment::new(1, 38, 1, payload.clone());
        region.payload.extend(suffix);
        region.unknown_length = true;
        let decoder = Decoder::default();
        let local = embedded(&[page(0, 1, 8, 1), region.clone()]);
        assert_malformed(decoder.inspect_embedded(&local).unwrap_err(), &expected);
        assert_malformed(decoder.decode_embedded(&local).unwrap_err(), &expected);
        for sequential in [true, false] {
            let mut segments = vec![page(0, 1, 8, 1), region.clone()];
            if !sequential {
                segments.push(eof(2));
            }
            let bytes = standalone(sequential, None, &segments);
            assert_malformed(decoder.inspect(&bytes).unwrap_err(), &expected);
            assert_malformed(decoder.decode(&bytes).unwrap_err(), &expected);
        }
    }
    let mut region = Segment::new(1, 38, 1, payload);
    // One white row, EOFB, then the byte-aligned separator and row count.
    region
        .payload
        .extend_from_slice(&[0x80, 0x08, 0x00, 0x80, 0, 0, 0, 0, 0, 1]);
    region.unknown_length = true;
    let segments = [page(0, 1, 8, 1), region, end(2, 1), eof(3)];
    for sequential in [true, false] {
        let document = Decoder::default()
            .decode(&standalone(sequential, Some(1), &segments))
            .unwrap();
        assert_info(
            document.info(),
            if sequential {
                Organization::Sequential
            } else {
                Organization::RandomAccess
            },
            Some(1),
            4,
            &[(1, 8, 1, 300, 600, false)],
        );
        assert_pixels(&document.pages()[0], 1, 8, 1, &[0]);
    }
}

#[test]
fn trailing_bytes_are_counted_after_each_organization_payload_area() {
    for sequential in [true, false] {
        let mut bytes = standalone(sequential, Some(1), &[page(0, 1, 8, 1), end(1, 1), eof(2)]);
        bytes.extend_from_slice(&[0xab, 0xcd, 0xef]);
        let decoder = Decoder::default();
        assert_malformed(
            decoder.inspect(&bytes).unwrap_err(),
            &MalformedError::TrailingData(3),
        );
        assert_malformed(
            decoder.decode_reader(&mut Cursor::new(bytes)).unwrap_err(),
            &MalformedError::TrailingData(3),
        );
    }
}

#[test]
fn page_segment_and_structural_work_limits_have_exact_boundary_values() {
    let segments = [page(0, 1, 9, 1), end(1, 1), eof(2)];
    for sequential in [true, false] {
        let bytes = standalone(sequential, Some(1), &segments);
        assert_eq!(bytes.len(), 65);
        let mut limits = Limits::default();
        limits.max_segments = 3;
        limits.max_pages = 1;
        limits.max_work_units = 68;
        let decoder = Decoder::new(limits);
        assert_info(
            &decoder.inspect(&bytes).unwrap(),
            if sequential {
                Organization::Sequential
            } else {
                Organization::RandomAccess
            },
            Some(1),
            3,
            &[(1, 9, 1, 300, 600, false)],
        );
        limits.max_segments = 2;
        assert_limit(
            Decoder::new(limits).inspect(&bytes).unwrap_err(),
            LimitKind::Segments,
            2,
            3,
        );
        limits.max_segments = 3;
        limits.max_pages = 0;
        assert_limit(
            Decoder::new(limits).inspect(&bytes).unwrap_err(),
            LimitKind::Pages,
            0,
            1,
        );
        limits.max_pages = 1;
        limits.max_work_units = 67;
        assert_limit(
            Decoder::new(limits).inspect(&bytes).unwrap_err(),
            LimitKind::Work,
            67,
            68,
        );
        limits.max_work_units = 64;
        assert_limit(
            Decoder::new(limits).inspect(&bytes).unwrap_err(),
            LimitKind::Work,
            64,
            65,
        );
    }
}

//! Project-generated envelope cases with literal metadata, pixels and typed errors.

use std::io::Cursor;

use jbig2_rs::{
    DecodeError, Decoder, DocumentInfo, LimitExceeded, LimitKind, Limits, MalformedError,
    Organization, Page, PageSink, UnsupportedFeature,
};

#[allow(dead_code)]
#[path = "../tools/distribution_consumer.rs"]
mod fixture;

#[derive(Clone)]
struct Segment {
    number: u32,
    kind: u8,
    page: u32,
    retention: u8,
    references: Vec<u8>,
    payload: Vec<u8>,
    unknown_length: bool,
    deferred: bool,
}

impl Segment {
    fn new(number: u32, kind: u8, page: u32, payload: Vec<u8>) -> Self {
        Self {
            number,
            kind,
            page,
            retention: 1,
            references: Vec::new(),
            payload,
            unknown_length: false,
            deferred: false,
        }
    }

    fn references(mut self, numbers: &[u8], retention: u8) -> Self {
        self.references = numbers.to_vec();
        self.retention = retention;
        self
    }

    fn retaining_references(self, numbers: &[u8]) -> Self {
        let retention = if numbers.len() <= 4 {
            u8::try_from((1_u16 << (numbers.len() + 1)) - 1).unwrap()
        } else {
            u8::MAX
        };
        self.references(numbers, retention)
    }

    fn header(&self) -> Vec<u8> {
        assert!(self.references.len() <= 256);
        assert!(self.references.is_empty() || self.number <= 256);
        let mut bytes = self.number.to_be_bytes().to_vec();
        let long_page = self.page > 255;
        bytes.push(self.kind | (u8::from(long_page) << 6) | (u8::from(self.deferred) << 7));
        if self.references.len() <= 4 {
            bytes.push((u8::try_from(self.references.len()).unwrap() << 5) | self.retention);
        } else {
            assert_eq!(self.retention, u8::MAX);
            bytes.extend_from_slice(
                &(0xe000_0000 | u32::try_from(self.references.len()).unwrap()).to_be_bytes(),
            );
            let flag_count = self.references.len() + 1;
            for first in (0..flag_count).step_by(8) {
                let count = (flag_count - first).min(8);
                bytes.push(u8::try_from((1_u16 << count) - 1).unwrap());
            }
        }
        bytes.extend_from_slice(&self.references);
        if long_page {
            bytes.extend_from_slice(&self.page.to_be_bytes());
        } else {
            bytes.push(u8::try_from(self.page).unwrap());
        }
        let length = if self.unknown_length {
            u32::MAX
        } else {
            u32::try_from(self.payload.len()).unwrap()
        };
        bytes.extend_from_slice(&length.to_be_bytes());
        bytes
    }
}

fn embedded(segments: &[Segment]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for segment in segments {
        bytes.extend(segment.header());
        bytes.extend_from_slice(&segment.payload);
    }
    bytes
}

fn standalone(sequential: bool, pages: Option<u32>, segments: &[Segment]) -> Vec<u8> {
    let mut bytes = b"\x97JB2\r\n\x1a\n".to_vec();
    bytes.push(u8::from(sequential) | (u8::from(pages.is_none()) << 1));
    if let Some(count) = pages {
        bytes.extend_from_slice(&count.to_be_bytes());
    }
    if sequential {
        bytes.extend(embedded(segments));
    } else {
        for segment in segments {
            bytes.extend(segment.header());
        }
        for segment in segments {
            bytes.extend_from_slice(&segment.payload);
        }
    }
    bytes
}

fn page(number: u32, association: u32, width: u32, height: u32) -> Segment {
    let mut payload = width.to_be_bytes().to_vec();
    payload.extend_from_slice(&height.to_be_bytes());
    payload.extend_from_slice(&300_u32.to_be_bytes());
    payload.extend_from_slice(&600_u32.to_be_bytes());
    payload.extend_from_slice(&[0, 0, 0]);
    Segment::new(number, 48, association, payload)
}

fn striped_page(number: u32, height: u32, maximum: u16) -> Segment {
    let mut segment = page(number, 1, 9, height);
    segment.payload[17..19].copy_from_slice(&(0x8000 | maximum).to_be_bytes());
    segment
}

fn end(number: u32, association: u32) -> Segment {
    Segment::new(number, 49, association, Vec::new())
}

fn eof(number: u32) -> Segment {
    Segment::new(number, 51, 0, Vec::new())
}

fn reference_payload(kind: u8) -> Vec<u8> {
    match kind {
        48 => page(0, 1, 8, 1).payload,
        50 | 52 | 62 => vec![0; 4],
        53 => vec![0x03, 0, 0, 0, 0, 0, 0, 0, 1, 0b0100_0101, 0],
        _ => Vec::new(),
    }
}

fn assert_reference_metadata(segments: &[Segment]) {
    let decoder = Decoder::default();
    let count = u32::try_from(segments.len()).unwrap();
    for sequential in [true, false] {
        let bytes = standalone(sequential, Some(1), segments);
        let info = decoder.inspect(&bytes).unwrap();
        assert_info(
            &info,
            if sequential {
                Organization::Sequential
            } else {
                Organization::RandomAccess
            },
            Some(1),
            count,
            &[(1, 8, 1, 300, 600, false)],
        );
        assert_eq!(
            decoder.inspect_reader(&mut Cursor::new(&bytes)).unwrap(),
            info
        );
    }
    let info = decoder.inspect_embedded(&embedded(segments)).unwrap();
    assert_info(
        &info,
        Organization::Sequential,
        None,
        count,
        &[(1, 8, 1, 300, 600, false)],
    );
}

fn stripe(number: u32, row: u32) -> Segment {
    Segment::new(number, 50, 1, row.to_be_bytes().to_vec())
}

fn profiles(number: u32, association: u32, values: &[u32]) -> Segment {
    let mut payload = u32::try_from(values.len()).unwrap().to_be_bytes().to_vec();
    for value in values {
        payload.extend_from_slice(&value.to_be_bytes());
    }
    Segment::new(number, 52, association, payload)
}

fn extension(number: u32, association: u32, kind: u32, data: &[u8]) -> Segment {
    let mut payload = kind.to_be_bytes().to_vec();
    payload.extend_from_slice(data);
    Segment::new(number, 62, association, payload)
}

type Metadata = (u32, u32, u32, u32, u32, bool);

fn assert_info(
    info: &DocumentInfo,
    organization: Organization,
    declared: Option<u32>,
    count: u32,
    expected: &[Metadata],
) {
    assert_eq!(info.organization(), organization);
    assert_eq!(info.declared_page_count(), declared);
    assert_eq!(info.segment_count(), count);
    let actual: Vec<_> = info
        .pages()
        .iter()
        .map(|page| {
            (
                page.number(),
                page.width(),
                page.height(),
                page.x_resolution(),
                page.y_resolution(),
                page.is_striped(),
            )
        })
        .collect();
    assert_eq!(actual, expected);
}

fn assert_pixels(page: &Page, number: u32, width: u32, height: u32, bytes: &[u8]) {
    assert_eq!(page.number(), number);
    assert_eq!(page.bitmap().width(), width);
    assert_eq!(page.bitmap().height(), height);
    assert_eq!(page.bitmap().stride(), width.div_ceil(8) as usize);
    assert_eq!(page.bitmap().bytes(), bytes);
}

fn assert_malformed(error: DecodeError, expected: &MalformedError) {
    match error {
        DecodeError::Malformed(actual) => assert_eq!(&actual, expected),
        other => panic!("expected {expected:?}, got {other:?}"),
    }
}

fn assert_limit(error: DecodeError, kind: LimitKind, limit: u64, actual: u64) {
    match error {
        DecodeError::LimitExceeded(detail) => {
            assert_eq!(
                detail,
                LimitExceeded {
                    kind,
                    limit,
                    actual
                }
            );
        }
        other => panic!("expected {kind:?} limit {limit}, actual {actual}; got {other:?}"),
    }
}

fn reject(segments: &[Segment], expected: &MalformedError) {
    let decoder = Decoder::default();
    for sequential in [true, false] {
        let bytes = standalone(sequential, None, segments);
        assert_malformed(decoder.inspect(&bytes).unwrap_err(), expected);
        assert_malformed(decoder.decode(&bytes).unwrap_err(), expected);
        let mut calls = 0;
        let error = decoder
            .decode_with_sink(&bytes, &mut |_| {
                calls += 1;
                Ok(())
            })
            .unwrap_err();
        assert_malformed(error, expected);
        assert_eq!(calls, 0, "envelope rejection precedes page delivery");
    }
    let bytes = embedded(segments);
    assert_malformed(decoder.inspect_embedded(&bytes).unwrap_err(), expected);
    assert_malformed(decoder.decode_embedded(&bytes).unwrap_err(), expected);
    let (valid, _) = fixture::generated_page();
    let recovered = decoder.decode(&valid).unwrap();
    assert_pixels(&recovered.pages()[0], 1, 8, 1, &[0]);
}

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
fn profile_supersets_and_empty_profile_sets_are_accepted_without_changing_pixels() {
    let decoder = Decoder::default();
    for (global, local) in [
        (vec![], vec![]),
        (vec![], vec![0xdead_beef]),
        (
            vec![1, 0x1020_3040, u32::MAX],
            vec![9, u32::MAX, 0x1020_3040, 1, 9],
        ),
    ] {
        let segments = [
            profiles(0, 0, &global),
            page(1, 1, 9, 2),
            profiles(2, 1, &local),
            end(3, 1),
            eof(4),
        ];
        for sequential in [true, false] {
            let bytes = standalone(sequential, Some(1), &segments);
            let document = decoder.decode(&bytes).unwrap();
            assert_info(
                document.info(),
                if sequential {
                    Organization::Sequential
                } else {
                    Organization::RandomAccess
                },
                Some(1),
                5,
                &[(1, 9, 2, 300, 600, false)],
            );
            assert_pixels(&document.pages()[0], 1, 9, 2, &[0; 4]);
        }
        let globals = decoder.parse_globals(&embedded(&segments[..1])).unwrap();
        assert_eq!(globals.segment_count(), 1);
        let local = embedded(&[
            segments[1].clone(),
            segments[3].clone(),
            segments[4].clone(),
        ]);
        let document = decoder
            .decode_embedded_with_globals(&local, &globals)
            .unwrap();
        assert_info(
            document.info(),
            Organization::Sequential,
            None,
            3,
            &[(1, 9, 2, 300, 600, false)],
        );
        assert_pixels(&document.pages()[0], 1, 9, 2, &[0; 4]);
    }
}

#[test]
fn misplaced_duplicate_and_non_superset_profiles_have_exact_segment_numbers() {
    let cases = [
        (
            vec![extension(0, 0, 0, &[]), profiles(1, 0, &[]), eof(2)],
            1,
        ),
        (vec![profiles(0, 1, &[]), eof(1)], 0),
        (vec![profiles(0, 0, &[]), profiles(1, 0, &[]), eof(2)], 1),
        (
            vec![
                profiles(0, 0, &[]),
                page(1, 1, 8, 1),
                profiles(2, 1, &[]),
                profiles(3, 1, &[]),
                end(4, 1),
                eof(5),
            ],
            3,
        ),
        (
            vec![
                profiles(0, 0, &[1, 2]),
                page(1, 1, 8, 1),
                profiles(2, 1, &[3, 1]),
                end(3, 1),
                eof(4),
            ],
            2,
        ),
        (
            vec![
                profiles(0, 0, &[1]),
                page(1, 1, 8, 1),
                profiles(2, 1, &[]),
                end(3, 1),
                eof(4),
            ],
            2,
        ),
    ];
    for (segments, number) in cases {
        reject(&segments, &MalformedError::InvalidProfilesSegment(number));
    }
}

#[test]
fn profile_lengths_report_exact_u64_expected_lengths_before_allocation() {
    for length in 0..4 {
        reject(
            &[Segment::new(0, 52, 0, vec![0; length]), eof(1)],
            &MalformedError::InvalidSegmentDataLength {
                segment_type: 52,
                expected: 4,
                actual: length as u64,
            },
        );
    }
    for (payload, expected) in [
        (1_u32.to_be_bytes().to_vec(), 8),
        (u32::MAX.to_be_bytes().to_vec(), 17_179_869_184),
        (vec![0; 8], 4),
    ] {
        let actual = payload.len() as u64;
        reject(
            &[Segment::new(0, 52, 0, payload), eof(1)],
            &MalformedError::InvalidSegmentDataLength {
                segment_type: 52,
                expected,
                actual,
            },
        );
    }
}

#[test]
fn optional_extensions_and_both_comment_encodings_preserve_literal_output() {
    let mut extensions = Vec::new();
    for (index, kind) in [0, 1, 0x2000_0001, 0x6000_0000, 0x7fff_ffff]
        .into_iter()
        .enumerate()
    {
        extensions.push(extension(
            u32::try_from(index).unwrap(),
            0,
            kind,
            &[0xff, 0, 0x80],
        ));
    }
    extensions.push(extension(
        5,
        0,
        0x2000_0000,
        b"author\0project\0title\0generated\0\0",
    ));
    extensions.push(extension(6, 0, 0x2000_0000, &[0]));
    extensions.push(extension(7, 0, 0x2000_0002, &[0, 0]));
    extensions.push(extension(
        8,
        0,
        0x2000_0002,
        &[0x01, 0, 0, 0, 0, 0x41, 0, 0, 0, 0],
    ));
    extensions.extend([page(9, 1, 9, 1), end(10, 1), eof(11)]);
    for sequential in [true, false] {
        let document = Decoder::default()
            .decode(&standalone(sequential, Some(1), &extensions))
            .unwrap();
        assert_info(
            document.info(),
            if sequential {
                Organization::Sequential
            } else {
                Organization::RandomAccess
            },
            Some(1),
            12,
            &[(1, 9, 1, 300, 600, false)],
        );
        assert_pixels(&document.pages()[0], 1, 9, 1, &[0, 0]);
    }
}

#[test]
fn malformed_comment_framing_is_not_an_ignored_optional_extension() {
    for data in [
        &b""[..],
        &b"x"[..],
        &b"key\0\0"[..],
        &b"k\0v\0"[..],
        &b"\0trailing"[..],
        &b"k\0v\0\0trailing"[..],
    ] {
        reject(
            &[extension(0, 0, 0x2000_0000, data), eof(1)],
            &MalformedError::InvalidCommentExtension,
        );
    }
    for data in [
        &[][..],
        &[0][..],
        &[0, 1][..],
        &[0, 1, 0, 0][..],
        &[0, 1, 0, 0, 0, 0][..],
        &[0, 0, 0, 1][..],
    ] {
        reject(
            &[extension(0, 0, 0x2000_0002, data), eof(1)],
            &MalformedError::InvalidCommentExtension,
        );
    }
}

#[test]
fn extension_length_and_necessary_flag_errors_preserve_all_bits() {
    for length in 0..4 {
        reject(
            &[Segment::new(0, 62, 0, vec![0; length]), eof(1)],
            &MalformedError::InvalidSegmentDataLength {
                segment_type: 62,
                expected: 4,
                actual: length as u64,
            },
        );
    }
    for kind in [0x8000_0000, 0xc123_4567] {
        reject(
            &[extension(0, 0, kind, &[]), eof(1)],
            &MalformedError::InvalidExtensionType(kind),
        );
    }
    for kind in [0xa000_0000, 0xffff_ffff] {
        let segments = [extension(0, 0, kind, &[]), eof(1)];
        for sequential in [true, false] {
            let decoder = Decoder::default();
            let bytes = standalone(sequential, None, &segments);
            for error in [
                decoder.inspect(&bytes).unwrap_err(),
                decoder.decode(&bytes).unwrap_err(),
            ] {
                assert!(matches!(
                    error, DecodeError::Unsupported(UnsupportedFeature::Extension(actual)) if actual == kind
                ));
            }
        }
    }
}

#[test]
fn page_lifecycle_rejections_are_exact_and_precede_all_sink_effects() {
    let cases = [
        (
            vec![end(0, 1), eof(1)],
            MalformedError::MissingPageInformation(1),
        ),
        (
            vec![page(0, 1, 8, 1), page(1, 1, 8, 1), end(2, 1), eof(3)],
            MalformedError::DuplicatePageBoundary {
                page: 1,
                segment_type: 48,
            },
        ),
        (
            vec![page(0, 1, 8, 1), end(1, 1), end(2, 1), eof(3)],
            MalformedError::SegmentAfterEndOfPage {
                segment: 2,
                page: 1,
            },
        ),
        (
            vec![page(0, 1, 8, 1), end(1, 1), extension(2, 1, 0, &[]), eof(3)],
            MalformedError::SegmentAfterEndOfPage {
                segment: 2,
                page: 1,
            },
        ),
        (
            vec![page(0, 2, 8, 1), end(1, 2), eof(2)],
            MalformedError::MissingPageInformation(1),
        ),
        (
            vec![page(0, 1, 8, 1), eof(1)],
            MalformedError::MissingEndOfPage(1),
        ),
        (
            vec![page(0, 0, 8, 1), eof(1)],
            MalformedError::InvalidPageAssociation {
                segment: 0,
                page: 0,
            },
        ),
        (
            vec![Segment::new(0, 51, 257, vec![])],
            MalformedError::InvalidPageAssociation {
                segment: 0,
                page: 257,
            },
        ),
    ];
    for (segments, expected) in cases {
        reject(&segments, &expected);
    }
}

#[test]
fn stripe_metadata_and_zero_padding_follow_independently_declared_rows() {
    for height in [3, u32::MAX] {
        let segments = [
            striped_page(0, height, 2),
            stripe(1, 0),
            stripe(2, 2),
            end(3, 1),
            eof(4),
        ];
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
                5,
                &[(1, 9, 3, 300, 600, true)],
            );
            assert_pixels(&document.pages()[0], 1, 9, 3, &[0; 6]);
        }
    }
}

#[test]
fn stripe_end_boundaries_and_unknown_height_failures_are_typed() {
    for (first, rows, invalid) in [
        (page(0, 1, 9, 3), vec![0], 0),
        (striped_page(0, 8, 2), vec![3], 3),
        (striped_page(0, 8, 2), vec![1, 1], 1),
        (striped_page(0, 8, 2), vec![1, 0], 0),
        (striped_page(0, 8, 2), vec![1, 4], 4),
        (striped_page(0, 3, 3), vec![3], 3),
        (striped_page(0, u32::MAX, 2), vec![], 0),
    ] {
        let mut segments = vec![first];
        for (index, row) in rows.into_iter().enumerate() {
            segments.push(stripe(u32::try_from(index + 1).unwrap(), row));
        }
        let next = u32::try_from(segments.len()).unwrap();
        segments.extend([end(next, 1), eof(next + 1)]);
        reject(&segments, &MalformedError::InvalidStripeEndRow(invalid));
    }
    reject(
        &[page(0, 1, 9, u32::MAX), end(1, 1), eof(2)],
        &MalformedError::UnknownHeightWithoutStriping,
    );
    let mut limits = Limits::default();
    limits.max_width = 8;
    assert_limit(
        Decoder::new(limits)
            .inspect_embedded(&embedded(&[
                striped_page(0, u32::MAX, 2),
                stripe(1, 0),
                end(2, 1),
            ]))
            .unwrap_err(),
        LimitKind::Width,
        8,
        9,
    );
}

#[test]
fn reference_retention_uses_the_last_reference_not_the_first_or_wire_order() {
    for deferred in [false, true] {
        let first = extension(0, 0, 0, &[]);
        let mut middle = extension(1, 0, 0, &[]).references(&[0], 3);
        middle.deferred = deferred;
        let last = extension(2, 0, 0, &[]).references(&[0, 1], 0);
        for order in [
            [first.clone(), middle.clone(), last.clone()],
            [last.clone(), first.clone(), middle.clone()],
        ] {
            let globals = Decoder::default().parse_globals(&embedded(&order)).unwrap();
            assert_eq!(globals.segment_count(), 3);
        }
        middle.retention = 1;
        reject(
            &[first.clone(), middle.clone(), last.clone(), eof(3)],
            &MalformedError::InvalidRetentionFlags {
                segment: 1,
                referred: 0,
            },
        );
        let mut released = first;
        released.retention = 0;
        middle.retention = 3;
        reject(
            &[released, middle, last, eof(3)],
            &MalformedError::InvalidRetentionFlags {
                segment: 0,
                referred: 0,
            },
        );
    }
}

#[test]
fn embedded_duplicate_numbers_and_cross_page_references_are_exact() {
    let decoder = Decoder::default();
    let duplicate = embedded(&[extension(7, 0, 0, &[]), extension(7, 0, 0, &[])]);
    assert_malformed(
        decoder.inspect_embedded(&duplicate).unwrap_err(),
        &MalformedError::DuplicateSegmentNumber(7),
    );
    assert_malformed(
        decoder.parse_globals(&duplicate).unwrap_err(),
        &MalformedError::DuplicateSegmentNumber(7),
    );
    let segments = [
        page(0, 2, 8, 1),
        extension(1, 2, 0, &[]),
        page(2, 1, 8, 1),
        extension(3, 1, 0, &[]).references(&[1], 3),
        end(4, 1),
        end(5, 2),
        eof(6),
    ];
    reject(
        &segments,
        &MalformedError::InvalidPageAssociation {
            segment: 3,
            page: 1,
        },
    );
}

#[test]
fn reference_roles_reject_invalid_consumers_before_page_delivery() {
    for source_kind in [0_u8, 4, 16, 36, 38, 53] {
        for consumer_kind in [
            0_u8, 4, 6, 7, 16, 20, 22, 23, 36, 38, 39, 40, 42, 43, 48, 49, 50, 52, 53, 62,
        ] {
            let source_page = u32::from(!matches!(source_kind, 0 | 16 | 53));
            let segments = [
                page(1, 1, 8, 1),
                Segment::new(2, source_kind, source_page, reference_payload(source_kind)),
                Segment::new(3, consumer_kind, 1, reference_payload(consumer_kind))
                    .retaining_references(&[2]),
                end(4, 1),
                eof(5),
            ];
            let allowed = match consumer_kind {
                0 | 4 | 6 | 7 => matches!(source_kind, 0 | 53),
                20 | 22 | 23 => source_kind == 16,
                40 | 42 | 43 => matches!(source_kind, 4 | 36),
                62 => true,
                _ => false,
            };
            if allowed {
                assert_reference_metadata(&segments);
            } else {
                reject(&segments, &MalformedError::InvalidSegmentReferences(3));
            }
        }
    }
}

#[test]
fn only_intermediate_region_roles_can_supply_explicit_refinement_references() {
    for source_kind in [4_u8, 6, 20, 22, 36, 38, 40, 42] {
        let prerequisite = match source_kind {
            20 | 22 => Segment::new(2, 16, 0, Vec::new()),
            40 => Segment::new(2, 36, 1, Vec::new()),
            _ => extension(2, 0, 0, &[]),
        };
        let source = Segment::new(3, source_kind, 1, Vec::new());
        let source = if matches!(source_kind, 20 | 22 | 40) {
            source.retaining_references(&[2])
        } else {
            source
        };
        for consumer_kind in [40_u8, 42, 43] {
            let segments = [
                page(1, 1, 8, 1),
                prerequisite.clone(),
                source.clone(),
                Segment::new(4, consumer_kind, 1, Vec::new()).retaining_references(&[3]),
                end(5, 1),
                eof(6),
            ];
            if matches!(source_kind, 4 | 20 | 36 | 40) {
                assert_reference_metadata(&segments);
            } else {
                reject(&segments, &MalformedError::InvalidSegmentReferences(4));
            }
        }
    }
}

#[test]
fn reference_table_limits_count_tables_not_dictionaries_in_both_header_forms() {
    for consumer_kind in [0_u8, 4, 6, 7] {
        for table_count in [0_u8, 1, 4, 5, 8, 9] {
            let mut segments = vec![page(1, 1, 8, 1), Segment::new(2, 0, 0, Vec::new())];
            for index in 0..table_count {
                segments.push(Segment::new(
                    u32::from(index) + 3,
                    53,
                    0,
                    reference_payload(53),
                ));
            }
            let number = u32::from(table_count) + 3;
            let references: Vec<_> = (2..table_count + 3).collect();
            segments.push(
                Segment::new(number, consumer_kind, 1, Vec::new())
                    .retaining_references(&references),
            );
            segments.extend([end(number + 1, 1), eof(number + 2)]);
            let maximum = if consumer_kind == 0 { 4 } else { 8 };
            if table_count <= maximum {
                assert_reference_metadata(&segments);
            } else {
                reject(&segments, &MalformedError::InvalidSegmentReferences(number));
            }
        }
    }
}

#[test]
fn halftone_and_refinement_reference_cardinality_is_exact() {
    for consumer_kind in [20_u8, 22, 23, 40, 42, 43] {
        for references in [&[][..], &[2][..], &[2, 3][..]] {
            let (source_kind, source_page) = if matches!(consumer_kind, 20 | 22 | 23) {
                (16, 0)
            } else {
                (36, 1)
            };
            let segments = [
                page(1, 1, 8, 1),
                Segment::new(2, source_kind, source_page, Vec::new()),
                Segment::new(3, source_kind, source_page, Vec::new()),
                Segment::new(4, consumer_kind, 1, Vec::new()).retaining_references(references),
                end(5, 1),
                eof(6),
            ];
            if references.len() == 1 || (references.is_empty() && matches!(consumer_kind, 42 | 43))
            {
                assert_reference_metadata(&segments);
            } else {
                reject(&segments, &MalformedError::InvalidSegmentReferences(4));
            }
        }
    }
}

#[test]
fn global_reference_roles_are_rechecked_without_mutating_shared_state() {
    let decoder = Decoder::default();
    for source_kind in [0_u8, 16, 53] {
        let globals = decoder
            .parse_globals(&embedded(&[Segment::new(
                2,
                source_kind,
                0,
                reference_payload(source_kind),
            )]))
            .unwrap();
        for consumer_kind in [0_u8, 4, 6, 7, 20, 22, 23, 40, 42, 43] {
            let bytes = embedded(&[
                page(3, 1, 8, 1),
                Segment::new(4, consumer_kind, 1, Vec::new()).retaining_references(&[2]),
                end(5, 1),
            ]);
            let allowed = if matches!(consumer_kind, 0 | 4 | 6 | 7) {
                matches!(source_kind, 0 | 53)
            } else {
                matches!(consumer_kind, 20 | 22 | 23) && source_kind == 16
            };
            let mut context = decoder.context();
            if allowed {
                let info = decoder
                    .inspect_embedded_with_globals(&bytes, &globals)
                    .unwrap();
                assert_info(
                    &info,
                    Organization::Sequential,
                    None,
                    3,
                    &[(1, 8, 1, 300, 600, false)],
                );
                assert_eq!(
                    context
                        .inspect_embedded_with_globals(&bytes, &globals)
                        .unwrap(),
                    info
                );
            } else {
                let expected = MalformedError::InvalidSegmentReferences(4);
                assert_malformed(
                    decoder
                        .inspect_embedded_with_globals(&bytes, &globals)
                        .unwrap_err(),
                    &expected,
                );
                assert_malformed(
                    decoder
                        .decode_embedded_with_globals(&bytes, &globals)
                        .unwrap_err(),
                    &expected,
                );
                assert_malformed(
                    context
                        .inspect_embedded_with_globals(&bytes, &globals)
                        .unwrap_err(),
                    &expected,
                );
                assert_malformed(
                    context
                        .decode_embedded_with_globals(&bytes, &globals)
                        .unwrap_err(),
                    &expected,
                );
            }
            assert_eq!(globals.segment_count(), 1);
            assert_malformed(
                context.inspect_embedded(&bytes).unwrap_err(),
                &MalformedError::MissingReferredSegment {
                    segment: 4,
                    referred: 2,
                },
            );
            let (valid, _) = fixture::generated_page();
            let recovered = context.decode(&valid).unwrap();
            assert_pixels(&recovered.pages()[0], 1, 8, 1, &[0]);
        }
    }
}

#[test]
fn nonempty_global_reference_chains_reuse_immutable_state_and_exact_depth() {
    let decoder = Decoder::default();
    let global_bytes = embedded(&[
        extension(0, 0, 0, &[]),
        extension(1, 0, 0, &[]).references(&[0], 3),
    ]);
    assert_eq!(global_bytes.len(), 31);
    let globals = decoder.parse_globals(&global_bytes).unwrap();
    assert_eq!(globals.segment_count(), 2);
    let bytes = embedded(&[
        page(2, 1, 9, 1),
        extension(3, 1, 0, &[]).references(&[1], 3),
        end(4, 1),
    ]);
    let mut limits = Limits::default();
    limits.max_reference_graph_depth = 3;
    let bounded = Decoder::new(limits);
    let mut context = bounded.context();
    for _ in 0..3 {
        let info = context
            .inspect_embedded_with_globals(&bytes, &globals)
            .unwrap();
        assert_info(
            &info,
            Organization::Sequential,
            None,
            3,
            &[(1, 9, 1, 300, 600, false)],
        );
        let document = context
            .decode_embedded_with_globals(&bytes, &globals)
            .unwrap();
        assert_pixels(&document.pages()[0], 1, 9, 1, &[0, 0]);
        assert_malformed(
            context.inspect_embedded(&bytes).unwrap_err(),
            &MalformedError::MissingReferredSegment {
                segment: 3,
                referred: 1,
            },
        );
        context.reset();
        context.release_scratch();
        assert_eq!(context.retained_scratch_bytes(), 0);
        assert_eq!(context.limits(), &limits);
    }
    limits.max_reference_graph_depth = 2;
    let bounded = Decoder::new(limits);
    assert_limit(
        bounded
            .inspect_embedded_with_globals(&bytes, &globals)
            .unwrap_err(),
        LimitKind::ReferenceDepth,
        2,
        3,
    );
    assert_limit(
        bounded
            .decode_embedded_with_globals(&bytes, &globals)
            .unwrap_err(),
        LimitKind::ReferenceDepth,
        2,
        3,
    );
    assert_eq!(globals.segment_count(), 2);
}

#[test]
fn globals_retained_bytes_are_inclusive_and_rechecked_by_the_consuming_decoder() {
    let bytes = embedded(&[extension(0, 0, 0, &[])]);
    assert_eq!(bytes.len(), 15);
    let mut limits = Limits::default();
    limits.max_retained_state_bytes = 15;
    let decoder = Decoder::new(limits);
    let globals = decoder.parse_globals(&bytes).unwrap();
    assert_eq!(globals.segment_count(), 1);
    assert!(decoder
        .decode_embedded_with_globals(&[], &globals)
        .unwrap()
        .pages()
        .is_empty());
    limits.max_retained_state_bytes = 14;
    let bounded = Decoder::new(limits);
    assert_limit(
        bounded.parse_globals(&bytes).unwrap_err(),
        LimitKind::RetainedState,
        14,
        15,
    );
    assert_limit(
        bounded
            .decode_embedded_with_globals(&[], &globals)
            .unwrap_err(),
        LimitKind::RetainedState,
        14,
        15,
    );
    let mut context = bounded.context();
    assert_limit(
        context.parse_globals(&bytes).unwrap_err(),
        LimitKind::RetainedState,
        14,
        15,
    );
    assert_eq!(context.parse_globals(&[]).unwrap().segment_count(), 0);
    let associated = embedded(&[extension(7, 257, 0, &[])]);
    assert_malformed(
        context.parse_globals(&associated).unwrap_err(),
        &MalformedError::InvalidPageAssociation {
            segment: 7,
            page: 257,
        },
    );
}

#[test]
fn public_reference_limits_accept_exact_counts_and_reject_one_less() {
    let bytes = embedded(&[
        extension(0, 0, 0, &[]),
        extension(1, 0, 0, &[]).references(&[0], 3),
        extension(2, 0, 0, &[]).references(&[0, 1], 7),
    ]);
    let mut limits = Limits::default();
    limits.max_total_references = 3;
    limits.max_references_per_segment = 2;
    limits.max_reference_graph_depth = 3;
    assert_eq!(
        Decoder::new(limits)
            .parse_globals(&bytes)
            .unwrap()
            .segment_count(),
        3
    );
    limits.max_total_references = 2;
    assert_limit(
        Decoder::new(limits).parse_globals(&bytes).unwrap_err(),
        LimitKind::TotalReferences,
        2,
        3,
    );
    limits.max_total_references = 3;
    limits.max_references_per_segment = 1;
    assert_limit(
        Decoder::new(limits).parse_globals(&bytes).unwrap_err(),
        LimitKind::ReferencesPerSegment,
        1,
        2,
    );
    limits.max_references_per_segment = 2;
    limits.max_reference_graph_depth = 2;
    assert_limit(
        Decoder::new(limits).parse_globals(&bytes).unwrap_err(),
        LimitKind::ReferenceDepth,
        2,
        3,
    );
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

struct StopAfterSecond {
    pages: Vec<(u32, Vec<u8>)>,
}

impl PageSink for StopAfterSecond {
    fn write_page(&mut self, page: Page) -> Result<(), DecodeError> {
        let number = page.number();
        let bitmap = page.into_bitmap();
        assert_eq!(
            (bitmap.width(), bitmap.height(), bitmap.stride()),
            (9, 1, 2)
        );
        assert_eq!(bitmap.bytes(), &[0, 0]);
        self.pages.push((number, bitmap.bytes().to_vec()));
        if self.pages.len() == 2 {
            Err(DecodeError::Allocation {
                requested_bytes: 123,
            })
        } else {
            Ok(())
        }
    }
}

#[test]
fn second_page_sink_failure_preserves_prior_effects_and_prevents_third_delivery() {
    let segments = [
        page(0, 1, 9, 1),
        end(1, 1),
        page(2, 2, 9, 1),
        end(3, 2),
        page(4, 3, 9, 1),
        end(5, 3),
        eof(6),
    ];
    let decoder = Decoder::default();
    for sequential in [true, false] {
        let bytes = standalone(sequential, Some(3), &segments);
        for reader in [false, true] {
            let mut sink = StopAfterSecond { pages: Vec::new() };
            let error = if reader {
                decoder.decode_reader_with_sink(&mut Cursor::new(&bytes), &mut sink)
            } else {
                decoder.decode_with_sink(&bytes, &mut sink)
            }
            .unwrap_err();
            assert!(matches!(
                error,
                DecodeError::Allocation {
                    requested_bytes: 123
                }
            ));
            assert_eq!(sink.pages, [(1, vec![0, 0]), (2, vec![0, 0])]);
            let recovered = decoder.decode(&bytes).unwrap();
            assert_eq!(recovered.pages().len(), 3);
            for (index, page) in recovered.pages().iter().enumerate() {
                assert_pixels(page, u32::try_from(index + 1).unwrap(), 9, 1, &[0, 0]);
            }
        }
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

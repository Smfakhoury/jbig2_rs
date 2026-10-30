#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

use super::*;

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

#![allow(clippy::duplicated_attributes)]
#![cfg(test)]

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn decode_generic_segment_oracle(
    segment: &ParsedSegment<'_>,
    pages: &mut RuntimePages,
    buffers: &mut RegionBuffers,
    retained: &mut RetainedBudget,
    work: &mut DecodeWork,
    limits: &Limits,
    mmr_scratch: &mut MmrScratch,
) -> Result<(), DecodeError> {
    decode_generic_segment_buffered(segment, pages, buffers, retained, work, limits, mmr_scratch)
}

#[cfg(test)]
fn clipped_coordinate(origin: u32, offset: u32, bound: u32) -> Option<u32> {
    let coordinate = origin.checked_add(offset)?;
    if coordinate < bound {
        Some(coordinate)
    } else {
        None
    }
}

use super::dispatch::{
    borrowed_symbol_memory, collect_referred_symbols, collect_referred_tables,
    decode_generic_segment_buffered, direct_generic_page_index, first_referred_segment,
    generic_template,
};
use super::rendering::{
    checked_page_write_state, checked_seal_stripe_state, same_combination_operator,
    seal_stripe_on_page,
};
use super::resources::structural_work_totals;
use super::*;

#[test]
fn raw_header_only_direct_dispatch_checks_every_page_gate_without_side_effects() {
    let generous = Limits::default();
    for scenario in 0..=21 {
        let mut header = symbol_reference_segment(&[]).header;
        header.segment_type = SegmentType::ImmediateGenericRegion;
        let mut page = runtime_test_page(9, 1, false, true, CombinationOperator::Or, &generous);
        let (mut width, mut height, mut x, mut y) = (9_u32, 1_u32, 0_u32, 0_u32);
        let (mut operator, mut flags) = (0_u8, 6_u8);
        let mut limits = generous;
        match scenario {
            1 => header.segment_type = SegmentType::IntermediateGenericRegion,
            2 => header.segment_type = SegmentType::ImmediateLosslessGenericRegion,
            3 => flags = 1,
            4 => x = 1,
            5 => y = 1,
            6 => operator = 1,
            7 => page.pristine = false,
            8 => page = runtime_test_page(9, 1, true, true, CombinationOperator::Or, &generous),
            9 => page.last_stripe_end = Some(0),
            10 => page.open_stripe_max_row = Some(0),
            11 => width = 8,
            12 => height = 2,
            13 => {
                page.combination_override = true;
                page.default_operator = CombinationOperator::And;
            }
            14 => page.default_operator = CombinationOperator::And,
            15 => header.page_association = 2,
            16 => {
                operator = 4;
                page.combination_override = true;
            }
            17 => operator = 4,
            18 => limits.max_work_units = 170,
            19 => flags = 0xe0,
            21 => limits.max_work_units = 171,
            _ => {}
        }
        let mut payload = Vec::new();
        for value in [width, height, x, y] {
            payload.extend_from_slice(&value.to_be_bytes());
        }
        payload.extend_from_slice(&[operator, flags]);
        if flags == 6 {
            payload.extend_from_slice(&[0xff, 0]);
        }
        if scenario == 20 {
            payload.clear();
        }
        let segment = ParsedSegment {
            header,
            payload: &payload,
            data: ParsedData::Other,
        };
        let before = page.bitmap.bytes().to_vec();
        let pages = vec![
            (
                7,
                runtime_test_page(1, 1, false, true, CombinationOperator::Or, &generous),
            ),
            (1, page),
        ];
        let work = DecodeWork { used: 0 };
        let expected = if matches!(scenario, 0 | 2 | 13 | 16 | 21) {
            Some(1)
        } else {
            None
        };
        assert_eq!(
            direct_generic_page_index(&segment, &pages, &work, &limits),
            expected,
            "header-only scenario {scenario}"
        );
        assert_eq!(pages[1].1.bitmap.bytes(), before);
        assert_eq!(work.used, 0);
    }
}

#[test]
fn raw_dispatch_selectors_match_complete_codes_and_operator_policy() {
    for code in 0..=u8::MAX {
        let result = generic_template(code);
        if code < 4 {
            assert_eq!(
                result.unwrap(),
                [
                    GenericTemplate::Template0,
                    GenericTemplate::Template1,
                    GenericTemplate::Template2,
                    GenericTemplate::Template3,
                ][usize::from(code)]
            );
        } else {
            assert!(matches!(
                result,
                Err(DecodeError::Malformed(MalformedError::InvalidPageState(
                    "invalid generic template"
                )))
            ));
        }
    }
    let operators = [
        CombinationOperator::Or,
        CombinationOperator::And,
        CombinationOperator::Xor,
        CombinationOperator::Xnor,
        CombinationOperator::Replace,
    ];
    for region in operators {
        for page in operators {
            assert_eq!(same_combination_operator(region, page), region == page);
            for combination_override in [false, true] {
                for refinement in [false, true] {
                    let allowed = region == page || combination_override || refinement;
                    let result =
                        check_combination_operator(combination_override, region, page, refinement);
                    assert_eq!(result.is_ok(), allowed);
                    if !allowed {
                        assert!(matches!(
                            result,
                            Err(DecodeError::Malformed(MalformedError::InvalidPageState(
                                "region overrides the page combination operator"
                            )))
                        ));
                    }
                }
            }
        }
    }
    assert!(require_matching_geometry(true).is_ok());
    assert!(matches!(
        require_matching_geometry(false),
        Err(DecodeError::Malformed(MalformedError::InvalidPageState(
            "refinement geometry differs from its intermediate reference"
        )))
    ));
}

#[test]
#[allow(clippy::too_many_lines)]
fn raw_resource_maps_preserve_key_order_identity_and_duplicate_state() {
    let limits = Limits::default();
    let mut pages = Vec::new();
    let mut tables = Vec::new();
    let mut patterns = Vec::new();
    let mut symbols = Vec::new();
    let mut buffers = Vec::new();
    for (index, number) in [7, 0, u32::MAX].into_iter().enumerate() {
        let width = u32::try_from(index + 1).unwrap();
        insert_runtime_page(
            &mut pages,
            number,
            runtime_test_page(width, 1, true, false, CombinationOperator::Replace, &limits),
        );
        assert!(insert_huffman_table(
            &mut tables,
            number,
            HuffmanTable::standard(crate::huffman::StandardHuffmanTable::A, &limits).unwrap()
        ));
        assert!(insert_patterns(
            &mut patterns,
            number,
            vec![Bitmap::blank(width, 1, &limits).unwrap()]
        ));
        assert!(insert_symbol_resource(
            &mut symbols,
            number,
            SymbolResource {
                exported: vec![Bitmap::blank(width, 1, &limits).unwrap()],
                contexts: None
            }
        ));
        let region = crate::segment_data::RegionInfo {
            width,
            height: 1,
            x: 0,
            y: 0,
            operator: CombinationOperator::Replace,
        };
        buffers.push((
            number,
            RegionBuffer {
                region,
                bitmap: Bitmap::blank(width, 1, &limits).unwrap(),
            },
        ));
    }
    for (index, number) in [7, 0, u32::MAX].into_iter().enumerate() {
        assert_eq!(runtime_page_index(&pages, number), Some(index));
        assert_eq!(
            get_runtime_page(&pages, number).unwrap().bitmap.width(),
            u32::try_from(index + 1).unwrap()
        );
        assert_eq!(
            get_region_buffer(&buffers, number).unwrap().region.width,
            u32::try_from(index + 1).unwrap()
        );
        let first = std::ptr::from_ref(get_huffman_table(&tables, number).unwrap());
        assert!(!insert_huffman_table(
            &mut tables,
            number,
            HuffmanTable::standard(crate::huffman::StandardHuffmanTable::B, &limits).unwrap()
        ));
        assert_eq!(
            std::ptr::from_ref(get_huffman_table(&tables, number).unwrap()),
            first
        );
        assert!(!get_huffman_table(&tables, number).unwrap().has_oob());
        assert!(!insert_patterns(&mut patterns, number, Vec::new()));
        assert_eq!(
            get_patterns(&patterns, number).unwrap()[0].width(),
            u32::try_from(index + 1).unwrap()
        );
        assert!(!insert_symbol_resource(
            &mut symbols,
            number,
            SymbolResource {
                exported: Vec::new(),
                contexts: None
            }
        ));
        assert_eq!(
            get_symbol_resource(&symbols, number).unwrap().exported[0].width(),
            u32::try_from(index + 1).unwrap()
        );
        insert_runtime_page(
            &mut pages,
            number,
            runtime_test_page(9, 1, false, true, CombinationOperator::Or, &limits),
        );
        assert_eq!(get_runtime_page(&pages, number).unwrap().bitmap.width(), 9);
    }
    assert_eq!(
        pages.iter().map(|(key, _)| *key).collect::<Vec<_>>(),
        vec![7, 0, u32::MAX]
    );
    assert_eq!(tables.len(), 3);
    assert_eq!(patterns.len(), 3);
    assert_eq!(symbols.len(), 3);
    for missing in [1, 8, u32::MAX - 1] {
        assert!(get_runtime_page(&pages, missing).is_none());
        assert!(get_region_buffer(&buffers, missing).is_none());
        assert!(get_huffman_table(&tables, missing).is_none());
        assert!(get_patterns(&patterns, missing).is_none());
        assert!(get_symbol_resource(&symbols, missing).is_none());
    }
}

#[test]
fn raw_referred_tables_preserve_order_repeats_missing_keys_and_retention() {
    let limits = Limits::default();
    let mut tables = Vec::new();
    for number in [1, 2, 7] {
        assert!(insert_huffman_table(
            &mut tables,
            number,
            HuffmanTable::standard(crate::huffman::StandardHuffmanTable::A, &limits).unwrap()
        ));
    }
    let choices = [0, 1, 2, 9];
    for encoded in 0..256 {
        let references = [
            choices[encoded % 4],
            choices[(encoded / 4) % 4],
            choices[(encoded / 16) % 4],
            choices[(encoded / 64) % 4],
        ];
        let segment = symbol_reference_segment(&references);
        let collected = collect_referred_tables(&segment, &tables).unwrap();
        let expected: Vec<_> = references
            .iter()
            .filter_map(|number| tables.iter().find(|(key, _)| key == number))
            .map(|(_, table)| table)
            .collect();
        assert_eq!(collected.len(), expected.len());
        for (actual, expected) in collected.iter().zip(expected) {
            assert!(std::ptr::eq(*actual, expected));
        }
        assert_eq!(
            first_referred_segment(&references, 100).unwrap(),
            references[0]
        );
        assert_eq!(
            tables.iter().map(|(key, _)| *key).collect::<Vec<_>>(),
            vec![1, 2, 7]
        );
    }
    assert!(
        collect_referred_tables(&symbol_reference_segment(&[]), &tables)
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        first_referred_segment(&[], u32::MAX),
        Err(DecodeError::Malformed(
            MalformedError::InvalidSegmentReferences(u32::MAX)
        ))
    ));
    let references = [2, 1, 2, 9];
    for mask in 0..16 {
        let mut segment = symbol_reference_segment(&references);
        segment.header.retention_flags.values = std::iter::once(1)
            .chain((0..4).map(|index| (mask >> index) & 1))
            .collect();
        let mut tables = Vec::new();
        for number in [1, 2, 7] {
            insert_huffman_table(
                &mut tables,
                number,
                HuffmanTable::standard(crate::huffman::StandardHuffmanTable::A, &limits).unwrap(),
            );
        }
        let expected: Vec<_> = [1, 2, 7]
            .into_iter()
            .filter(|number| {
                references
                    .iter()
                    .enumerate()
                    .all(|(index, referred)| referred != number || mask & (1 << index) != 0)
            })
            .collect();
        release_referred_tables(&segment, &mut tables);
        assert_eq!(
            tables.iter().map(|(key, _)| *key).collect::<Vec<_>>(),
            expected
        );
        release_referred_tables(&segment, &mut tables);
        assert_eq!(
            tables.iter().map(|(key, _)| *key).collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn raw_page_collector_guards_preserve_order_pixels_and_rejected_totals() {
    for memory_limit in [0, 1, 2, 3, 4, u64::MAX] {
        for decoded_limit in [0, 1, 2, 3, 4, u64::MAX] {
            let limits = Limits {
                max_decoder_managed_memory_bytes: memory_limit,
                max_cumulative_decoded_bitmap_bytes: decoded_limit,
                ..Limits::default()
            };
            let mut collector = PageCollector::new(&limits);
            let mut accepted = 0;
            for number in [u32::MAX, 1] {
                let mut bitmap = Bitmap::blank(9, 1, &Limits::default()).unwrap();
                bitmap.set_pixel(8, 0, true).unwrap();
                let expected = collector.bitmap_bytes + 2;
                let result = collector.write_page(Page { number, bitmap });
                if expected > decoded_limit || expected > memory_limit {
                    let (kind, limit) = if expected > decoded_limit {
                        (crate::LimitKind::DecodedBitmapBytes, decoded_limit)
                    } else {
                        (crate::LimitKind::Memory, memory_limit)
                    };
                    assert!(matches!(result, Err(DecodeError::LimitExceeded(
                        crate::LimitExceeded { kind: actual_kind, limit: actual_limit, actual }
                    )) if actual_kind == kind && actual_limit == limit && actual == expected));
                } else {
                    result.unwrap();
                    accepted += 1;
                }
                assert_eq!(collector.pages.len(), accepted);
                assert_eq!(collector.bitmap_bytes, u64::try_from(accepted).unwrap() * 2);
                for page in &collector.pages {
                    assert_eq!(page.bitmap.bytes(), &[0, 0x80]);
                }
            }
            if accepted == 2 {
                assert_eq!(collector.pages[0].number, u32::MAX);
                assert_eq!(collector.pages[1].number, 1);
            }
        }
    }
    let mut collector = PageCollector::new(&Limits {
        max_decoder_managed_memory_bytes: u64::MAX,
        max_cumulative_decoded_bitmap_bytes: u64::MAX,
        ..Limits::default()
    });
    collector.bitmap_bytes = u64::MAX;
    assert!(matches!(
        collector.write_page(Page {
            number: 1,
            bitmap: Bitmap::blank(1, 1, &Limits::default()).unwrap(),
        }),
        Err(DecodeError::Malformed(MalformedError::IntegerOverflow))
    ));
    assert_eq!(collector.bitmap_bytes, u64::MAX);
    assert!(collector.pages.is_empty());
}

#[test]
fn generic_error_conversion_preserves_every_classification_and_field() {
    let check = |error, expected| match generic_error_into_decode(error) {
        DecodeError::Malformed(MalformedError::InvalidGenericRegion(actual)) => {
            assert_eq!(actual, expected);
        }
        other => panic!("wrong generic error classification: {other:?}"),
    };
    for flags in [0, u8::MAX] {
        check(
            GenericRegionError::ReservedFlags(flags),
            GenericRegionErrorKind::ReservedFlags(flags),
        );
        check(
            GenericRegionError::InvalidFlags(flags),
            GenericRegionErrorKind::InvalidFlags(flags),
        );
    }
    for (error, expected) in [
        (
            GenericRegionError::InvalidAdaptivePixel {
                number: u8::MAX,
                x: i32::MIN,
                y: i32::MAX,
            },
            GenericRegionErrorKind::InvalidAdaptivePixel {
                number: u8::MAX,
                x: i32::MIN,
                y: i32::MAX,
            },
        ),
        (
            GenericRegionError::InvalidSkipDimensions,
            GenericRegionErrorKind::InvalidSkipDimensions,
        ),
        (
            GenericRegionError::InvalidArithmeticTerminator,
            GenericRegionErrorKind::InvalidArithmeticTerminator,
        ),
        (
            GenericRegionError::UnexpectedArithmeticMarker {
                offset: usize::MAX,
                code: 0xab,
            },
            GenericRegionErrorKind::UnexpectedArithmeticMarker {
                offset: usize::MAX,
                code: 0xab,
            },
        ),
        (
            GenericRegionError::RowCountMismatch {
                expected: u32::MAX,
                actual: 0,
            },
            GenericRegionErrorKind::RowCountMismatch {
                expected: u32::MAX,
                actual: 0,
            },
        ),
        (
            GenericRegionError::ContextIndex {
                index: usize::MAX,
                count: 7,
            },
            GenericRegionErrorKind::ContextIndex {
                index: usize::MAX,
                count: 7,
            },
        ),
    ] {
        check(error, expected);
    }
}

#[test]
fn symbol_error_conversion_preserves_reasons_and_structured_fields() {
    let constructors: [fn(&'static str) -> SymbolTextError; 6] = [
        SymbolTextError::InvalidFlags,
        SymbolTextError::InvalidTableAssociation,
        SymbolTextError::InvalidCount,
        SymbolTextError::InvalidEntropyValue,
        SymbolTextError::UnexpectedOutOfBand,
        SymbolTextError::InvalidState,
    ];
    for construct in constructors {
        for reason in ["", "symbol error sentinel"] {
            match symbol_text_error_into_decode(construct(reason)) {
                DecodeError::Malformed(MalformedError::InvalidSymbolText(actual)) => {
                    assert_eq!(actual, reason);
                }
                other => panic!("wrong symbol-text reason classification: {other:?}"),
            }
        }
    }
    assert!(matches!(
        symbol_text_error_into_decode(SymbolTextError::InvalidDimensions),
        DecodeError::Malformed(MalformedError::InvalidSymbolDimensions)
    ));
    for (id, available) in [(0, u32::MAX), (u32::MAX, 7)] {
        assert!(matches!(
            symbol_text_error_into_decode(SymbolTextError::SymbolIdOutOfRange { id, available }),
            DecodeError::Malformed(MalformedError::SymbolIdOutOfRange { id: actual_id, available: actual_count })
                if (actual_id, actual_count) == (id, available)
        ));
    }
    assert!(matches!(
        symbol_text_error_into_decode(SymbolTextError::InvalidSymbolLengthRun),
        DecodeError::Malformed(MalformedError::InvalidSymbolText(
            "invalid symbol-code length run"
        ))
    ));
}

#[test]
fn huffman_error_conversion_preserves_every_table_kind_and_field() {
    use crate::error::HuffmanErrorKind;

    for expected in [
        HuffmanErrorKind::ReservedFlags(u8::MAX),
        HuffmanErrorKind::InvalidBounds {
            low: i32::MIN,
            high: i32::MAX,
        },
        HuffmanErrorKind::InvalidRangeLength(u8::MAX),
        HuffmanErrorKind::InvalidRangePartition,
        HuffmanErrorKind::InvalidLineOrder,
        HuffmanErrorKind::InvalidSpecialLine,
        HuffmanErrorKind::DuplicateSpecialLine,
        HuffmanErrorKind::NoCodes,
        HuffmanErrorKind::Oversubscribed,
        HuffmanErrorKind::TrailingBits(u64::MAX),
        HuffmanErrorKind::NonZeroPadding,
        HuffmanErrorKind::InvalidCode,
    ] {
        match huffman_error_into_decode(HuffmanError::InvalidTable(expected)) {
            DecodeError::Malformed(MalformedError::InvalidHuffman(actual)) => {
                assert_eq!(actual, expected);
            }
            other => panic!("wrong Huffman table classification: {other:?}"),
        }
    }
}

#[test]
fn error_adapters_preserve_existing_decode_errors() {
    use crate::{LimitExceeded, LimitKind};

    let converters: [fn(DecodeError) -> DecodeError; 3] = [
        |error| generic_error_into_decode(GenericRegionError::Decode(error)),
        |error| symbol_text_error_into_decode(SymbolTextError::Decode(error)),
        |error| huffman_error_into_decode(HuffmanError::Decode(error)),
    ];
    for convert in converters {
        assert!(matches!(
            convert(DecodeError::Io(crate::error::IoErrorKind::PermissionDenied)),
            DecodeError::Io(crate::error::IoErrorKind::PermissionDenied)
        ));
        assert!(matches!(
            convert(DecodeError::Allocation {
                requested_bytes: u64::MAX
            }),
            DecodeError::Allocation {
                requested_bytes: u64::MAX
            }
        ));
        assert!(matches!(
            convert(DecodeError::LimitExceeded(LimitExceeded {
                kind: LimitKind::Work, limit: u64::MAX - 1, actual: u64::MAX,
            })),
            DecodeError::LimitExceeded(detail)
                if detail.kind == LimitKind::Work && detail.limit == u64::MAX - 1 && detail.actual == u64::MAX
        ));
        assert!(matches!(
            convert(DecodeError::Malformed(MalformedError::UnexpectedEof {
                offset: u64::MAX,
                needed: 17,
                remaining: 5,
            })),
            DecodeError::Malformed(MalformedError::UnexpectedEof {
                offset: u64::MAX,
                needed: 17,
                remaining: 5
            })
        ));
    }
}

#[test]
fn all_combination_operator_pairs_match_derived_equality() {
    let operators = [
        CombinationOperator::Or,
        CombinationOperator::And,
        CombinationOperator::Xor,
        CombinationOperator::Xnor,
        CombinationOperator::Replace,
    ];
    for left in operators {
        for right in operators {
            assert_eq!(same_combination_operator(left, right), left == right);
        }
    }
}

const SIGNATURE: [u8; 8] = [0x97, 0x4A, 0x42, 0x32, 0x0D, 0x0A, 0x1A, 0x0A];

fn segment_header(number: u32, segment_type: u8, page: u8, length: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&number.to_be_bytes());
    bytes.push(segment_type);
    bytes.push(0);
    bytes.push(page);
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes
}

fn referenced_segment_header(
    number: u32,
    segment_type: u8,
    page: u8,
    referred: u8,
    length: u32,
) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&number.to_be_bytes());
    bytes.push(segment_type);
    bytes.push(0x20);
    bytes.push(referred);
    bytes.push(page);
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes
}

fn page_data() -> [u8; 19] {
    let mut data = [0_u8; 19];
    data[0..4].copy_from_slice(&8_u32.to_be_bytes());
    data[4..8].copy_from_slice(&1_u32.to_be_bytes());
    data[16] = 0x40;
    data
}

#[test]
fn borrowed_symbol_memory_counts_only_pointer_capacity() {
    let retained = 4_096_u64;
    let count = 37_usize;
    let (allocation, peak) =
        borrowed_symbol_memory(count, retained).expect("representable pointer storage");
    assert_eq!(
        allocation,
        u64::try_from(count * core::mem::size_of::<&Bitmap>()).expect("allocation")
    );
    assert_eq!(peak, retained + allocation);
}

fn symbol_reference_segment(references: &[u32]) -> ParsedSegment<'static> {
    assert!(references.len() <= 4);
    let mut bytes = segment_header(100, 6, 1, 0);
    let count = u8::try_from(references.len()).unwrap();
    bytes[5] = (count << 5) | ((1 << (count + 1)) - 1);
    for (index, number) in references.iter().enumerate() {
        bytes.insert(6 + index, u8::try_from(*number).unwrap());
    }
    let mut reader = ByteReader::new(&bytes);
    let header =
        crate::segment_header::parse_segment_header(&mut reader, &Limits::default()).unwrap();
    assert_eq!(reader.position(), bytes.len());
    assert_eq!(header.referred_segments, references);
    ParsedSegment {
        header,
        payload: &[],
        data: ParsedData::Other,
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn referred_symbol_collection_preserves_order_identity_and_exact_borrow_budget() {
    let mut cases = 0;
    let mut borrowed_cases = 0;
    for first_count in 0..=2 {
        for second_count in 0..=2 {
            for third_count in 0..=2 {
                let mut store = Vec::new();
                for (number, count) in [(10, first_count), (20, second_count), (30, third_count)] {
                    let mut exported = Vec::new();
                    for index in 0..count {
                        let mut bitmap = Bitmap::blank(9, 1, &Limits::default()).unwrap();
                        bitmap.data[0] = u8::try_from(number + index).unwrap();
                        bitmap.data[1] = u8::try_from(number).unwrap();
                        exported.push(bitmap);
                    }
                    store.push((
                        number,
                        SymbolResource {
                            exported,
                            contexts: None,
                        },
                    ));
                }
                for length in 0..=4 {
                    for mut encoded in 0..4_usize.pow(length) {
                        let mut references = Vec::new();
                        for _ in 0..length {
                            references.push([10, 20, 30, 99][encoded % 4]);
                            encoded /= 4;
                        }
                        let segment = symbol_reference_segment(&references);
                        let mut expected = Vec::new();
                        let mut nonempty_groups = 0;
                        for number in &references {
                            if let Some((_, resource)) =
                                store.iter().find(|(candidate, _)| candidate == number)
                            {
                                nonempty_groups += usize::from(!resource.exported.is_empty());
                                expected.extend(resource.exported.iter());
                            }
                        }
                        let retained = 4096;
                        let requested_extra = if nonempty_groups > 1 {
                            u64::try_from(expected.len() * core::mem::size_of::<&Bitmap>()).unwrap()
                        } else {
                            0
                        };
                        let probe = collect_referred_symbols(
                            &segment,
                            &store,
                            retained,
                            &Limits::default(),
                        )
                        .unwrap();
                        let observed_extra = match &probe {
                            CollectedSymbols::Contiguous(_) => 0,
                            CollectedSymbols::Borrowed(symbols) => {
                                u64::try_from(symbols.capacity() * core::mem::size_of::<&Bitmap>())
                                    .unwrap()
                            }
                        };
                        assert!(observed_extra >= requested_extra);
                        drop(probe);
                        let peak = retained + observed_extra;
                        let limits = Limits {
                            max_decoder_managed_memory_bytes: peak,
                            ..Limits::default()
                        };
                        let collected =
                            collect_referred_symbols(&segment, &store, retained, &limits).unwrap();
                        if let CollectedSymbols::Borrowed(symbols) = &collected {
                            let actual_extra =
                                u64::try_from(symbols.capacity() * core::mem::size_of::<&Bitmap>())
                                    .unwrap();
                            assert!(
                                retained + actual_extra <= limits.max_decoder_managed_memory_bytes
                            );
                        }
                        match collected.codebook() {
                            SymbolCodebook::Contiguous(symbols) => {
                                assert!(nonempty_groups <= 1);
                                assert_eq!(symbols.len(), expected.len());
                                for (actual, expected) in symbols.iter().zip(&expected) {
                                    assert!(core::ptr::eq(actual, *expected));
                                }
                            }
                            SymbolCodebook::Borrowed(symbols) => {
                                assert!(nonempty_groups > 1);
                                assert_eq!(symbols.len(), expected.len());
                                for (actual, expected) in symbols.iter().zip(&expected) {
                                    assert!(core::ptr::eq(*actual, *expected));
                                }
                            }
                        }
                        if nonempty_groups > 1 {
                            let tight = Limits {
                                max_decoder_managed_memory_bytes: retained + requested_extra - 1,
                                ..Limits::default()
                            };
                            match collect_referred_symbols(&segment, &store, retained, &tight) {
                                Err(DecodeError::LimitExceeded(error)) => {
                                    assert_eq!(error.kind, crate::LimitKind::Memory);
                                    assert_eq!(error.limit, retained + requested_extra - 1);
                                    assert_eq!(error.actual, retained + requested_extra);
                                }
                                Err(other) => {
                                    panic!("unexpected borrowed-pointer error: {other:?}")
                                }
                                Ok(_) => {
                                    panic!("one byte below requested pointer minimum succeeded")
                                }
                            }
                            borrowed_cases += 1;
                        }
                        assert_eq!(segment.header.referred_segments, references);
                        assert!(store
                            .iter()
                            .all(|(_, resource)| resource.contexts.is_none()));
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases, 9207);
    assert_eq!(borrowed_cases, 5460);
}

#[test]
fn borrowed_symbol_memory_numeric_boundaries_match_wide_preflight() {
    let pointer_size = core::mem::size_of::<&Bitmap>();
    let mut cases = 0;
    for count in [0, 1, 37, usize::MAX / pointer_size, usize::MAX] {
        for retained in [0, 4096, u64::MAX - 1, u64::MAX] {
            let requested = (count as u128) * (pointer_size as u128);
            let peak = requested + u128::from(retained);
            let fits = requested <= usize::MAX as u128
                && requested <= u128::from(u64::MAX)
                && peak <= u128::from(u64::MAX);
            match borrowed_symbol_memory(count, retained) {
                Ok((actual_bytes, actual_peak)) => {
                    assert!(fits);
                    assert_eq!(u128::from(actual_bytes), requested);
                    assert_eq!(u128::from(actual_peak), peak);
                }
                Err(DecodeError::Malformed(MalformedError::IntegerOverflow)) => {
                    assert!(!fits);
                }
                other => panic!("unexpected numeric preflight result: {other:?}"),
            }
            cases += 1;
        }
    }
    assert_eq!(cases, 20);
}

#[test]
fn first_matching_symbol_group_controls_duplicate_keys_and_collection_count() {
    let bitmap = Bitmap::blank(9, 1, &Limits::default()).unwrap();
    let symbols = vec![
        (
            10,
            SymbolResource {
                exported: vec![bitmap.clone()],
                contexts: None,
            },
        ),
        (
            10,
            SymbolResource {
                exported: vec![bitmap.clone(), bitmap],
                contexts: None,
            },
        ),
        (
            20,
            SymbolResource {
                exported: Vec::new(),
                contexts: None,
            },
        ),
    ];
    assert_eq!(symbol_index(&symbols, 10), Some(0));
    assert_eq!(get_symbol_resource(&symbols, 10).unwrap().exported.len(), 1);
    assert_eq!(symbol_index(&symbols, 20), Some(2));
    assert_eq!(get_symbol_resource(&symbols, 20).unwrap().exported.len(), 0);
    assert_eq!(symbol_index(&symbols, 99), None);
    assert!(get_symbol_resource(&symbols, 99).is_none());
    let segment = symbol_reference_segment(&[10, 10, 20, 99]);
    let collected = collect_referred_symbols(&segment, &symbols, 0, &Limits::default()).unwrap();
    let SymbolCodebook::Borrowed(bitmaps) = collected.codebook() else {
        panic!("two nonempty reference occurrences require borrowed storage");
    };
    assert_eq!(bitmaps.len(), 2);
    let expected = &symbols[0].1.exported[0];
    assert!(bitmaps
        .iter()
        .all(|bitmap| core::ptr::eq(*bitmap, expected)));
}

fn generic_data(coded: &[u8]) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&8_u32.to_be_bytes());
    data.extend_from_slice(&1_u32.to_be_bytes());
    data.extend_from_slice(&0_u32.to_be_bytes());
    data.extend_from_slice(&0_u32.to_be_bytes());
    data.push(0);
    data.push(1);
    data.extend_from_slice(coded);
    data
}

fn document_with_generic(coded: &[u8]) -> Vec<u8> {
    let generic = generic_data(coded);
    let mut bytes = SIGNATURE.to_vec();
    bytes.push(0x01);
    bytes.extend_from_slice(&1_u32.to_be_bytes());
    bytes.extend_from_slice(&segment_header(0, 48, 1, 19));
    bytes.extend_from_slice(&page_data());
    bytes.extend_from_slice(&segment_header(
        1,
        38,
        1,
        u32::try_from(generic.len()).expect("test payload fits"),
    ));
    bytes.extend_from_slice(&generic);
    bytes.extend_from_slice(&segment_header(2, 49, 1, 0));
    bytes.extend_from_slice(&segment_header(3, 51, 0, 0));
    bytes
}

fn arithmetic_generic_document(width: u32, height: u32, region_flags: u8, coded: &[u8]) -> Vec<u8> {
    let mut page = [0_u8; 19];
    page[0..4].copy_from_slice(&width.to_be_bytes());
    page[4..8].copy_from_slice(&height.to_be_bytes());
    let mut generic = Vec::new();
    generic.extend_from_slice(&width.to_be_bytes());
    generic.extend_from_slice(&height.to_be_bytes());
    generic.extend_from_slice(&0_u32.to_be_bytes());
    generic.extend_from_slice(&0_u32.to_be_bytes());
    generic.push(region_flags);
    generic.push(0);
    generic.extend_from_slice(&[3, 0xff, 0xfd, 0xff, 2, 0xfe, 0xfe, 0xfe]);
    generic.extend_from_slice(coded);
    let mut bytes = SIGNATURE.to_vec();
    bytes.push(0x01);
    bytes.extend_from_slice(&1_u32.to_be_bytes());
    bytes.extend_from_slice(&segment_header(0, 48, 1, 19));
    bytes.extend_from_slice(&page);
    bytes.extend_from_slice(&segment_header(
        1,
        38,
        1,
        u32::try_from(generic.len()).expect("payload fits"),
    ));
    bytes.extend_from_slice(&generic);
    bytes.extend_from_slice(&segment_header(2, 49, 1, 0));
    bytes.extend_from_slice(&segment_header(3, 51, 0, 0));
    bytes
}

fn arithmetic_test_bytes(state: &mut u64) -> Vec<u8> {
    let mut coded = Vec::with_capacity(1_026);
    for _ in 0..1_024 {
        *state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        coded.push(state.to_be_bytes()[3] & 0x7f);
    }
    coded.extend_from_slice(&[0xff, 0xac]);
    coded
}

fn all_white_mmr_document(width: u32, height: u32) -> Vec<u8> {
    let mut page = [0_u8; 19];
    page[0..4].copy_from_slice(&width.to_be_bytes());
    page[4..8].copy_from_slice(&height.to_be_bytes());
    page[16] = 0x40;
    let mut generic = Vec::new();
    generic.extend_from_slice(&width.to_be_bytes());
    generic.extend_from_slice(&height.to_be_bytes());
    generic.extend_from_slice(&0_u32.to_be_bytes());
    generic.extend_from_slice(&0_u32.to_be_bytes());
    generic.push(0);
    generic.push(1);
    let coded_len = usize::try_from(height.div_ceil(8)).expect("test height fits");
    generic.extend(std::iter::repeat_n(0xFF, coded_len));
    if height % 8 != 0 {
        *generic.last_mut().expect("coded byte") = 0xFF << (8 - height % 8);
    }
    let mut bytes = SIGNATURE.to_vec();
    bytes.push(0x01);
    bytes.extend_from_slice(&1_u32.to_be_bytes());
    bytes.extend_from_slice(&segment_header(0, 48, 1, 19));
    bytes.extend_from_slice(&page);
    bytes.extend_from_slice(&segment_header(
        1,
        38,
        1,
        u32::try_from(generic.len()).expect("payload fits"),
    ));
    bytes.extend_from_slice(&generic);
    bytes.extend_from_slice(&segment_header(2, 49, 1, 0));
    bytes.extend_from_slice(&segment_header(3, 51, 0, 0));
    bytes
}

#[test]
fn decodes_generic_page_and_collects_document() {
    let decoded = Decoder::default()
        .decode(&document_with_generic(&[0x26, 0xA2, 0x80]))
        .expect("MMR page decodes");
    assert_eq!(decoded.pages().len(), 1);
    assert_eq!(decoded.pages()[0].number(), 1);
    assert_eq!(decoded.pages()[0].bitmap().bytes(), &[0xFF]);
}

fn runtime_test_page(
    width: u32,
    height: u32,
    default_pixel: bool,
    pristine: bool,
    operator: CombinationOperator,
    limits: &Limits,
) -> RuntimePage {
    RuntimePage {
        default_pixel,
        pristine,
        default_operator: operator,
        combination_override: false,
        refinements_allowed: false,
        auxiliary_allowed: false,
        last_stripe_end: None,
        open_stripe_max_row: None,
        bitmap: Bitmap::filled(width, height, default_pixel, limits).expect("test page bitmap"),
    }
}

fn assert_direct_page_matches_oracle(
    document: &[u8],
    default_pixel: bool,
    pristine: bool,
    page_operator: CombinationOperator,
    limits: &Limits,
) {
    let (_info, segments) =
        parse_standalone_document(document, &Limits::default()).expect("test document");
    let segment = &segments[1];
    let (header, _) =
        parse_generic_region_header(segment.payload, &Limits::default()).expect("header");
    let mut direct_pages = vec![(
        1,
        runtime_test_page(
            header.region.width,
            header.region.height,
            default_pixel,
            pristine,
            page_operator,
            &Limits::default(),
        ),
    )];
    let mut oracle_pages = vec![(
        1,
        runtime_test_page(
            header.region.width,
            header.region.height,
            default_pixel,
            pristine,
            page_operator,
            &Limits::default(),
        ),
    )];
    let mut direct_buffers = Vec::new();
    let mut oracle_buffers = Vec::new();
    let mut direct_retained = RetainedBudget::new();
    let mut oracle_retained = RetainedBudget::new();
    let mut direct_work = DecodeWork::new(0, limits).expect("direct work");
    let mut oracle_work = DecodeWork::new(0, limits).expect("oracle work");
    let mut direct_scratch = MmrScratch::default();
    let mut oracle_scratch = MmrScratch::default();
    let direct = decode_generic_segment(
        segment,
        &mut direct_pages,
        &mut direct_buffers,
        &mut direct_retained,
        &mut direct_work,
        limits,
        &mut direct_scratch,
    );
    let oracle = decode_generic_segment_oracle(
        segment,
        &mut oracle_pages,
        &mut oracle_buffers,
        &mut oracle_retained,
        &mut oracle_work,
        limits,
        &mut oracle_scratch,
    );
    assert_eq!(
        direct.as_ref().err().map(ToString::to_string),
        oracle.as_ref().err().map(ToString::to_string)
    );
    if direct.is_ok() {
        let direct_page = &direct_pages[0].1;
        let oracle_page = &oracle_pages[0].1;
        assert_eq!(direct_page.bitmap, oracle_page.bitmap);
        assert_eq!(direct_page.pristine, oracle_page.pristine);
        assert_eq!(
            direct_page.open_stripe_max_row,
            oracle_page.open_stripe_max_row
        );
        assert_eq!(direct_work.used, oracle_work.used);
        assert_eq!(direct_retained.bytes, oracle_retained.bytes);
    }
}

#[test]
fn direct_page_matches_intermediate_compose_oracle() {
    let limits = Limits::default();
    let mut state = 0x5ca1_49e7_2d83_b604;
    for width in 1..=65_u32 {
        let height = 1 + width % 4;
        let coded = arithmetic_test_bytes(&mut state);
        let document = arithmetic_generic_document(width, height, 0, &coded);
        assert_direct_page_matches_oracle(&document, false, true, CombinationOperator::Or, &limits);
    }
}

#[test]
fn incompatible_and_limited_direct_pages_use_exact_oracle_fallback() {
    let mut state = 0xd630_18ab_795c_42ef;
    let coded = arithmetic_test_bytes(&mut state);
    let document = arithmetic_generic_document(97, 11, 0, &coded);
    for (default_pixel, pristine, operator) in [
        (true, true, CombinationOperator::Or),
        (false, false, CombinationOperator::Or),
        (false, true, CombinationOperator::Xor),
    ] {
        assert_direct_page_matches_oracle(
            &document,
            default_pixel,
            pristine,
            operator,
            &Limits::default(),
        );
    }
    let limits = Limits {
        max_work_units: 1,
        ..Limits::default()
    };
    assert_direct_page_matches_oracle(&document, false, true, CombinationOperator::Or, &limits);
}

#[test]
fn reusable_context_matches_stateless_and_reuses_capacity() {
    let input = document_with_generic(&[0x80]);
    let decoder = Decoder::default();
    let expected = decoder.decode(&input).expect("stateless decode");
    let mut context = decoder.context();
    let first = context.decode(&input).expect("context decode");
    assert_eq!(first, expected);
    let retained = context.retained_scratch_bytes();
    let reserve_events = context.mmr.reserve_events;
    assert!(retained > 0);
    assert_eq!(reserve_events, 2);
    let second = context.decode(&input).expect("reused context decode");
    assert_eq!(second, expected);
    assert_eq!(context.retained_scratch_bytes(), retained);
    assert_eq!(context.mmr.reserve_events, reserve_events);
    context.release_scratch();
    assert_eq!(context.retained_scratch_bytes(), 0);
    assert_eq!(context.mmr.reserve_events, 0);
}

#[test]
fn reusable_context_recovers_after_error_without_state_leakage() {
    let decoder = Decoder::default();
    let valid = document_with_generic(&[0x80]);
    let expected = decoder.decode(&valid).expect("stateless decode");
    let mut context = decoder.context();
    assert!(context.decode(&[0]).is_err());
    let recovered = context.decode(&valid).expect("decode after error");
    assert_eq!(recovered, expected);
    context.reset();
    assert_eq!(
        context.decode(&valid).expect("decode after reset"),
        expected
    );
}

#[test]
fn reusable_context_obeys_retained_state_limit() {
    let limits = Limits {
        max_retained_state_bytes: 1,
        ..Limits::default()
    };
    let decoder = Decoder::new(limits);
    let mut context = decoder.context();
    context
        .decode(&document_with_generic(&[0x80]))
        .expect("scratch retention is optional");
    assert!(context.retained_scratch_bytes() <= 1);
}

#[test]
fn reusable_context_handles_mixed_sizes_deterministically() {
    let decoder = Decoder::default();
    let small = all_white_mmr_document(64, 64);
    let large = all_white_mmr_document(512, 512);
    let mut context = decoder.context();
    let small_expected = decoder.decode(&small).expect("small stateless");
    let large_expected = decoder.decode(&large).expect("large stateless");
    assert_eq!(
        context.decode(&small).expect("small context"),
        small_expected
    );
    let small_capacity = context.retained_scratch_bytes();
    let small_events = context.mmr.reserve_events;
    assert_eq!(
        context.decode(&large).expect("large context"),
        large_expected
    );
    let large_capacity = context.retained_scratch_bytes();
    assert!(large_capacity >= small_capacity);
    assert!(context.mmr.reserve_events > small_events);
    let large_events = context.mmr.reserve_events;
    assert_eq!(
        context.decode(&small).expect("small reused"),
        small_expected
    );
    assert_eq!(context.retained_scratch_bytes(), large_capacity);
    assert_eq!(context.mmr.reserve_events, large_events);
}

#[test]
#[ignore = "microbenchmark; run explicitly with --release --ignored --nocapture"]
fn benchmark_reusable_context_against_d669954_stateless() {
    let decoder = Decoder::default();
    let same = all_white_mmr_document(64, 64);
    let mixed = [
        all_white_mmr_document(64, 64),
        same.clone(),
        all_white_mmr_document(2048, 2048),
        same.clone(),
    ];
    let mut stateless_times = Vec::new();
    let mut reused_times = Vec::new();
    for sample in 0..9 {
        if sample % 2 == 0 {
            let start = std::time::Instant::now();
            for _ in 0..50 {
                std::hint::black_box(decoder.decode(&same).expect("stateless"));
            }
            stateless_times.push(start.elapsed());
            let mut context = decoder.context();
            let start = std::time::Instant::now();
            for _ in 0..50 {
                std::hint::black_box(context.decode(&same).expect("reused"));
            }
            reused_times.push(start.elapsed());
        } else {
            let mut context = decoder.context();
            let start = std::time::Instant::now();
            for _ in 0..50 {
                std::hint::black_box(context.decode(&same).expect("reused"));
            }
            reused_times.push(start.elapsed());
            let start = std::time::Instant::now();
            for _ in 0..50 {
                std::hint::black_box(decoder.decode(&same).expect("stateless"));
            }
            stateless_times.push(start.elapsed());
        }
    }
    stateless_times.sort_unstable();
    reused_times.sort_unstable();
    let stateless = stateless_times[stateless_times.len() / 2];
    let reused = reused_times[reused_times.len() / 2];

    let mut stateless_mixed = Vec::new();
    let mut reused_mixed = Vec::new();
    for sample in 0..7 {
        if sample % 2 == 0 {
            let start = std::time::Instant::now();
            for _ in 0..20 {
                for input in &mixed {
                    std::hint::black_box(decoder.decode(input).expect("stateless mixed"));
                }
            }
            stateless_mixed.push(start.elapsed());
            let mut context = decoder.context();
            let start = std::time::Instant::now();
            for _ in 0..20 {
                for input in &mixed {
                    std::hint::black_box(context.decode(input).expect("reused mixed"));
                }
            }
            reused_mixed.push(start.elapsed());
        } else {
            let mut context = decoder.context();
            let start = std::time::Instant::now();
            for _ in 0..20 {
                for input in &mixed {
                    std::hint::black_box(context.decode(input).expect("reused mixed"));
                }
            }
            reused_mixed.push(start.elapsed());
            let start = std::time::Instant::now();
            for _ in 0..20 {
                for input in &mixed {
                    std::hint::black_box(decoder.decode(input).expect("stateless mixed"));
                }
            }
            stateless_mixed.push(start.elapsed());
        }
    }
    stateless_mixed.sort_unstable();
    reused_mixed.sort_unstable();
    let stateless_mixed = stateless_mixed[stateless_mixed.len() / 2];
    let reused_mixed = reused_mixed[reused_mixed.len() / 2];
    println!(
        "context same_stateless_ns={} same_reused_ns={} same_speedup={:.3} \
         mixed_stateless_ns={} mixed_reused_ns={} mixed_speedup={:.3}",
        stateless.as_nanos(),
        reused.as_nanos(),
        stateless.as_secs_f64() / reused.as_secs_f64(),
        stateless_mixed.as_nanos(),
        reused_mixed.as_nanos(),
        stateless_mixed.as_secs_f64() / reused_mixed.as_secs_f64(),
    );
}

#[test]
fn page_sink_receives_completed_page() {
    let mut pages = 0;
    let mut sink = |page: Page| {
        pages += 1;
        assert_eq!(page.bitmap().width(), 8);
        Ok(())
    };
    Decoder::default()
        .decode_with_sink(&document_with_generic(&[0x80]), &mut sink)
        .expect("page streams");
    assert_eq!(pages, 1);
}

#[test]
fn decodes_random_access_generic_page() {
    let generic = generic_data(&[0x80]);
    let mut bytes = SIGNATURE.to_vec();
    bytes.push(0x00);
    bytes.extend_from_slice(&1_u32.to_be_bytes());
    bytes.extend_from_slice(&segment_header(0, 48, 1, 19));
    bytes.extend_from_slice(&segment_header(
        1,
        38,
        1,
        u32::try_from(generic.len()).expect("payload fits"),
    ));
    bytes.extend_from_slice(&segment_header(2, 49, 1, 0));
    bytes.extend_from_slice(&segment_header(3, 51, 0, 0));
    bytes.extend_from_slice(&page_data());
    bytes.extend_from_slice(&generic);

    let decoded = Decoder::default()
        .decode(&bytes)
        .expect("random access decodes");
    assert_eq!(
        decoded.info().organization(),
        crate::Organization::RandomAccess
    );
    assert_eq!(decoded.pages()[0].bitmap().bytes(), &[0]);
}

#[test]
fn sequential_and_random_access_outputs_match() {
    let sequential = Decoder::default()
        .decode(&document_with_generic(&[0x80]))
        .expect("sequential decodes");

    let generic = generic_data(&[0x80]);
    let mut random = SIGNATURE.to_vec();
    random.push(0x00);
    random.extend_from_slice(&1_u32.to_be_bytes());
    random.extend_from_slice(&segment_header(0, 48, 1, 19));
    random.extend_from_slice(&segment_header(
        1,
        38,
        1,
        u32::try_from(generic.len()).expect("payload fits"),
    ));
    random.extend_from_slice(&segment_header(2, 49, 1, 0));
    random.extend_from_slice(&segment_header(3, 51, 0, 0));
    random.extend_from_slice(&page_data());
    random.extend_from_slice(&generic);
    let random = Decoder::default().decode(&random).expect("random decodes");

    assert_eq!(sequential.pages()[0].bitmap(), random.pages()[0].bitmap());
}

#[test]
fn decodes_unknown_length_mmr_region_and_row_count() {
    let mut page = page_data();
    page[4..8].copy_from_slice(&2_u32.to_be_bytes());
    let mut generic = Vec::new();
    generic.extend_from_slice(&8_u32.to_be_bytes());
    generic.extend_from_slice(&2_u32.to_be_bytes());
    generic.extend_from_slice(&0_u32.to_be_bytes());
    generic.extend_from_slice(&0_u32.to_be_bytes());
    generic.push(0);
    generic.push(1);
    generic.extend_from_slice(&[0x80, 0x08, 0x00, 0x80]);
    generic.extend_from_slice(&[0x00, 0x00]);
    generic.extend_from_slice(&1_u32.to_be_bytes());

    let mut bytes = SIGNATURE.to_vec();
    bytes.push(0x01);
    bytes.extend_from_slice(&1_u32.to_be_bytes());
    bytes.extend_from_slice(&segment_header(0, 48, 1, 19));
    bytes.extend_from_slice(&page);
    bytes.extend_from_slice(&segment_header(1, 38, 1, u32::MAX));
    bytes.extend_from_slice(&generic);
    bytes.extend_from_slice(&segment_header(2, 49, 1, 0));

    let decoded = Decoder::default()
        .decode(&bytes)
        .expect("unknown-length region decodes");
    assert_eq!(decoded.pages()[0].bitmap().height(), 2);
    assert_eq!(decoded.pages()[0].bitmap().bytes(), &[0, 0]);
}

#[test]
fn decodes_unknown_height_striped_page() {
    let mut page = page_data();
    page[4..8].copy_from_slice(&u32::MAX.to_be_bytes());
    page[17..19].copy_from_slice(&0x8001_u16.to_be_bytes());
    let generic = generic_data(&[0x80]);

    let mut bytes = SIGNATURE.to_vec();
    bytes.push(0x01);
    bytes.extend_from_slice(&1_u32.to_be_bytes());
    bytes.extend_from_slice(&segment_header(0, 48, 1, 19));
    bytes.extend_from_slice(&page);
    bytes.extend_from_slice(&segment_header(
        1,
        38,
        1,
        u32::try_from(generic.len()).expect("payload fits"),
    ));
    bytes.extend_from_slice(&generic);
    bytes.extend_from_slice(&segment_header(2, 50, 1, 4));
    bytes.extend_from_slice(&0_u32.to_be_bytes());
    bytes.extend_from_slice(&segment_header(3, 49, 1, 0));

    let decoded = Decoder::default()
        .decode(&bytes)
        .expect("striped page decodes");
    assert_eq!(decoded.pages()[0].bitmap().height(), 1);
}

#[test]
fn bounded_reader_decode_matches_slice_decode() {
    let bytes = document_with_generic(&[0x80]);
    let expected = Decoder::default().decode(&bytes).expect("slice decodes");
    let mut reader = std::io::Cursor::new(bytes);
    let actual = Decoder::default()
        .decode_reader(&mut reader)
        .expect("reader decodes");
    assert_eq!(actual, expected);
}

#[test]
fn embedded_decode_matches_standalone_page() {
    let bytes = document_with_generic(&[0x80]);
    let standalone = Decoder::default()
        .decode(&bytes)
        .expect("standalone decodes");
    let embedded = Decoder::default()
        .decode_embedded(&bytes[13..])
        .expect("embedded stream decodes");
    assert_eq!(embedded.pages(), standalone.pages());
}

#[test]
fn decodes_mmr_pattern_dictionary_and_halftone_page() {
    let mut pattern = vec![0x01, 0x01, 0x01, 0, 0, 0, 0];
    pattern.extend_from_slice(&[0x26, 0xA8]);

    let mut halftone = Vec::new();
    halftone.extend_from_slice(&1_u32.to_be_bytes());
    halftone.extend_from_slice(&1_u32.to_be_bytes());
    halftone.extend_from_slice(&0_u32.to_be_bytes());
    halftone.extend_from_slice(&0_u32.to_be_bytes());
    halftone.push(0);
    halftone.push(0x01);
    halftone.extend_from_slice(&1_u32.to_be_bytes());
    halftone.extend_from_slice(&1_u32.to_be_bytes());
    halftone.extend_from_slice(&0_i32.to_be_bytes());
    halftone.extend_from_slice(&0_i32.to_be_bytes());
    halftone.extend_from_slice(&256_u16.to_be_bytes());
    halftone.extend_from_slice(&0_u16.to_be_bytes());

    let mut page = page_data();
    page[0..4].copy_from_slice(&1_u32.to_be_bytes());

    let mut bytes = SIGNATURE.to_vec();
    bytes.push(0x01);
    bytes.extend_from_slice(&1_u32.to_be_bytes());
    let mut pattern_header = segment_header(
        0,
        16,
        0,
        u32::try_from(pattern.len()).expect("pattern payload fits"),
    );
    pattern_header[5] = 1;
    bytes.extend_from_slice(&pattern_header);
    bytes.extend_from_slice(&pattern);
    bytes.extend_from_slice(&segment_header(1, 48, 1, 19));
    bytes.extend_from_slice(&page);
    bytes.extend_from_slice(&referenced_segment_header(
        2,
        22,
        1,
        0,
        u32::try_from(halftone.len()).expect("halftone payload fits"),
    ));
    bytes.extend_from_slice(&halftone);
    bytes.extend_from_slice(&segment_header(3, 49, 1, 0));
    bytes.extend_from_slice(&segment_header(4, 51, 0, 0));

    let document = Decoder::default().decode(&bytes).expect("halftone decodes");
    assert_eq!(document.pages()[0].bitmap().bytes(), &[0x80]);
}

#[test]
fn embedded_halftone_uses_reusable_global_pattern_dictionary() {
    let mut pattern = vec![0x01, 0x01, 0x01, 0, 0, 0, 0];
    pattern.extend_from_slice(&[0x26, 0xA8]);
    let mut global_bytes = segment_header(
        0,
        16,
        0,
        u32::try_from(pattern.len()).expect("pattern payload fits"),
    );
    global_bytes.extend_from_slice(&pattern);
    let globals = Decoder::default()
        .parse_globals(&global_bytes)
        .expect("globals validate");

    let mut halftone = Vec::new();
    halftone.extend_from_slice(&1_u32.to_be_bytes());
    halftone.extend_from_slice(&1_u32.to_be_bytes());
    halftone.extend_from_slice(&0_u32.to_be_bytes());
    halftone.extend_from_slice(&0_u32.to_be_bytes());
    halftone.push(0);
    halftone.push(0x01);
    halftone.extend_from_slice(&1_u32.to_be_bytes());
    halftone.extend_from_slice(&1_u32.to_be_bytes());
    halftone.extend_from_slice(&0_i32.to_be_bytes());
    halftone.extend_from_slice(&0_i32.to_be_bytes());
    halftone.extend_from_slice(&256_u16.to_be_bytes());
    halftone.extend_from_slice(&0_u16.to_be_bytes());

    let mut page = page_data();
    page[0..4].copy_from_slice(&1_u32.to_be_bytes());
    let mut embedded = segment_header(1, 48, 1, 19);
    embedded.extend_from_slice(&page);
    embedded.extend_from_slice(&referenced_segment_header(
        2,
        22,
        1,
        0,
        u32::try_from(halftone.len()).expect("halftone payload fits"),
    ));
    embedded.extend_from_slice(&halftone);
    embedded.extend_from_slice(&segment_header(3, 49, 1, 0));

    let document = Decoder::default()
        .decode_embedded_with_globals(&embedded, &globals)
        .expect("global pattern resolves");
    assert_eq!(document.pages()[0].bitmap().bytes(), &[0x80]);
}

#[test]
fn decodes_symbol_dictionary_and_arithmetic_text_region() {
    const ANNEX_H_ENCODED: [u8; 30] = [
        0x84, 0xc7, 0x3b, 0xfc, 0xe1, 0xa1, 0x43, 0x04, 0x02, 0x20, 0x00, 0x00, 0x41, 0x0d, 0xbb,
        0x86, 0xf4, 0x31, 0x7f, 0xff, 0x88, 0xff, 0x37, 0x47, 0x1a, 0xdb, 0x6a, 0xdf, 0xff, 0xac,
    ];
    let mut dictionary = vec![0x00, 0x00];
    dictionary.extend_from_slice(&[3, 0xff, 0xfd, 0xff, 2, 0xfe, 0xfe, 0xfe]);
    dictionary.extend_from_slice(&0_u32.to_be_bytes());
    dictionary.extend_from_slice(&0_u32.to_be_bytes());
    dictionary.extend_from_slice(&[0xff, 0xac]);

    let mut text = vec![0_u8; 17];
    text[3] = 1;
    text[7] = 1;
    text.extend_from_slice(&0_u16.to_be_bytes());
    text.extend_from_slice(&0_u32.to_be_bytes());
    text.extend_from_slice(&ANNEX_H_ENCODED);

    let mut page = page_data();
    page[0..4].copy_from_slice(&1_u32.to_be_bytes());
    let mut bytes = SIGNATURE.to_vec();
    bytes.push(0x01);
    bytes.extend_from_slice(&1_u32.to_be_bytes());
    let mut dictionary_header = segment_header(
        0,
        0,
        0,
        u32::try_from(dictionary.len()).expect("dictionary payload fits"),
    );
    dictionary_header[5] = 1;
    bytes.extend_from_slice(&dictionary_header);
    bytes.extend_from_slice(&dictionary);
    bytes.extend_from_slice(&segment_header(1, 48, 1, 19));
    bytes.extend_from_slice(&page);
    bytes.extend_from_slice(&referenced_segment_header(
        2,
        6,
        1,
        0,
        u32::try_from(text.len()).expect("text payload fits"),
    ));
    bytes.extend_from_slice(&text);
    bytes.extend_from_slice(&segment_header(3, 49, 1, 0));
    bytes.extend_from_slice(&segment_header(4, 51, 0, 0));

    let document = Decoder::default()
        .decode(&bytes)
        .expect("text document decodes");
    assert_eq!(document.pages()[0].bitmap().bytes(), &[0]);
}

#[test]
fn embedded_text_uses_reusable_global_symbol_dictionary() {
    const ANNEX_H_ENCODED: [u8; 30] = [
        0x84, 0xc7, 0x3b, 0xfc, 0xe1, 0xa1, 0x43, 0x04, 0x02, 0x20, 0x00, 0x00, 0x41, 0x0d, 0xbb,
        0x86, 0xf4, 0x31, 0x7f, 0xff, 0x88, 0xff, 0x37, 0x47, 0x1a, 0xdb, 0x6a, 0xdf, 0xff, 0xac,
    ];
    let mut dictionary = vec![0x00, 0x00];
    dictionary.extend_from_slice(&[3, 0xff, 0xfd, 0xff, 2, 0xfe, 0xfe, 0xfe]);
    dictionary.extend_from_slice(&0_u32.to_be_bytes());
    dictionary.extend_from_slice(&0_u32.to_be_bytes());
    dictionary.extend_from_slice(&[0xff, 0xac]);
    let mut globals_bytes = segment_header(
        0,
        0,
        0,
        u32::try_from(dictionary.len()).expect("dictionary payload fits"),
    );
    globals_bytes.extend_from_slice(&dictionary);
    let globals = Decoder::default()
        .parse_globals(&globals_bytes)
        .expect("global dictionary validates");

    let mut text = vec![0_u8; 17];
    text[3] = 1;
    text[7] = 1;
    text.extend_from_slice(&0_u16.to_be_bytes());
    text.extend_from_slice(&0_u32.to_be_bytes());
    text.extend_from_slice(&ANNEX_H_ENCODED);
    let mut page = page_data();
    page[0..4].copy_from_slice(&1_u32.to_be_bytes());
    let mut embedded = segment_header(1, 48, 1, 19);
    embedded.extend_from_slice(&page);
    embedded.extend_from_slice(&referenced_segment_header(
        2,
        6,
        1,
        0,
        u32::try_from(text.len()).expect("text payload fits"),
    ));
    embedded.extend_from_slice(&text);
    embedded.extend_from_slice(&segment_header(3, 49, 1, 0));

    let document = Decoder::default()
        .decode_embedded_with_globals(&embedded, &globals)
        .expect("global symbols resolve");
    assert_eq!(document.pages()[0].bitmap().bytes(), &[0]);
}

#[test]
fn decodes_huffman_symbol_dictionary_and_text_instance() {
    let mut dictionary = vec![0x00, 0x01];
    dictionary.extend_from_slice(&1_u32.to_be_bytes());
    dictionary.extend_from_slice(&1_u32.to_be_bytes());
    dictionary.extend_from_slice(&[0x5f, 0x80, 0x80, 0x00, 0x40]);

    let mut text = vec![0_u8; 17];
    text[3] = 1;
    text[7] = 1;
    text.extend_from_slice(&1_u16.to_be_bytes());
    text.extend_from_slice(&0_u16.to_be_bytes());
    text.extend_from_slice(&1_u32.to_be_bytes());
    text.push(0x01);
    text.extend_from_slice(&[0_u8; 19]);

    let mut page = page_data();
    page[0..4].copy_from_slice(&1_u32.to_be_bytes());
    let mut bytes = SIGNATURE.to_vec();
    bytes.push(0x01);
    bytes.extend_from_slice(&1_u32.to_be_bytes());
    let mut dictionary_header = segment_header(
        0,
        0,
        0,
        u32::try_from(dictionary.len()).expect("dictionary payload fits"),
    );
    dictionary_header[5] = 1;
    bytes.extend_from_slice(&dictionary_header);
    bytes.extend_from_slice(&dictionary);
    bytes.extend_from_slice(&segment_header(1, 48, 1, 19));
    bytes.extend_from_slice(&page);
    bytes.extend_from_slice(&referenced_segment_header(
        2,
        6,
        1,
        0,
        u32::try_from(text.len()).expect("text payload fits"),
    ));
    bytes.extend_from_slice(&text);
    bytes.extend_from_slice(&segment_header(3, 49, 1, 0));

    let document = Decoder::default()
        .decode(&bytes)
        .expect("Huffman text document decodes");
    assert_eq!(document.pages()[0].bitmap().bytes(), &[0x80]);
}

#[test]
fn rejects_region_that_modifies_a_sealed_stripe() {
    let mut page = page_data();
    page[4..8].copy_from_slice(&2_u32.to_be_bytes());
    page[17..19].copy_from_slice(&0x8002_u16.to_be_bytes());
    let first = generic_data(&[0x80]);
    let second = generic_data(&[0x80]);

    let mut bytes = SIGNATURE.to_vec();
    bytes.push(0x01);
    bytes.extend_from_slice(&1_u32.to_be_bytes());
    bytes.extend_from_slice(&segment_header(0, 48, 1, 19));
    bytes.extend_from_slice(&page);
    bytes.extend_from_slice(&segment_header(
        1,
        38,
        1,
        u32::try_from(first.len()).expect("payload fits"),
    ));
    bytes.extend_from_slice(&first);
    bytes.extend_from_slice(&segment_header(2, 50, 1, 4));
    bytes.extend_from_slice(&0_u32.to_be_bytes());
    bytes.extend_from_slice(&segment_header(
        3,
        38,
        1,
        u32::try_from(second.len()).expect("payload fits"),
    ));
    bytes.extend_from_slice(&second);
    bytes.extend_from_slice(&segment_header(4, 49, 1, 0));
    bytes.extend_from_slice(&segment_header(5, 51, 0, 0));

    assert!(matches!(
        Decoder::default().decode(&bytes),
        Err(DecodeError::Malformed(MalformedError::InvalidPageState(
            "region modifies a sealed stripe"
        )))
    ));
}

#[test]
fn decoder_enforces_cumulative_retained_state_limit() {
    let limits = Limits {
        max_retained_state_bytes: 0,
        ..Limits::default()
    };
    assert!(matches!(
        Decoder::new(limits).decode(&document_with_generic(&[0x80])),
        Err(DecodeError::LimitExceeded(crate::LimitExceeded {
            kind: crate::LimitKind::RetainedState,
            limit: 0,
            actual: 1
        }))
    ));
}

#[test]
fn owned_document_enforces_cumulative_page_memory() {
    let mut bytes = SIGNATURE.to_vec();
    bytes.push(0x01);
    bytes.extend_from_slice(&2_u32.to_be_bytes());
    bytes.extend_from_slice(&segment_header(0, 48, 1, 19));
    bytes.extend_from_slice(&page_data());
    bytes.extend_from_slice(&segment_header(1, 49, 1, 0));
    bytes.extend_from_slice(&segment_header(2, 48, 2, 19));
    bytes.extend_from_slice(&page_data());
    bytes.extend_from_slice(&segment_header(3, 49, 2, 0));

    let limits = Limits {
        max_decoder_managed_memory_bytes: 1,
        ..Limits::default()
    };
    let mut emitted = 0;
    Decoder::new(limits)
        .decode_with_sink(&bytes, &mut |_page: Page| {
            emitted += 1;
            Ok(())
        })
        .expect("streaming sink releases each page");
    assert_eq!(emitted, 2);
    assert!(matches!(
        Decoder::new(limits).decode(&bytes),
        Err(DecodeError::LimitExceeded(crate::LimitExceeded {
            kind: crate::LimitKind::Memory,
            limit: 1,
            actual: 2
        }))
    ));
}

#[test]
fn verified_decode_helpers_enforce_boundaries() {
    assert_eq!(
        checked_page_write_state(None, None, 1, 1, 0, 0, 8, 8).expect("first write is accepted"),
        Some(0)
    );
    assert!(checked_page_write_state(Some(3), None, 1, 1, 0, 3, 8, 8).is_err());
    assert_eq!(
        checked_seal_stripe_state(Some(3), 3).expect("stripe closes at its maximum"),
        None
    );
    assert!(checked_seal_stripe_state(Some(4), 3).is_err());
    assert_eq!(clipped_coordinate(u32::MAX, 1, u32::MAX), None);
    assert_eq!(clipped_coordinate(3, 4, 8), Some(7));
    assert_eq!(structural_work_totals(5, 2, 3).expect("work sum fits"), 10);
    assert!(structural_work_totals(u64::MAX, 1, 0).is_err());
}

fn assert_wide_budget_result(
    result: Result<(), DecodeError>,
    total: u128,
    limit: u64,
    kind: crate::LimitKind,
) {
    if total > u128::from(u64::MAX) {
        assert!(matches!(
            result,
            Err(DecodeError::Malformed(MalformedError::IntegerOverflow))
        ));
    } else if total > u128::from(limit) {
        assert!(matches!(
            result,
            Err(DecodeError::LimitExceeded(error))
                if error.kind == kind
                    && error.limit == limit
                    && u128::from(error.actual) == total
        ));
    } else {
        result.expect("representable total within the inclusive limit");
    }
}

#[test]
fn runtime_budgets_match_wide_arithmetic_and_preserve_rejected_state() {
    let values = [0, 1, 7, u64::MAX / 2, u64::MAX - 1, u64::MAX];
    for initial in values {
        for limit in values {
            let limits = Limits {
                max_work_units: limit,
                max_retained_state_bytes: limit,
                ..Limits::default()
            };
            let initialized = DecodeWork::new(initial, &limits);
            if let Ok(work) = &initialized {
                assert_eq!(work.used, initial);
            }
            assert_wide_budget_result(
                initialized.map(|_| ()),
                u128::from(initial),
                limit,
                crate::LimitKind::Work,
            );
            for amount in values {
                let total = u128::from(initial) + u128::from(amount);
                let accepted = total <= u128::from(limit);
                let mut work = DecodeWork { used: initial };
                assert_wide_budget_result(
                    work.charge(amount, &limits),
                    total,
                    limit,
                    crate::LimitKind::Work,
                );
                let expected = if accepted {
                    u64::try_from(total).unwrap()
                } else {
                    initial
                };
                assert_eq!(work.used, expected);
                let mut retained = RetainedBudget { bytes: initial };
                assert_wide_budget_result(
                    retained.retain(amount, &limits),
                    total,
                    limit,
                    crate::LimitKind::RetainedState,
                );
                assert_eq!(retained.bytes, expected);

                let mut retained = RetainedBudget { bytes: initial };
                let released = retained.release(amount);
                if amount <= initial {
                    released.unwrap();
                    assert_eq!(retained.bytes, initial - amount);
                } else {
                    assert!(matches!(
                        released,
                        Err(DecodeError::Malformed(MalformedError::IntegerOverflow))
                    ));
                    assert_eq!(retained.bytes, initial);
                }
                for second in values {
                    let pair_total = total + u128::from(second);
                    let work = DecodeWork { used: initial };
                    assert_eq!(
                        work.can_charge_pair(amount, second, &limits),
                        pair_total <= u128::from(limit),
                    );
                    assert_eq!(work.used, initial);
                }
            }
        }
    }
}

#[test]
fn referred_bitmap_resources_release_only_selected_existing_keys_once() {
    let limits = Limits::default();
    let references = [2, 1, 2, 3];
    let descriptors = [(2, 2_u32), (1, 1), (9, 3)];
    for mask in 0..16_u8 {
        let mut segment = symbol_reference_segment(&references);
        segment.header.retention_flags.values = std::iter::once(1)
            .chain((0..4).map(|i| (mask >> i) & 1))
            .collect();
        let expected: Vec<_> = descriptors
            .iter()
            .filter(|(number, _)| {
                !references
                    .iter()
                    .enumerate()
                    .any(|(i, referred)| referred == number && mask & (1 << i) == 0)
            })
            .map(|(number, _)| *number)
            .collect();
        let expected_bytes: u64 = descriptors
            .iter()
            .filter(|(number, _)| expected.contains(number))
            .map(|(_, height)| u64::from(*height))
            .sum();
        let mut buffers = Vec::new();
        let mut patterns = Vec::new();
        let mut symbols = Vec::new();
        for (number, height) in descriptors {
            let region = crate::segment_data::RegionInfo {
                width: 8,
                height,
                x: 0,
                y: 0,
                operator: CombinationOperator::Replace,
            };
            buffers.push((
                number,
                RegionBuffer {
                    region,
                    bitmap: Bitmap::blank(8, height, &limits).unwrap(),
                },
            ));
            patterns.push((
                number,
                vec![
                    Bitmap::blank(8, height, &limits).unwrap(),
                    Bitmap::blank(0, 1, &limits).unwrap(),
                ],
            ));
            symbols.push((
                number,
                SymbolResource {
                    exported: vec![
                        Bitmap::blank(8, height, &limits).unwrap(),
                        Bitmap::blank(0, 1, &limits).unwrap(),
                    ],
                    contexts: None,
                },
            ));
        }
        let mut buffer_budget = RetainedBudget { bytes: 6 };
        let mut pattern_budget = RetainedBudget { bytes: 6 };
        let mut symbol_budget = RetainedBudget { bytes: 6 };
        for _ in 0..2 {
            release_referred_buffers(&segment, &mut buffers, &mut buffer_budget).unwrap();
            release_referred_patterns(&segment, &mut patterns, &mut pattern_budget).unwrap();
            release_referred_symbols(&segment, &mut symbols, &mut symbol_budget).unwrap();
            assert_eq!(buffer_budget.bytes, expected_bytes);
            assert_eq!(pattern_budget.bytes, expected_bytes);
            assert_eq!(symbol_budget.bytes, expected_bytes);
            assert_eq!(
                buffers.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
                expected
            );
            assert_eq!(
                patterns.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
                expected
            );
            assert_eq!(
                symbols.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
                expected
            );
        }
    }
}

#[test]
fn raw_bitmap_snapshot_matches_literal_scalar_clipping_and_padding() {
    let limits = Limits::default();
    let bytes = [0xa5_u8, 0x3c];
    let mut page = Bitmap::blank(8, 2, &limits).unwrap();
    for y in 0..2 {
        for x in 0..8 {
            page.set_pixel(x, y, bytes[y as usize] & (0x80 >> x) != 0)
                .unwrap();
        }
    }
    for x in 0..=10 {
        for y in 0..=4 {
            for width in [0, 1, 3, 8, 10] {
                for height in 0..=3 {
                    let region = crate::segment_data::RegionInfo {
                        width,
                        height,
                        x,
                        y,
                        operator: CombinationOperator::Replace,
                    };
                    let snapshot = snapshot_region(&page, &region, &limits).unwrap();
                    let stride = usize::try_from(width.div_ceil(8)).unwrap();
                    let mut expected = vec![0; stride * usize::try_from(height).unwrap()];
                    for target_y in 0..height {
                        for target_x in 0..width {
                            let source_x = x + target_x;
                            let source_y = y + target_y;
                            if source_x < 8
                                && source_y < 2
                                && bytes[source_y as usize] & (0x80 >> source_x) != 0
                            {
                                let index = usize::try_from(target_y).unwrap() * stride
                                    + usize::try_from(target_x / 8).unwrap();
                                expected[index] |= 0x80 >> (target_x % 8);
                            }
                        }
                    }
                    assert_eq!(snapshot.width(), width);
                    assert_eq!(snapshot.height(), height);
                    assert_eq!(snapshot.stride(), stride);
                    assert_eq!(snapshot.bytes(), expected);
                    assert_eq!(page.bytes(), bytes);
                }
            }
        }
    }
}

#[test]
fn raw_page_write_bounds_match_wide_intersection_and_error_priority() {
    let coordinates = [0, 1, 2, u32::MAX - 1, u32::MAX];
    let mut cases = 0;
    for (page_width, page_height) in [(0, 0), (1, 1), (9, 3), (u32::MAX, u32::MAX)] {
        for width in coordinates {
            for height in coordinates {
                for x in coordinates {
                    for y in coordinates {
                        for sealed in [None, Some(0), Some(1), Some(u32::MAX)] {
                            for open in [None, Some(0), Some(2), Some(u32::MAX)] {
                                let bottom = u64::from(y) + u64::from(height);
                                let expected = if width == 0
                                    || height == 0
                                    || x >= page_width
                                    || y >= page_height
                                {
                                    Ok(open)
                                } else if sealed.is_some_and(|row| y <= row) {
                                    Err(MalformedError::InvalidPageState(
                                        "region modifies a sealed stripe",
                                    ))
                                } else if bottom > u64::from(u32::MAX) {
                                    Err(MalformedError::IntegerOverflow)
                                } else {
                                    let last =
                                        u32::try_from(bottom.min(u64::from(page_height)) - 1)
                                            .unwrap();
                                    Ok(Some(open.map_or(last, |old| old.max(last))))
                                };
                                match checked_page_write_state(
                                    sealed,
                                    open,
                                    width,
                                    height,
                                    x,
                                    y,
                                    page_width,
                                    page_height,
                                ) {
                                    Ok(next) => assert_eq!(expected, Ok(next)),
                                    Err(DecodeError::Malformed(error)) => {
                                        assert_eq!(expected, Err(error));
                                    }
                                    other => panic!("unexpected page-write result: {other:?}"),
                                }
                                cases += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 40_000);
}

#[test]
fn raw_stripe_sealing_preserves_pixels_and_rejected_page_state() {
    let limits = Limits::default();
    for sealed in [None, Some(0), Some(u32::MAX)] {
        for open in [None, Some(0), Some(2), Some(u32::MAX)] {
            for row in [0, 1, 2, u32::MAX] {
                let mut page =
                    runtime_test_page(9, 3, true, false, CombinationOperator::Replace, &limits);
                page.last_stripe_end = sealed;
                page.open_stripe_max_row = open;
                let before = page.bitmap.bytes().to_vec();
                let result = seal_stripe_on_page(&mut page, row);
                if open.is_some_and(|maximum| maximum > row) {
                    assert!(matches!(
                        result,
                        Err(DecodeError::Malformed(MalformedError::InvalidPageState(
                            "region preceding stripe extends below its end row"
                        )))
                    ));
                    assert_eq!(page.last_stripe_end, sealed);
                    assert_eq!(page.open_stripe_max_row, open);
                } else {
                    result.unwrap();
                    assert_eq!(page.last_stripe_end, Some(row));
                    assert_eq!(page.open_stripe_max_row, None);
                }
                assert_eq!(page.bitmap.width(), 9);
                assert_eq!(page.bitmap.height(), 3);
                assert_eq!(page.bitmap.bytes(), before);
                assert!(!page.pristine);
            }
        }
    }
}

#[test]
fn duplicate_buffer_does_not_change_retained_accounting() {
    let limits = Limits::default();
    let region = crate::segment_data::RegionInfo {
        width: 8,
        height: 1,
        x: 0,
        y: 0,
        operator: crate::bitmap::CombinationOperator::Replace,
    };
    let mut buffers = Vec::new();
    let mut retained = RetainedBudget::new();
    insert_buffer(
        &mut buffers,
        7,
        region,
        Bitmap::blank(8, 1, &limits).expect("bitmap allocates"),
        &mut retained,
        &limits,
    )
    .expect("first buffer inserts");
    assert_eq!(retained.bytes, 1);

    assert!(matches!(
        insert_buffer(
            &mut buffers,
            7,
            region,
            Bitmap::blank(8, 1, &limits).expect("bitmap allocates"),
            &mut retained,
            &limits,
        ),
        Err(DecodeError::Malformed(
            MalformedError::DuplicateSegmentNumber(7)
        ))
    ));
    assert_eq!(retained.bytes, 1);
    assert_eq!(buffers.len(), 1);
}

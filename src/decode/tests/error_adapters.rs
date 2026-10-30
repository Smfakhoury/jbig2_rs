#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

use super::*;

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

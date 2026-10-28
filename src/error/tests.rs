#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

use super::{
    DecodeError, GenericRegionErrorKind, HuffmanErrorKind, IoErrorKind, LimitExceeded, LimitKind,
    MalformedError, MmrErrorKind, UnsupportedFeature,
};
use crate::generic_region::GenericRegionError;
use crate::huffman::HuffmanError;
use crate::mmr::MmrError;
use crate::symbol_text::SymbolTextError;

fn same_error(actual: DecodeError, expected: DecodeError) {
    match (actual, expected) {
        (DecodeError::Malformed(actual), DecodeError::Malformed(expected)) => {
            assert_eq!(actual, expected);
        }
        (DecodeError::Unsupported(actual), DecodeError::Unsupported(expected)) => {
            assert_eq!(actual, expected);
        }
        (DecodeError::LimitExceeded(actual), DecodeError::LimitExceeded(expected)) => {
            assert_eq!(actual, expected);
        }
        (
            DecodeError::Allocation {
                requested_bytes: actual,
            },
            DecodeError::Allocation {
                requested_bytes: expected,
            },
        ) => assert_eq!(actual, expected),
        (DecodeError::Io(actual), DecodeError::Io(expected)) => assert_eq!(actual, expected),
        (actual, expected) => {
            panic!("wrong error variant: actual={actual:?}, expected={expected:?}")
        }
    }
}

fn wrapped_errors() -> [DecodeError; 5] {
    [
        DecodeError::Malformed(MalformedError::IntegerOverflow),
        DecodeError::LimitExceeded(LimitExceeded {
            kind: LimitKind::Work,
            limit: 17,
            actual: u64::MAX,
        }),
        DecodeError::Allocation {
            requested_bytes: u64::MAX,
        },
        DecodeError::Io(IoErrorKind::Interrupted),
        DecodeError::Unsupported(UnsupportedFeature::Extension(u32::MAX)),
    ]
}

fn passthrough<E: Into<DecodeError>>(wrap: fn(DecodeError) -> E) {
    for (input, expected) in wrapped_errors().into_iter().zip(wrapped_errors()) {
        same_error(wrap(input).into(), expected);
    }
}

#[test]
fn generic_error_conversion_preserves_every_variant_and_payload() {
    passthrough(GenericRegionError::Decode);
    let cases = [
        (
            GenericRegionError::ReservedFlags(u8::MAX),
            GenericRegionErrorKind::ReservedFlags(u8::MAX),
        ),
        (
            GenericRegionError::InvalidFlags(0),
            GenericRegionErrorKind::InvalidFlags(0),
        ),
        (
            GenericRegionError::InvalidAdaptivePixel {
                number: 7,
                x: i32::MIN,
                y: i32::MAX,
            },
            GenericRegionErrorKind::InvalidAdaptivePixel {
                number: 7,
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
                code: 0xAC,
            },
            GenericRegionErrorKind::UnexpectedArithmeticMarker {
                offset: usize::MAX,
                code: 0xAC,
            },
        ),
        (
            GenericRegionError::RowCountMismatch {
                expected: 0,
                actual: u32::MAX,
            },
            GenericRegionErrorKind::RowCountMismatch {
                expected: 0,
                actual: u32::MAX,
            },
        ),
        (
            GenericRegionError::ContextIndex {
                index: usize::MAX,
                count: 13,
            },
            GenericRegionErrorKind::ContextIndex {
                index: usize::MAX,
                count: 13,
            },
        ),
    ];
    for (input, expected) in cases {
        same_error(
            input.into(),
            DecodeError::Malformed(MalformedError::InvalidGenericRegion(expected)),
        );
    }
}

#[test]
fn huffman_error_conversion_preserves_table_and_wrapped_errors() {
    passthrough(HuffmanError::Decode);
    same_error(
        HuffmanError::InvalidTable(HuffmanErrorKind::InvalidBounds {
            low: i32::MIN,
            high: i32::MAX,
        })
        .into(),
        DecodeError::Malformed(MalformedError::InvalidHuffman(
            HuffmanErrorKind::InvalidBounds {
                low: i32::MIN,
                high: i32::MAX,
            },
        )),
    );
}

#[test]
fn mmr_error_conversion_preserves_every_variant_and_payload() {
    passthrough(MmrError::Bitmap);
    same_error(
        MmrError::Reader(MalformedError::UnexpectedEof {
            offset: 19,
            needed: 7,
            remaining: 3,
        })
        .into(),
        DecodeError::Malformed(MalformedError::UnexpectedEof {
            offset: 19,
            needed: 7,
            remaining: 3,
        }),
    );
    same_error(
        MmrError::Allocation {
            requested_bytes: u64::MAX,
        }
        .into(),
        DecodeError::Allocation {
            requested_bytes: u64::MAX,
        },
    );
    same_error(
        MmrError::IntegerOverflow.into(),
        DecodeError::Malformed(MalformedError::IntegerOverflow),
    );
    let cases = [
        (
            MmrError::InvalidMode {
                bit_offset: usize::MAX,
            },
            MmrErrorKind::InvalidMode {
                bit_offset: usize::MAX,
            },
        ),
        (
            MmrError::ExtensionMode { bit_offset: 0 },
            MmrErrorKind::ExtensionMode { bit_offset: 0 },
        ),
        (
            MmrError::InvalidRunCode {
                bit_offset: 91,
                black: true,
            },
            MmrErrorKind::InvalidRunCode {
                bit_offset: 91,
                black: true,
            },
        ),
        (
            MmrError::InvalidRunCode {
                bit_offset: usize::MAX,
                black: false,
            },
            MmrErrorKind::InvalidRunCode {
                bit_offset: usize::MAX,
                black: false,
            },
        ),
        (MmrError::RunExceedsRow, MmrErrorKind::RunExceedsRow),
        (MmrError::InvalidTransition, MmrErrorKind::InvalidTransition),
        (MmrError::NonProgress, MmrErrorKind::NonProgress),
        (MmrError::TooManyModes, MmrErrorKind::TooManyModes),
        (MmrError::MissingEofb, MmrErrorKind::MissingEofb),
        (MmrError::InvalidEofb, MmrErrorKind::InvalidEofb),
        (MmrError::TrailingData, MmrErrorKind::TrailingData),
        (MmrError::NonZeroEofbPadding, MmrErrorKind::NonZeroPadding),
    ];
    for (input, expected) in cases {
        same_error(
            input.into(),
            DecodeError::Malformed(MalformedError::InvalidMmr(expected)),
        );
    }
}

#[test]
fn symbol_error_conversion_preserves_reasons_ids_and_wrapped_errors() {
    passthrough(SymbolTextError::Decode);
    for input in [
        SymbolTextError::InvalidFlags("reason sentinel"),
        SymbolTextError::InvalidTableAssociation("reason sentinel"),
        SymbolTextError::InvalidCount("reason sentinel"),
        SymbolTextError::InvalidEntropyValue("reason sentinel"),
        SymbolTextError::UnexpectedOutOfBand("reason sentinel"),
        SymbolTextError::InvalidState("reason sentinel"),
    ] {
        same_error(
            input.into(),
            DecodeError::Malformed(MalformedError::InvalidSymbolText("reason sentinel")),
        );
    }
    same_error(
        SymbolTextError::InvalidDimensions.into(),
        DecodeError::Malformed(MalformedError::InvalidSymbolDimensions),
    );
    same_error(
        SymbolTextError::SymbolIdOutOfRange {
            id: u32::MAX,
            available: 3,
        }
        .into(),
        DecodeError::Malformed(MalformedError::SymbolIdOutOfRange {
            id: u32::MAX,
            available: 3,
        }),
    );
    same_error(
        SymbolTextError::InvalidSymbolLengthRun.into(),
        DecodeError::Malformed(MalformedError::InvalidSymbolText(
            "invalid symbol-code length run",
        )),
    );
}

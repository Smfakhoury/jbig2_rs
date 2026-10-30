#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

use super::*;

#[test]
fn generic_adapter_preserves_all_variants_and_full_width_fields() {
    let cases = [
        (
            GenericRegionError::ReservedFlags(255),
            GenericRegionErrorKind::ReservedFlags(255),
        ),
        (
            GenericRegionError::InvalidFlags(255),
            GenericRegionErrorKind::InvalidFlags(255),
        ),
        (
            GenericRegionError::InvalidAdaptivePixel {
                number: 255,
                x: i32::MIN,
                y: i32::MAX,
            },
            GenericRegionErrorKind::InvalidAdaptivePixel {
                number: 255,
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
                code: 255,
            },
            GenericRegionErrorKind::UnexpectedArithmeticMarker {
                offset: usize::MAX,
                code: 255,
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
                count: usize::MAX - 1,
            },
            GenericRegionErrorKind::ContextIndex {
                index: usize::MAX,
                count: usize::MAX - 1,
            },
        ),
    ];
    for (error, expected) in cases {
        assert_decode_error_exact(
            generic_error_into_decode(error),
            DecodeError::Malformed(MalformedError::InvalidGenericRegion(expected)),
        );
    }
    assert_decode_error_exact(
        generic_error_into_decode(GenericRegionError::Decode(DecodeError::Allocation {
            requested_bytes: u64::MAX,
        })),
        DecodeError::Allocation {
            requested_bytes: u64::MAX,
        },
    );
    assert_decode_error_exact(
        generic_error_into_decode(GenericRegionError::Decode(DecodeError::Io(
            crate::IoErrorKind::PermissionDenied,
        ))),
        DecodeError::Io(crate::IoErrorKind::PermissionDenied),
    );
}

#[test]
fn mmr_adapter_preserves_every_kind_and_delegated_error() {
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
            MmrError::ExtensionMode {
                bit_offset: usize::MAX,
            },
            MmrErrorKind::ExtensionMode {
                bit_offset: usize::MAX,
            },
        ),
        (
            MmrError::InvalidRunCode {
                bit_offset: usize::MAX,
                black: true,
            },
            MmrErrorKind::InvalidRunCode {
                bit_offset: usize::MAX,
                black: true,
            },
        ),
        (
            MmrError::InvalidRunCode {
                bit_offset: 0,
                black: false,
            },
            MmrErrorKind::InvalidRunCode {
                bit_offset: 0,
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
    for (error, expected) in cases {
        assert_decode_error_exact(
            mmr_error_into_decode(error),
            DecodeError::Malformed(MalformedError::InvalidMmr(expected)),
        );
    }
    assert_decode_error_exact(
        mmr_error_into_decode(MmrError::IntegerOverflow),
        DecodeError::Malformed(MalformedError::IntegerOverflow),
    );
    assert_decode_error_exact(
        mmr_error_into_decode(MmrError::Allocation {
            requested_bytes: u64::MAX,
        }),
        DecodeError::Allocation {
            requested_bytes: u64::MAX,
        },
    );
    assert_decode_error_exact(
        mmr_error_into_decode(MmrError::Reader(MalformedError::UnexpectedEof {
            offset: u64::MAX,
            needed: u64::MAX,
            remaining: 0,
        })),
        DecodeError::Malformed(MalformedError::UnexpectedEof {
            offset: u64::MAX,
            needed: u64::MAX,
            remaining: 0,
        }),
    );
    assert_decode_error_exact(
        mmr_error_into_decode(MmrError::Bitmap(DecodeError::LimitExceeded(
            LimitExceeded {
                kind: LimitKind::Memory,
                limit: u64::MAX - 1,
                actual: u64::MAX,
            },
        ))),
        DecodeError::LimitExceeded(LimitExceeded {
            kind: LimitKind::Memory,
            limit: u64::MAX - 1,
            actual: u64::MAX,
        }),
    );
}

#[test]
fn halftone_adapter_preserves_every_variant_and_standard_forwarder() {
    let cases = [
        (
            HalftoneError::InvalidPatternDictionaryFlags(255),
            HalftoneErrorKind::InvalidPatternDictionaryFlags(255),
        ),
        (
            HalftoneError::InvalidPatternDimensions {
                width: 255,
                height: 0,
            },
            HalftoneErrorKind::InvalidPatternDimensions {
                width: 255,
                height: 0,
            },
        ),
        (
            HalftoneError::InvalidHalftoneFlags(255),
            HalftoneErrorKind::InvalidHalftoneFlags(255),
        ),
        (
            HalftoneError::InvalidHalftoneConfiguration,
            HalftoneErrorKind::InvalidConfiguration,
        ),
        (
            HalftoneError::InvalidBitmapDimensions {
                expected_width: u32::MAX,
                expected_height: 0,
                actual_width: 1,
                actual_height: u32::MAX - 1,
            },
            HalftoneErrorKind::InvalidBitmapDimensions {
                expected_width: u32::MAX,
                expected_height: 0,
                actual_width: 1,
                actual_height: u32::MAX - 1,
            },
        ),
        (
            HalftoneError::InvalidGrayBitDepth(255),
            HalftoneErrorKind::InvalidGrayBitDepth(255),
        ),
        (
            HalftoneError::EmptyPatternDictionary,
            HalftoneErrorKind::EmptyPatternDictionary,
        ),
        (
            HalftoneError::InconsistentPatternDimensions,
            HalftoneErrorKind::InconsistentPatternDimensions,
        ),
        (
            HalftoneError::InvalidPatternIndex {
                index: u32::MAX,
                pattern_count: u64::MAX,
            },
            HalftoneErrorKind::InvalidPatternIndex {
                index: u32::MAX,
                pattern_count: u64::MAX,
            },
        ),
    ];
    for (error, expected) in cases {
        assert_decode_error_exact(
            DecodeError::from(error),
            DecodeError::Malformed(MalformedError::InvalidHalftone(expected)),
        );
    }
    assert_decode_error_exact(
        halftone_error_into_decode(HalftoneError::from(MalformedError::IntegerOverflow)),
        DecodeError::Malformed(MalformedError::IntegerOverflow),
    );
    assert_decode_error_exact(
        halftone_error_into_decode(HalftoneError::from(LimitExceeded {
            kind: LimitKind::Work,
            limit: u64::MAX - 1,
            actual: u64::MAX,
        })),
        DecodeError::LimitExceeded(LimitExceeded {
            kind: LimitKind::Work,
            limit: u64::MAX - 1,
            actual: u64::MAX,
        }),
    );
}

//! Public error classifications, diagnostic values, and formatting failures.

use std::error::Error;
use std::fmt;
use std::io;

use jbig2_decoder::{
    DecodeError, GenericRegionErrorKind, HalftoneErrorKind, HuffmanErrorKind, IoErrorKind,
    LimitExceeded, LimitKind, MalformedError, MmrErrorKind, UnsupportedFeature,
};

#[test]
fn io_error_conversion_preserves_public_kind_without_retaining_private_message() {
    let cases = [
        (io::ErrorKind::NotFound, IoErrorKind::NotFound),
        (
            io::ErrorKind::PermissionDenied,
            IoErrorKind::PermissionDenied,
        ),
        (
            io::ErrorKind::ConnectionRefused,
            IoErrorKind::ConnectionRefused,
        ),
        (io::ErrorKind::ConnectionReset, IoErrorKind::ConnectionReset),
        (
            io::ErrorKind::ConnectionAborted,
            IoErrorKind::ConnectionAborted,
        ),
        (io::ErrorKind::NotConnected, IoErrorKind::NotConnected),
        (io::ErrorKind::AddrInUse, IoErrorKind::AddressInUse),
        (
            io::ErrorKind::AddrNotAvailable,
            IoErrorKind::AddressNotAvailable,
        ),
        (io::ErrorKind::BrokenPipe, IoErrorKind::BrokenPipe),
        (io::ErrorKind::AlreadyExists, IoErrorKind::AlreadyExists),
        (io::ErrorKind::WouldBlock, IoErrorKind::WouldBlock),
        (io::ErrorKind::InvalidInput, IoErrorKind::InvalidInput),
        (io::ErrorKind::InvalidData, IoErrorKind::InvalidData),
        (io::ErrorKind::TimedOut, IoErrorKind::TimedOut),
        (io::ErrorKind::WriteZero, IoErrorKind::WriteZero),
        (io::ErrorKind::Interrupted, IoErrorKind::Interrupted),
        (io::ErrorKind::Unsupported, IoErrorKind::Unsupported),
        (io::ErrorKind::UnexpectedEof, IoErrorKind::UnexpectedEof),
        (io::ErrorKind::OutOfMemory, IoErrorKind::OutOfMemory),
        (io::ErrorKind::Other, IoErrorKind::Other),
        (io::ErrorKind::NotADirectory, IoErrorKind::Other),
    ];
    for (input, expected) in cases {
        assert_eq!(IoErrorKind::from(input), expected);
        let error = DecodeError::from(io::Error::new(input, "private reader message"));
        assert!(matches!(&error, DecodeError::Io(actual) if *actual == expected));
        assert_eq!(
            error.to_string(),
            format!("input/output error: {expected:?}")
        );
        assert!(error.source().is_none());
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn malformed_diagnostics_preserve_classification_and_parameter_values() {
    use MalformedError as M;
    let cases = [
        (
            M::UnexpectedEof {
                offset: 17,
                needed: 9,
                remaining: 2,
            },
            "unexpected end of input at 17: needed 9, remaining 2",
        ),
        (M::InvalidFileSignature, "invalid JBIG2 file signature"),
        (
            M::ReservedFileHeaderFlags(0x80),
            "reserved JBIG2 file-header flags are set: 0x80",
        ),
        (
            M::InvalidBitCount(33),
            "cannot read 33 bits into a 32-bit value",
        ),
        (M::ReservedSegmentType(63), "reserved JBIG2 segment type 63"),
        (
            M::InvalidReferredToCountCode(5),
            "reserved referred-to count code 5",
        ),
        (
            M::NonCanonicalReferredToCount(3),
            "long-form referred-to count used for short count 3",
        ),
        (
            M::NonZeroUnusedRetentionFlags,
            "unused retention flag bits are non-zero",
        ),
        (
            M::InvalidReferredSegmentNumber {
                segment: 7,
                referred: 8,
            },
            "segment 7 refers to non-preceding segment 8",
        ),
        (
            M::InvalidExtensionType(0x1234_abcd),
            "invalid extension type 0x1234abcd",
        ),
        (M::InvalidCommentExtension, "malformed comment extension"),
        (M::InvalidProfilesSegment(7), "invalid profiles segment 7"),
        (
            M::InvalidHuffman(HuffmanErrorKind::InvalidCode),
            "invalid Huffman data: InvalidCode",
        ),
        (
            M::InvalidArithmeticContextState {
                state_index: 47,
                mps: 2,
            },
            "invalid arithmetic context state I=47, MPS=2",
        ),
        (
            M::InsufficientArithmeticContexts {
                required: 512,
                actual: 4,
            },
            "arithmetic decoding requires 512 contexts, got 4",
        ),
        (M::InvalidIaidCodeLength(32), "invalid IAID code length 32"),
        (
            M::InvalidMmr(MmrErrorKind::RunExceedsRow),
            "invalid MMR data: RunExceedsRow",
        ),
        (
            M::InvalidSymbolText("export sentinel"),
            "invalid symbol/text data: export sentinel",
        ),
        (
            M::SymbolIdOutOfRange {
                id: 9,
                available: 2,
            },
            "symbol ID 9 is outside 2 available symbols",
        ),
        (M::InvalidSymbolDimensions, "invalid symbol dimensions"),
        (
            M::InvalidHalftone(HalftoneErrorKind::EmptyPatternDictionary),
            "invalid halftone data: EmptyPatternDictionary",
        ),
        (
            M::InvalidGenericRegion(GenericRegionErrorKind::InvalidArithmeticTerminator),
            "invalid generic-region data: InvalidArithmeticTerminator",
        ),
        (
            M::InvalidPageState("stripe sentinel"),
            "invalid page state: stripe sentinel",
        ),
        (
            M::InvalidUnknownSegmentLength(48),
            "unknown data length is invalid for segment type 48",
        ),
        (
            M::InvalidSegmentDataLength {
                segment_type: 48,
                expected: 19,
                actual: 3,
            },
            "segment type 48 requires 19 data bytes, got 3",
        ),
        (
            M::ReservedRegionFlags(0xf0),
            "reserved region flags are set: 0xf0",
        ),
        (
            M::InvalidCombinationOperator(7),
            "invalid combination operator 7",
        ),
        (
            M::UnknownHeightWithoutStriping,
            "unknown-height page is not striped",
        ),
        (M::InvalidStripeEndRow(23), "invalid end-of-stripe row 23"),
        (M::DuplicateSegmentNumber(7), "duplicate segment number 7"),
        (
            M::NonIncreasingSegmentNumber {
                previous: 9,
                current: 7,
            },
            "standalone segment number 7 does not follow 9",
        ),
        (
            M::MissingReferredSegment {
                segment: 9,
                referred: 2,
            },
            "segment 9 refers to missing segment 2",
        ),
        (
            M::InvalidPageAssociation {
                segment: 9,
                page: 2,
            },
            "segment 9 has invalid page association 2",
        ),
        (
            M::MissingPageInformation(2),
            "page 2 does not start with page information",
        ),
        (
            M::DuplicatePageBoundary {
                page: 2,
                segment_type: 49,
            },
            "page 2 has duplicate boundary segment type 49",
        ),
        (
            M::SegmentAfterEndOfPage {
                segment: 9,
                page: 2,
            },
            "segment 9 occurs after end of page 2",
        ),
        (M::MissingEndOfPage(2), "page 2 has no end-of-page segment"),
        (
            M::MissingEndOfFile,
            "random-access file has no final end-of-file segment",
        ),
        (
            M::TrailingData(17),
            "17 trailing bytes follow the JBIG2 stream",
        ),
        (
            M::PageCountMismatch {
                declared: 3,
                actual: 2,
            },
            "file declares 3 pages but contains 2",
        ),
        (
            M::InvalidSegmentReferences(9),
            "segment 9 has invalid referred-to segment types",
        ),
        (
            M::MissingGenericRegionTerminator,
            "unknown-length generic region has no terminator",
        ),
        (
            M::GenericRegionRowCount {
                declared: 2,
                actual: 3,
            },
            "generic region row count 3 exceeds declared height 2",
        ),
        (
            M::InvalidRetentionFlags {
                segment: 9,
                referred: 2,
            },
            "segment 9 has invalid retention flags for segment 2",
        ),
        (M::IntegerOverflow, "input-derived integer overflow"),
    ];
    for (malformed, expected) in cases {
        assert_eq!(malformed.to_string(), expected);
        let error = DecodeError::from(malformed);
        assert!(matches!(error, DecodeError::Malformed(_)));
        assert_eq!(
            error.to_string(),
            format!("malformed JBIG2 input: {expected}")
        );
        assert!(error.source().is_none());
    }
}

#[test]
fn unsupported_diagnostics_preserve_type_and_extension_number() {
    for (feature, expected) in [
        (UnsupportedFeature::Colour, "JBIG2 colour extensions"),
        (UnsupportedFeature::SegmentType(7), "JBIG2 segment type 7"),
        (
            UnsupportedFeature::Extension(0x1234_abcd),
            "necessary JBIG2 extension 0x1234abcd",
        ),
    ] {
        assert_eq!(feature.to_string(), expected);
        let error = DecodeError::Unsupported(feature);
        assert_eq!(
            error.to_string(),
            format!("unsupported JBIG2 feature: {expected}")
        );
        assert!(error.source().is_none());
    }
}

#[test]
fn resource_diagnostics_preserve_kind_inclusive_limit_and_actual_value() {
    for (kind, name) in [
        (LimitKind::Width, "Width"),
        (LimitKind::Height, "Height"),
        (LimitKind::PagePixels, "PagePixels"),
        (LimitKind::Memory, "Memory"),
        (LimitKind::StreamBytes, "StreamBytes"),
        (LimitKind::Pages, "Pages"),
        (LimitKind::Segments, "Segments"),
        (LimitKind::Symbols, "Symbols"),
        (LimitKind::ReferencesPerSegment, "ReferencesPerSegment"),
        (LimitKind::TotalReferences, "TotalReferences"),
        (LimitKind::ReferenceDepth, "ReferenceDepth"),
        (LimitKind::NestingDepth, "NestingDepth"),
        (LimitKind::Work, "Work"),
        (LimitKind::DecodedBitmapBytes, "DecodedBitmapBytes"),
        (LimitKind::RetainedState, "RetainedState"),
    ] {
        assert_eq!(kind.to_string(), name);
        let exceeded = LimitExceeded {
            kind,
            limit: 11,
            actual: 12,
        };
        let expected = format!("{name} limit exceeded: maximum 11, actual 12");
        assert_eq!(exceeded.to_string(), expected);
        let error = DecodeError::from(exceeded);
        assert!(matches!(&error, DecodeError::LimitExceeded(actual) if *actual == exceeded));
        assert_eq!(error.to_string(), expected);
        assert!(error.source().is_none());
    }
}

#[test]
fn allocation_diagnostic_keeps_full_u64_size() {
    let error = DecodeError::Allocation {
        requested_bytes: u64::MAX,
    };
    assert_eq!(
        error.to_string(),
        "failed to allocate 18446744073709551615 bytes"
    );
    assert!(error.source().is_none());
}

struct FailingWriter;

impl fmt::Write for FailingWriter {
    fn write_str(&mut self, _value: &str) -> fmt::Result {
        Err(fmt::Error)
    }
}

#[test]
fn diagnostic_formatting_propagates_writer_failure() {
    let malformed = MalformedError::IntegerOverflow;
    let unsupported = UnsupportedFeature::Colour;
    let limit = LimitExceeded {
        kind: LimitKind::Width,
        limit: 1,
        actual: 2,
    };
    let errors: [&dyn fmt::Display; 9] = [
        &malformed,
        &unsupported,
        &limit.kind,
        &limit,
        &DecodeError::Malformed(MalformedError::IntegerOverflow),
        &DecodeError::Unsupported(UnsupportedFeature::Colour),
        &DecodeError::LimitExceeded(limit),
        &DecodeError::Allocation { requested_bytes: 7 },
        &DecodeError::Io(IoErrorKind::Other),
    ];
    for error in errors {
        assert!(fmt::write(&mut FailingWriter, format_args!("{error}")).is_err());
    }
}

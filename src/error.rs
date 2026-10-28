use core::fmt;
use std::io;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
use vstd::std_specs::convert::FromSpecImpl;

/// Stable deterministic I/O failure classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub enum IoErrorKind {
    /// Requested input was not found.
    NotFound,
    /// Input access was denied.
    PermissionDenied,
    /// A connection was refused.
    ConnectionRefused,
    /// A connection was reset.
    ConnectionReset,
    /// A connection was aborted.
    ConnectionAborted,
    /// An input source is not connected.
    NotConnected,
    /// An address is already in use.
    AddressInUse,
    /// An address is unavailable.
    AddressNotAvailable,
    /// A pipe was closed.
    BrokenPipe,
    /// A resource already exists.
    AlreadyExists,
    /// The operation would block.
    WouldBlock,
    /// An I/O argument is invalid.
    InvalidInput,
    /// Input data is invalid.
    InvalidData,
    /// The operation timed out.
    TimedOut,
    /// A write produced no bytes.
    WriteZero,
    /// The operation was interrupted.
    Interrupted,
    /// The operation is unsupported.
    Unsupported,
    /// Input ended unexpectedly.
    UnexpectedEof,
    /// The operating system reported memory exhaustion.
    OutOfMemory,
    /// Any unclassified I/O failure.
    Other,
}

impl From<io::ErrorKind> for IoErrorKind {
    fn from(kind: io::ErrorKind) -> Self {
        match kind {
            io::ErrorKind::NotFound => Self::NotFound,
            io::ErrorKind::PermissionDenied => Self::PermissionDenied,
            io::ErrorKind::ConnectionRefused => Self::ConnectionRefused,
            io::ErrorKind::ConnectionReset => Self::ConnectionReset,
            io::ErrorKind::ConnectionAborted => Self::ConnectionAborted,
            io::ErrorKind::NotConnected => Self::NotConnected,
            io::ErrorKind::AddrInUse => Self::AddressInUse,
            io::ErrorKind::AddrNotAvailable => Self::AddressNotAvailable,
            io::ErrorKind::BrokenPipe => Self::BrokenPipe,
            io::ErrorKind::AlreadyExists => Self::AlreadyExists,
            io::ErrorKind::WouldBlock => Self::WouldBlock,
            io::ErrorKind::InvalidInput => Self::InvalidInput,
            io::ErrorKind::InvalidData => Self::InvalidData,
            io::ErrorKind::TimedOut => Self::TimedOut,
            io::ErrorKind::WriteZero => Self::WriteZero,
            io::ErrorKind::Interrupted => Self::Interrupted,
            io::ErrorKind::Unsupported => Self::Unsupported,
            io::ErrorKind::UnexpectedEof => Self::UnexpectedEof,
            io::ErrorKind::OutOfMemory => Self::OutOfMemory,
            _ => Self::Other,
        }
    }
}

/// Detailed malformed Huffman table or code classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub enum HuffmanErrorKind {
    /// Reserved code-table flag bits were set.
    ReservedFlags(u8),
    /// The declared low/high bounds are invalid.
    InvalidBounds {
        /// Inclusive low bound.
        low: i32,
        /// Exclusive high bound.
        high: i32,
    },
    /// A range suffix length exceeds 32 bits.
    InvalidRangeLength(u8),
    /// Normal ranges do not exactly partition the declared interval.
    InvalidRangePartition,
    /// Normal and special lines are not in normative order.
    InvalidLineOrder,
    /// A LOW, HIGH, or OOB line has invalid fields.
    InvalidSpecialLine,
    /// A special line occurs more than once.
    DuplicateSpecialLine,
    /// The table defines no decodable codes.
    NoCodes,
    /// Canonical prefix assignment exceeds the available code space.
    Oversubscribed,
    /// More than seven bits follow an encoded custom table.
    TrailingBits(u64),
    /// Encoded custom-table padding contains a one bit.
    NonZeroPadding,
    /// No table line matches the coded prefix.
    InvalidCode,
}

/// Detailed malformed MMR stream classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub enum MmrErrorKind {
    /// No two-dimensional mode matches the coded prefix.
    InvalidMode {
        /// Bit offset of the mode prefix.
        bit_offset: usize,
    },
    /// T.6 extension modes are forbidden by JBIG2.
    ExtensionMode {
        /// Bit offset of the extension prefix.
        bit_offset: usize,
    },
    /// No run code matches the selected colour and prefix.
    InvalidRunCode {
        /// Bit offset of the run prefix.
        bit_offset: usize,
        /// Whether the run is black.
        black: bool,
    },
    /// A run exceeds the remaining row width.
    RunExceedsRow,
    /// A pass or vertical transition is out of bounds or moves backward.
    InvalidTransition,
    /// A mode fails to make semantic progress.
    NonProgress,
    /// A row uses more modes than its dimension-derived bound.
    TooManyModes,
    /// A required EOFB marker is absent.
    MissingEofb,
    /// EOFB bits do not match the normative marker.
    InvalidEofb,
    /// More than byte-alignment padding remains.
    TrailingData,
    /// Final byte-alignment padding contains a one bit.
    NonZeroPadding,
}

/// Detailed malformed pattern-dictionary or halftone classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub enum HalftoneErrorKind {
    /// Pattern dictionary flags are invalid.
    InvalidPatternDictionaryFlags(u8),
    /// Pattern dimensions are zero or otherwise invalid.
    InvalidPatternDimensions {
        /// Pattern width.
        width: u8,
        /// Pattern height.
        height: u8,
    },
    /// Halftone flags are invalid.
    InvalidHalftoneFlags(u8),
    /// MMR/template/skip configuration is invalid.
    InvalidConfiguration,
    /// A callback returned a bitmap with unexpected dimensions.
    InvalidBitmapDimensions {
        /// Expected width.
        expected_width: u32,
        /// Expected height.
        expected_height: u32,
        /// Actual width.
        actual_width: u32,
        /// Actual height.
        actual_height: u32,
    },
    /// Gray bit depth exceeds the supported 32-bit index domain.
    InvalidGrayBitDepth(u8),
    /// A halftone region references no patterns.
    EmptyPatternDictionary,
    /// Patterns do not share one fixed size.
    InconsistentPatternDimensions,
    /// A decoded gray index does not name a pattern.
    InvalidPatternIndex {
        /// Decoded pattern index.
        index: u32,
        /// Available pattern count.
        pattern_count: u64,
    },
}

/// Detailed malformed generic or refinement region classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub enum GenericRegionErrorKind {
    /// Reserved generic/refinement flag bits are set.
    ReservedFlags(u8),
    /// A generic/refinement flag combination is invalid.
    InvalidFlags(u8),
    /// A target adaptive pixel does not name an earlier pixel.
    InvalidAdaptivePixel {
        /// One-based adaptive-pixel number.
        number: u8,
        /// Horizontal offset.
        x: i32,
        /// Vertical offset.
        y: i32,
    },
    /// Skip bitmap dimensions do not match the target.
    InvalidSkipDimensions,
    /// An arithmetic bitmap lacks its FF AC terminator.
    InvalidArithmeticTerminator,
    /// An unexpected arithmetic marker was observed.
    UnexpectedArithmeticMarker {
        /// Marker prefix byte offset.
        offset: usize,
        /// Marker code byte.
        code: u8,
    },
    /// Terminal row count differs from decoded rows.
    RowCountMismatch {
        /// Expected decoded rows.
        expected: u32,
        /// Terminal row count.
        actual: u32,
    },
    /// A generated context index exceeds allocated statistics.
    ContextIndex {
        /// Generated context index.
        index: usize,
        /// Allocated context count.
        count: usize,
    },
}

/// A deterministic classification of malformed input.
#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub enum MalformedError {
    /// The input ended before a complete value was available.
    UnexpectedEof {
        /// Byte or bit offset at which the read started.
        offset: u64,
        /// Number of bytes or bits requested.
        needed: u64,
        /// Number of bytes or bits that remained.
        remaining: u64,
    },
    /// A standalone file did not start with the JBIG2 identifier.
    InvalidFileSignature,
    /// Reserved file-header flag bits were non-zero.
    ReservedFileHeaderFlags(u8),
    /// A bit read requested more than the supported 32 bits.
    InvalidBitCount(u8),
    /// A reserved segment type appeared in a segment header.
    ReservedSegmentType(u8),
    /// The three-bit referred-to count discriminator was reserved.
    InvalidReferredToCountCode(u8),
    /// Long-form referred-to counts were used for four or fewer references.
    NonCanonicalReferredToCount(u32),
    /// Unused retention bits were non-zero.
    NonZeroUnusedRetentionFlags,
    /// A segment referred to itself or a later-numbered segment.
    InvalidReferredSegmentNumber {
        /// Number of the segment containing the reference.
        segment: u32,
        /// Invalid referred-to segment number.
        referred: u32,
    },
    /// An extension header violates reserved/necessary flag rules.
    InvalidExtensionType(u32),
    /// A defined comment extension is not correctly terminated.
    InvalidCommentExtension,
    /// Profiles metadata is misplaced, duplicated, or not restrictive enough.
    InvalidProfilesSegment(u32),
    /// A Huffman table or coded prefix is malformed.
    InvalidHuffman(HuffmanErrorKind),
    /// A restored arithmetic probability context is invalid.
    InvalidArithmeticContextState {
        /// Table E.1 state index.
        state_index: u8,
        /// More-probable symbol value.
        mps: u8,
    },
    /// An arithmetic procedure was given too few probability contexts.
    InsufficientArithmeticContexts {
        /// Required number of contexts.
        required: usize,
        /// Supplied number of contexts.
        actual: usize,
    },
    /// IAID code length cannot be represented safely.
    InvalidIaidCodeLength(u8),
    /// An MMR mode, run, transition, marker, or padding rule is invalid.
    InvalidMmr(MmrErrorKind),
    /// A symbol dictionary or text-region state-machine rule is invalid.
    InvalidSymbolText(&'static str),
    /// A decoded symbol ID does not name an available symbol.
    SymbolIdOutOfRange {
        /// Decoded symbol ID.
        id: u32,
        /// Number of available symbols.
        available: u32,
    },
    /// Refined or collective symbol dimensions are invalid.
    InvalidSymbolDimensions,
    /// A pattern dictionary or halftone rule is invalid.
    InvalidHalftone(HalftoneErrorKind),
    /// A generic or refinement region rule is invalid.
    InvalidGenericRegion(GenericRegionErrorKind),
    /// Decoded segment order or page policy is inconsistent.
    InvalidPageState(&'static str),
    /// An unknown segment length appeared on a segment type that forbids it.
    InvalidUnknownSegmentLength(u8),
    /// A segment payload length did not match its fixed syntax.
    InvalidSegmentDataLength {
        /// Numeric segment type.
        segment_type: u8,
        /// Required number of bytes.
        expected: u64,
        /// Actual number of bytes.
        actual: u64,
    },
    /// A reserved region information flag was set.
    ReservedRegionFlags(u8),
    /// A region selected an undefined external combination operator.
    InvalidCombinationOperator(u8),
    /// An unknown-height page was not marked as striped.
    UnknownHeightWithoutStriping,
    /// An end-of-stripe row exceeds its page or is not increasing.
    InvalidStripeEndRow(u32),
    /// Segment numbers are duplicated.
    DuplicateSegmentNumber(u32),
    /// Standalone segment headers are not in increasing number order.
    NonIncreasingSegmentNumber {
        /// Previous segment number.
        previous: u32,
        /// Current segment number.
        current: u32,
    },
    /// A referred-to segment is absent from the available document state.
    MissingReferredSegment {
        /// Referring segment number.
        segment: u32,
        /// Missing segment number.
        referred: u32,
    },
    /// A segment has an invalid global or page association.
    InvalidPageAssociation {
        /// Segment number.
        segment: u32,
        /// Page association.
        page: u32,
    },
    /// A page's first associated segment was not page information.
    MissingPageInformation(u32),
    /// A page has duplicate page-information or end-of-page segments.
    DuplicatePageBoundary {
        /// Page number.
        page: u32,
        /// Boundary segment type.
        segment_type: u8,
    },
    /// A segment associated with a page appeared after its end-of-page segment.
    SegmentAfterEndOfPage {
        /// Segment number.
        segment: u32,
        /// Page number.
        page: u32,
    },
    /// A page did not end with an end-of-page segment.
    MissingEndOfPage(u32),
    /// A random-access file did not contain a final end-of-file header.
    MissingEndOfFile,
    /// Bytes occurred after a terminal end-of-file segment.
    TrailingData(u64),
    /// The declared page count differs from parsed pages.
    PageCountMismatch {
        /// Declared page count.
        declared: u32,
        /// Parsed page count.
        actual: u32,
    },
    /// A fixed-length segment contains semantically invalid reference types.
    InvalidSegmentReferences(u32),
    /// An unknown-length generic region terminator was not found.
    MissingGenericRegionTerminator,
    /// An unknown-length generic region row count exceeds its declared height.
    GenericRegionRowCount {
        /// Declared region height.
        declared: u32,
        /// Encoded terminal row count.
        actual: u32,
    },
    /// Retention flags permit discarding a segment before its final use.
    InvalidRetentionFlags {
        /// Segment carrying the invalid retention decision.
        segment: u32,
        /// Segment whose state would be discarded incorrectly.
        referred: u32,
    },
    /// An input-derived arithmetic operation overflowed.
    IntegerOverflow,
}

impl fmt::Display for MalformedError {
    #[allow(clippy::too_many_lines)]
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof {
                offset,
                needed,
                remaining,
            } => write!(
                formatter,
                "unexpected end of input at {offset}: needed {needed}, remaining {remaining}"
            ),
            Self::InvalidFileSignature => formatter.write_str("invalid JBIG2 file signature"),
            Self::ReservedFileHeaderFlags(flags) => {
                write!(
                    formatter,
                    "reserved JBIG2 file-header flags are set: {flags:#04x}"
                )
            }
            Self::InvalidBitCount(count) => {
                write!(formatter, "cannot read {count} bits into a 32-bit value")
            }
            Self::ReservedSegmentType(segment_type) => {
                write!(formatter, "reserved JBIG2 segment type {segment_type}")
            }
            Self::InvalidReferredToCountCode(code) => {
                write!(formatter, "reserved referred-to count code {code}")
            }
            Self::NonCanonicalReferredToCount(count) => {
                write!(
                    formatter,
                    "long-form referred-to count used for short count {count}"
                )
            }
            Self::NonZeroUnusedRetentionFlags => {
                formatter.write_str("unused retention flag bits are non-zero")
            }
            Self::InvalidReferredSegmentNumber { segment, referred } => write!(
                formatter,
                "segment {segment} refers to non-preceding segment {referred}"
            ),
            Self::InvalidUnknownSegmentLength(segment_type) => write!(
                formatter,
                "unknown data length is invalid for segment type {segment_type}"
            ),
            Self::InvalidSegmentDataLength {
                segment_type,
                expected,
                actual,
            } => write!(
                formatter,
                "segment type {segment_type} requires {expected} data bytes, got {actual}"
            ),
            Self::ReservedRegionFlags(flags) => {
                write!(formatter, "reserved region flags are set: {flags:#04x}")
            }
            Self::InvalidCombinationOperator(operator) => {
                write!(formatter, "invalid combination operator {operator}")
            }
            Self::UnknownHeightWithoutStriping => {
                formatter.write_str("unknown-height page is not striped")
            }
            Self::InvalidStripeEndRow(row) => {
                write!(formatter, "invalid end-of-stripe row {row}")
            }
            Self::DuplicateSegmentNumber(segment) => {
                write!(formatter, "duplicate segment number {segment}")
            }
            Self::NonIncreasingSegmentNumber { previous, current } => write!(
                formatter,
                "standalone segment number {current} does not follow {previous}"
            ),
            Self::MissingReferredSegment { segment, referred } => {
                write!(
                    formatter,
                    "segment {segment} refers to missing segment {referred}"
                )
            }
            Self::InvalidPageAssociation { segment, page } => {
                write!(
                    formatter,
                    "segment {segment} has invalid page association {page}"
                )
            }
            Self::MissingPageInformation(page) => {
                write!(
                    formatter,
                    "page {page} does not start with page information"
                )
            }
            Self::DuplicatePageBoundary { page, segment_type } => write!(
                formatter,
                "page {page} has duplicate boundary segment type {segment_type}"
            ),
            Self::SegmentAfterEndOfPage { segment, page } => {
                write!(
                    formatter,
                    "segment {segment} occurs after end of page {page}"
                )
            }
            Self::MissingEndOfPage(page) => {
                write!(formatter, "page {page} has no end-of-page segment")
            }
            Self::MissingEndOfFile => {
                formatter.write_str("random-access file has no final end-of-file segment")
            }
            Self::TrailingData(bytes) => {
                write!(formatter, "{bytes} trailing bytes follow the JBIG2 stream")
            }
            Self::PageCountMismatch { declared, actual } => write!(
                formatter,
                "file declares {declared} pages but contains {actual}"
            ),
            Self::InvalidSegmentReferences(segment) => {
                write!(
                    formatter,
                    "segment {segment} has invalid referred-to segment types"
                )
            }
            Self::MissingGenericRegionTerminator => {
                formatter.write_str("unknown-length generic region has no terminator")
            }
            Self::GenericRegionRowCount { declared, actual } => write!(
                formatter,
                "generic region row count {actual} exceeds declared height {declared}"
            ),
            Self::InvalidRetentionFlags { segment, referred } => write!(
                formatter,
                "segment {segment} has invalid retention flags for segment {referred}"
            ),
            Self::InvalidExtensionType(extension_type) => {
                write!(formatter, "invalid extension type {extension_type:#010x}")
            }
            Self::InvalidCommentExtension => formatter.write_str("malformed comment extension"),
            Self::InvalidProfilesSegment(segment) => {
                write!(formatter, "invalid profiles segment {segment}")
            }
            Self::InvalidHuffman(kind) => {
                write!(formatter, "invalid Huffman data: {kind:?}")
            }
            Self::InvalidArithmeticContextState { state_index, mps } => write!(
                formatter,
                "invalid arithmetic context state I={state_index}, MPS={mps}"
            ),
            Self::InsufficientArithmeticContexts { required, actual } => write!(
                formatter,
                "arithmetic decoding requires {required} contexts, got {actual}"
            ),
            Self::InvalidIaidCodeLength(length) => {
                write!(formatter, "invalid IAID code length {length}")
            }
            Self::InvalidMmr(kind) => {
                write!(formatter, "invalid MMR data: {kind:?}")
            }
            Self::InvalidSymbolText(reason) => {
                write!(formatter, "invalid symbol/text data: {reason}")
            }
            Self::SymbolIdOutOfRange { id, available } => write!(
                formatter,
                "symbol ID {id} is outside {available} available symbols"
            ),
            Self::InvalidSymbolDimensions => formatter.write_str("invalid symbol dimensions"),
            Self::InvalidHalftone(kind) => {
                write!(formatter, "invalid halftone data: {kind:?}")
            }
            Self::InvalidGenericRegion(kind) => {
                write!(formatter, "invalid generic-region data: {kind:?}")
            }
            Self::InvalidPageState(reason) => {
                write!(formatter, "invalid page state: {reason}")
            }
            Self::IntegerOverflow => formatter.write_str("input-derived integer overflow"),
        }
    }
}

/// A feature that is valid JBIG2 syntax but outside the approved profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub enum UnsupportedFeature {
    /// Colour palette and coloured-region extensions.
    Colour,
    /// A recognized but unsupported segment type.
    SegmentType(u8),
    /// A necessary extension with an unknown type.
    Extension(u32),
}

impl fmt::Display for UnsupportedFeature {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Colour => formatter.write_str("JBIG2 colour extensions"),
            Self::SegmentType(segment_type) => {
                write!(formatter, "JBIG2 segment type {segment_type}")
            }
            Self::Extension(extension_type) => {
                write!(
                    formatter,
                    "necessary JBIG2 extension {extension_type:#010x}"
                )
            }
        }
    }
}

/// The resource whose configured limit was exceeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub enum LimitKind {
    /// Page or region width.
    Width,
    /// Page or region height.
    Height,
    /// Pixels in one page or bitmap.
    PagePixels,
    /// Decoder-managed memory.
    Memory,
    /// Input stream bytes.
    StreamBytes,
    /// Number of pages.
    Pages,
    /// Number of segments.
    Segments,
    /// Number of symbols.
    Symbols,
    /// References on one segment.
    ReferencesPerSegment,
    /// References across the decode operation.
    TotalReferences,
    /// Reference graph depth.
    ReferenceDepth,
    /// Nested decoding depth.
    NestingDepth,
    /// Abstract work units.
    Work,
    /// Cumulative decoded bitmap bytes.
    DecodedBitmapBytes,
    /// Retained decoder state.
    RetainedState,
}

impl fmt::Display for LimitKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

/// Details of a resource-limit failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub struct LimitExceeded {
    /// Resource that exceeded its limit.
    pub kind: LimitKind,
    /// Configured inclusive maximum.
    pub limit: u64,
    /// Input-derived value that exceeded the maximum.
    pub actual: u64,
}

impl fmt::Display for LimitExceeded {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} limit exceeded: maximum {}, actual {}",
            self.kind, self.limit, self.actual
        )
    }
}

/// Error returned by parsing and decoding operations.
#[derive(Debug)]
#[non_exhaustive]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub enum DecodeError {
    /// The input violates the JBIG2 syntax or semantics.
    Malformed(MalformedError),
    /// The input uses valid syntax outside the approved support matrix.
    Unsupported(UnsupportedFeature),
    /// An explicit resource limit was exceeded.
    LimitExceeded(LimitExceeded),
    /// A fallible allocation could not reserve the requested storage.
    Allocation {
        /// Requested allocation size in bytes.
        requested_bytes: u64,
    },
    /// Reading from an input source failed.
    Io(IoErrorKind),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(error) => write!(formatter, "malformed JBIG2 input: {error}"),
            Self::Unsupported(feature) => {
                write!(formatter, "unsupported JBIG2 feature: {feature}")
            }
            Self::LimitExceeded(error) => error.fmt(formatter),
            Self::Allocation { requested_bytes } => {
                write!(formatter, "failed to allocate {requested_bytes} bytes")
            }
            Self::Io(kind) => write!(formatter, "input/output error: {kind:?}"),
        }
    }
}

impl std::error::Error for DecodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

impl DecodeError {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == DecodeError::Malformed(error),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    fn from_malformed(error: MalformedError) -> Self {
        Self::Malformed(error)
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == DecodeError::LimitExceeded(error),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    fn from_limit_exceeded(error: LimitExceeded) -> Self {
        Self::LimitExceeded(error)
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl From<MalformedError> for DecodeError {
    fn from(error: MalformedError) -> Self {
        Self::from_malformed(error)
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl From<LimitExceeded> for DecodeError {
    fn from(error: LimitExceeded) -> Self {
        Self::from_limit_exceeded(error)
    }
}

#[cfg(verus_keep_ghost)]
verus! {

impl FromSpecImpl<MalformedError> for DecodeError {
    open spec fn obeys_from_spec() -> bool {
        true
    }

    open spec fn from_spec(error: MalformedError) -> DecodeError {
        DecodeError::Malformed(error)
    }
}

impl FromSpecImpl<LimitExceeded> for DecodeError {
    open spec fn obeys_from_spec() -> bool {
        true
    }

    open spec fn from_spec(error: LimitExceeded) -> DecodeError {
        DecodeError::LimitExceeded(error)
    }
}

}

impl From<io::Error> for DecodeError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.kind().into())
    }
}

#[cfg(test)]
mod tests;

//! Generic and generic-refinement region decoding.
//!
//! This module implements T.88 clauses 6.2, 6.3, 7.4.6, and 7.4.7 without
//! duplicating the crate's arithmetic or MMR entropy decoders.  Context slots
//! are kept in normative geometric order; moving an adaptive pixel changes
//! only the sampled coordinate, never the probability-context bit assigned to
//! that slot.

use core::fmt;

use crate::arithmetic::{reset_contexts, ArithmeticContext, ArithmeticDecoder, ArithmeticError};
use crate::mmr::{decode_mmr_reused, EofbPolicy, MmrError, MmrScratch};
use crate::reader::ByteReader;
use crate::segment_data::{parse_region_info, RegionInfo};
use crate::{
    Bitmap, DecodeError, GenericRegionErrorKind, LimitExceeded, LimitKind, Limits, MalformedError,
};
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg(verus_keep_ghost)]
#[rustfmt::skip]
#[path = "../specs/generic_region.rs"]
mod generic_region_spec;

#[cfg(verus_keep_ghost)]
#[rustfmt::skip]
#[path = "../proofs/spec_properties/generic_region.rs"]
mod generic_region_properties;

#[cfg(verus_keep_ghost)]
#[rustfmt::skip]
#[path = "../proofs/shared/generic_region.rs"]
mod generic_region_shared;

#[cfg(verus_keep_ghost)]
#[macro_use]
#[rustfmt::skip]
#[path = "../proofs/production_impl/generic_region.rs"]
mod generic_region_proofs;

#[cfg_attr(verus_keep_ghost, verus_verify)]
const ARITHMETIC_TERMINATOR: [u8; 2] = [0xff, 0xac];
#[cfg_attr(verus_keep_ghost, verus_verify)]
const MMR_UNKNOWN_LENGTH_SEPARATOR: [u8; 2] = [0x00, 0x00];
#[cfg_attr(verus_keep_ghost, verus_verify)]
const MAX_GENERIC_CONTEXTS: usize = 1 << 16;
#[cfg_attr(verus_keep_ghost, verus_verify)]
const MAX_REFINEMENT_CONTEXTS: usize = 1 << 13;

/// A module-local error whose dedicated crate-wide malformed classifications
/// can be added when this isolated module is wired into the document decoder.
#[derive(Debug)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) enum GenericRegionError {
    /// An error already represented by the crate-wide error vocabulary.
    Decode(DecodeError),
    /// Reserved bits were set in a generic or refinement flag byte.
    ReservedFlags(u8),
    /// A flag combination violates the segment syntax.
    InvalidFlags(u8),
    /// A target-plane adaptive pixel does not refer to an earlier pixel.
    InvalidAdaptivePixel {
        /// One-based adaptive-pixel number.
        number: u8,
        /// Horizontal offset.
        x: i32,
        /// Vertical offset.
        y: i32,
    },
    /// A caller supplied a skip bitmap of the wrong size.
    InvalidSkipDimensions,
    /// A required arithmetic marker was absent or not the final coded bytes.
    InvalidArithmeticTerminator,
    /// A marker other than `FF AC` occurred in an arithmetic block.
    UnexpectedArithmeticMarker { offset: usize, code: u8 },
    /// The unknown-length terminal count disagreed with decoded rows.
    RowCountMismatch { expected: u32, actual: u32 },
    /// A generated context index was outside its allocated statistics set.
    ContextIndex { index: usize, count: usize },
}

impl fmt::Display for GenericRegionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(error) => error.fmt(formatter),
            Self::ReservedFlags(flags) => {
                write!(
                    formatter,
                    "reserved generic-region flags are set: {flags:#04x}"
                )
            }
            Self::InvalidFlags(flags) => {
                write!(
                    formatter,
                    "invalid generic-region flag combination: {flags:#04x}"
                )
            }
            Self::InvalidAdaptivePixel { number, x, y } => write!(
                formatter,
                "adaptive pixel {number} at ({x}, {y}) is not an earlier target pixel"
            ),
            Self::InvalidSkipDimensions => {
                formatter.write_str("generic-region skip bitmap has the wrong dimensions")
            }
            Self::InvalidArithmeticTerminator => {
                formatter.write_str("arithmetic region does not end in FF AC")
            }
            Self::UnexpectedArithmeticMarker { offset, code } => write!(
                formatter,
                "unexpected arithmetic marker FF {code:02X} at byte {offset}"
            ),
            Self::RowCountMismatch { expected, actual } => write!(
                formatter,
                "terminal row count {actual} differs from decoded row count {expected}"
            ),
            Self::ContextIndex { index, count } => write!(
                formatter,
                "context index {index} is outside a statistics set of length {count}"
            ),
        }
    }
}

impl std::error::Error for GenericRegionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Decode(error) => Some(error),
            _ => None,
        }
    }
}

impl From<DecodeError> for GenericRegionError {
    fn from(error: DecodeError) -> Self {
        Self::Decode(error)
    }
}

impl From<MalformedError> for GenericRegionError {
    fn from(error: MalformedError) -> Self {
        Self::Decode(error.into())
    }
}

impl From<MmrError> for GenericRegionError {
    fn from(error: MmrError) -> Self {
        Self::Decode(error.into())
    }
}

impl From<GenericRegionError> for DecodeError {
    fn from(error: GenericRegionError) -> Self {
        let kind = match error {
            GenericRegionError::Decode(error) => return error,
            GenericRegionError::ReservedFlags(flags) => {
                GenericRegionErrorKind::ReservedFlags(flags)
            }
            GenericRegionError::InvalidFlags(flags) => GenericRegionErrorKind::InvalidFlags(flags),
            GenericRegionError::InvalidAdaptivePixel { number, x, y } => {
                GenericRegionErrorKind::InvalidAdaptivePixel { number, x, y }
            }
            GenericRegionError::InvalidSkipDimensions => {
                GenericRegionErrorKind::InvalidSkipDimensions
            }
            GenericRegionError::InvalidArithmeticTerminator => {
                GenericRegionErrorKind::InvalidArithmeticTerminator
            }
            GenericRegionError::UnexpectedArithmeticMarker { offset, code } => {
                GenericRegionErrorKind::UnexpectedArithmeticMarker { offset, code }
            }
            GenericRegionError::RowCountMismatch { expected, actual } => {
                GenericRegionErrorKind::RowCountMismatch { expected, actual }
            }
            GenericRegionError::ContextIndex { index, count } => {
                GenericRegionErrorKind::ContextIndex { index, count }
            }
        };
        Self::Malformed(MalformedError::InvalidGenericRegion(kind))
    }
}

/// A signed template offset.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct TemplateOffset {
    pub(crate) x: i32,
    pub(crate) y: i32,
}

impl TemplateOffset {
    const ZERO: Self = Self { x: 0, y: 0 };

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out.x == x, out.y == y,
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// One of the four generic-region arithmetic templates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) enum GenericTemplate {
    Template0,
    Template1,
    Template2,
    Template3,
}

impl GenericTemplate {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures generic_region_spec::generic_template_code(out)
            == ((flags >> 1) & 0x03) as nat,
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    fn from_flags(flags: u8) -> Self {
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(((flags >> 1) & 0x03) <= 3) by (bit_vector);
        }

        match (flags >> 1) & 0x03 {
            0 => Self::Template0,
            1 => Self::Template1,
            2 => Self::Template2,
            _ => Self::Template3,
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures
            out as nat == generic_region_spec::generic_context_bits(self),
            10 <= out <= 16,
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    const fn context_bits(self) -> u8 {
        #[cfg(verus_keep_ghost)]
        production_generic_template_facts!(self);
        match self {
            Self::Template0 => 16,
            Self::Template1 => 13,
            Self::Template2 | Self::Template3 => 10,
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures
            out as nat == generic_region_spec::generic_sltp_context(self),
            (out as nat) < generic_region_spec::pow2(
                generic_region_spec::generic_context_bits(self),
            ),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    const fn sltp_context(self) -> usize {
        #[cfg(verus_keep_ghost)]
        production_generic_template_facts!(self);
        match self {
            Self::Template0 => 0x9b25,
            Self::Template1 => 0x0795,
            Self::Template2 => 0x00e5,
            Self::Template3 => 0x0195,
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out == (flags & 0x01 != 0),
))]
fn generic_mmr_flag(flags: u8) -> bool {
    flags & 0x01 != 0
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out == (flags & 0x08 != 0),
))]
fn generic_typical_prediction_flag(flags: u8) -> bool {
    flags & 0x08 != 0
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out == (flags & 0x10 != 0),
))]
fn generic_extended_template_flag(flags: u8) -> bool {
    flags & 0x10 != 0
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out == flags & 0xe0,
))]
fn generic_reserved_flags(flags: u8) -> u8 {
    flags & 0xe0
}

/// Parsed generic-region data header.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct GenericRegionHeader {
    pub(crate) region: RegionInfo,
    pub(crate) mmr: bool,
    pub(crate) template: GenericTemplate,
    pub(crate) typical_prediction: bool,
    pub(crate) extended_template: bool,
    pub(crate) adaptive: [TemplateOffset; 12],
}

/// The decoded bitmap and number of data bytes consumed.
#[derive(Debug)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct DecodedGenericRegion {
    pub(crate) header: GenericRegionHeader,
    pub(crate) bitmap: Bitmap,
    pub(crate) consumed_bytes: usize,
    pub(crate) decoded_rows: u32,
}

/// Reusable `GB` probability statistics for one generic template.
///
/// This type intentionally has no infallible [`Clone`] implementation:
/// retaining dictionary statistics must use [`Self::try_snapshot`] so memory
/// limits and allocation failure remain observable.
#[derive(Debug)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct GenericContexts {
    pub(crate) template: GenericTemplate,
    pub(crate) extended_template: bool,
    pub(crate) contexts: Vec<ArithmeticContext>,
    pub(crate) storage_bytes: u64,
}

/// A fallibly allocated retained copy of generic arithmetic statistics.
#[derive(Debug)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct GenericContextSnapshot {
    template: GenericTemplate,
    extended_template: bool,
    contexts: Vec<ArithmeticContext>,
    storage_bytes: u64,
}

impl GenericContexts {
    /// Allocate a default statistics set compatible with `template`.
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures match out {
            Ok(contexts) =>
                contexts.template == template
                && contexts.extended_template == extended_template
                && generic_region_spec::generic_contexts_valid(&contexts),
            Err(_) => true,
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn new(
        template: GenericTemplate,
        extended_template: bool,
        limits: &Limits,
    ) -> Result<Self, GenericRegionError> {
        let count = generic_context_count(template);
        let storage_bytes = context_storage_bytes(count)?;
        let contexts = allocate_contexts(count, 0, limits)?;
        Ok(Self {
            template,
            extended_template,
            contexts,
            storage_bytes,
        })
    }

    /// Reset all contexts to `I = 0, MPS = 0`.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub(crate) fn reset(&mut self) -> Result<(), GenericRegionError> {
        validate_generic_context_layout(self.template, self.extended_template, &self.contexts)?;
        reset_contexts(&mut self.contexts);
        Ok(())
    }

    /// Fallibly retain the current statistics for later dictionary use.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub(crate) fn try_snapshot(
        &self,
        limits: &Limits,
    ) -> Result<GenericContextSnapshot, GenericRegionError> {
        validate_generic_context_layout(self.template, self.extended_template, &self.contexts)?;
        let contexts = try_copy_contexts(&self.contexts, self.storage_bytes, limits)?;
        Ok(GenericContextSnapshot {
            template: self.template,
            extended_template: self.extended_template,
            contexts,
            storage_bytes: self.storage_bytes,
        })
    }

    /// Restore retained statistics, rejecting a snapshot for another template.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub(crate) fn restore(
        &mut self,
        snapshot: &GenericContextSnapshot,
        limits: &Limits,
    ) -> Result<(), GenericRegionError> {
        if self.template != snapshot.template
            || self.extended_template != snapshot.extended_template
        {
            return Err(GenericRegionError::InvalidFlags(generic_template_code(
                snapshot.template,
                snapshot.extended_template,
            )));
        }
        validate_generic_context_layout(
            snapshot.template,
            snapshot.extended_template,
            &snapshot.contexts,
        )?;
        validate_snapshot_limits(snapshot.storage_bytes, limits)?;
        if self.contexts.len() != snapshot.contexts.len() {
            return Err(GenericRegionError::ContextIndex {
                index: snapshot.contexts.len(),
                count: self.contexts.len(),
            });
        }
        validate_probability_states(&snapshot.contexts)?;
        let mut index = 0_usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                self.contexts@.len() == snapshot.contexts@.len(),
                index <= self.contexts@.len(),
            decreases self.contexts@.len() - index,
        ))]
        while index < self.contexts.len() {
            self.contexts[index] = snapshot.contexts[index];
            index += 1;
        }
        Ok(())
    }
}

/// Parameters for one bitmap decoded from a reusable generic arithmetic
/// session.
///
/// A session fixes the template and its probability-statistics set, while
/// dimensions, adaptive locations, typical prediction, and skip data belong
/// to each individual generic-region procedure invocation.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct GenericArithmeticCall<'a> {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) adaptive: [TemplateOffset; 12],
    pub(crate) typical_prediction: bool,
    pub(crate) skip: Option<&'a Bitmap>,
}

/// A continuous arithmetic stream and reusable generic-region statistics.
///
/// This is used when multiple bitmaps, such as halftone gray planes, are coded
/// consecutively with one arithmetic decoder and one `GB` statistics set.
/// Calling [`Self::decode_bitmap`] does not reset either the entropy decoder or
/// the contexts.
#[derive(Debug)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct GenericArithmeticSession<'a> {
    pub(crate) decoder: ArithmeticDecoder<'a>,
    pub(crate) contexts: GenericContexts,
    pub(crate) coded_length: usize,
    pub(crate) work_used: u64,
    pub(crate) limits: Limits,
}

impl<'a> GenericArithmeticSession<'a> {
    /// Start a reusable session at the beginning of one bounded arithmetic
    /// stream.
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures match out {
            Ok(session) => generic_region_spec::generic_session_valid(&session),
            Err(_) => true,
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn new(
        coded: &'a [u8],
        template: GenericTemplate,
        extended_template: bool,
        limits: &Limits,
    ) -> Result<Self, GenericRegionError> {
        enforce_input_limit(coded, limits)?;
        validate_arithmetic_terminator(coded)?;
        let contexts = GenericContexts::new(template, extended_template, limits)?;
        let decoder = match ArithmeticDecoder::new(coded) {
            Ok(decoder) => decoder,
            Err(error) => return Err(GenericRegionError::Decode(error.into_decode())),
        };
        Ok(Self {
            decoder,
            contexts,
            coded_length: coded.len(),
            work_used: 0,
            limits: *limits,
        })
    }

    /// Decode another bitmap while preserving this session's decoder position
    /// and adaptive probability statistics.
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            generic_region_spec::generic_session_valid(old(self)),
            match call.skip {
                Some(mask) => crate::spec::bitmap::storage_valid(mask),
                None => true,
            },
        ensures
            out.is_ok() ==> generic_region_spec::generic_session_valid(final(self)),
            match out {
                Ok(bitmap) => crate::spec::bitmap::storage_valid(&bitmap),
                Err(_) => true,
            },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn decode_bitmap(
        &mut self,
        call: GenericArithmeticCall<'_>,
    ) -> Result<Bitmap, GenericRegionError> {
        self.decode_bitmap_rows(call, call.height)
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            generic_region_spec::generic_session_valid(old(self)),
            match call.skip {
                Some(mask) => crate::spec::bitmap::storage_valid(mask),
                None => true,
            },
        ensures
            out.is_ok() ==> generic_region_spec::generic_session_valid(final(self)),
            match out {
                Ok(bitmap) => crate::spec::bitmap::storage_valid(&bitmap),
                Err(_) => true,
            },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    fn decode_bitmap_rows(
        &mut self,
        call: GenericArithmeticCall<'_>,
        rows: u32,
    ) -> Result<Bitmap, GenericRegionError> {
        decode_generic_on(
            &mut self.decoder,
            &mut self.contexts,
            call,
            rows,
            &self.limits,
            &mut self.work_used,
        )
    }

    /// Current compressed-byte position. A recognized marker is not consumed,
    /// so this equals the marker prefix offset after marker recognition.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub(crate) const fn consumed_byte_offset(&self) -> usize {
        self.decoder.byte_position()
    }

    /// Offset of a marker already observed by the arithmetic decoder.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub(crate) fn observed_marker_offset(&self) -> Option<usize> {
        self.decoder.marker().map(|marker| marker.offset)
    }

    /// Validate that any observed marker is the bounded final `FF AC` marker
    /// and return that marker's offset.
    ///
    /// The arithmetic decoder need not read the marker to complete the
    /// requested decisions. The complete input block was already checked by
    /// [`Self::new`], so the returned offset is still authoritative when
    /// [`Self::observed_marker_offset`] is `None`.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub(crate) fn validate_final_marker(&self) -> Result<usize, GenericRegionError> {
        validate_observed_marker(&self.decoder, self.coded_length)?;
        self.coded_length
            .checked_sub(2)
            .ok_or(GenericRegionError::InvalidArithmeticTerminator)
    }
}

/// Parse a generic-region header and return its byte length.
///
/// For extended template 0 this deliberately consumes 24 bytes: twelve signed
/// X/Y pairs.  T.88's isolated phrase "32-byte field" conflicts with both the
/// enumerated byte range 0..=23 and the twelve defined adaptive pixels.
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) fn parse_generic_region_header(
    payload: &[u8],
    limits: &Limits,
) -> Result<(GenericRegionHeader, usize), GenericRegionError> {
    enforce_input_limit(payload, limits)?;
    let mut reader = ByteReader::new(payload);
    let region = parse_region_info(&mut reader, limits)?;
    let flags = reader.read_u8()?;
    let reserved = generic_reserved_flags(flags);
    if reserved != 0 {
        return Err(GenericRegionError::ReservedFlags(reserved));
    }

    let mmr = generic_mmr_flag(flags);
    let template = GenericTemplate::from_flags(flags);
    let typical_prediction = generic_typical_prediction_flag(flags);
    let extended_template = generic_extended_template_flag(flags);
    if mmr && template != GenericTemplate::Template0 {
        return Err(GenericRegionError::InvalidFlags(flags));
    }

    let mut adaptive = [TemplateOffset { x: 0, y: 0 }; 12];
    if !mmr {
        let count = if template == GenericTemplate::Template0 {
            if extended_template {
                12
            } else {
                4
            }
        } else {
            1
        };
        let mut index = 0_usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                index <= count,
                count <= 12,
                reader.position <= reader.bytes@.len(),
            decreases count - index,
        ))]
        while index < count {
            adaptive[index] = read_offset(&mut reader)?;
            let Ok(number) = u8::try_from(index + 1) else {
                return Err(MalformedError::IntegerOverflow.into());
            };
            validate_target_adaptive(number, adaptive[index])?;
            index += 1;
        }
    }

    Ok((
        GenericRegionHeader {
            region,
            mmr,
            template,
            typical_prediction,
            extended_template,
            adaptive,
        },
        reader.position(),
    ))
}

/// Decode a complete generic-region data part.
///
/// If `unknown_length` is true, `payload` may include following input.  The
/// returned `consumed_bytes` stops immediately after the terminal row count.
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(decoded) => crate::spec::bitmap::storage_valid(&decoded.bitmap),
        Err(_) => true,
    },
))]
pub(crate) fn decode_generic_region_segment(
    payload: &[u8],
    unknown_length: bool,
    limits: &Limits,
) -> Result<DecodedGenericRegion, GenericRegionError> {
    let mut scratch = MmrScratch::default();
    decode_generic_region_segment_reused(payload, unknown_length, limits, &mut scratch)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(decoded) => crate::spec::bitmap::storage_valid(&decoded.bitmap),
        Err(_) => true,
    },
))]
pub(crate) fn decode_generic_region_segment_reused(
    payload: &[u8],
    unknown_length: bool,
    limits: &Limits,
    scratch: &mut MmrScratch,
) -> Result<DecodedGenericRegion, GenericRegionError> {
    let (header, header_length) = parse_generic_region_header(payload, limits)?;
    if header_length > payload.len() {
        return Err(MalformedError::IntegerOverflow.into());
    }
    let coded_and_following = &payload[header_length..];

    let (coded, rows, coded_consumed, eofb_policy) = if unknown_length {
        let terminal = split_unknown_length_data(
            coded_and_following,
            header.mmr,
            header.region.height,
            limits,
        )?;
        (
            terminal.coded,
            terminal.rows,
            terminal.consumed,
            EofbPolicy::Required,
        )
    } else {
        (
            coded_and_following,
            header.region.height,
            coded_and_following.len(),
            EofbPolicy::Optional,
        )
    };

    let bitmap = if header.mmr {
        let decoded = decode_mmr_reused(
            coded,
            header.region.width,
            rows,
            limits,
            eofb_policy,
            scratch,
        )?;
        pad_bitmap_height(decoded, header.region.height, limits)?
    } else if unknown_length {
        decode_arithmetic_generic(coded, &header, rows, None, limits)?
    } else {
        decode_known_length_arithmetic_generic(coded, &header, rows, limits)?
    };

    let consumed_bytes = header_length
        .checked_add(coded_consumed)
        .ok_or(MalformedError::IntegerOverflow)?;
    Ok(DecodedGenericRegion {
        header,
        bitmap,
        consumed_bytes,
        decoded_rows: rows,
    })
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(bitmap) => crate::spec::bitmap::storage_valid(&bitmap),
        Err(_) => true,
    },
))]
fn decode_known_length_arithmetic_generic(
    coded: &[u8],
    header: &GenericRegionHeader,
    rows: u32,
    limits: &Limits,
) -> Result<Bitmap, GenericRegionError> {
    let has_terminator = coded.len() >= 2
        && coded[coded.len() - 2] == ARITHMETIC_TERMINATOR[0]
        && coded[coded.len() - 1] == ARITHMETIC_TERMINATOR[1];
    if has_terminator {
        return decode_arithmetic_generic(coded, header, rows, None, limits);
    }
    if coded.last() != Some(&0xff) {
        return Err(GenericRegionError::InvalidArithmeticTerminator);
    }
    let Ok(requested_bytes) = u64::try_from(coded.len()) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    let requested_bytes = requested_bytes
        .checked_add(1)
        .ok_or(MalformedError::IntegerOverflow)?;
    if requested_bytes > limits.max_decoder_managed_memory_bytes {
        return Err(DecodeError::LimitExceeded(LimitExceeded {
            kind: LimitKind::Memory,
            limit: limits.max_decoder_managed_memory_bytes,
            actual: requested_bytes,
        })
        .into());
    }
    let mut normalized = Vec::new();
    let capacity = coded
        .len()
        .checked_add(1)
        .ok_or(MalformedError::IntegerOverflow)?;
    if normalized.try_reserve(capacity).is_err() {
        return Err(DecodeError::Allocation { requested_bytes }.into());
    }
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index <= coded@.len(),
            normalized@ == coded@.subrange(0, index as int),
        decreases coded@.len() - index,
    ))]
    while index < coded.len() {
        normalized.push(coded[index]);
        index += 1;
    }
    normalized.push(ARITHMETIC_TERMINATOR[1]);
    decode_arithmetic_generic(&normalized, header, rows, None, limits)
}

#[derive(Debug)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
struct UnknownLengthData<'a> {
    coded: &'a [u8],
    rows: u32,
    consumed: usize,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn split_unknown_length_data<'a>(
    data: &'a [u8],
    mmr: bool,
    declared_height: u32,
    limits: &Limits,
) -> Result<UnknownLengthData<'a>, GenericRegionError> {
    enforce_input_limit(data, limits)?;
    let marker = if mmr {
        MMR_UNKNOWN_LENGTH_SEPARATOR
    } else {
        ARITHMETIC_TERMINATOR
    };

    let mut marker_offset = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant marker_offset <= data@.len(),
        decreases data@.len() - marker_offset,
    ))]
    while marker_offset < data.len() {
        let Some(next) = marker_offset.checked_add(1) else {
            return Err(MalformedError::IntegerOverflow.into());
        };
        if next >= data.len() {
            return Err(MalformedError::MissingGenericRegionTerminator.into());
        }
        if data[marker_offset] == marker[0] && data[next] == marker[1] {
            break;
        }
        marker_offset = next;
    }
    if marker_offset == data.len() {
        return Err(MalformedError::MissingGenericRegionTerminator.into());
    }
    let count_start = marker_offset
        .checked_add(2)
        .ok_or(MalformedError::IntegerOverflow)?;
    let count_end = count_start
        .checked_add(4)
        .ok_or(MalformedError::IntegerOverflow)?;
    let remaining = if count_start <= data.len() {
        data.len() - count_start
    } else {
        0
    };
    if count_end > data.len() {
        return Err(unexpected_eof(count_start, 4, remaining));
    }
    let rows = (u32::from(data[count_start]) << 24)
        | (u32::from(data[count_start + 1]) << 16)
        | (u32::from(data[count_start + 2]) << 8)
        | u32::from(data[count_start + 3]);
    if rows > declared_height {
        return Err(MalformedError::GenericRegionRowCount {
            declared: declared_height,
            actual: rows,
        }
        .into());
    }

    let coded_end = if mmr { marker_offset } else { count_start };
    if coded_end > data.len() {
        return Err(MalformedError::IntegerOverflow.into());
    }
    let coded = &data[..coded_end];
    if !mmr {
        validate_arithmetic_terminator(coded)?;
    }
    Ok(UnknownLengthData {
        coded,
        rows,
        consumed: count_end,
    })
}

/// Decode arithmetic generic-region data, optionally with a skip bitmap.
///
/// `rows` may be less than the declared height only for unknown-length
/// immediate generic regions.  Undecoded nominal rows remain zero.
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires match skip {
        Some(mask) => crate::spec::bitmap::storage_valid(mask),
        None => true,
    },
    ensures match out {
        Ok(bitmap) => crate::spec::bitmap::storage_valid(&bitmap),
        Err(_) => true,
    },
))]
pub(crate) fn decode_arithmetic_generic(
    coded: &[u8],
    header: &GenericRegionHeader,
    rows: u32,
    skip: Option<&Bitmap>,
    limits: &Limits,
) -> Result<Bitmap, GenericRegionError> {
    if header.mmr || rows > header.region.height {
        return Err(GenericRegionError::InvalidFlags(0x01));
    }
    validate_arithmetic_terminator(coded)?;
    validate_generic_adaptive_values(header.template, header.extended_template, &header.adaptive)?;
    enforce_generic_work(header.region.width, rows, header.typical_prediction, limits)?;
    if let Some(skip_bitmap) = skip {
        if skip_bitmap.width() != header.region.width
            || skip_bitmap.height() != header.region.height
        {
            return Err(GenericRegionError::InvalidSkipDimensions);
        }
    }

    let mut session =
        GenericArithmeticSession::new(coded, header.template, header.extended_template, limits)?;
    let bitmap = session.decode_bitmap_rows(
        GenericArithmeticCall {
            width: header.region.width,
            height: header.region.height,
            adaptive: header.adaptive,
            typical_prediction: header.typical_prediction,
            skip,
        },
        rows,
    )?;
    session.validate_final_marker()?;
    Ok(bitmap)
}

/// Decode one generic bitmap on an existing arithmetic stream and statistics
/// set.
///
/// This kernel neither initializes nor resets `decoder` or `contexts`, and it
/// does not require or validate an end marker. `cumulative_work` is charged
/// before the first arithmetic decision and is preserved across interleaved
/// decoding procedures.
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        crate::arithmetic::arithmetic_spec::decoder_valid(old(decoder)),
        match call.skip {
            Some(mask) => crate::spec::bitmap::storage_valid(mask),
            None => true,
        },
    ensures match out {
        Ok(bitmap) =>
            crate::spec::bitmap::storage_valid(&bitmap)
            && crate::arithmetic::arithmetic_spec::decoder_valid(final(decoder))
            && generic_region_spec::generic_contexts_valid(final(contexts)),
        Err(_) => true,
    },
))]
pub(crate) fn decode_generic_on(
    decoder: &mut ArithmeticDecoder<'_>,
    contexts: &mut GenericContexts,
    call: GenericArithmeticCall<'_>,
    rows: u32,
    limits: &Limits,
    cumulative_work: &mut u64,
) -> Result<Bitmap, GenericRegionError> {
    if rows > call.height {
        return Err(MalformedError::GenericRegionRowCount {
            declared: call.height,
            actual: rows,
        }
        .into());
    }
    validate_generic_context_layout(
        contexts.template,
        contexts.extended_template,
        &contexts.contexts,
    )?;
    validate_probability_states(&contexts.contexts)?;
    validate_generic_adaptive_values(
        contexts.template,
        contexts.extended_template,
        &call.adaptive,
    )?;
    if let Some(skip) = call.skip {
        if skip.width() != call.width || skip.height() != call.height {
            return Err(GenericRegionError::InvalidSkipDimensions);
        }
    }

    let call_work = generic_work(call.width, rows, call.typical_prediction)?;
    let next_work = cumulative_work
        .checked_add(call_work)
        .ok_or(MalformedError::IntegerOverflow)?;
    enforce_work(next_work, limits)?;
    let bitmap_bytes = bitmap_storage_bytes(call.width, call.height)?;
    enforce_managed_memory(contexts.storage_bytes, bitmap_bytes, limits)?;
    let mut bitmap = Bitmap::blank(call.width, call.height, limits)?;
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::views::bitmap_fields(&bitmap);
        reveal(generic_region_spec::generic_contexts_valid);
    }
    *cumulative_work = next_work;

    decode_generic_with(
        &mut bitmap,
        rows,
        contexts.template,
        contexts.extended_template,
        &call.adaptive,
        call.typical_prediction,
        call.skip,
        decoder,
        &mut contexts.contexts,
    )?;
    #[cfg(verus_keep_ghost)]
    proof! {
        reveal(generic_region_spec::generic_contexts_valid);
    }
    Ok(bitmap)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        crate::arithmetic::arithmetic_spec::decoder_valid(old(decoder)),
        generic_region_spec::contexts_valid(old(contexts)@),
    ensures
        generic_region_spec::contexts_valid(final(contexts)@),
        match out {
            Ok(bit) =>
                bit <= 1
                && crate::arithmetic::arithmetic_spec::decoder_valid(final(decoder)),
            Err(_) => true,
        },
))]
#[allow(clippy::ptr_arg)]
fn decode_probability(
    decoder: &mut ArithmeticDecoder<'_>,
    contexts: &mut Vec<ArithmeticContext>,
    index: usize,
) -> Result<u8, GenericRegionError> {
    if index >= contexts.len() {
        return Err(GenericRegionError::ContextIndex {
            index,
            count: contexts.len(),
        });
    }
    let result = decoder.decode(&mut contexts[index]);
    match result {
        Ok(bit) => Ok(bit),
        Err(error) => Err(GenericRegionError::Decode(error.into_decode())),
    }
}

#[allow(clippy::ptr_arg)]
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        crate::spec::bitmap::storage_valid(old(bitmap)),
        rows <= old(bitmap).height,
        crate::arithmetic::arithmetic_spec::decoder_valid(old(decoder)),
        generic_region_spec::contexts_valid(old(contexts)@),
        match skip {
            Some(mask) => crate::spec::bitmap::storage_valid(mask),
            None => true,
        },
    ensures
        crate::spec::bitmap::storage_valid(final(bitmap)),
        generic_region_spec::contexts_valid(final(contexts)@),
        out.is_ok() ==> crate::arithmetic::arithmetic_spec::decoder_valid(final(decoder)),
))]
#[allow(clippy::too_many_arguments)]
fn decode_generic_with(
    bitmap: &mut Bitmap,
    rows: u32,
    template: GenericTemplate,
    extended: bool,
    adaptive: &[TemplateOffset; 12],
    typical_prediction: bool,
    skip: Option<&Bitmap>,
    decoder: &mut ArithmeticDecoder<'_>,
    contexts: &mut Vec<ArithmeticContext>,
) -> Result<(), GenericRegionError> {
    let mut ltp = false;
    let bitmap_width = bitmap.width;
    let bitmap_height = bitmap.height;
    if rows > bitmap_height {
        return Err(GenericRegionError::RowCountMismatch {
            expected: bitmap_height,
            actual: rows,
        });
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::views::bitmap_fields(bitmap);
    }
    let mut y = 0_u32;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            y <= rows,
            rows <= bitmap_height,
            crate::views::bitmap_width(bitmap) == bitmap_width,
            crate::views::bitmap_height(bitmap) == bitmap_height,
            crate::spec::bitmap::storage_valid(bitmap),
            crate::arithmetic::arithmetic_spec::decoder_valid(decoder),
            generic_region_spec::contexts_valid(contexts@),
            match skip {
                Some(mask) => crate::spec::bitmap::storage_valid(mask),
                None => true,
            },
        decreases rows - y,
    ))]
    while y < rows {
        if typical_prediction {
            let sltp = decode_probability(decoder, contexts, template.sltp_context())?;
            ltp ^= sltp != 0;
        }

        if ltp {
            if y != 0 {
                #[cfg(verus_keep_ghost)]
                proof! {
                    crate::views::bitmap_fields(bitmap);
                    assert(bitmap.height == bitmap_height);
                    assert(y - 1 < bitmap_height);
                    assert(y < bitmap_height);
                }
                copy_bitmap_row(bitmap, y - 1, y)?;
                #[cfg(verus_keep_ghost)]
                proof! {
                    crate::views::bitmap_fields(bitmap);
                    assert(bitmap.width == bitmap_width);
                    assert(bitmap.height == bitmap_height);
                }
            }
            y += 1;
            continue;
        }

        let mut x = 0_u32;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                x <= bitmap_width,
                y < rows,
                rows <= bitmap_height,
                crate::views::bitmap_width(bitmap) == bitmap_width,
                crate::views::bitmap_height(bitmap) == bitmap_height,
                crate::spec::bitmap::storage_valid(bitmap),
                crate::arithmetic::arithmetic_spec::decoder_valid(decoder),
                generic_region_spec::contexts_valid(contexts@),
                match skip {
                    Some(mask) => crate::spec::bitmap::storage_valid(mask),
                    None => true,
                },
            decreases bitmap_width - x,
        ))]
        while x < bitmap_width {
            let skipped = match skip {
                Some(mask) => mask.pixel(x, y).unwrap_or(false),
                None => false,
            };
            if skipped {
                bitmap.set_pixel(x, y, false)?;
                #[cfg(verus_keep_ghost)]
                proof! {
                    crate::views::bitmap_fields(bitmap);
                    assert(x < bitmap_width);
                }
                x += 1;
                continue;
            }
            let context = generic_context(bitmap, x, y, template, extended, adaptive);
            let value = decode_probability(decoder, contexts, context)? != 0;
            bitmap.set_pixel(x, y, value)?;
            #[cfg(verus_keep_ghost)]
            proof! {
                crate::views::bitmap_fields(bitmap);
                assert(x < bitmap_width);
            }
            x += 1;
        }
        y += 1;
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn decode_generic_with_callback<F>(
    bitmap: &mut Bitmap,
    rows: u32,
    template: GenericTemplate,
    extended: bool,
    adaptive: &[TemplateOffset; 12],
    typical_prediction: bool,
    skip: Option<&Bitmap>,
    mut decode: F,
) -> Result<(), GenericRegionError>
where
    F: FnMut(usize) -> Result<u8, GenericRegionError>,
{
    let mut ltp = false;
    for y in 0..rows {
        if typical_prediction {
            let sltp = decode(template.sltp_context())?;
            ltp ^= sltp != 0;
        }
        if ltp {
            if y != 0 {
                copy_bitmap_row(bitmap, y - 1, y)?;
            }
            continue;
        }
        for x in 0..bitmap.width() {
            let skipped = skip.and_then(|mask| mask.pixel(x, y)).unwrap_or(false);
            if skipped {
                bitmap.set_pixel(x, y, false)?;
                continue;
            }
            let context = generic_context(bitmap, x, y, template, extended, adaptive);
            let value = decode(context)? != 0;
            bitmap.set_pixel(x, y, value)?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
struct GenericSlot {
    nominal: TemplateOffset,
    adaptive: Option<u8>,
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures
        out.nominal.x == x,
        out.nominal.y == y,
        out.adaptive.is_none(),
))]
const fn generic_slot(x: i32, y: i32) -> GenericSlot {
    GenericSlot {
        nominal: TemplateOffset::new(x, y),
        adaptive: None,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures
        out.nominal.x == x,
        out.nominal.y == y,
        out.adaptive == Some(number),
))]
const fn generic_at(x: i32, y: i32, number: u8) -> GenericSlot {
    GenericSlot {
        nominal: TemplateOffset::new(x, y),
        adaptive: Some(number),
    }
}

macro_rules! generic_slot_const {
    ($x:expr, $y:expr) => {
        GenericSlot {
            nominal: TemplateOffset { x: $x, y: $y },
            adaptive: None,
        }
    };
}

macro_rules! generic_at_const {
    ($x:expr, $y:expr, $number:expr) => {
        GenericSlot {
            nominal: TemplateOffset { x: $x, y: $y },
            adaptive: Some($number),
        }
    };
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
const GENERIC_TEMPLATE_0: [GenericSlot; 16] = [
    generic_at_const!(-2, -2, 4),
    generic_slot_const!(-1, -2),
    generic_slot_const!(0, -2),
    generic_slot_const!(1, -2),
    generic_at_const!(2, -2, 3),
    generic_at_const!(-3, -1, 2),
    generic_slot_const!(-2, -1),
    generic_slot_const!(-1, -1),
    generic_slot_const!(0, -1),
    generic_slot_const!(1, -1),
    generic_slot_const!(2, -1),
    generic_at_const!(3, -1, 1),
    generic_slot_const!(-4, 0),
    generic_slot_const!(-3, 0),
    generic_slot_const!(-2, 0),
    generic_slot_const!(-1, 0),
];

#[cfg_attr(verus_keep_ghost, verus_verify)]
const GENERIC_TEMPLATE_0_EXTENDED: [GenericSlot; 16] = [
    generic_at_const!(-2, -2, 11),
    generic_at_const!(-1, -2, 4),
    generic_at_const!(0, -2, 2),
    generic_at_const!(1, -2, 5),
    generic_at_const!(2, -2, 9),
    generic_at_const!(-3, -1, 12),
    generic_at_const!(-2, -1, 3),
    generic_slot_const!(-1, -1),
    generic_slot_const!(0, -1),
    generic_slot_const!(1, -1),
    generic_at_const!(2, -1, 6),
    generic_at_const!(3, -1, 10),
    generic_at_const!(-4, 0, 8),
    generic_at_const!(-3, 0, 7),
    generic_at_const!(-2, 0, 1),
    generic_slot_const!(-1, 0),
];

#[cfg_attr(verus_keep_ghost, verus_verify)]
const GENERIC_TEMPLATE_1: [GenericSlot; 13] = [
    generic_slot_const!(-1, -2),
    generic_slot_const!(0, -2),
    generic_slot_const!(1, -2),
    generic_slot_const!(2, -2),
    generic_slot_const!(-2, -1),
    generic_slot_const!(-1, -1),
    generic_slot_const!(0, -1),
    generic_slot_const!(1, -1),
    generic_slot_const!(2, -1),
    generic_at_const!(3, -1, 1),
    generic_slot_const!(-3, 0),
    generic_slot_const!(-2, 0),
    generic_slot_const!(-1, 0),
];

#[cfg_attr(verus_keep_ghost, verus_verify)]
const GENERIC_TEMPLATE_2: [GenericSlot; 10] = [
    generic_slot_const!(-1, -2),
    generic_slot_const!(0, -2),
    generic_slot_const!(1, -2),
    generic_slot_const!(-2, -1),
    generic_slot_const!(-1, -1),
    generic_slot_const!(0, -1),
    generic_slot_const!(1, -1),
    generic_at_const!(2, -1, 1),
    generic_slot_const!(-2, 0),
    generic_slot_const!(-1, 0),
];

#[cfg_attr(verus_keep_ghost, verus_verify)]
const GENERIC_TEMPLATE_3: [GenericSlot; 10] = [
    generic_slot_const!(-3, -1),
    generic_slot_const!(-2, -1),
    generic_slot_const!(-1, -1),
    generic_slot_const!(0, -1),
    generic_slot_const!(1, -1),
    generic_at_const!(2, -1, 1),
    generic_slot_const!(-4, 0),
    generic_slot_const!(-3, 0),
    generic_slot_const!(-2, 0),
    generic_slot_const!(-1, 0),
];

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn generic_adaptive_offset(
    adaptive: &[TemplateOffset; 12],
    number: u8,
    nominal: TemplateOffset,
) -> TemplateOffset {
    match number {
        1 => adaptive[0],
        2 => adaptive[1],
        3 => adaptive[2],
        4 => adaptive[3],
        5 => adaptive[4],
        6 => adaptive[5],
        7 => adaptive[6],
        8 => adaptive[7],
        9 => adaptive[8],
        10 => adaptive[9],
        11 => adaptive[10],
        12 => adaptive[11],
        _ => nominal,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(
    requires crate::spec::bitmap::storage_valid(bitmap),
))]
#[allow(clippy::bool_to_int_with_if)]
fn generic_context(
    bitmap: &Bitmap,
    x: u32,
    y: u32,
    template: GenericTemplate,
    extended: bool,
    adaptive: &[TemplateOffset; 12],
) -> usize {
    let count = match template {
        GenericTemplate::Template0 => 16usize,
        GenericTemplate::Template1 => 13usize,
        GenericTemplate::Template2 | GenericTemplate::Template3 => 10usize,
    };
    let mut context = 0_usize;
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index <= count,
            count <= 16,
            count == match template {
                GenericTemplate::Template0 => 16usize,
                GenericTemplate::Template1 => 13usize,
                GenericTemplate::Template2 | GenericTemplate::Template3 => 10usize,
            },
            crate::spec::bitmap::storage_valid(bitmap),
        decreases count - index,
    ))]
    while index < count {
        let slot = match (template, extended) {
            (GenericTemplate::Template0, true) => GENERIC_TEMPLATE_0_EXTENDED[index],
            (GenericTemplate::Template0, false) => GENERIC_TEMPLATE_0[index],
            (GenericTemplate::Template1, _) => GENERIC_TEMPLATE_1[index],
            (GenericTemplate::Template2, _) => GENERIC_TEMPLATE_2[index],
            (GenericTemplate::Template3, _) => GENERIC_TEMPLATE_3[index],
        };
        let offset = match slot.adaptive {
            Some(number) => generic_adaptive_offset(adaptive, number, slot.nominal),
            None => slot.nominal,
        };
        let bit = if sample_bitmap(bitmap, x, y, offset) {
            1
        } else {
            0
        };
        context = (context << 1) | bit;
        index += 1;
    }
    context
}

/// One of the two generic-refinement templates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) enum RefinementTemplate {
    Template0,
    Template1,
}

impl RefinementTemplate {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == if flags & 0x01 == 0 {
            RefinementTemplate::Template0
        } else {
            RefinementTemplate::Template1
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    fn from_flags(flags: u8) -> Self {
        if flags & 0x01 == 0 {
            Self::Template0
        } else {
            Self::Template1
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures
            out as nat == generic_region_spec::refinement_context_bits(self),
            10 <= out <= 13,
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    const fn context_bits(self) -> u8 {
        #[cfg(verus_keep_ghost)]
        production_refinement_template_facts!(self);
        match self {
            Self::Template0 => 13,
            Self::Template1 => 10,
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures
            out as nat == generic_region_spec::refinement_sltp_context(self),
            (out as nat) < generic_region_spec::pow2(
                generic_region_spec::refinement_context_bits(self),
            ),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    const fn sltp_context(self) -> usize {
        #[cfg(verus_keep_ghost)]
        production_refinement_template_facts!(self);
        match self {
            Self::Template0 => 0x0010,
            Self::Template1 => 0x0008,
        }
    }
}

/// Reusable `GR` probability statistics for one refinement template.
///
/// Retention is explicitly fallible through [`Self::try_snapshot`]; this type
/// intentionally does not implement [`Clone`].
#[derive(Debug)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct RefinementContexts {
    pub(crate) template: RefinementTemplate,
    pub(crate) contexts: Vec<ArithmeticContext>,
    pub(crate) storage_bytes: u64,
}

/// A fallibly allocated retained copy of refinement arithmetic statistics.
#[derive(Debug)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct RefinementContextSnapshot {
    template: RefinementTemplate,
    contexts: Vec<ArithmeticContext>,
    storage_bytes: u64,
}

impl RefinementContexts {
    /// Allocate a default statistics set compatible with `template`.
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures match out {
            Ok(contexts) =>
                contexts.template == template
                && generic_region_spec::refinement_contexts_valid(&contexts),
            Err(_) => true,
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn new(
        template: RefinementTemplate,
        limits: &Limits,
    ) -> Result<Self, GenericRegionError> {
        let count = refinement_context_count(template);
        let storage_bytes = context_storage_bytes(count)?;
        let contexts = allocate_contexts(count, 0, limits)?;
        Ok(Self {
            template,
            contexts,
            storage_bytes,
        })
    }

    /// Reset all contexts to `I = 0, MPS = 0`.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub(crate) fn reset(&mut self) -> Result<(), GenericRegionError> {
        validate_refinement_context_layout(self.template, &self.contexts)?;
        reset_contexts(&mut self.contexts);
        Ok(())
    }

    /// Fallibly retain the current statistics for later dictionary use.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub(crate) fn try_snapshot(
        &self,
        limits: &Limits,
    ) -> Result<RefinementContextSnapshot, GenericRegionError> {
        validate_refinement_context_layout(self.template, &self.contexts)?;
        let contexts = try_copy_contexts(&self.contexts, self.storage_bytes, limits)?;
        Ok(RefinementContextSnapshot {
            template: self.template,
            contexts,
            storage_bytes: self.storage_bytes,
        })
    }

    /// Restore retained statistics, rejecting a snapshot for another template.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub(crate) fn restore(
        &mut self,
        snapshot: &RefinementContextSnapshot,
        limits: &Limits,
    ) -> Result<(), GenericRegionError> {
        if self.template != snapshot.template {
            return Err(GenericRegionError::InvalidFlags(refinement_template_code(
                snapshot.template,
            )));
        }
        validate_refinement_context_layout(snapshot.template, &snapshot.contexts)?;
        validate_snapshot_limits(snapshot.storage_bytes, limits)?;
        if self.contexts.len() != snapshot.contexts.len() {
            return Err(GenericRegionError::ContextIndex {
                index: snapshot.contexts.len(),
                count: self.contexts.len(),
            });
        }
        validate_probability_states(&snapshot.contexts)?;
        let mut index = 0_usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                self.contexts@.len() == snapshot.contexts@.len(),
                index <= self.contexts@.len(),
            decreases self.contexts@.len() - index,
        ))]
        while index < self.contexts.len() {
            self.contexts[index] = snapshot.contexts[index];
            index += 1;
        }
        Ok(())
    }
}

/// Per-bitmap parameters for decoding on retained refinement statistics.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct RefinementArithmeticCall {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) adaptive: [TemplateOffset; 2],
    pub(crate) typical_prediction: bool,
}

/// Parsed generic-refinement region header.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct RefinementRegionHeader {
    pub(crate) region: RegionInfo,
    pub(crate) template: RefinementTemplate,
    pub(crate) typical_prediction: bool,
    pub(crate) adaptive: [TemplateOffset; 2],
}

/// Runtime parameters not carried by a standalone refinement data header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct RefinementParameters {
    pub(crate) reference_dx: i32,
    pub(crate) reference_dy: i32,
}

/// Parse a generic-refinement region data header and return its byte length.
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) fn parse_refinement_region_header(
    payload: &[u8],
    limits: &Limits,
) -> Result<(RefinementRegionHeader, usize), GenericRegionError> {
    enforce_input_limit(payload, limits)?;
    let mut reader = ByteReader::new(payload);
    let region = parse_region_info(&mut reader, limits)?;
    let flags = reader.read_u8()?;
    if flags & 0xfc != 0 {
        return Err(GenericRegionError::ReservedFlags(flags & 0xfc));
    }
    let template = RefinementTemplate::from_flags(flags);
    let typical_prediction = flags & 0x02 != 0;
    let mut adaptive = [TemplateOffset { x: 0, y: 0 }; 2];
    if template == RefinementTemplate::Template0 {
        adaptive[0] = read_offset(&mut reader)?;
        adaptive[1] = read_offset(&mut reader)?;
        validate_target_adaptive(1, adaptive[0])?;
    }
    Ok((
        RefinementRegionHeader {
            region,
            template,
            typical_prediction,
            adaptive,
        },
        reader.position(),
    ))
}

/// Decode one bounded arithmetic generic-refinement bitmap.
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires crate::spec::bitmap::storage_valid(reference),
    ensures match out {
        Ok(bitmap) => crate::spec::bitmap::storage_valid(&bitmap),
        Err(_) => true,
    },
))]
pub(crate) fn decode_arithmetic_refinement(
    coded: &[u8],
    header: &RefinementRegionHeader,
    parameters: RefinementParameters,
    reference: &Bitmap,
    limits: &Limits,
) -> Result<Bitmap, GenericRegionError> {
    validate_arithmetic_terminator(coded)?;
    if header.template == RefinementTemplate::Template0 {
        validate_target_adaptive(1, header.adaptive[0])?;
    }
    enforce_refinement_work(
        header.region.width,
        header.region.height,
        header.typical_prediction,
        limits,
    )?;

    let mut contexts = RefinementContexts::new(header.template, limits)?;
    let mut decoder = match ArithmeticDecoder::new(coded) {
        Ok(decoder) => decoder,
        Err(error) => return Err(GenericRegionError::Decode(error.into_decode())),
    };
    let mut cumulative_work = 0;
    let bitmap = decode_refinement_on(
        &mut decoder,
        &mut contexts,
        RefinementArithmeticCall {
            width: header.region.width,
            height: header.region.height,
            adaptive: header.adaptive,
            typical_prediction: header.typical_prediction,
        },
        reference,
        parameters,
        limits,
        &mut cumulative_work,
    )?;
    validate_observed_marker(&decoder, coded.len())?;
    Ok(bitmap)
}

/// Decode one refinement bitmap on an existing arithmetic stream and
/// statistics set.
///
/// This kernel does not create or reset the decoder, does not reset `GR`
/// statistics, and does not validate an end marker. It is therefore safe to
/// interleave with arithmetic integers and generic-region decisions.
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        crate::arithmetic::arithmetic_spec::decoder_valid(old(decoder)),
        crate::spec::bitmap::storage_valid(reference),
    ensures match out {
        Ok(bitmap) =>
            crate::spec::bitmap::storage_valid(&bitmap)
            && crate::arithmetic::arithmetic_spec::decoder_valid(final(decoder))
            && generic_region_spec::refinement_contexts_valid(final(contexts)),
        Err(_) => true,
    },
))]
#[allow(clippy::too_many_arguments)]
pub(crate) fn decode_refinement_on(
    decoder: &mut ArithmeticDecoder<'_>,
    contexts: &mut RefinementContexts,
    call: RefinementArithmeticCall,
    reference: &Bitmap,
    parameters: RefinementParameters,
    limits: &Limits,
    cumulative_work: &mut u64,
) -> Result<Bitmap, GenericRegionError> {
    validate_refinement_context_layout(contexts.template, &contexts.contexts)?;
    validate_probability_states(&contexts.contexts)?;
    if contexts.template == RefinementTemplate::Template0 {
        validate_target_adaptive(1, call.adaptive[0])?;
    }

    let call_work = refinement_work(call.width, call.height, call.typical_prediction)?;
    let next_work = cumulative_work
        .checked_add(call_work)
        .ok_or(MalformedError::IntegerOverflow)?;
    enforce_work(next_work, limits)?;
    let bitmap_bytes = bitmap_storage_bytes(call.width, call.height)?;
    enforce_managed_memory(contexts.storage_bytes, bitmap_bytes, limits)?;
    let mut bitmap = Bitmap::blank(call.width, call.height, limits)?;
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::views::bitmap_fields(&bitmap);
        reveal(generic_region_spec::refinement_contexts_valid);
    }
    *cumulative_work = next_work;

    decode_refinement_with(
        &mut bitmap,
        reference,
        contexts.template,
        call.adaptive,
        call.typical_prediction,
        parameters,
        decoder,
        &mut contexts.contexts,
    )?;
    #[cfg(verus_keep_ghost)]
    proof! {
        reveal(generic_region_spec::refinement_contexts_valid);
    }
    Ok(bitmap)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        crate::spec::bitmap::storage_valid(old(bitmap)),
        crate::spec::bitmap::storage_valid(reference),
        crate::arithmetic::arithmetic_spec::decoder_valid(old(decoder)),
        generic_region_spec::contexts_valid(old(contexts)@),
    ensures
        crate::spec::bitmap::storage_valid(final(bitmap)),
        generic_region_spec::contexts_valid(final(contexts)@),
        out.is_ok() ==> crate::arithmetic::arithmetic_spec::decoder_valid(final(decoder)),
))]
#[allow(clippy::ptr_arg, clippy::too_many_arguments)]
fn decode_refinement_with(
    bitmap: &mut Bitmap,
    reference: &Bitmap,
    template: RefinementTemplate,
    adaptive: [TemplateOffset; 2],
    typical_prediction: bool,
    parameters: RefinementParameters,
    decoder: &mut ArithmeticDecoder<'_>,
    contexts: &mut Vec<ArithmeticContext>,
) -> Result<(), GenericRegionError> {
    let mut ltp = false;
    let bitmap_width = bitmap.width;
    let bitmap_height = bitmap.height;
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::views::bitmap_fields(bitmap);
    }
    let mut y = 0_u32;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            y <= bitmap_height,
            crate::views::bitmap_width(bitmap) == bitmap_width,
            crate::views::bitmap_height(bitmap) == bitmap_height,
            crate::spec::bitmap::storage_valid(bitmap),
            crate::spec::bitmap::storage_valid(reference),
            crate::arithmetic::arithmetic_spec::decoder_valid(decoder),
            generic_region_spec::contexts_valid(contexts@),
        decreases bitmap_height - y,
    ))]
    while y < bitmap_height {
        if typical_prediction {
            let sltp = decode_probability(decoder, contexts, template.sltp_context())?;
            ltp ^= sltp != 0;
        }
        let mut x = 0_u32;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                x <= bitmap_width,
                y < bitmap_height,
                crate::views::bitmap_width(bitmap) == bitmap_width,
                crate::views::bitmap_height(bitmap) == bitmap_height,
                crate::spec::bitmap::storage_valid(bitmap),
                crate::spec::bitmap::storage_valid(reference),
                crate::arithmetic::arithmetic_spec::decoder_valid(decoder),
                generic_region_spec::contexts_valid(contexts@),
            decreases bitmap_width - x,
        ))]
        while x < bitmap_width {
            if ltp {
                if let Some(prediction) = refinement_prediction(reference, x, y, parameters) {
                    bitmap.set_pixel(x, y, prediction)?;
                    #[cfg(verus_keep_ghost)]
                    proof! {
                        crate::views::bitmap_fields(bitmap);
                    }
                    x += 1;
                    continue;
                }
            }
            let context =
                refinement_context(bitmap, reference, x, y, template, adaptive, parameters);
            let value = decode_probability(decoder, contexts, context)? != 0;
            bitmap.set_pixel(x, y, value)?;
            #[cfg(verus_keep_ghost)]
            proof! {
                crate::views::bitmap_fields(bitmap);
            }
            x += 1;
        }
        y += 1;
    }
    Ok(())
}

#[cfg(test)]
fn decode_refinement_with_callback<F>(
    bitmap: &mut Bitmap,
    reference: &Bitmap,
    template: RefinementTemplate,
    adaptive: [TemplateOffset; 2],
    typical_prediction: bool,
    parameters: RefinementParameters,
    mut decode: F,
) -> Result<(), GenericRegionError>
where
    F: FnMut(usize) -> Result<u8, GenericRegionError>,
{
    let mut ltp = false;
    for y in 0..bitmap.height() {
        if typical_prediction {
            let sltp = decode(template.sltp_context())?;
            ltp ^= sltp != 0;
        }
        for x in 0..bitmap.width() {
            if ltp {
                if let Some(prediction) = refinement_prediction(reference, x, y, parameters) {
                    bitmap.set_pixel(x, y, prediction)?;
                    continue;
                }
            }
            let context =
                refinement_context(bitmap, reference, x, y, template, adaptive, parameters);
            let value = decode(context)? != 0;
            bitmap.set_pixel(x, y, value)?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
enum RefinementPlane {
    Target,
    Reference,
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
struct RefinementSlot {
    plane: RefinementPlane,
    nominal: TemplateOffset,
    adaptive: Option<u8>,
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures
        out.plane == RefinementPlane::Target,
        out.nominal.x == x,
        out.nominal.y == y,
        out.adaptive.is_none(),
))]
const fn target_slot(x: i32, y: i32) -> RefinementSlot {
    RefinementSlot {
        plane: RefinementPlane::Target,
        nominal: TemplateOffset::new(x, y),
        adaptive: None,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures
        out.plane == RefinementPlane::Target,
        out.nominal.x == x,
        out.nominal.y == y,
        out.adaptive == Some(number),
))]
const fn target_at(x: i32, y: i32, number: u8) -> RefinementSlot {
    RefinementSlot {
        plane: RefinementPlane::Target,
        nominal: TemplateOffset::new(x, y),
        adaptive: Some(number),
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures
        out.plane == RefinementPlane::Reference,
        out.nominal.x == x,
        out.nominal.y == y,
        out.adaptive.is_none(),
))]
const fn reference_slot(x: i32, y: i32) -> RefinementSlot {
    RefinementSlot {
        plane: RefinementPlane::Reference,
        nominal: TemplateOffset::new(x, y),
        adaptive: None,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures
        out.plane == RefinementPlane::Reference,
        out.nominal.x == x,
        out.nominal.y == y,
        out.adaptive == Some(number),
))]
const fn reference_at(x: i32, y: i32, number: u8) -> RefinementSlot {
    RefinementSlot {
        plane: RefinementPlane::Reference,
        nominal: TemplateOffset::new(x, y),
        adaptive: Some(number),
    }
}

macro_rules! target_slot_const {
    ($x:expr, $y:expr) => {
        RefinementSlot {
            plane: RefinementPlane::Target,
            nominal: TemplateOffset { x: $x, y: $y },
            adaptive: None,
        }
    };
}

macro_rules! target_at_const {
    ($x:expr, $y:expr, $number:expr) => {
        RefinementSlot {
            plane: RefinementPlane::Target,
            nominal: TemplateOffset { x: $x, y: $y },
            adaptive: Some($number),
        }
    };
}

macro_rules! reference_slot_const {
    ($x:expr, $y:expr) => {
        RefinementSlot {
            plane: RefinementPlane::Reference,
            nominal: TemplateOffset { x: $x, y: $y },
            adaptive: None,
        }
    };
}

macro_rules! reference_at_const {
    ($x:expr, $y:expr, $number:expr) => {
        RefinementSlot {
            plane: RefinementPlane::Reference,
            nominal: TemplateOffset { x: $x, y: $y },
            adaptive: Some($number),
        }
    };
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
const REFINEMENT_TEMPLATE_0: [RefinementSlot; 13] = [
    target_at_const!(-1, -1, 1),
    target_slot_const!(0, -1),
    target_slot_const!(1, -1),
    target_slot_const!(-1, 0),
    reference_at_const!(-1, -1, 2),
    reference_slot_const!(0, -1),
    reference_slot_const!(1, -1),
    reference_slot_const!(-1, 0),
    reference_slot_const!(0, 0),
    reference_slot_const!(1, 0),
    reference_slot_const!(-1, 1),
    reference_slot_const!(0, 1),
    reference_slot_const!(1, 1),
];

#[cfg_attr(verus_keep_ghost, verus_verify)]
const REFINEMENT_TEMPLATE_1: [RefinementSlot; 10] = [
    target_slot_const!(-1, -1),
    target_slot_const!(0, -1),
    target_slot_const!(1, -1),
    target_slot_const!(-1, 0),
    reference_slot_const!(0, -1),
    reference_slot_const!(-1, 0),
    reference_slot_const!(0, 0),
    reference_slot_const!(1, 0),
    reference_slot_const!(0, 1),
    reference_slot_const!(1, 1),
];

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn refinement_adaptive_offset(
    adaptive: [TemplateOffset; 2],
    number: u8,
    nominal: TemplateOffset,
) -> TemplateOffset {
    match number {
        1 => adaptive[0],
        2 => adaptive[1],
        _ => nominal,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(
    requires
        crate::spec::bitmap::storage_valid(target),
        crate::spec::bitmap::storage_valid(reference),
))]
#[allow(clippy::bool_to_int_with_if)]
fn refinement_context(
    target: &Bitmap,
    reference: &Bitmap,
    x: u32,
    y: u32,
    template: RefinementTemplate,
    adaptive: [TemplateOffset; 2],
    parameters: RefinementParameters,
) -> usize {
    let count = match template {
        RefinementTemplate::Template0 => 13usize,
        RefinementTemplate::Template1 => 10usize,
    };
    let mut context = 0_usize;
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index <= count,
            count <= 13,
            count == match template {
                RefinementTemplate::Template0 => 13usize,
                RefinementTemplate::Template1 => 10usize,
            },
            crate::spec::bitmap::storage_valid(target),
            crate::spec::bitmap::storage_valid(reference),
        decreases count - index,
    ))]
    while index < count {
        let slot = match template {
            RefinementTemplate::Template0 => REFINEMENT_TEMPLATE_0[index],
            RefinementTemplate::Template1 => REFINEMENT_TEMPLATE_1[index],
        };
        let offset = match slot.adaptive {
            Some(number) => refinement_adaptive_offset(adaptive, number, slot.nominal),
            None => slot.nominal,
        };
        let value = match slot.plane {
            RefinementPlane::Target => sample_bitmap(target, x, y, offset),
            RefinementPlane::Reference => sample_reference(reference, x, y, offset, parameters),
        };
        context = (context << 1) | if value { 1 } else { 0 };
        index += 1;
    }
    context
}

#[cfg_attr(verus_keep_ghost, verus_spec(
    requires crate::spec::bitmap::storage_valid(reference),
))]
fn refinement_prediction(
    reference: &Bitmap,
    x: u32,
    y: u32,
    parameters: RefinementParameters,
) -> Option<bool> {
    let common = sample_reference(reference, x, y, TemplateOffset::new(-1, -1), parameters);
    if sample_reference(reference, x, y, TemplateOffset::new(0, -1), parameters) != common
        || sample_reference(reference, x, y, TemplateOffset::new(1, -1), parameters) != common
        || sample_reference(reference, x, y, TemplateOffset::new(-1, 0), parameters) != common
        || sample_reference(reference, x, y, TemplateOffset::new(0, 0), parameters) != common
        || sample_reference(reference, x, y, TemplateOffset::new(1, 0), parameters) != common
        || sample_reference(reference, x, y, TemplateOffset::new(-1, 1), parameters) != common
        || sample_reference(reference, x, y, TemplateOffset::new(0, 1), parameters) != common
        || sample_reference(reference, x, y, TemplateOffset::new(1, 1), parameters) != common
    {
        return None;
    }
    Some(common)
}

#[cfg_attr(verus_keep_ghost, verus_spec(
    requires crate::spec::bitmap::storage_valid(bitmap),
))]
#[allow(clippy::cast_lossless)]
fn sample_reference(
    bitmap: &Bitmap,
    x: u32,
    y: u32,
    offset: TemplateOffset,
    parameters: RefinementParameters,
) -> bool {
    let reference_x = (x as i64)
        .checked_sub(parameters.reference_dx as i64)
        .and_then(|value| value.checked_add(offset.x as i64));
    let reference_y = (y as i64)
        .checked_sub(parameters.reference_dy as i64)
        .and_then(|value| value.checked_add(offset.y as i64));
    sample_signed(bitmap, reference_x, reference_y)
}

#[cfg_attr(verus_keep_ghost, verus_spec(
    requires crate::spec::bitmap::storage_valid(bitmap),
))]
#[allow(clippy::cast_lossless)]
fn sample_bitmap(bitmap: &Bitmap, x: u32, y: u32, offset: TemplateOffset) -> bool {
    let sample_x = (x as i64).checked_add(offset.x as i64);
    let sample_y = (y as i64).checked_add(offset.y as i64);
    sample_signed(bitmap, sample_x, sample_y)
}

#[cfg_attr(verus_keep_ghost, verus_spec(
    requires crate::spec::bitmap::storage_valid(bitmap),
))]
#[allow(
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn sample_signed(bitmap: &Bitmap, x: Option<i64>, y: Option<i64>) -> bool {
    let (Some(x), Some(y)) = (x, y) else {
        return false;
    };
    if x < 0 || x > u32::MAX as i64 || y < 0 || y > u32::MAX as i64 {
        return false;
    }
    bitmap.pixel(x as u32, y as u32).unwrap_or(false)
}

#[cfg_attr(verus_keep_ghost, verus_spec(
    requires old(reader).position <= old(reader).bytes@.len(),
    ensures
        final(reader).bytes@ == old(reader).bytes@,
        final(reader).position <= final(reader).bytes@.len(),
))]
fn read_offset(reader: &mut ByteReader<'_>) -> Result<TemplateOffset, GenericRegionError> {
    let x = signed_offset_byte(reader.read_u8()?);
    let y = signed_offset_byte(reader.read_u8()?);
    Ok(TemplateOffset::new(x, y))
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out as int == if value <= i8::MAX as u8 {
        value as int
    } else {
        value as int - 256
    },
))]
#[allow(clippy::cast_lossless, clippy::checked_conversions)]
fn signed_offset_byte(value: u8) -> i32 {
    if value <= i8::MAX as u8 {
        value as i32
    } else {
        value as i32 - 256
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) => generic_region_spec::target_adaptive_valid(offset),
        Err(_) => !generic_region_spec::target_adaptive_valid(offset),
    },
))]
fn validate_target_adaptive(number: u8, offset: TemplateOffset) -> Result<(), GenericRegionError> {
    if offset.y < 0 || (offset.y == 0 && offset.x < 0) {
        return Ok(());
    }
    Err(GenericRegionError::InvalidAdaptivePixel {
        number,
        x: offset.x,
        y: offset.y,
    })
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[allow(clippy::cast_possible_truncation)]
fn validate_generic_adaptive_values(
    template: GenericTemplate,
    extended_template: bool,
    adaptive: &[TemplateOffset; 12],
) -> Result<(), GenericRegionError> {
    let count = match template {
        GenericTemplate::Template0 if extended_template => 12,
        GenericTemplate::Template0 => 4,
        _ => 1,
    };
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            count <= adaptive@.len(),
            index <= count,
        decreases count - index,
    ))]
    while index < count {
        let number = (index + 1) as u8;
        validate_target_adaptive(number, adaptive[index])?;
        index += 1;
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn validate_arithmetic_terminator(data: &[u8]) -> Result<(), GenericRegionError> {
    if data.len() < 2
        || data[data.len() - 2] != ARITHMETIC_TERMINATOR[0]
        || data[data.len() - 1] != ARITHMETIC_TERMINATOR[1]
    {
        return Err(GenericRegionError::InvalidArithmeticTerminator);
    }
    let final_offset = data
        .len()
        .checked_sub(2)
        .ok_or(GenericRegionError::InvalidArithmeticTerminator)?;
    let mut offset = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            data@.len() >= 2,
            final_offset == data@.len() - 2,
            offset <= data@.len() - 1,
        decreases data@.len() - 1 - offset,
    ))]
    while offset < data.len() - 1 {
        if data[offset] == 0xff && data[offset + 1] > 0x8f && offset != final_offset {
            return Err(GenericRegionError::UnexpectedArithmeticMarker {
                offset,
                code: data[offset + 1],
            });
        }
        offset += 1;
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn validate_observed_marker(
    decoder: &ArithmeticDecoder<'_>,
    coded_length: usize,
) -> Result<(), GenericRegionError> {
    if let Some(marker) = decoder.marker() {
        let expected_offset = coded_length
            .checked_sub(2)
            .ok_or(GenericRegionError::InvalidArithmeticTerminator)?;
        if marker.code != ARITHMETIC_TERMINATOR[1] || marker.offset != expected_offset {
            return Err(GenericRegionError::UnexpectedArithmeticMarker {
                offset: marker.offset,
                code: marker.code,
            });
        }
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out == match template {
        GenericTemplate::Template0 => 65_536usize,
        GenericTemplate::Template1 => 8_192usize,
        GenericTemplate::Template2 | GenericTemplate::Template3 => 1_024usize,
    },
))]
fn generic_context_count(template: GenericTemplate) -> usize {
    match template {
        GenericTemplate::Template0 => 65_536,
        GenericTemplate::Template1 => 8_192,
        GenericTemplate::Template2 | GenericTemplate::Template3 => 1_024,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out == match template {
        RefinementTemplate::Template0 => 8_192usize,
        RefinementTemplate::Template1 => 1_024usize,
    },
))]
fn refinement_context_count(template: RefinementTemplate) -> usize {
    match template {
        RefinementTemplate::Template0 => 8_192,
        RefinementTemplate::Template1 => 1_024,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) => contexts@.len() == match template {
            GenericTemplate::Template0 => 65_536usize,
            GenericTemplate::Template1 => 8_192usize,
            GenericTemplate::Template2 | GenericTemplate::Template3 => 1_024usize,
        },
        Err(_) => true,
    },
))]
fn validate_generic_context_layout(
    template: GenericTemplate,
    _extended_template: bool,
    contexts: &[ArithmeticContext],
) -> Result<(), GenericRegionError> {
    let expected = generic_context_count(template);
    if contexts.len() != expected {
        return Err(GenericRegionError::ContextIndex {
            index: expected,
            count: contexts.len(),
        });
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) => contexts@.len() == match template {
            RefinementTemplate::Template0 => 8_192usize,
            RefinementTemplate::Template1 => 1_024usize,
        },
        Err(_) => true,
    },
))]
fn validate_refinement_context_layout(
    template: RefinementTemplate,
    contexts: &[ArithmeticContext],
) -> Result<(), GenericRegionError> {
    let expected = refinement_context_count(template);
    if contexts.len() != expected {
        return Err(GenericRegionError::ContextIndex {
            index: expected,
            count: contexts.len(),
        });
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) => forall|index: int| 0 <= index < contexts@.len() ==>
            contexts@[index].state_index < 47 && contexts@[index].mps <= 1,
        Err(_) => true,
    },
))]
fn validate_probability_states(contexts: &[ArithmeticContext]) -> Result<(), GenericRegionError> {
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index <= contexts@.len(),
            forall|completed: int| 0 <= completed < index ==>
                contexts@[completed].state_index < 47
                    && contexts@[completed].mps <= 1,
        decreases contexts@.len() - index,
    ))]
    while index < contexts.len() {
        let context = contexts[index];
        if let Err(error) = ArithmeticContext::from_state(context.state_index(), context.mps()) {
            let error = match error {
                ArithmeticError::Malformed(error) => DecodeError::Malformed(error),
                ArithmeticError::InvalidContextState { state_index, mps } => {
                    DecodeError::Malformed(MalformedError::InvalidArithmeticContextState {
                        state_index,
                        mps,
                    })
                }
                ArithmeticError::InsufficientContexts { required, actual } => {
                    DecodeError::Malformed(MalformedError::InsufficientArithmeticContexts {
                        required,
                        actual,
                    })
                }
                ArithmeticError::InvalidIaidCodeLength(length) => {
                    DecodeError::Malformed(MalformedError::InvalidIaidCodeLength(length))
                }
            };
            return Err(GenericRegionError::Decode(error));
        }
        index += 1;
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out
        == ((generic_region_spec::generic_template_code(template) as u8) << 1)
            | if extended { 0x10 } else { 0 },
))]
fn generic_template_code(template: GenericTemplate, extended: bool) -> u8 {
    let template_bits = match template {
        GenericTemplate::Template0 => 0,
        GenericTemplate::Template1 => 1,
        GenericTemplate::Template2 => 2,
        GenericTemplate::Template3 => 3,
    };
    (template_bits << 1) | if extended { 0x10 } else { 0 }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out == match template {
        RefinementTemplate::Template0 => 0u8,
        RefinementTemplate::Template1 => 1u8,
    },
))]
fn refinement_template_code(template: RefinementTemplate) -> u8 {
    match template {
        RefinementTemplate::Template0 => 0,
        RefinementTemplate::Template1 => 1,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) =>
            storage_bytes <= limits.max_retained_state_bytes
            && storage_bytes as nat * 2
                <= limits.max_decoder_managed_memory_bytes,
        Err(_) => true,
    },
))]
fn validate_snapshot_limits(storage_bytes: u64, limits: &Limits) -> Result<(), GenericRegionError> {
    if storage_bytes > limits.max_retained_state_bytes {
        return Err(DecodeError::LimitExceeded(LimitExceeded {
            kind: LimitKind::RetainedState,
            limit: limits.max_retained_state_bytes,
            actual: storage_bytes,
        })
        .into());
    }
    let Some(combined) = storage_bytes.checked_mul(2) else {
        return Err(integer_overflow());
    };
    if combined > limits.max_decoder_managed_memory_bytes {
        return Err(DecodeError::LimitExceeded(LimitExceeded {
            kind: LimitKind::Memory,
            limit: limits.max_decoder_managed_memory_bytes,
            actual: combined,
        })
        .into());
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(copied) => copied@ == source@,
        Err(_) => true,
    },
))]
fn try_copy_contexts(
    source: &[ArithmeticContext],
    storage_bytes: u64,
    limits: &Limits,
) -> Result<Vec<ArithmeticContext>, GenericRegionError> {
    validate_probability_states(source)?;
    validate_snapshot_limits(storage_bytes, limits)?;
    let mut copied = Vec::new();
    if copied.try_reserve(source.len()).is_err() {
        return Err(DecodeError::Allocation {
            requested_bytes: storage_bytes,
        }
        .into());
    }
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index <= source@.len(),
            copied@ == source@.subrange(0, index as int),
        decreases source@.len() - index,
    ))]
    while index < source.len() {
        copied.push(source[index]);
        index += 1;
    }
    Ok(copied)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(contexts) =>
            contexts@.len() == count
            && forall|index: int| 0 <= index < contexts@.len() ==>
                contexts@[index].state_index == 0
                && contexts@[index].mps == 0,
        Err(_) => true,
    },
))]
fn allocate_contexts(
    count: usize,
    bitmap_bytes: u64,
    limits: &Limits,
) -> Result<Vec<ArithmeticContext>, GenericRegionError> {
    if count > MAX_GENERIC_CONTEXTS.max(MAX_REFINEMENT_CONTEXTS) {
        return Err(MalformedError::IntegerOverflow.into());
    }
    let context_bytes = context_storage_bytes(count)?;
    enforce_managed_memory(context_bytes, bitmap_bytes, limits)?;
    let mut contexts = Vec::new();
    if contexts.try_reserve(count).is_err() {
        return Err(DecodeError::Allocation {
            requested_bytes: context_bytes,
        }
        .into());
    }
    contexts.resize(count, ArithmeticContext::new());
    Ok(contexts)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(bytes) => bytes as nat == count as nat * 2,
        Err(_) => count as nat * 2 > u64::MAX,
    },
))]
fn context_storage_bytes(count: usize) -> Result<u64, GenericRegionError> {
    let Ok(count) = u64::try_from(count) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    count
        .checked_mul(2)
        .ok_or_else(|| MalformedError::IntegerOverflow.into())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) =>
            bitmap_bytes as nat + context_bytes as nat
                <= limits.max_decoder_managed_memory_bytes,
        Err(_) => true,
    },
))]
fn enforce_managed_memory(
    context_bytes: u64,
    bitmap_bytes: u64,
    limits: &Limits,
) -> Result<(), GenericRegionError> {
    let Some(managed) = bitmap_bytes.checked_add(context_bytes) else {
        return Err(integer_overflow());
    };
    if managed > limits.max_decoder_managed_memory_bytes {
        return Err(DecodeError::LimitExceeded(LimitExceeded {
            kind: LimitKind::Memory,
            limit: limits.max_decoder_managed_memory_bytes,
            actual: managed,
        })
        .into());
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(bytes) => bytes as nat
            == generic_region_spec::bitmap_storage_spec(width as nat, height as nat),
        Err(_) => true,
    },
))]
fn bitmap_storage_bytes(width: u32, height: u32) -> Result<u64, GenericRegionError> {
    let Some(stride) = u64::from(width).checked_add(7) else {
        return Err(integer_overflow());
    };
    let Some(bytes) = (stride / 8).checked_mul(u64::from(height)) else {
        return Err(integer_overflow());
    };
    Ok(bytes)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) => generic_region_spec::generic_work_spec(
            width as nat,
            rows as nat,
            typical_prediction,
        ) <= limits.max_work_units,
        Err(_) => true,
    },
))]
fn enforce_generic_work(
    width: u32,
    rows: u32,
    typical_prediction: bool,
    limits: &Limits,
) -> Result<(), GenericRegionError> {
    enforce_work(generic_work(width, rows, typical_prediction)?, limits)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(work) => work as nat == generic_region_spec::generic_work_spec(
            width as nat,
            rows as nat,
            typical_prediction,
        ),
        Err(_) => generic_region_spec::generic_work_spec(
            width as nat,
            rows as nat,
            typical_prediction,
        ) > u64::MAX,
    },
))]
fn generic_work(
    width: u32,
    rows: u32,
    typical_prediction: bool,
) -> Result<u64, GenericRegionError> {
    let Some(pixels) = u64::from(width).checked_mul(u64::from(rows)) else {
        return Err(integer_overflow());
    };
    let Some(mut work) = pixels.checked_mul(17) else {
        return Err(integer_overflow());
    };
    if typical_prediction {
        let Some(next_work) = work.checked_add(u64::from(rows)) else {
            return Err(integer_overflow());
        };
        work = next_work;
    }
    Ok(work)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) => generic_region_spec::refinement_work_spec(
            width as nat,
            height as nat,
            typical_prediction,
        ) <= limits.max_work_units,
        Err(_) => true,
    },
))]
fn enforce_refinement_work(
    width: u32,
    height: u32,
    typical_prediction: bool,
    limits: &Limits,
) -> Result<(), GenericRegionError> {
    enforce_work(refinement_work(width, height, typical_prediction)?, limits)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(work) => work as nat
            == generic_region_spec::refinement_work_spec(
                width as nat,
                height as nat,
                typical_prediction,
            ),
        Err(_) => true,
    },
))]
fn refinement_work(
    width: u32,
    height: u32,
    typical_prediction: bool,
) -> Result<u64, GenericRegionError> {
    let Some(pixels) = u64::from(width).checked_mul(u64::from(height)) else {
        return Err(integer_overflow());
    };
    let multiplier = if typical_prediction { 23 } else { 14 };
    let Some(partial) = pixels.checked_mul(multiplier) else {
        return Err(integer_overflow());
    };
    let Some(work) = partial.checked_add(u64::from(height)) else {
        return Err(integer_overflow());
    };
    Ok(work)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) => work <= limits.max_work_units,
        Err(_) => work > limits.max_work_units,
    },
))]
fn enforce_work(work: u64, limits: &Limits) -> Result<(), GenericRegionError> {
    if work > limits.max_work_units {
        return Err(DecodeError::LimitExceeded(LimitExceeded {
            kind: LimitKind::Work,
            limit: limits.max_work_units,
            actual: work,
        })
        .into());
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) => data@.len() <= limits.max_stream_bytes,
        Err(_) => true,
    },
))]
fn enforce_input_limit(data: &[u8], limits: &Limits) -> Result<(), GenericRegionError> {
    let actual = u64::try_from(data.len()).map_err(|_error| MalformedError::IntegerOverflow)?;
    if actual > limits.max_stream_bytes {
        return Err(DecodeError::LimitExceeded(LimitExceeded {
            kind: LimitKind::StreamBytes,
            limit: limits.max_stream_bytes,
            actual,
        })
        .into());
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn integer_overflow() -> GenericRegionError {
    GenericRegionError::Decode(DecodeError::Malformed(MalformedError::IntegerOverflow))
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn unexpected_eof(offset: usize, needed: usize, remaining: usize) -> GenericRegionError {
    let Ok(offset) = u64::try_from(offset) else {
        return GenericRegionError::Decode(MalformedError::IntegerOverflow.into());
    };
    let Ok(needed) = u64::try_from(needed) else {
        return GenericRegionError::Decode(MalformedError::IntegerOverflow.into());
    };
    let Ok(remaining) = u64::try_from(remaining) else {
        return GenericRegionError::Decode(MalformedError::IntegerOverflow.into());
    };
    GenericRegionError::Decode(
        MalformedError::UnexpectedEof {
            offset,
            needed,
            remaining,
        }
        .into(),
    )
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        crate::spec::bitmap::storage_valid(old(bitmap)),
        source < old(bitmap).height,
        destination < old(bitmap).height,
    ensures
        crate::spec::bitmap::storage_valid(final(bitmap)),
        final(bitmap).width == old(bitmap).width,
        final(bitmap).height == old(bitmap).height,
        final(bitmap).stride == old(bitmap).stride,
        final(bitmap).data@.len() == old(bitmap).data@.len(),
        out.is_ok(),
))]
#[allow(clippy::cast_possible_truncation)]
fn copy_bitmap_row(
    bitmap: &mut Bitmap,
    source: u32,
    destination: u32,
) -> Result<(), GenericRegionError> {
    #[cfg(verus_keep_ghost)]
    bitmap.copy_row_scalar_verified(source, destination)?;
    #[cfg(not(verus_keep_ghost))]
    bitmap.copy_row(source, destination)?;
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::views::bitmap_fields(bitmap);
        reveal(crate::spec::bitmap::storage_valid);
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires crate::spec::bitmap::storage_valid(&decoded),
    ensures match out {
        Ok(bitmap) =>
            crate::spec::bitmap::storage_valid(&bitmap)
            && bitmap.width == decoded.width
            && bitmap.height == nominal_height,
        Err(_) => true,
    },
))]
fn pad_bitmap_height(
    decoded: Bitmap,
    nominal_height: u32,
    limits: &Limits,
) -> Result<Bitmap, GenericRegionError> {
    if decoded.height == nominal_height {
        return Ok(decoded);
    }
    if decoded.height > nominal_height {
        return Err(GenericRegionError::RowCountMismatch {
            expected: nominal_height,
            actual: decoded.height,
        });
    }
    let mut padded = Bitmap::blank(decoded.width, nominal_height, limits)?;
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::views::bitmap_fields(&padded);
        reveal(crate::spec::bitmap::storage_valid);
    }
    let byte_count = decoded.data.len();
    if byte_count > padded.data.len() {
        return Err(MalformedError::IntegerOverflow.into());
    }
    padded.copy_prefix_from(&decoded, byte_count)?;
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::views::bitmap_fields(&padded);
        reveal(crate::spec::bitmap::storage_valid);
    }
    Ok(padded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitmap::CombinationOperator;
    use std::collections::VecDeque;

    fn region_prefix(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&0_u32.to_be_bytes());
        bytes.extend_from_slice(&0_u32.to_be_bytes());
        bytes.push(4);
        bytes
    }

    fn region_info(width: u32, height: u32) -> RegionInfo {
        RegionInfo {
            width,
            height,
            x: 0,
            y: 0,
            operator: CombinationOperator::Replace,
        }
    }

    fn generic_header(
        width: u32,
        height: u32,
        template: GenericTemplate,
        extended: bool,
        typical: bool,
    ) -> GenericRegionHeader {
        let mut adaptive = [TemplateOffset::ZERO; 12];
        match (template, extended) {
            (GenericTemplate::Template0, false) => {
                adaptive[..4].copy_from_slice(&[
                    TemplateOffset::new(3, -1),
                    TemplateOffset::new(-3, -1),
                    TemplateOffset::new(2, -2),
                    TemplateOffset::new(-2, -2),
                ]);
            }
            (GenericTemplate::Template0, true) => {
                adaptive.copy_from_slice(&[
                    TemplateOffset::new(-2, 0),
                    TemplateOffset::new(0, -2),
                    TemplateOffset::new(-2, -1),
                    TemplateOffset::new(-1, -2),
                    TemplateOffset::new(1, -2),
                    TemplateOffset::new(2, -1),
                    TemplateOffset::new(-3, 0),
                    TemplateOffset::new(-4, 0),
                    TemplateOffset::new(2, -2),
                    TemplateOffset::new(3, -1),
                    TemplateOffset::new(-2, -2),
                    TemplateOffset::new(-3, -1),
                ]);
            }
            (GenericTemplate::Template1, _) => {
                adaptive[0] = TemplateOffset::new(3, -1);
            }
            (GenericTemplate::Template2 | GenericTemplate::Template3, _) => {
                adaptive[0] = TemplateOffset::new(2, -1);
            }
        }
        GenericRegionHeader {
            region: region_info(width, height),
            mmr: false,
            template,
            typical_prediction: typical,
            extended_template: extended,
            adaptive,
        }
    }

    fn decode_bits(
        bitmap: &mut Bitmap,
        rows: u32,
        header: &GenericRegionHeader,
        skip: Option<&Bitmap>,
        bits: &[u8],
    ) -> Result<Vec<usize>, GenericRegionError> {
        let mut bits = VecDeque::from(bits.to_vec());
        let mut contexts = Vec::new();
        decode_generic_with_callback(
            bitmap,
            rows,
            header.template,
            header.extended_template,
            &header.adaptive,
            header.typical_prediction,
            skip,
            |context| {
                contexts.push(context);
                bits.pop_front()
                    .ok_or(GenericRegionError::RowCountMismatch {
                        expected: 1,
                        actual: 0,
                    })
            },
        )?;
        Ok(contexts)
    }

    #[test]
    fn parses_all_generic_header_shapes() {
        let limits = Limits::default();

        let mut standard = region_prefix(8, 2);
        standard.push(0);
        standard.extend_from_slice(&[3, 0xff, 0xfd, 0xff, 2, 0xfe, 0xfe, 0xfe]);
        standard.push(0xaa);
        let (header, used) =
            parse_generic_region_header(&standard, &limits).expect("standard header");
        assert_eq!(used, 26);
        assert_eq!(header.template, GenericTemplate::Template0);
        assert_eq!(header.adaptive[0], TemplateOffset::new(3, -1));
        assert_eq!(standard[used], 0xaa);

        let mut extended = region_prefix(8, 2);
        extended.push(0x10);
        for offset in generic_header(8, 2, GenericTemplate::Template0, true, false).adaptive {
            extended.push(offset.x.to_ne_bytes()[0]);
            extended.push(offset.y.to_ne_bytes()[0]);
        }
        extended.push(0xbb);
        let (header, used) =
            parse_generic_region_header(&extended, &limits).expect("extended header");
        assert!(header.extended_template);
        assert_eq!(used, 42, "17 + flags + twelve signed pairs");
        assert_eq!(extended[used], 0xbb);

        for (flag, template) in [
            (0x02, GenericTemplate::Template1),
            (0x04, GenericTemplate::Template2),
            (0x06, GenericTemplate::Template3),
        ] {
            let mut bytes = region_prefix(1, 1);
            bytes.extend_from_slice(&[flag, 2, 0xff]);
            let (header, used) =
                parse_generic_region_header(&bytes, &limits).expect("small template");
            assert_eq!(header.template, template);
            assert_eq!(used, 20);
        }
    }

    #[test]
    fn rejects_reserved_flags_mmr_template_and_future_at() {
        let limits = Limits::default();
        for flags in [0x20, 0x03] {
            let mut bytes = region_prefix(1, 1);
            bytes.push(flags);
            assert!(parse_generic_region_header(&bytes, &limits).is_err());
        }

        let mut future = region_prefix(1, 1);
        future.extend_from_slice(&[0x02, 0, 0]);
        assert!(matches!(
            parse_generic_region_header(&future, &limits),
            Err(GenericRegionError::InvalidAdaptivePixel { .. })
        ));
    }

    #[test]
    fn generic_template_geometry_and_sltp_indices_are_exact() {
        assert_eq!(GENERIC_TEMPLATE_0.len(), 16);
        assert_eq!(GENERIC_TEMPLATE_0_EXTENDED.len(), 16);
        assert_eq!(GENERIC_TEMPLATE_1.len(), 13);
        assert_eq!(GENERIC_TEMPLATE_2.len(), 10);
        assert_eq!(GENERIC_TEMPLATE_3.len(), 10);
        assert_eq!(GenericTemplate::Template0.sltp_context(), 0x9b25);
        assert_eq!(GenericTemplate::Template1.sltp_context(), 0x0795);
        assert_eq!(GenericTemplate::Template2.sltp_context(), 0x00e5);
        assert_eq!(GenericTemplate::Template3.sltp_context(), 0x0195);
        assert_eq!(
            GENERIC_TEMPLATE_0_EXTENDED
                .iter()
                .filter(|slot| slot.adaptive.is_some())
                .count(),
            12
        );
    }

    #[test]
    fn moved_adaptive_pixel_keeps_its_slot_identity() {
        let limits = Limits::default();
        let mut bitmap = Bitmap::blank(10, 3, &limits).expect("bitmap");
        bitmap.set_pixel(0, 2, true).expect("set");
        let mut header = generic_header(10, 3, GenericTemplate::Template0, false, false);
        header.adaptive[0] = TemplateOffset::new(-5, 0);
        let context = generic_context(&bitmap, 5, 2, header.template, false, &header.adaptive);
        assert_eq!(context, 1 << 4, "A1 remains the twelfth row-major slot");

        bitmap.set_pixel(1, 2, true).expect("set overlap");
        header.adaptive[0] = TemplateOffset::new(-4, 0);
        let overlap = generic_context(&bitmap, 5, 2, header.template, false, &header.adaptive);
        assert_eq!(
            overlap, 0x18,
            "overlap keeps both the A1 and fixed template slots"
        );
    }

    #[test]
    fn out_of_bounds_generic_samples_are_zero() {
        let limits = Limits::default();
        let bitmap = Bitmap::blank(1, 1, &limits).expect("bitmap");
        let header = generic_header(1, 1, GenericTemplate::Template3, false, false);
        assert_eq!(
            generic_context(&bitmap, 0, 0, header.template, false, &header.adaptive),
            0
        );
    }

    #[test]
    fn tpgd_toggles_and_copies_without_pixel_decisions() {
        let limits = Limits::default();
        let header = generic_header(3, 3, GenericTemplate::Template2, false, true);
        let mut bitmap = Bitmap::blank(3, 3, &limits).expect("bitmap");
        // Row 0: explicit 101. Row 1: SLTP=1, copy. Row 2: SLTP=0, copy.
        let contexts =
            decode_bits(&mut bitmap, 3, &header, None, &[0, 1, 0, 1, 1, 0]).expect("decode");
        assert_eq!(contexts.len(), 6);
        assert_eq!(contexts[0], 0x00e5);
        for y in 0..3 {
            assert_eq!(bitmap.pixel(0, y), Some(true));
            assert_eq!(bitmap.pixel(1, y), Some(false));
            assert_eq!(bitmap.pixel(2, y), Some(true));
        }
    }

    #[test]
    fn first_typical_generic_row_is_zero() {
        let limits = Limits::default();
        let header = generic_header(2, 1, GenericTemplate::Template3, false, true);
        let mut bitmap = Bitmap::blank(2, 1, &limits).expect("bitmap");
        let contexts = decode_bits(&mut bitmap, 1, &header, None, &[1]).expect("decode");
        assert_eq!(contexts, vec![0x0195]);
        assert_eq!(bitmap.pixel(0, 0), Some(false));
        assert_eq!(bitmap.pixel(1, 0), Some(false));
    }

    #[test]
    fn skip_outputs_zero_and_consumes_no_decision() {
        let limits = Limits::default();
        let header = generic_header(3, 1, GenericTemplate::Template3, false, false);
        let mut bitmap = Bitmap::blank(3, 1, &limits).expect("bitmap");
        let mut skip = Bitmap::blank(3, 1, &limits).expect("skip");
        skip.set_pixel(1, 0, true).expect("set skip");
        let contexts = decode_bits(&mut bitmap, 1, &header, Some(&skip), &[1, 1]).expect("decode");
        assert_eq!(contexts.len(), 2);
        assert_eq!(bitmap.pixel(0, 0), Some(true));
        assert_eq!(bitmap.pixel(1, 0), Some(false));
        assert_eq!(bitmap.pixel(2, 0), Some(true));
    }

    #[test]
    fn parses_refinement_headers_and_nominal_adaptive_pixels() {
        let limits = Limits::default();
        let mut template0 = region_prefix(4, 4);
        template0.extend_from_slice(&[0x02, 0xff, 0xff, 0xff, 0xff]);
        let (header, used) =
            parse_refinement_region_header(&template0, &limits).expect("template 0");
        assert_eq!(used, 22);
        assert_eq!(header.template, RefinementTemplate::Template0);
        assert!(header.typical_prediction);
        assert_eq!(header.adaptive, [TemplateOffset::new(-1, -1); 2]);

        let mut template1 = region_prefix(4, 4);
        template1.push(0x01);
        let (header, used) =
            parse_refinement_region_header(&template1, &limits).expect("template 1");
        assert_eq!(used, 18);
        assert_eq!(header.template, RefinementTemplate::Template1);
    }

    #[test]
    fn refinement_templates_and_sltp_indices_are_exact() {
        assert_eq!(REFINEMENT_TEMPLATE_0.len(), 13);
        assert_eq!(REFINEMENT_TEMPLATE_1.len(), 10);
        assert_eq!(RefinementTemplate::Template0.sltp_context(), 0x10);
        assert_eq!(RefinementTemplate::Template1.sltp_context(), 0x08);
    }

    #[test]
    fn reference_offset_uses_target_minus_displacement() {
        let limits = Limits::default();
        let mut reference = Bitmap::blank(4, 4, &limits).expect("reference");
        reference.set_pixel(0, 1, true).expect("set");
        assert!(sample_reference(
            &reference,
            2,
            1,
            TemplateOffset::ZERO,
            RefinementParameters {
                reference_dx: 2,
                reference_dy: 0,
            }
        ));
        assert!(!sample_reference(
            &reference,
            2,
            1,
            TemplateOffset::ZERO,
            RefinementParameters {
                reference_dx: -2,
                reference_dy: 0,
            }
        ));
    }

    #[test]
    fn tpgr_predicts_only_uniform_reference_neighbourhoods() {
        let limits = Limits::default();
        let zeros = Bitmap::blank(3, 3, &limits).expect("zeros");
        assert_eq!(
            refinement_prediction(
                &zeros,
                1,
                1,
                RefinementParameters {
                    reference_dx: 0,
                    reference_dy: 0
                }
            ),
            Some(false)
        );

        let ones = Bitmap::filled(3, 3, true, &limits).expect("ones");
        assert_eq!(
            refinement_prediction(
                &ones,
                1,
                1,
                RefinementParameters {
                    reference_dx: 0,
                    reference_dy: 0
                }
            ),
            Some(true)
        );
        assert_eq!(
            refinement_prediction(
                &ones,
                0,
                0,
                RefinementParameters {
                    reference_dx: 0,
                    reference_dy: 0
                }
            ),
            None,
            "out-of-bounds zeroes prevent edge all-one prediction"
        );
    }

    #[test]
    fn tpgr_skips_only_predictable_pixels() {
        let limits = Limits::default();
        let reference = Bitmap::blank(3, 1, &limits).expect("reference");
        let mut target = Bitmap::blank(3, 1, &limits).expect("target");
        let mut decisions = VecDeque::from(vec![1]);
        let mut contexts = Vec::new();
        decode_refinement_with_callback(
            &mut target,
            &reference,
            RefinementTemplate::Template1,
            [TemplateOffset::ZERO; 2],
            true,
            RefinementParameters {
                reference_dx: 0,
                reference_dy: 0,
            },
            |context| {
                contexts.push(context);
                decisions
                    .pop_front()
                    .ok_or(GenericRegionError::RowCountMismatch {
                        expected: 1,
                        actual: 0,
                    })
            },
        )
        .expect("uniform zero reference predicts every pixel");
        assert_eq!(contexts, vec![0x08]);
        assert!(target.bytes().iter().all(|byte| *byte == 0));
    }

    #[test]
    fn arithmetic_marker_validation_is_bounded_and_strict() {
        assert!(validate_arithmetic_terminator(&[0x12, 0xff, 0xac]).is_ok());
        assert!(matches!(
            validate_arithmetic_terminator(&[0xff, 0x90, 0xff, 0xac]),
            Err(GenericRegionError::UnexpectedArithmeticMarker { .. })
        ));
        assert!(matches!(
            validate_arithmetic_terminator(&[0x12, 0xff]),
            Err(GenericRegionError::InvalidArithmeticTerminator)
        ));
    }

    #[test]
    fn generic_arithmetic_session_preserves_contexts_and_stream() {
        const ANNEX_H_ENCODED: [u8; 30] = [
            0x84, 0xc7, 0x3b, 0xfc, 0xe1, 0xa1, 0x43, 0x04, 0x02, 0x20, 0x00, 0x00, 0x41, 0x0d,
            0xbb, 0x86, 0xf4, 0x31, 0x7f, 0xff, 0x88, 0xff, 0x37, 0x47, 0x1a, 0xdb, 0x6a, 0xdf,
            0xff, 0xac,
        ];
        let limits = Limits::default();
        let header = generic_header(4, 1, GenericTemplate::Template3, false, false);
        let call = GenericArithmeticCall {
            width: 4,
            height: 1,
            adaptive: header.adaptive,
            typical_prediction: false,
            skip: None,
        };
        let mut session = GenericArithmeticSession::new(
            &ANNEX_H_ENCODED,
            GenericTemplate::Template3,
            false,
            &limits,
        )
        .expect("session");

        let first = session.decode_bitmap(call).expect("first bitmap");
        let first_state = session.contexts.contexts[0];
        let first_position = session.consumed_byte_offset();
        let second = session.decode_bitmap(call).expect("second bitmap");
        let second_state = session.contexts.contexts[0];
        let second_position = session.consumed_byte_offset();

        assert!(first.bytes().iter().all(|byte| *byte == 0));
        assert!(second.bytes().iter().all(|byte| *byte == 0));
        assert_ne!(
            first_state, second_state,
            "the second call must reuse, not reset, context zero"
        );
        assert!(
            second_position >= first_position,
            "the continuous stream cannot move backwards"
        );
        assert_eq!(session.validate_final_marker().expect("marker"), 28);
    }

    #[test]
    fn generic_arithmetic_session_stays_bounded_at_observed_marker() {
        let limits = Limits::default();
        let header = generic_header(1, 1, GenericTemplate::Template3, false, false);
        let call = GenericArithmeticCall {
            width: 1,
            height: 1,
            adaptive: header.adaptive,
            typical_prediction: false,
            skip: None,
        };
        let mut session = GenericArithmeticSession::new(
            &[0xff, 0xac],
            GenericTemplate::Template3,
            false,
            &limits,
        )
        .expect("marker-fill session");

        let state_before = session.contexts.contexts[0];
        session.decode_bitmap(call).expect("first bitmap");
        let state_after_first = session.contexts.contexts[0];
        session.decode_bitmap(call).expect("second bitmap");
        let state_after_second = session.contexts.contexts[0];

        assert_ne!(state_before, state_after_first);
        assert_ne!(state_after_first, state_after_second);
        assert_eq!(session.observed_marker_offset(), Some(0));
        assert_eq!(session.consumed_byte_offset(), 0);
        assert_eq!(session.validate_final_marker().expect("bounded marker"), 0);
    }

    #[test]
    fn generic_arithmetic_session_charges_work_cumulatively() {
        let limits = Limits {
            max_work_units: 20,
            ..Limits::default()
        };
        let header = generic_header(1, 1, GenericTemplate::Template3, false, false);
        let call = GenericArithmeticCall {
            width: 1,
            height: 1,
            adaptive: header.adaptive,
            typical_prediction: false,
            skip: None,
        };
        let mut session = GenericArithmeticSession::new(
            &[0xff, 0xac],
            GenericTemplate::Template3,
            false,
            &limits,
        )
        .expect("session");
        session.decode_bitmap(call).expect("first call costs 17");
        assert!(matches!(
            session.decode_bitmap(call),
            Err(GenericRegionError::Decode(DecodeError::LimitExceeded(
                LimitExceeded {
                    kind: LimitKind::Work,
                    limit: 20,
                    actual: 34
                }
            )))
        ));
    }

    #[test]
    fn generic_and_refinement_contexts_snapshot_reset_and_restore() {
        let limits = Limits::default();
        let header = generic_header(1, 1, GenericTemplate::Template3, false, false);
        let mut generic = GenericContexts::new(GenericTemplate::Template3, false, &limits)
            .expect("generic contexts");
        let mut decoder = ArithmeticDecoder::new(&[0xff, 0xac]).expect("bounded decoder");
        let mut work = 0;
        decode_generic_on(
            &mut decoder,
            &mut generic,
            GenericArithmeticCall {
                width: 1,
                height: 1,
                adaptive: header.adaptive,
                typical_prediction: false,
                skip: None,
            },
            1,
            &limits,
            &mut work,
        )
        .expect("mutate generic contexts");
        let retained_generic = generic.try_snapshot(&limits).expect("retain generic");
        let generic_state = generic.contexts[0];
        generic.reset().expect("reset generic");
        assert_eq!(generic.contexts[0], ArithmeticContext::new());
        generic
            .restore(&retained_generic, &limits)
            .expect("restore generic");
        assert_eq!(generic.contexts[0], generic_state);

        let mut incompatible = GenericContexts::new(GenericTemplate::Template2, false, &limits)
            .expect("same-sized incompatible contexts");
        assert!(incompatible.restore(&retained_generic, &limits).is_err());

        let mut refinement = RefinementContexts::new(RefinementTemplate::Template1, &limits)
            .expect("refinement contexts");
        let reference = Bitmap::blank(1, 1, &limits).expect("reference");
        decode_refinement_on(
            &mut decoder,
            &mut refinement,
            RefinementArithmeticCall {
                width: 1,
                height: 1,
                adaptive: [TemplateOffset::ZERO; 2],
                typical_prediction: false,
            },
            &reference,
            RefinementParameters {
                reference_dx: 0,
                reference_dy: 0,
            },
            &limits,
            &mut work,
        )
        .expect("mutate refinement contexts");
        let retained_refinement = refinement.try_snapshot(&limits).expect("retain refinement");
        let refinement_state = refinement.contexts[0];
        refinement.reset().expect("reset refinement");
        assert_eq!(refinement.contexts[0], ArithmeticContext::new());
        refinement
            .restore(&retained_refinement, &limits)
            .expect("restore refinement");
        assert_eq!(refinement.contexts[0], refinement_state);

        let mut retained_limited = limits;
        retained_limited.max_retained_state_bytes = 0;
        assert!(matches!(
            generic.try_snapshot(&retained_limited),
            Err(GenericRegionError::Decode(DecodeError::LimitExceeded(
                LimitExceeded {
                    kind: LimitKind::RetainedState,
                    ..
                }
            )))
        ));

        let mut allocation_limited = limits;
        allocation_limited.max_decoder_managed_memory_bytes = 0;
        assert!(matches!(
            GenericContexts::new(GenericTemplate::Template3, false, &allocation_limited),
            Err(GenericRegionError::Decode(DecodeError::LimitExceeded(
                LimitExceeded {
                    kind: LimitKind::Memory,
                    ..
                }
            )))
        ));
    }

    #[test]
    fn arithmetic_procedures_interleave_on_one_decoder() {
        const ANNEX_H_ENCODED: [u8; 30] = [
            0x84, 0xc7, 0x3b, 0xfc, 0xe1, 0xa1, 0x43, 0x04, 0x02, 0x20, 0x00, 0x00, 0x41, 0x0d,
            0xbb, 0x86, 0xf4, 0x31, 0x7f, 0xff, 0x88, 0xff, 0x37, 0x47, 0x1a, 0xdb, 0x6a, 0xdf,
            0xff, 0xac,
        ];
        let limits = Limits::default();
        let mut decoder = ArithmeticDecoder::new(&ANNEX_H_ENCODED).expect("decoder");
        let mut integer_contexts = [ArithmeticContext::new(); 512];
        let first_integer = decoder
            .decode_integer(&mut integer_contexts)
            .expect("first integer");

        let header = generic_header(1, 1, GenericTemplate::Template3, false, false);
        let mut generic = GenericContexts::new(GenericTemplate::Template3, false, &limits)
            .expect("generic contexts");
        let mut cumulative_work = 0;
        let generic_bitmap = decode_generic_on(
            &mut decoder,
            &mut generic,
            GenericArithmeticCall {
                width: 1,
                height: 1,
                adaptive: header.adaptive,
                typical_prediction: false,
                skip: None,
            },
            1,
            &limits,
            &mut cumulative_work,
        )
        .expect("generic bitmap");

        let second_integer = decoder
            .decode_integer(&mut integer_contexts)
            .expect("second integer");

        let reference = Bitmap::blank(1, 1, &limits).expect("reference");
        let mut refinement = RefinementContexts::new(RefinementTemplate::Template1, &limits)
            .expect("refinement contexts");
        let refined_bitmap = decode_refinement_on(
            &mut decoder,
            &mut refinement,
            RefinementArithmeticCall {
                width: 1,
                height: 1,
                adaptive: [TemplateOffset::ZERO; 2],
                typical_prediction: false,
            },
            &reference,
            RefinementParameters {
                reference_dx: 0,
                reference_dy: 0,
            },
            &limits,
            &mut cumulative_work,
        )
        .expect("refinement bitmap");

        assert_ne!(first_integer, second_integer);
        assert!(generic_bitmap.pixel(0, 0).is_some());
        assert!(refined_bitmap.pixel(0, 0).is_some());
        assert_eq!(cumulative_work, 32);
        validate_observed_marker(&decoder, ANNEX_H_ENCODED.len())
            .expect("any observed marker remains the bounded final marker");
    }

    #[test]
    fn unknown_length_arithmetic_count_is_validated() {
        let limits = Limits::default();
        let data = [0x12, 0xff, 0xac, 0, 0, 0, 3, 0xde, 0xad];
        let terminal = split_unknown_length_data(&data, false, 4, &limits).expect("terminal");
        assert_eq!(terminal.coded, &[0x12, 0xff, 0xac]);
        assert_eq!(terminal.rows, 3);
        assert_eq!(terminal.consumed, 7);

        let too_many = [0xff, 0xac, 0, 0, 0, 5];
        assert!(matches!(
            split_unknown_length_data(&too_many, false, 4, &limits),
            Err(GenericRegionError::Decode(DecodeError::Malformed(
                MalformedError::GenericRegionRowCount {
                    declared: 4,
                    actual: 5
                }
            )))
        ));
    }

    #[test]
    fn known_length_mmr_dispatches_to_shared_decoder() {
        let limits = Limits::default();
        let mut payload = region_prefix(8, 1);
        payload.push(0x01);
        payload.push(0x80);
        let decoded = decode_generic_region_segment(&payload, false, &limits).expect("MMR");
        assert_eq!(decoded.decoded_rows, 1);
        assert_eq!(decoded.consumed_bytes, payload.len());
        assert!(decoded.bitmap.bytes().iter().all(|byte| *byte == 0));
    }

    #[test]
    fn annex_h_generic_arithmetic_segment_decodes() {
        let limits = Limits::default();
        let mut payload = region_prefix(54, 44);
        payload[8..12].copy_from_slice(&4_u32.to_be_bytes());
        payload[12..16].copy_from_slice(&11_u32.to_be_bytes());
        payload[16] = 0;
        payload.push(0x08);
        payload.extend_from_slice(&[3, 0xff, 0xfd, 0xff, 2, 0xfe, 0xfe, 0xfe]);
        payload.extend_from_slice(&[0x04, 0xee, 0xed, 0x87, 0xfb, 0xcb, 0x2b, 0xff, 0xac]);

        let decoded = decode_generic_region_segment(&payload, false, &limits)
            .expect("Annex H segment must decode");
        assert_eq!(decoded.bitmap.width(), 54);
        assert_eq!(decoded.bitmap.height(), 44);
        assert_eq!(decoded.consumed_bytes, payload.len());
        assert!(
            decoded.bitmap.bytes().iter().any(|byte| *byte != 0),
            "the Annex H region is not blank"
        );
    }

    #[test]
    fn bounded_refinement_uses_shared_arithmetic_decoder() {
        let limits = Limits::default();
        let reference = Bitmap::blank(1, 1, &limits).expect("reference");
        let header = RefinementRegionHeader {
            region: region_info(1, 1),
            template: RefinementTemplate::Template1,
            typical_prediction: false,
            adaptive: [TemplateOffset::ZERO; 2],
        };
        let decoded = decode_arithmetic_refinement(
            &[0xff, 0xac],
            &header,
            RefinementParameters {
                reference_dx: 0,
                reference_dy: 0,
            },
            &reference,
            &limits,
        )
        .expect("marker fill supplies a bounded final decision");
        assert_eq!(decoded.pixel(0, 0), Some(true));
    }

    #[test]
    fn unknown_length_mmr_requires_eofb_and_pads_nominal_rows() {
        let limits = Limits::default();
        let mut payload = region_prefix(8, 2);
        payload.push(0x01);
        payload.extend_from_slice(&[0x80, 0x08, 0x00, 0x80]);
        payload.extend_from_slice(&[0x00, 0x00, 0, 0, 0, 1]);
        payload.extend_from_slice(&[0xde, 0xad]);
        let decoded = decode_generic_region_segment(&payload, true, &limits).expect("MMR");
        assert_eq!(decoded.decoded_rows, 1);
        assert_eq!(decoded.bitmap.height(), 2);
        assert_eq!(decoded.consumed_bytes, payload.len() - 2);
        assert!(decoded.bitmap.bytes().iter().all(|byte| *byte == 0));
    }

    #[test]
    fn work_and_memory_limits_are_enforced_before_decode() {
        let limits = Limits {
            max_work_units: 1,
            ..Limits::default()
        };
        assert!(matches!(
            enforce_generic_work(2, 1, false, &limits),
            Err(GenericRegionError::Decode(DecodeError::LimitExceeded(
                LimitExceeded {
                    kind: LimitKind::Work,
                    ..
                }
            )))
        ));

        let memory_limits = Limits {
            max_decoder_managed_memory_bytes: 1,
            ..Limits::default()
        };
        assert!(matches!(
            allocate_contexts(1024, 1, &memory_limits),
            Err(GenericRegionError::Decode(DecodeError::LimitExceeded(
                LimitExceeded {
                    kind: LimitKind::Memory,
                    ..
                }
            )))
        ));
    }
}

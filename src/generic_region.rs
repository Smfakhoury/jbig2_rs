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
pub(crate) mod generic_region_spec;

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
    pub(crate) const fn sltp_context(self) -> usize {
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

/// Metadata for a generic region decoded directly into caller-owned storage.
#[derive(Debug)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct DecodedGenericRegionDirect {
    pub(crate) header: GenericRegionHeader,
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

/// Test-only full-raster oracle for the rolling nominal template-0 decoder.
#[cfg(test)]
#[derive(Debug)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
struct GenericWordRaster {
    width: u32,
    height: u32,
    stride_words: usize,
    words: Vec<u32>,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[allow(clippy::cast_possible_truncation)]
pub(crate) fn word_byte(word: u32, index: usize) -> u8 {
    match index {
        0 => (word >> 24) as u8,
        1 => (word >> 16) as u8,
        2 => (word >> 8) as u8,
        _ => word as u8,
    }
}

#[cfg(test)]
impl GenericWordRaster {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out.is_some() ==> {
            let raster = out.unwrap();
            &&& raster.width == width
            &&& raster.height == height
            &&& raster.words@.len() == raster.stride_words as nat * height as nat
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    fn try_new(width: u32, height: u32) -> Option<Self> {
        let stride_u32 = width.checked_add(31)? / 32;
        let stride_words = usize::try_from(stride_u32).ok()?;
        let height_usize = usize::try_from(height).ok()?;
        let word_count = stride_words.checked_mul(height_usize)?;
        let mut words = Vec::new();
        if words.try_reserve(word_count).is_err() {
            return None;
        }
        words.resize(word_count, 0);
        Some(Self {
            width,
            height,
            stride_words,
            words,
        })
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn row_start(&self, row: i64) -> Option<usize> {
        if row < 0 || row > i64::from(u32::MAX) {
            return None;
        }
        let row = u32::try_from(row).ok()?;
        if row >= self.height {
            return None;
        }
        usize::try_from(row).ok()?.checked_mul(self.stride_words)
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn pixel(&self, x: i64, y: i64) -> bool {
        if x < 0 || x >= i64::from(self.width) {
            return false;
        }
        let Some(row_start) = self.row_start(y) else {
            return false;
        };
        let Ok(x) = u32::try_from(x) else {
            return false;
        };
        let Ok(word_column) = usize::try_from(x / 32) else {
            return false;
        };
        let Some(index) = row_start.checked_add(word_column) else {
            return false;
        };
        let Some(word) = self.words.get(index) else {
            return false;
        };
        *word & (0x8000_0000_u32 >> (x % 32)) != 0
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(
        requires count <= 7,
    ))]
    fn row_bits(&self, row: i64, start_x: i64, count: u8) -> usize {
        let mut bits = 0_usize;
        let mut index = 0_u8;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                index <= count,
                count <= 7,
            decreases count - index,
        ))]
        while index < count {
            let Some(sample_x) = start_x.checked_add(i64::from(index)) else {
                return bits;
            };
            bits = (bits << 1) | usize::from(self.pixel(sample_x, row));
            index += 1;
        }
        bits
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn row_word(&self, row_start: Option<usize>, x: u64) -> u32 {
        let Some(row_start) = row_start else {
            return 0;
        };
        let word_column_u64 = x / 32;
        let Ok(word_column) = usize::try_from(word_column_u64) else {
            return 0;
        };
        if word_column >= self.stride_words {
            return 0;
        }
        let Some(first_index) = row_start.checked_add(word_column) else {
            return 0;
        };
        let Some(first) = self.words.get(first_index) else {
            return 0;
        };
        let first = *first;
        let shift = (x % 32) as u32;
        if shift == 0 {
            return first;
        }
        let second = match word_column.checked_add(1) {
            Some(column) if column < self.stride_words => match row_start.checked_add(column) {
                Some(index) if index < self.words.len() => self.words[index],
                _ => 0,
            },
            _ => 0,
        };
        (first << shift) | (second >> (32 - shift))
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures
            final(self).width == old(self).width,
            final(self).height == old(self).height,
            final(self).stride_words == old(self).stride_words,
    ))]
    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn set_black_in_row(&mut self, row_start: usize, x: u32) -> Result<(), GenericRegionError> {
        let column = usize::try_from(x / 32).map_err(|_error| integer_overflow())?;
        let index = row_start.checked_add(column).ok_or_else(integer_overflow)?;
        let Some(word) = self.words.get_mut(index) else {
            return Err(integer_overflow());
        };
        *word |= 0x8000_0000_u32 >> (x % 32);
        Ok(())
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures
            final(self).width == old(self).width,
            final(self).height == old(self).height,
            final(self).stride_words == old(self).stride_words,
    ))]
    fn copy_row(&mut self, source: u32, destination: u32) -> Result<(), GenericRegionError> {
        if source >= self.height || destination >= self.height {
            return Err(integer_overflow());
        }
        let source = usize::try_from(source).map_err(|_error| integer_overflow())?;
        let destination = usize::try_from(destination).map_err(|_error| integer_overflow())?;
        let source_start = source
            .checked_mul(self.stride_words)
            .ok_or_else(integer_overflow)?;
        let destination_start = destination
            .checked_mul(self.stride_words)
            .ok_or_else(integer_overflow)?;
        let source_end = source_start
            .checked_add(self.stride_words)
            .ok_or_else(integer_overflow)?;
        let destination_end = destination_start
            .checked_add(self.stride_words)
            .ok_or_else(integer_overflow)?;
        if source_end > self.words.len() || destination_end > self.words.len() {
            return Err(integer_overflow());
        }
        #[cfg(not(verus_keep_ghost))]
        {
            self.words
                .copy_within(source_start..source_end, destination_start);
        }
        #[cfg(verus_keep_ghost)]
        let mut column = 0_usize;
        #[cfg(verus_keep_ghost)]
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant column <= self.stride_words,
            decreases self.stride_words - column,
        ))]
        #[cfg(verus_keep_ghost)]
        while column < self.stride_words {
            let source_index = source_start
                .checked_add(column)
                .ok_or_else(integer_overflow)?;
            let destination_index = destination_start
                .checked_add(column)
                .ok_or_else(integer_overflow)?;
            let Some(value) = self.words.get(source_index) else {
                return Err(integer_overflow());
            };
            let value = *value;
            let Some(word) = self.words.get_mut(destination_index) else {
                return Err(integer_overflow());
            };
            *word = value;
            column += 1;
        }
        Ok(())
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            crate::spec::bitmap::storage_valid(old(bitmap)),
            old(bitmap).width == self.width,
            old(bitmap).height == self.height,
        ensures match out {
            Ok(()) =>
                crate::spec::bitmap::storage_valid(final(bitmap))
                && final(bitmap).width == old(bitmap).width
                && final(bitmap).height == old(bitmap).height
                && final(bitmap).stride == old(bitmap).stride,
            Err(_) => true,
        },
    ))]
    #[allow(clippy::cast_possible_truncation, clippy::too_many_lines)]
    fn materialize(&self, bitmap: &mut Bitmap) -> Result<(), GenericRegionError> {
        if bitmap.width != self.width || bitmap.height != self.height {
            return Err(integer_overflow());
        }
        #[cfg(verus_keep_ghost)]
        let bitmap_width = bitmap.width;
        #[cfg(verus_keep_ghost)]
        let bitmap_height = bitmap.height;
        #[cfg(verus_keep_ghost)]
        let bitmap_stride = bitmap.stride;
        #[cfg(verus_keep_ghost)]
        let bitmap_length = bitmap.data.len();
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::views::bitmap_fields(bitmap);
            reveal(crate::spec::bitmap::storage_valid);
            assert(bitmap_length as nat
                == bitmap_stride as nat * bitmap_height as nat);
        }
        let mut y = 0_u32;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                y <= self.height,
                bitmap.width == self.width,
                bitmap.height == self.height,
                bitmap.width == bitmap_width,
                bitmap.height == bitmap_height,
                bitmap.stride == bitmap_stride,
                bitmap.data@.len() == bitmap_length,
                bitmap_length as nat
                    == bitmap_stride as nat * bitmap_height as nat,
            decreases self.height - y,
        ))]
        while y < self.height {
            let row = usize::try_from(y).map_err(|_error| integer_overflow())?;
            let word_start = row
                .checked_mul(self.stride_words)
                .ok_or_else(integer_overflow)?;
            let byte_start = row
                .checked_mul(bitmap.stride)
                .ok_or_else(integer_overflow)?;
            let full_words = bitmap.stride / 4;
            let mut word_column = 0_usize;
            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant
                    word_column <= full_words,
                    bitmap.width == self.width,
                    bitmap.height == self.height,
                    bitmap.width == bitmap_width,
                    bitmap.height == bitmap_height,
                    bitmap.stride == bitmap_stride,
                    bitmap.data@.len() == bitmap_length,
                    bitmap_length as nat
                        == bitmap_stride as nat * bitmap_height as nat,
                decreases full_words - word_column,
            ))]
            while word_column < full_words {
                let word_index = word_start
                    .checked_add(word_column)
                    .ok_or_else(integer_overflow)?;
                let Some(word) = self.words.get(word_index) else {
                    return Err(integer_overflow());
                };
                let word = *word;
                let byte_index = byte_start
                    .checked_add(word_column.checked_mul(4).ok_or_else(integer_overflow)?)
                    .ok_or_else(integer_overflow)?;
                let Some(byte_end) = byte_index.checked_add(4) else {
                    return Err(integer_overflow());
                };
                #[cfg(not(verus_keep_ghost))]
                {
                    let Some(bytes) = bitmap.data.get_mut(byte_index..byte_end) else {
                        return Err(integer_overflow());
                    };
                    bytes.copy_from_slice(&[
                        (word >> 24) as u8,
                        (word >> 16) as u8,
                        (word >> 8) as u8,
                        word as u8,
                    ]);
                }
                #[cfg(verus_keep_ghost)]
                {
                    let mut offset = 0_usize;
                    #[cfg_attr(verus_keep_ghost, verus_spec(
                        invariant
                            offset <= 4,
                            bitmap.width == bitmap_width,
                            bitmap.height == bitmap_height,
                            bitmap.stride == bitmap_stride,
                            bitmap.data@.len() == bitmap_length,
                            bitmap_length as nat
                                == bitmap_stride as nat * bitmap_height as nat,
                        decreases 4 - offset,
                    ))]
                    while offset < 4 {
                        let Some(offset_index) = byte_index.checked_add(offset) else {
                            return Err(integer_overflow());
                        };
                        let Some(byte) = bitmap.data.get_mut(offset_index) else {
                            return Err(integer_overflow());
                        };
                        *byte = word_byte(word, offset);
                        offset += 1;
                    }
                }
                word_column += 1;
            }
            let tail_start = full_words.checked_mul(4).ok_or_else(integer_overflow)?;
            let tail_length = bitmap.stride - tail_start;
            let mut tail_offset = 0_usize;
            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant
                    tail_offset <= tail_length,
                    tail_length <= 3,
                    bitmap.width == self.width,
                    bitmap.height == self.height,
                    bitmap.width == bitmap_width,
                    bitmap.height == bitmap_height,
                    bitmap.stride == bitmap_stride,
                    bitmap.data@.len() == bitmap_length,
                    bitmap_length as nat
                        == bitmap_stride as nat * bitmap_height as nat,
                decreases tail_length - tail_offset,
            ))]
            while tail_offset < tail_length {
                let word_index = word_start
                    .checked_add(full_words)
                    .ok_or_else(integer_overflow)?;
                let Some(word) = self.words.get(word_index) else {
                    return Err(integer_overflow());
                };
                let word = *word;
                let Some(byte_column) = tail_start.checked_add(tail_offset) else {
                    return Err(integer_overflow());
                };
                let byte_index = byte_start
                    .checked_add(byte_column)
                    .ok_or_else(integer_overflow)?;
                let Some(byte) = bitmap.data.get_mut(byte_index) else {
                    return Err(integer_overflow());
                };
                *byte = word_byte(word, tail_offset);
                tail_offset += 1;
            }
            y += 1;
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::views::bitmap_fields(bitmap);
            reveal(crate::spec::bitmap::storage_valid);
        }
        Ok(())
    }
}

/// Minimal MSB-first rolling rows for nominal template-0 arithmetic decoding.
///
/// At most the current row and its two predecessors are retained. Completed
/// rows are written immediately into the public byte-packed bitmap.
#[cfg(test)]
#[derive(Debug)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
struct GenericRowRing {
    width: u32,
    stride_words: usize,
    row_slots: usize,
    words: Vec<u32>,
    charged_bytes: u64,
}

#[cfg(test)]
impl GenericRowRing {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out.is_some() ==> {
            let ring = out.unwrap();
            &&& ring.width == width
            &&& ring.words@.len() == ring.stride_words as nat * ring.row_slots as nat
            &&& ring.row_slots == (if rows < 3 { rows } else { 3 }) as usize
            &&& ring.charged_bytes <= available_bytes
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    fn try_new(width: u32, rows: u32, available_bytes: u64) -> Option<Self> {
        let stride_u32 = width.checked_add(31)? / 32;
        let stride_words = usize::try_from(stride_u32).ok()?;
        let row_slots = usize::try_from(rows.min(3)).ok()?;
        let word_count = stride_words.checked_mul(row_slots)?;
        let requested_bytes = u64::try_from(word_count).ok()?.checked_mul(4)?;
        if requested_bytes > available_bytes {
            return None;
        }
        let mut words = Vec::new();
        if words.try_reserve(word_count).is_err() {
            return None;
        }
        #[cfg(not(verus_keep_ghost))]
        let charged_bytes = u64::try_from(words.capacity()).ok()?.checked_mul(4)?;
        #[cfg(verus_keep_ghost)]
        let charged_bytes = requested_bytes;
        if charged_bytes > available_bytes {
            return None;
        }
        words.resize(word_count, 0);
        Some(Self {
            width,
            stride_words,
            row_slots,
            words,
            charged_bytes,
        })
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn slot_for_row(&self, row: u32) -> Option<usize> {
        if self.row_slots == 0 {
            return None;
        }
        usize::try_from(row).ok().map(|row| row % self.row_slots)
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn row_start(&self, slot: Option<usize>) -> Option<usize> {
        let slot = slot?;
        if slot >= self.row_slots {
            return None;
        }
        slot.checked_mul(self.stride_words)
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn pixel(&self, slot: Option<usize>, x: i64) -> bool {
        if x < 0 || x >= i64::from(self.width) {
            return false;
        }
        let Some(row_start) = self.row_start(slot) else {
            return false;
        };
        let Ok(x) = u32::try_from(x) else {
            return false;
        };
        let Ok(word_column) = usize::try_from(x / 32) else {
            return false;
        };
        let Some(index) = row_start.checked_add(word_column) else {
            return false;
        };
        let Some(word) = self.words.get(index) else {
            return false;
        };
        *word & (0x8000_0000_u32 >> (x % 32)) != 0
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(
        requires count <= 7,
    ))]
    fn row_bits(&self, slot: Option<usize>, start_x: i64, count: u8) -> usize {
        let mut bits = 0_usize;
        let mut index = 0_u8;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                index <= count,
                count <= 7,
            decreases count - index,
        ))]
        while index < count {
            let Some(sample_x) = start_x.checked_add(i64::from(index)) else {
                return bits;
            };
            bits = (bits << 1) | usize::from(self.pixel(slot, sample_x));
            index += 1;
        }
        bits
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn row_word(&self, row_start: Option<usize>, x: u64) -> u32 {
        let Some(row_start) = row_start else {
            return 0;
        };
        let Ok(word_column) = usize::try_from(x / 32) else {
            return 0;
        };
        if word_column >= self.stride_words {
            return 0;
        }
        let Some(first_index) = row_start.checked_add(word_column) else {
            return 0;
        };
        let Some(first) = self.words.get(first_index) else {
            return 0;
        };
        let first = *first;
        let shift = (x % 32) as u32;
        if shift == 0 {
            return first;
        }
        let second = match word_column.checked_add(1) {
            Some(column) if column < self.stride_words => match row_start.checked_add(column) {
                Some(index) if index < self.words.len() => self.words[index],
                _ => 0,
            },
            _ => 0,
        };
        (first << shift) | (second >> (32 - shift))
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures
            final(self).width == old(self).width,
            final(self).stride_words == old(self).stride_words,
            final(self).row_slots == old(self).row_slots,
            final(self).charged_bytes == old(self).charged_bytes,
            out.is_ok() ==>
                final(self).words@.len() == old(self).words@.len(),
    ))]
    fn clear_row(&mut self, slot: usize) -> Result<(), GenericRegionError> {
        let start = self.row_start(Some(slot)).ok_or_else(integer_overflow)?;
        let end = start
            .checked_add(self.stride_words)
            .ok_or_else(integer_overflow)?;
        let Some(row) = self.words.get_mut(start..end) else {
            return Err(integer_overflow());
        };
        let mut column = 0_usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant column <= row@.len(),
            decreases row@.len() - column,
        ))]
        while column < row.len() {
            row[column] = 0;
            column += 1;
        }
        Ok(())
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures
            final(self).width == old(self).width,
            final(self).stride_words == old(self).stride_words,
            final(self).row_slots == old(self).row_slots,
            final(self).charged_bytes == old(self).charged_bytes,
            out.is_ok() ==>
                final(self).words@.len() == old(self).words@.len(),
    ))]
    fn copy_row(&mut self, source: usize, destination: usize) -> Result<(), GenericRegionError> {
        let source_start = self.row_start(Some(source)).ok_or_else(integer_overflow)?;
        let destination_start = self
            .row_start(Some(destination))
            .ok_or_else(integer_overflow)?;
        let source_end = source_start
            .checked_add(self.stride_words)
            .ok_or_else(integer_overflow)?;
        if source_end > self.words.len() {
            return Err(integer_overflow());
        }
        #[cfg(verus_keep_ghost)]
        let word_length = self.words.len();
        #[cfg(not(verus_keep_ghost))]
        self.words
            .copy_within(source_start..source_end, destination_start);
        #[cfg(verus_keep_ghost)]
        let mut column = 0_usize;
        #[cfg(verus_keep_ghost)]
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                column <= self.stride_words,
                self.words@.len() == word_length,
            decreases self.stride_words - column,
        ))]
        #[cfg(verus_keep_ghost)]
        while column < self.stride_words {
            let source_index = source_start
                .checked_add(column)
                .ok_or_else(integer_overflow)?;
            let destination_index = destination_start
                .checked_add(column)
                .ok_or_else(integer_overflow)?;
            let Some(value) = self.words.get(source_index) else {
                return Err(integer_overflow());
            };
            let value = *value;
            let Some(word) = self.words.get_mut(destination_index) else {
                return Err(integer_overflow());
            };
            *word = value;
            column += 1;
        }
        Ok(())
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures
            final(self).width == old(self).width,
            final(self).stride_words == old(self).stride_words,
            final(self).row_slots == old(self).row_slots,
            final(self).charged_bytes == old(self).charged_bytes,
            out.is_ok() ==>
                final(self).words@.len() == old(self).words@.len(),
    ))]
    fn set_black_in_row(&mut self, row_start: usize, x: u32) -> Result<(), GenericRegionError> {
        let column = usize::try_from(x / 32).map_err(|_error| integer_overflow())?;
        let index = row_start.checked_add(column).ok_or_else(integer_overflow)?;
        let Some(word) = self.words.get_mut(index) else {
            return Err(integer_overflow());
        };
        *word |= 0x8000_0000_u32 >> (x % 32);
        Ok(())
    }

    #[allow(clippy::cast_possible_truncation, clippy::too_many_lines)]
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            crate::spec::bitmap::storage_valid(old(bitmap)),
            old(bitmap).width == self.width,
        ensures match out {
            Ok(()) =>
                crate::spec::bitmap::storage_valid(final(bitmap))
                && final(bitmap).width == old(bitmap).width
                && final(bitmap).height == old(bitmap).height
                && final(bitmap).stride == old(bitmap).stride,
            Err(_) => true,
        },
    ))]
    fn flush_row(
        &self,
        slot: usize,
        bitmap: &mut Bitmap,
        y: u32,
    ) -> Result<(), GenericRegionError> {
        if bitmap.width != self.width || y >= bitmap.height {
            return Err(integer_overflow());
        }
        #[cfg(verus_keep_ghost)]
        let bitmap_width = bitmap.width;
        #[cfg(verus_keep_ghost)]
        let bitmap_height = bitmap.height;
        #[cfg(verus_keep_ghost)]
        let bitmap_stride = bitmap.stride;
        #[cfg(verus_keep_ghost)]
        let bitmap_length = bitmap.data.len();
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::views::bitmap_fields(bitmap);
            reveal(crate::spec::bitmap::storage_valid);
            assert(bitmap_length as nat
                == bitmap_stride as nat * bitmap_height as nat);
        }
        let word_start = self.row_start(Some(slot)).ok_or_else(integer_overflow)?;
        let row = usize::try_from(y).map_err(|_error| integer_overflow())?;
        let byte_start = row
            .checked_mul(bitmap.stride)
            .ok_or_else(integer_overflow)?;
        let full_words = bitmap.stride / 4;
        let mut word_column = 0_usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                word_column <= full_words,
                bitmap.width == bitmap_width,
                bitmap.height == bitmap_height,
                bitmap.stride == bitmap_stride,
                bitmap.data@.len() == bitmap_length,
                bitmap_length as nat
                    == bitmap_stride as nat * bitmap_height as nat,
            decreases full_words - word_column,
        ))]
        while word_column < full_words {
            let word = *self
                .words
                .get(
                    word_start
                        .checked_add(word_column)
                        .ok_or_else(integer_overflow)?,
                )
                .ok_or_else(integer_overflow)?;
            let byte_column = word_column.checked_mul(4).ok_or_else(integer_overflow)?;
            let byte_index = byte_start
                .checked_add(byte_column)
                .ok_or_else(integer_overflow)?;
            let byte_end = byte_index.checked_add(4).ok_or_else(integer_overflow)?;
            #[cfg(not(verus_keep_ghost))]
            {
                let Some(bytes) = bitmap.data.get_mut(byte_index..byte_end) else {
                    return Err(integer_overflow());
                };
                bytes.copy_from_slice(&[
                    (word >> 24) as u8,
                    (word >> 16) as u8,
                    (word >> 8) as u8,
                    word as u8,
                ]);
            }
            #[cfg(verus_keep_ghost)]
            {
                let mut offset = 0_usize;
                #[cfg_attr(verus_keep_ghost, verus_spec(
                    invariant
                        offset <= 4,
                        bitmap.width == bitmap_width,
                        bitmap.height == bitmap_height,
                        bitmap.stride == bitmap_stride,
                        bitmap.data@.len() == bitmap_length,
                        bitmap_length as nat
                            == bitmap_stride as nat * bitmap_height as nat,
                    decreases 4 - offset,
                ))]
                while offset < 4 {
                    let Some(offset_index) = byte_index.checked_add(offset) else {
                        return Err(integer_overflow());
                    };
                    let Some(byte) = bitmap.data.get_mut(offset_index) else {
                        return Err(integer_overflow());
                    };
                    *byte = word_byte(word, offset);
                    offset += 1;
                }
            }
            word_column += 1;
        }
        let tail_start = full_words.checked_mul(4).ok_or_else(integer_overflow)?;
        let tail_length = bitmap.stride - tail_start;
        let mut tail = 0_usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                tail <= tail_length,
                tail_length <= 3,
                bitmap.width == bitmap_width,
                bitmap.height == bitmap_height,
                bitmap.stride == bitmap_stride,
                bitmap.data@.len() == bitmap_length,
                bitmap_length as nat
                    == bitmap_stride as nat * bitmap_height as nat,
            decreases tail_length - tail,
        ))]
        while tail < tail_length {
            let word = *self
                .words
                .get(
                    word_start
                        .checked_add(full_words)
                        .ok_or_else(integer_overflow)?,
                )
                .ok_or_else(integer_overflow)?;
            let byte_column = tail_start.checked_add(tail).ok_or_else(integer_overflow)?;
            let byte_index = byte_start
                .checked_add(byte_column)
                .ok_or_else(integer_overflow)?;
            let Some(byte) = bitmap.data.get_mut(byte_index) else {
                return Err(integer_overflow());
            };
            *byte = word_byte(word, tail);
            tail += 1;
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::views::bitmap_fields(bitmap);
            reveal(crate::spec::bitmap::storage_valid);
        }
        Ok(())
    }
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

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            generic_region_spec::generic_session_valid(old(self)),
            crate::spec::bitmap::storage_valid(old(bitmap)),
            bitmap.width == call.width,
            bitmap.height == call.height,
        ensures
            out.is_ok() ==> crate::spec::bitmap::storage_valid(final(bitmap)),
            out.is_ok() ==> generic_region_spec::generic_session_valid(final(self)),
    ))]
    fn decode_bitmap_rows_into(
        &mut self,
        call: GenericArithmeticCall<'_>,
        rows: u32,
        bitmap: &mut Bitmap,
    ) -> Result<(), GenericRegionError> {
        decode_generic_on_into(
            &mut self.decoder,
            &mut self.contexts,
            call,
            rows,
            &self.limits,
            &mut self.work_used,
            bitmap,
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

/// Decode an arithmetic generic region directly into a matching, caller-owned
/// blank bitmap. Unsupported encodings return `Ok(None)` before modification.
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires crate::spec::bitmap::storage_valid(old(bitmap)),
    ensures out.is_ok() ==> crate::spec::bitmap::storage_valid(final(bitmap)),
))]
pub(crate) fn decode_generic_region_segment_into(
    payload: &[u8],
    unknown_length: bool,
    limits: &Limits,
    bitmap: &mut Bitmap,
) -> Result<Option<DecodedGenericRegionDirect>, GenericRegionError> {
    let (header, header_length) = parse_generic_region_header(payload, limits)?;
    if header.mmr
        || bitmap.width() != header.region.width
        || bitmap.height() != header.region.height
    {
        return Ok(None);
    }
    let coded_and_following = payload
        .get(header_length..)
        .ok_or(MalformedError::IntegerOverflow)?;
    let (rows, coded_consumed) = if unknown_length {
        let terminal =
            split_unknown_length_data(coded_and_following, false, header.region.height, limits)?;
        decode_arithmetic_generic_into(terminal.coded, &header, terminal.rows, limits, bitmap)?;
        (terminal.rows, terminal.consumed)
    } else {
        decode_known_length_arithmetic_generic_into(
            coded_and_following,
            &header,
            header.region.height,
            limits,
            bitmap,
        )?;
        (header.region.height, coded_and_following.len())
    };
    let consumed_bytes = header_length
        .checked_add(coded_consumed)
        .ok_or(MalformedError::IntegerOverflow)?;
    Ok(Some(DecodedGenericRegionDirect {
        header,
        consumed_bytes,
        decoded_rows: rows,
    }))
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

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires crate::spec::bitmap::storage_valid(old(bitmap)),
    ensures out.is_ok() ==> crate::spec::bitmap::storage_valid(final(bitmap)),
))]
fn decode_known_length_arithmetic_generic_into(
    coded: &[u8],
    header: &GenericRegionHeader,
    rows: u32,
    limits: &Limits,
    bitmap: &mut Bitmap,
) -> Result<(), GenericRegionError> {
    let has_terminator = coded.len() >= 2
        && coded[coded.len() - 2] == ARITHMETIC_TERMINATOR[0]
        && coded[coded.len() - 1] == ARITHMETIC_TERMINATOR[1];
    if has_terminator {
        return decode_arithmetic_generic_into(coded, header, rows, limits, bitmap);
    }
    if coded.last() != Some(&0xff) {
        return Err(GenericRegionError::InvalidArithmeticTerminator);
    }
    let requested_bytes = u64::try_from(coded.len())
        .map_err(|_error| MalformedError::IntegerOverflow)?
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
    let capacity = coded
        .len()
        .checked_add(1)
        .ok_or(MalformedError::IntegerOverflow)?;
    let mut normalized = Vec::new();
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
    decode_arithmetic_generic_into(&normalized, header, rows, limits, bitmap)
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

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires crate::spec::bitmap::storage_valid(old(bitmap)),
    ensures out.is_ok() ==> crate::spec::bitmap::storage_valid(final(bitmap)),
))]
fn decode_arithmetic_generic_into(
    coded: &[u8],
    header: &GenericRegionHeader,
    rows: u32,
    limits: &Limits,
    bitmap: &mut Bitmap,
) -> Result<(), GenericRegionError> {
    if header.mmr
        || rows > header.region.height
        || bitmap.width() != header.region.width
        || bitmap.height() != header.region.height
    {
        return Err(GenericRegionError::InvalidFlags(0x01));
    }
    validate_arithmetic_terminator(coded)?;
    validate_generic_adaptive_values(header.template, header.extended_template, &header.adaptive)?;
    enforce_generic_work(header.region.width, rows, header.typical_prediction, limits)?;
    let mut session =
        GenericArithmeticSession::new(coded, header.template, header.extended_template, limits)?;
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::views::bitmap_fields(bitmap);
    }
    session.decode_bitmap_rows_into(
        GenericArithmeticCall {
            width: header.region.width,
            height: header.region.height,
            adaptive: header.adaptive,
            typical_prediction: header.typical_prediction,
            skip: None,
        },
        rows,
        bitmap,
    )?;
    session.validate_final_marker()?;
    Ok(())
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
#[allow(clippy::collapsible_if, clippy::too_many_lines)]
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

    let descriptor = GenericContextDescriptor::new(
        contexts.template,
        contexts.extended_template,
        &call.adaptive,
    );
    if matches!(descriptor, GenericContextDescriptor::NominalTemplate0) && call.skip.is_none() {
        if generic_row_ring::try_decode(
            &mut bitmap,
            call.width,
            rows,
            contexts.storage_bytes,
            bitmap_bytes,
            limits,
            call.typical_prediction,
            decoder,
            &mut contexts.contexts,
        )? {
            #[cfg(verus_keep_ghost)]
            proof! {
                reveal(generic_region_spec::generic_contexts_valid);
            }
            return Ok(bitmap);
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal(generic_region_spec::generic_contexts_valid);
        }
    }

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
        crate::spec::bitmap::storage_valid(old(bitmap)),
        bitmap.width == call.width,
        bitmap.height == call.height,
    ensures
        out.is_ok() ==> crate::spec::bitmap::storage_valid(final(bitmap)),
        out.is_ok() ==>
            crate::arithmetic::arithmetic_spec::decoder_valid(final(decoder))
            && generic_region_spec::generic_contexts_valid(final(contexts)),
))]
#[allow(
    clippy::collapsible_if,
    clippy::too_many_arguments,
    clippy::too_many_lines
)]
fn decode_generic_on_into(
    decoder: &mut ArithmeticDecoder<'_>,
    contexts: &mut GenericContexts,
    call: GenericArithmeticCall<'_>,
    rows: u32,
    limits: &Limits,
    cumulative_work: &mut u64,
    bitmap: &mut Bitmap,
) -> Result<(), GenericRegionError> {
    if rows > call.height || bitmap.width() != call.width || bitmap.height() != call.height {
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
    let call_work = generic_work(call.width, rows, call.typical_prediction)?;
    let next_work = cumulative_work
        .checked_add(call_work)
        .ok_or(MalformedError::IntegerOverflow)?;
    enforce_work(next_work, limits)?;
    let bitmap_bytes = bitmap_storage_bytes(call.width, call.height)?;
    enforce_managed_memory(contexts.storage_bytes, bitmap_bytes, limits)?;
    *cumulative_work = next_work;

    let descriptor = GenericContextDescriptor::new(
        contexts.template,
        contexts.extended_template,
        &call.adaptive,
    );
    if matches!(descriptor, GenericContextDescriptor::NominalTemplate0) {
        if generic_row_ring::try_decode(
            bitmap,
            call.width,
            rows,
            contexts.storage_bytes,
            bitmap_bytes,
            limits,
            call.typical_prediction,
            decoder,
            &mut contexts.contexts,
        )? {
            return Ok(());
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal(generic_region_spec::generic_contexts_valid);
        }
    }
    decode_generic_with(
        bitmap,
        rows,
        contexts.template,
        contexts.extended_template,
        &call.adaptive,
        call.typical_prediction,
        None,
        decoder,
        &mut contexts.contexts,
    )
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

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) const RASTER_SIGNAL_CONTEXT: u8 = 4;

#[allow(clippy::ptr_arg)]
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        crate::arithmetic::arithmetic_spec::decoder_valid(old(decoder)),
        generic_region_spec::contexts_valid(old(contexts)@),
    ensures
        out <= RASTER_SIGNAL_CONTEXT,
        generic_region_spec::contexts_valid(final(contexts)@),
        out <= 1 ==> crate::arithmetic::arithmetic_spec::decoder_valid(final(decoder)),
))]
pub(crate) fn decode_probability_signal(
    decoder: &mut ArithmeticDecoder<'_>,
    contexts: &mut Vec<ArithmeticContext>,
    index: usize,
) -> u8 {
    let Some(context) = contexts.get_mut(index) else {
        return RASTER_SIGNAL_CONTEXT;
    };
    decoder.decode_signal(context)
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[cold]
pub(crate) fn probability_signal_error(
    decoder: &ArithmeticDecoder<'_>,
    index: usize,
    context_count: usize,
    signal: u8,
) -> GenericRegionError {
    if signal == RASTER_SIGNAL_CONTEXT {
        GenericRegionError::ContextIndex {
            index,
            count: context_count,
        }
    } else {
        GenericRegionError::Decode(decoder.signal_error(signal).into_decode())
    }
}

#[allow(clippy::ptr_arg)]
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        crate::spec::bitmap::storage_valid(old(bitmap)),
        rows <= old(bitmap).height,
        old(ring).width == old(bitmap).width,
        crate::arithmetic::arithmetic_spec::decoder_valid(old(decoder)),
        generic_region_spec::contexts_valid(old(contexts)@),
    ensures
        out.is_ok() ==> crate::spec::bitmap::storage_valid(final(bitmap)),
        generic_region_spec::contexts_valid(final(contexts)@),
        out.is_ok() ==> crate::arithmetic::arithmetic_spec::decoder_valid(final(decoder)),
))]
#[allow(clippy::too_many_lines)]
#[cfg(test)]
fn decode_nominal_template0_row_ring(
    ring: &mut GenericRowRing,
    bitmap: &mut Bitmap,
    rows: u32,
    typical_prediction: bool,
    decoder: &mut ArithmeticDecoder<'_>,
    contexts: &mut Vec<ArithmeticContext>,
) -> Result<(), GenericRegionError> {
    if rows > bitmap.height || ring.width != bitmap.width {
        return Err(GenericRegionError::RowCountMismatch {
            expected: bitmap.height,
            actual: rows,
        });
    }
    let mut ltp = false;
    let mut y = 0_u32;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            y <= rows,
            rows <= bitmap.height,
            ring.width == bitmap.width,
            crate::spec::bitmap::storage_valid(bitmap),
            crate::arithmetic::arithmetic_spec::decoder_valid(decoder),
            generic_region_spec::contexts_valid(contexts@),
        decreases rows - y,
    ))]
    while y < rows {
        let current_slot = ring.slot_for_row(y).ok_or_else(integer_overflow)?;
        ring.clear_row(current_slot)?;
        let current_row = ring
            .row_start(Some(current_slot))
            .ok_or_else(integer_overflow)?;
        if typical_prediction {
            let context = GenericTemplate::Template0.sltp_context();
            let sltp = decode_probability_signal(decoder, contexts, context);
            if sltp > 1 {
                return Err(probability_signal_error(
                    decoder,
                    context,
                    contexts.len(),
                    sltp,
                ));
            }
            ltp ^= sltp != 0;
        }
        if ltp {
            if y != 0 {
                let previous_slot = ring.slot_for_row(y - 1).ok_or_else(integer_overflow)?;
                ring.copy_row(previous_slot, current_slot)?;
            }
            ring.flush_row(current_slot, bitmap, y)?;
            y += 1;
            continue;
        }

        let top_slot = y.checked_sub(2).and_then(|row| ring.slot_for_row(row));
        let previous_slot = y.checked_sub(1).and_then(|row| ring.slot_for_row(row));
        let top_row = ring.row_start(top_slot);
        let previous_row = ring.row_start(previous_slot);
        let mut history = 0_usize;
        let mut top = ring.row_bits(top_slot, -2, 5);
        let mut previous = ring.row_bits(previous_slot, -3, 7);
        let mut x = 0_u32;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                x <= ring.width,
                y < rows,
                rows <= bitmap.height,
                ring.width == bitmap.width,
                crate::spec::bitmap::storage_valid(bitmap),
                crate::arithmetic::arithmetic_spec::decoder_valid(decoder),
                generic_region_spec::contexts_valid(contexts@),
            decreases ring.width - x,
        ))]
        while x < ring.width {
            let mut top_queue = ring.row_word(top_row, u64::from(x) + 3);
            let mut previous_queue = ring.row_word(previous_row, u64::from(x) + 4);
            let remaining = ring.width - x;
            let chunk_width = remaining.min(32);
            let chunk_end = x.checked_add(chunk_width).ok_or_else(integer_overflow)?;
            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant
                    x <= chunk_end,
                    chunk_end <= ring.width,
                    y < rows,
                    rows <= bitmap.height,
                    ring.width == bitmap.width,
                    crate::spec::bitmap::storage_valid(bitmap),
                    crate::arithmetic::arithmetic_spec::decoder_valid(decoder),
                    generic_region_spec::contexts_valid(contexts@),
                decreases chunk_end - x,
            ))]
            while x < chunk_end {
                let context = (top << 11) | (previous << 4) | history & 0x0f;
                top = ((top << 1) | usize::from(top_queue & 0x8000_0000 != 0)) & 0x1f;
                previous =
                    ((previous << 1) | usize::from(previous_queue & 0x8000_0000 != 0)) & 0x7f;
                top_queue <<= 1;
                previous_queue <<= 1;

                let signal = decode_probability_signal(decoder, contexts, context);
                if signal > 1 {
                    return Err(probability_signal_error(
                        decoder,
                        context,
                        contexts.len(),
                        signal,
                    ));
                }
                history = ((history << 1) | usize::from(signal)) & 0x0f;
                if signal != 0 {
                    ring.set_black_in_row(current_row, x)?;
                }
                x += 1;
            }
        }
        ring.flush_row(current_slot, bitmap, y)?;
        y += 1;
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::ptr_arg)]
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        rows <= old(raster).height,
        crate::arithmetic::arithmetic_spec::decoder_valid(old(decoder)),
        generic_region_spec::contexts_valid(old(contexts)@),
    ensures
        out.is_ok() ==> final(raster).width == old(raster).width,
        out.is_ok() ==> final(raster).height == old(raster).height,
        out.is_ok() ==> final(raster).stride_words == old(raster).stride_words,
        generic_region_spec::contexts_valid(final(contexts)@),
        out.is_ok() ==> crate::arithmetic::arithmetic_spec::decoder_valid(final(decoder)),
))]
#[allow(clippy::too_many_lines)]
fn decode_nominal_template0_words(
    raster: &mut GenericWordRaster,
    rows: u32,
    typical_prediction: bool,
    decoder: &mut ArithmeticDecoder<'_>,
    contexts: &mut Vec<ArithmeticContext>,
) -> Result<(), GenericRegionError> {
    if rows > raster.height {
        return Err(GenericRegionError::RowCountMismatch {
            expected: raster.height,
            actual: rows,
        });
    }
    #[cfg(verus_keep_ghost)]
    let raster_width = raster.width;
    #[cfg(verus_keep_ghost)]
    let raster_height = raster.height;
    #[cfg(verus_keep_ghost)]
    let raster_stride = raster.stride_words;
    let mut ltp = false;
    let mut y = 0_u32;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            y <= rows,
            rows <= raster.height,
            raster.width == raster_width,
            raster.height == raster_height,
            raster.stride_words == raster_stride,
            crate::arithmetic::arithmetic_spec::decoder_valid(decoder),
            generic_region_spec::contexts_valid(contexts@),
        decreases rows - y,
    ))]
    while y < rows {
        if typical_prediction {
            let context = GenericTemplate::Template0.sltp_context();
            let sltp = decode_probability_signal(decoder, contexts, context);
            if sltp > 1 {
                return Err(probability_signal_error(
                    decoder,
                    context,
                    contexts.len(),
                    sltp,
                ));
            }
            ltp ^= sltp != 0;
        }
        if ltp {
            if y != 0 {
                raster.copy_row(y - 1, y)?;
            }
            y += 1;
            continue;
        }

        let mut history = 0_usize;
        let y_i64 = i64::from(y);
        let top_row = raster.row_start(y_i64 - 2);
        let previous_row = raster.row_start(y_i64 - 1);
        let mut top = raster.row_bits(y_i64 - 2, -2, 5);
        let mut previous = raster.row_bits(y_i64 - 1, -3, 7);
        let row = usize::try_from(y).map_err(|_error| integer_overflow())?;
        let row_start = row
            .checked_mul(raster.stride_words)
            .ok_or_else(integer_overflow)?;
        let mut x = 0_u32;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                x <= raster.width,
                y < rows,
                rows <= raster.height,
                raster.width == raster_width,
                raster.height == raster_height,
                raster.stride_words == raster_stride,
                crate::arithmetic::arithmetic_spec::decoder_valid(decoder),
                generic_region_spec::contexts_valid(contexts@),
            decreases raster.width - x,
        ))]
        while x < raster.width {
            #[cfg(verus_keep_ghost)]
            let chunk_start = x;
            let mut top_queue = raster.row_word(top_row, u64::from(x) + 3);
            let mut previous_queue = raster.row_word(previous_row, u64::from(x) + 4);
            let remaining = raster.width - x;
            let chunk_width = remaining.min(32);
            let Some(chunk_end) = x.checked_add(chunk_width) else {
                return Err(integer_overflow());
            };
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(chunk_width > 0);
                assert(chunk_end <= raster.width);
            }
            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant
                    x <= chunk_end,
                    chunk_end <= raster.width,
                    y < rows,
                    rows <= raster.height,
                    raster.width == raster_width,
                    raster.height == raster_height,
                    raster.stride_words == raster_stride,
                    crate::arithmetic::arithmetic_spec::decoder_valid(decoder),
                    generic_region_spec::contexts_valid(contexts@),
                decreases chunk_end - x,
            ))]
            while x < chunk_end {
                let context = (top << 11) | (previous << 4) | history & 0x0f;
                top = ((top << 1) | usize::from(top_queue & 0x8000_0000 != 0)) & 0x1f;
                previous =
                    ((previous << 1) | usize::from(previous_queue & 0x8000_0000 != 0)) & 0x7f;
                top_queue <<= 1;
                previous_queue <<= 1;

                let signal = decode_probability_signal(decoder, contexts, context);
                if signal > 1 {
                    return Err(probability_signal_error(
                        decoder,
                        context,
                        contexts.len(),
                        signal,
                    ));
                }
                history = ((history << 1) | usize::from(signal)) & 0x0f;
                if signal != 0 {
                    raster.set_black_in_row(row_start, x)?;
                }
                x += 1;
            }
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(x == chunk_end);
                assert(x > chunk_start);
            }
        }
        y += 1;
    }
    Ok(())
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
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
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
    let descriptor = GenericContextDescriptor::new(template, extended, adaptive);
    let mut ltp = false;
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
            let context = template.sltp_context();
            let sltp = decode_probability_signal(decoder, contexts, context);
            if sltp > 1 {
                return Err(probability_signal_error(
                    decoder,
                    context,
                    contexts.len(),
                    sltp,
                ));
            }
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

        let mut history = 0_usize;
        let mut row_context = GenericRowContext::new(descriptor, bitmap, y);
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
                Some(mask) => packed_pixel(mask, i64::from(x), i64::from(y)),
                None => false,
            };
            if skipped {
                history = (history << 1) & 0x0f;
                row_context.advance(bitmap, x);
                x += 1;
                continue;
            }
            let context = row_context.context_and_advance(bitmap, x, y, history);
            let signal = decode_probability_signal(decoder, contexts, context);
            if signal > 1 {
                return Err(probability_signal_error(
                    decoder,
                    context,
                    contexts.len(),
                    signal,
                ));
            }
            let value = signal != 0;
            history = ((history << 1) | usize::from(value)) & 0x0f;
            if value {
                bitmap.set_pixel(x, y, true)?;
            }
            x += 1;
        }
        y += 1;
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::too_many_arguments, clippy::ptr_arg)]
fn decode_generic_with_oracle(
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

#[derive(Debug, Clone, Copy)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
enum GenericContextDescriptor {
    NominalTemplate0,
    Template0([TemplateOffset; 4]),
    Template0Extended([TemplateOffset; 16]),
    NominalTemplate1,
    Template1(TemplateOffset),
    NominalTemplate2,
    Template2(TemplateOffset),
    NominalTemplate3,
    Template3(TemplateOffset),
}

impl GenericContextDescriptor {
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn new(template: GenericTemplate, extended: bool, adaptive: &[TemplateOffset; 12]) -> Self {
        match (template, extended) {
            (GenericTemplate::Template0, true) => {
                if *adaptive
                    == [
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
                    ]
                {
                    return Self::NominalTemplate0;
                }
                let mut offsets = [TemplateOffset { x: 0, y: 0 }; 16];
                let mut index = 0_usize;
                #[cfg_attr(verus_keep_ghost, verus_spec(
                    invariant index <= offsets@.len(),
                    decreases offsets@.len() - index,
                ))]
                while index < offsets.len() {
                    let slot = GENERIC_TEMPLATE_0_EXTENDED[index];
                    offsets[index] = match slot.adaptive {
                        Some(number) => generic_adaptive_offset(adaptive, number, slot.nominal),
                        None => slot.nominal,
                    };
                    index += 1;
                }
                Self::Template0Extended(offsets)
            }
            (GenericTemplate::Template0, false) => {
                let values = [adaptive[0], adaptive[1], adaptive[2], adaptive[3]];
                if values
                    == [
                        TemplateOffset::new(3, -1),
                        TemplateOffset::new(-3, -1),
                        TemplateOffset::new(2, -2),
                        TemplateOffset::new(-2, -2),
                    ]
                {
                    Self::NominalTemplate0
                } else {
                    Self::Template0(values)
                }
            }
            (GenericTemplate::Template1, _) => {
                if adaptive[0] == TemplateOffset::new(3, -1) {
                    Self::NominalTemplate1
                } else {
                    Self::Template1(adaptive[0])
                }
            }
            (GenericTemplate::Template2, _) => {
                if adaptive[0] == TemplateOffset::new(2, -1) {
                    Self::NominalTemplate2
                } else {
                    Self::Template2(adaptive[0])
                }
            }
            (GenericTemplate::Template3, _) => {
                if adaptive[0] == TemplateOffset::new(2, -1) {
                    Self::NominalTemplate3
                } else {
                    Self::Template3(adaptive[0])
                }
            }
        }
    }

    #[allow(clippy::similar_names)]
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn context(self, bitmap: &Bitmap, x: u32, y: u32, history: usize) -> usize {
        match self {
            Self::NominalTemplate0 => {
                let top = packed_row_bits(bitmap, i64::from(x), i64::from(y) - 2, -2, 5);
                let previous = packed_row_bits(bitmap, i64::from(x), i64::from(y) - 1, -3, 7);
                (top << 11) | (previous << 4) | history & 0x0f
            }
            Self::Template0(adaptive) => {
                let top_fixed = packed_row_bits(bitmap, i64::from(x), i64::from(y) - 2, -1, 3);
                let previous_fixed = packed_row_bits(bitmap, i64::from(x), i64::from(y) - 1, -2, 5);
                let a4 = usize::from(sample_cached(bitmap, x, y, adaptive[3]));
                let a3 = usize::from(sample_cached(bitmap, x, y, adaptive[2]));
                let a2 = usize::from(sample_cached(bitmap, x, y, adaptive[1]));
                let a1 = usize::from(sample_cached(bitmap, x, y, adaptive[0]));
                let mut context = (a4 << 3) | top_fixed;
                context = (context << 1) | a3;
                context = (context << 1) | a2;
                context = (context << 5) | previous_fixed;
                context = (context << 1) | a1;
                (context << 4) | history & 0x0f
            }
            Self::Template0Extended(offsets) => {
                let mut context = 0_usize;
                let mut index = 0_usize;
                #[cfg_attr(verus_keep_ghost, verus_spec(
                    invariant index <= offsets@.len(),
                    decreases offsets@.len() - index,
                ))]
                while index < offsets.len() {
                    context =
                        (context << 1) | usize::from(sample_cached(bitmap, x, y, offsets[index]));
                    index += 1;
                }
                context
            }
            Self::NominalTemplate1 => {
                let top = packed_row_bits(bitmap, i64::from(x), i64::from(y) - 2, -1, 4);
                let previous = packed_row_bits(bitmap, i64::from(x), i64::from(y) - 1, -2, 6);
                (top << 9) | (previous << 3) | history & 0x07
            }
            Self::Template1(adaptive) => {
                let top = packed_row_bits(bitmap, i64::from(x), i64::from(y) - 2, -1, 4);
                let previous = packed_row_bits(bitmap, i64::from(x), i64::from(y) - 1, -2, 5);
                let adaptive = usize::from(sample_cached(bitmap, x, y, adaptive));
                (((top << 5) | previous) << 4) | (adaptive << 3) | history & 0x07
            }
            Self::NominalTemplate2 => {
                let top = packed_row_bits(bitmap, i64::from(x), i64::from(y) - 2, -1, 3);
                let previous = packed_row_bits(bitmap, i64::from(x), i64::from(y) - 1, -2, 5);
                (top << 7) | (previous << 2) | history & 0x03
            }
            Self::Template2(adaptive) => {
                let top = packed_row_bits(bitmap, i64::from(x), i64::from(y) - 2, -1, 3);
                let previous = packed_row_bits(bitmap, i64::from(x), i64::from(y) - 1, -2, 4);
                let adaptive = usize::from(sample_cached(bitmap, x, y, adaptive));
                (((top << 4) | previous) << 3) | (adaptive << 2) | history & 0x03
            }
            Self::NominalTemplate3 => {
                let previous = packed_row_bits(bitmap, i64::from(x), i64::from(y) - 1, -3, 6);
                (previous << 4) | history & 0x0f
            }
            Self::Template3(adaptive) => {
                let previous = packed_row_bits(bitmap, i64::from(x), i64::from(y) - 1, -3, 5);
                let adaptive = usize::from(sample_cached(bitmap, x, y, adaptive));
                (previous << 5) | (adaptive << 4) | history & 0x0f
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
enum GenericRowContext {
    Template0 {
        top: usize,
        previous: usize,
        top_row: Option<usize>,
        previous_row: Option<usize>,
        top_queue: u8,
        previous_queue: u8,
        remaining: u8,
    },
    Template1 {
        top: usize,
        previous: usize,
        top_row: Option<usize>,
        previous_row: Option<usize>,
        top_queue: u8,
        previous_queue: u8,
        remaining: u8,
    },
    Template2 {
        top: usize,
        previous: usize,
        top_row: Option<usize>,
        previous_row: Option<usize>,
        top_queue: u8,
        previous_queue: u8,
        remaining: u8,
    },
    Template3 {
        previous: usize,
        previous_row: Option<usize>,
        previous_queue: u8,
        remaining: u8,
    },
    Adaptive(GenericContextDescriptor),
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[inline]
fn queue_exhausted(remaining: &mut u8) -> bool {
    if *remaining == 0 {
        return true;
    }
    *remaining -= 1;
    *remaining == 0
}

impl GenericRowContext {
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn new(descriptor: GenericContextDescriptor, bitmap: &Bitmap, y: u32) -> Self {
        let y = i64::from(y);
        match descriptor {
            GenericContextDescriptor::NominalTemplate0 => Self::Template0 {
                top: packed_row_bits(bitmap, 0, y - 2, -2, 5),
                previous: packed_row_bits(bitmap, 0, y - 1, -3, 7),
                top_row: packed_row_start(bitmap, y - 2),
                previous_row: packed_row_start(bitmap, y - 1),
                top_queue: packed_row_byte(bitmap, packed_row_start(bitmap, y - 2), 3),
                previous_queue: packed_row_byte(bitmap, packed_row_start(bitmap, y - 1), 4),
                remaining: 8,
            },
            GenericContextDescriptor::NominalTemplate1 => Self::Template1 {
                top: packed_row_bits(bitmap, 0, y - 2, -1, 4),
                previous: packed_row_bits(bitmap, 0, y - 1, -2, 6),
                top_row: packed_row_start(bitmap, y - 2),
                previous_row: packed_row_start(bitmap, y - 1),
                top_queue: packed_row_byte(bitmap, packed_row_start(bitmap, y - 2), 3),
                previous_queue: packed_row_byte(bitmap, packed_row_start(bitmap, y - 1), 4),
                remaining: 8,
            },
            GenericContextDescriptor::NominalTemplate2 => Self::Template2 {
                top: packed_row_bits(bitmap, 0, y - 2, -1, 3),
                previous: packed_row_bits(bitmap, 0, y - 1, -2, 5),
                top_row: packed_row_start(bitmap, y - 2),
                previous_row: packed_row_start(bitmap, y - 1),
                top_queue: packed_row_byte(bitmap, packed_row_start(bitmap, y - 2), 2),
                previous_queue: packed_row_byte(bitmap, packed_row_start(bitmap, y - 1), 3),
                remaining: 8,
            },
            GenericContextDescriptor::NominalTemplate3 => Self::Template3 {
                previous: packed_row_bits(bitmap, 0, y - 1, -3, 6),
                previous_row: packed_row_start(bitmap, y - 1),
                previous_queue: packed_row_byte(bitmap, packed_row_start(bitmap, y - 1), 3),
                remaining: 8,
            },
            _ => Self::Adaptive(descriptor),
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn context_and_advance(&mut self, bitmap: &Bitmap, x: u32, y: u32, history: usize) -> usize {
        let sample_x = i64::from(x);
        match self {
            Self::Template0 {
                top,
                previous,
                top_row,
                previous_row,
                top_queue,
                previous_queue,
                remaining,
            } => {
                let context = (*top << 11) | (*previous << 4) | history & 0x0f;
                *top = ((*top << 1) | usize::from(*top_queue & 0x80 != 0)) & 0x1f;
                *previous = ((*previous << 1) | usize::from(*previous_queue & 0x80 != 0)) & 0x7f;
                *top_queue <<= 1;
                *previous_queue <<= 1;
                if queue_exhausted(remaining) {
                    *top_queue = packed_row_byte(bitmap, *top_row, sample_x + 4);
                    *previous_queue = packed_row_byte(bitmap, *previous_row, sample_x + 5);
                    *remaining = 8;
                }
                context
            }
            Self::Template1 {
                top,
                previous,
                top_row,
                previous_row,
                top_queue,
                previous_queue,
                remaining,
            } => {
                let context = (*top << 9) | (*previous << 3) | history & 0x07;
                *top = ((*top << 1) | usize::from(*top_queue & 0x80 != 0)) & 0x0f;
                *previous = ((*previous << 1) | usize::from(*previous_queue & 0x80 != 0)) & 0x3f;
                *top_queue <<= 1;
                *previous_queue <<= 1;
                if queue_exhausted(remaining) {
                    *top_queue = packed_row_byte(bitmap, *top_row, sample_x + 4);
                    *previous_queue = packed_row_byte(bitmap, *previous_row, sample_x + 5);
                    *remaining = 8;
                }
                context
            }
            Self::Template2 {
                top,
                previous,
                top_row,
                previous_row,
                top_queue,
                previous_queue,
                remaining,
            } => {
                let context = (*top << 7) | (*previous << 2) | history & 0x03;
                *top = ((*top << 1) | usize::from(*top_queue & 0x80 != 0)) & 0x07;
                *previous = ((*previous << 1) | usize::from(*previous_queue & 0x80 != 0)) & 0x1f;
                *top_queue <<= 1;
                *previous_queue <<= 1;
                if queue_exhausted(remaining) {
                    *top_queue = packed_row_byte(bitmap, *top_row, sample_x + 3);
                    *previous_queue = packed_row_byte(bitmap, *previous_row, sample_x + 4);
                    *remaining = 8;
                }
                context
            }
            Self::Template3 {
                previous,
                previous_row,
                previous_queue,
                remaining,
            } => {
                let context = (*previous << 4) | history & 0x0f;
                *previous = ((*previous << 1) | usize::from(*previous_queue & 0x80 != 0)) & 0x3f;
                *previous_queue <<= 1;
                if queue_exhausted(remaining) {
                    *previous_queue = packed_row_byte(bitmap, *previous_row, sample_x + 4);
                    *remaining = 8;
                }
                context
            }
            Self::Adaptive(descriptor) => descriptor.context(bitmap, x, y, history),
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn advance(&mut self, bitmap: &Bitmap, x: u32) {
        let x = i64::from(x);
        match self {
            Self::Template0 {
                top,
                previous,
                top_row,
                previous_row,
                top_queue,
                previous_queue,
                remaining,
            } => {
                *top = ((*top << 1) | usize::from(*top_queue & 0x80 != 0)) & 0x1f;
                *previous = ((*previous << 1) | usize::from(*previous_queue & 0x80 != 0)) & 0x7f;
                *top_queue <<= 1;
                *previous_queue <<= 1;
                if queue_exhausted(remaining) {
                    *top_queue = packed_row_byte(bitmap, *top_row, x + 4);
                    *previous_queue = packed_row_byte(bitmap, *previous_row, x + 5);
                    *remaining = 8;
                }
            }
            Self::Template1 {
                top,
                previous,
                top_row,
                previous_row,
                top_queue,
                previous_queue,
                remaining,
            } => {
                *top = ((*top << 1) | usize::from(*top_queue & 0x80 != 0)) & 0x0f;
                *previous = ((*previous << 1) | usize::from(*previous_queue & 0x80 != 0)) & 0x3f;
                *top_queue <<= 1;
                *previous_queue <<= 1;
                if queue_exhausted(remaining) {
                    *top_queue = packed_row_byte(bitmap, *top_row, x + 4);
                    *previous_queue = packed_row_byte(bitmap, *previous_row, x + 5);
                    *remaining = 8;
                }
            }
            Self::Template2 {
                top,
                previous,
                top_row,
                previous_row,
                top_queue,
                previous_queue,
                remaining,
            } => {
                *top = ((*top << 1) | usize::from(*top_queue & 0x80 != 0)) & 0x07;
                *previous = ((*previous << 1) | usize::from(*previous_queue & 0x80 != 0)) & 0x1f;
                *top_queue <<= 1;
                *previous_queue <<= 1;
                if queue_exhausted(remaining) {
                    *top_queue = packed_row_byte(bitmap, *top_row, x + 3);
                    *previous_queue = packed_row_byte(bitmap, *previous_row, x + 4);
                    *remaining = 8;
                }
            }
            Self::Template3 {
                previous,
                previous_row,
                previous_queue,
                remaining,
            } => {
                *previous = ((*previous << 1) | usize::from(*previous_queue & 0x80 != 0)) & 0x3f;
                *previous_queue <<= 1;
                if queue_exhausted(remaining) {
                    *previous_queue = packed_row_byte(bitmap, *previous_row, x + 4);
                    *remaining = 8;
                }
            }
            Self::Adaptive(_) => {}
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn sample_cached(bitmap: &Bitmap, x: u32, y: u32, offset: TemplateOffset) -> bool {
    packed_pixel(
        bitmap,
        i64::from(x) + i64::from(offset.x),
        i64::from(y) + i64::from(offset.y),
    )
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
fn packed_pixel(bitmap: &Bitmap, x: i64, y: i64) -> bool {
    if x < 0 || y < 0 || x >= i64::from(bitmap.width) || y >= i64::from(bitmap.height) {
        return false;
    }
    let row = y as usize;
    let column = x as usize / 8;
    let Some(index) = row
        .checked_mul(bitmap.stride)
        .and_then(|start| start.checked_add(column))
    else {
        return false;
    };
    let mask = match x as u32 % 8 {
        0 => 0x80,
        1 => 0x40,
        2 => 0x20,
        3 => 0x10,
        4 => 0x08,
        5 => 0x04,
        6 => 0x02,
        _ => 0x01,
    };
    match bitmap.data.get(index) {
        Some(byte) => *byte & mask != 0,
        None => false,
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn packed_row_start(bitmap: &Bitmap, row: i64) -> Option<usize> {
    if row < 0 || row >= i64::from(bitmap.height) {
        return None;
    }
    (row as usize).checked_mul(bitmap.stride)
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn packed_row_byte(bitmap: &Bitmap, row_start: Option<usize>, x: i64) -> u8 {
    let Some(row_start) = row_start else {
        return 0;
    };
    if x < 0 || x >= i64::from(bitmap.width) {
        return 0;
    }
    let x = x as usize;
    let Some(index) = row_start.checked_add(x / 8) else {
        return 0;
    };
    let Some(first) = bitmap.data.get(index) else {
        return 0;
    };
    let second = match index.checked_add(1) {
        Some(next) => match bitmap.data.get(next) {
            Some(byte) => *byte,
            None => 0,
        },
        None => 0,
    };
    let combined = (u16::from(*first) << 8) | u16::from(second);
    let shift = match x & 7 {
        0 => 8,
        1 => 7,
        2 => 6,
        3 => 5,
        4 => 4,
        5 => 3,
        6 => 2,
        _ => 1,
    };
    let shifted = (combined >> shift) & 0xff;
    let Ok(value) = u8::try_from(shifted) else {
        return 0;
    };
    let remaining = usize::try_from(bitmap.width)
        .ok()
        .and_then(|width| width.checked_sub(x))
        .unwrap_or(0);
    let mask = match remaining {
        0 => 0x00,
        1 => 0x80,
        2 => 0xc0,
        3 => 0xe0,
        4 => 0xf0,
        5 => 0xf8,
        6 => 0xfc,
        7 => 0xfe,
        _ => 0xff,
    };
    value & mask
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
fn packed_row_bits(bitmap: &Bitmap, center_x: i64, row: i64, left: i32, count: u8) -> usize {
    if count == 0 || count > 8 || row < 0 || row >= i64::from(bitmap.height) {
        return 0;
    }
    let Some(start) = center_x.checked_add(i64::from(left)) else {
        return 0;
    };
    let Some(end) = start.checked_add(i64::from(count)) else {
        return 0;
    };
    if start < 0 || end > i64::from(bitmap.width) {
        let mut bits = 0_usize;
        let mut offset = 0_u8;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant offset <= count,
            decreases count - offset,
        ))]
        while offset < count {
            let Some(sample_x) = start.checked_add(i64::from(offset)) else {
                return 0;
            };
            let Some(shifted) = bits.checked_mul(2) else {
                return 0;
            };
            bits = shifted | usize::from(packed_pixel(bitmap, sample_x, row));
            offset += 1;
        }
        return bits;
    }

    let row = row as usize;
    let start = start as usize;
    let Some(index) = row
        .checked_mul(bitmap.stride)
        .and_then(|base| base.checked_add(start / 8))
    else {
        return 0;
    };
    let Some(first) = bitmap.data.get(index) else {
        return 0;
    };
    let first = *first;
    let bit_offset = start & 7;
    let count = usize::from(count);
    let mask = match count {
        1 => 0x01,
        2 => 0x03,
        3 => 0x07,
        4 => 0x0f,
        5 => 0x1f,
        6 => 0x3f,
        7 => 0x7f,
        _ => 0xff,
    };
    let Some(total_bits) = bit_offset.checked_add(count) else {
        return 0;
    };
    if total_bits <= 8 {
        return (usize::from(first) >> (8 - total_bits)) & mask;
    }
    let second = match index.checked_add(1) {
        Some(next) => match bitmap.data.get(next) {
            Some(byte) => *byte,
            None => 0,
        },
        None => 0,
    };
    let combined = (usize::from(first) << 8) | usize::from(second);
    let Some(shift) = 16_usize.checked_sub(total_bits) else {
        return 0;
    };
    (combined >> shift) & mask
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
    let descriptor = RefinementContextDescriptor::new(template, adaptive);
    let bitmap_width = bitmap.width;
    let bitmap_height = bitmap.height;
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::views::bitmap_fields(bitmap);
    }
    let mut ltp = false;
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
            let context = template.sltp_context();
            let sltp = decode_probability_signal(decoder, contexts, context);
            if sltp > 1 {
                return Err(probability_signal_error(
                    decoder,
                    context,
                    contexts.len(),
                    sltp,
                ));
            }
            ltp ^= sltp != 0;
        }
        let mut history = 0_usize;
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
                if let Some(prediction) = refinement_prediction_cached(reference, x, y, parameters)
                {
                    history = ((history << 1) | usize::from(prediction)) & 1;
                    if prediction {
                        bitmap.set_pixel(x, y, true)?;
                    }
                    x += 1;
                    continue;
                }
            }
            let context = descriptor.context(bitmap, reference, x, y, history, parameters);
            let signal = decode_probability_signal(decoder, contexts, context);
            if signal > 1 {
                return Err(probability_signal_error(
                    decoder,
                    context,
                    contexts.len(),
                    signal,
                ));
            }
            let value = signal != 0;
            history = ((history << 1) | usize::from(value)) & 1;
            if value {
                bitmap.set_pixel(x, y, true)?;
            }
            x += 1;
        }
        y += 1;
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::ptr_arg, clippy::too_many_arguments)]
fn decode_refinement_with_oracle(
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

#[derive(Debug, Clone, Copy)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
enum RefinementContextDescriptor {
    NominalTemplate0,
    Template0 {
        target: TemplateOffset,
        reference: TemplateOffset,
    },
    Template1,
}

impl RefinementContextDescriptor {
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    const fn new(template: RefinementTemplate, adaptive: [TemplateOffset; 2]) -> Self {
        match template {
            RefinementTemplate::Template0 => {
                if adaptive[0].x == -1
                    && adaptive[0].y == -1
                    && adaptive[1].x == -1
                    && adaptive[1].y == -1
                {
                    Self::NominalTemplate0
                } else {
                    Self::Template0 {
                        target: adaptive[0],
                        reference: adaptive[1],
                    }
                }
            }
            RefinementTemplate::Template1 => Self::Template1,
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn context(
        self,
        target: &Bitmap,
        reference: &Bitmap,
        x: u32,
        y: u32,
        history: usize,
        parameters: RefinementParameters,
    ) -> usize {
        let reference_x = i64::from(x) - i64::from(parameters.reference_dx);
        let reference_y = i64::from(y) - i64::from(parameters.reference_dy);
        match self {
            Self::NominalTemplate0 => {
                let target_previous =
                    packed_row_bits(target, i64::from(x), i64::from(y) - 1, -1, 3);
                let reference_previous =
                    packed_row_bits(reference, reference_x, reference_y - 1, -1, 3);
                let reference_current = packed_row_bits(reference, reference_x, reference_y, -1, 3);
                let reference_next =
                    packed_row_bits(reference, reference_x, reference_y + 1, -1, 3);
                let mut context = (target_previous << 1) | history & 1;
                context = (context << 3) | reference_previous;
                context = (context << 3) | reference_current;
                (context << 3) | reference_next
            }
            Self::Template0 {
                target: target_adaptive,
                reference: reference_adaptive,
            } => {
                let target_a = usize::from(sample_cached(target, x, y, target_adaptive));
                let target_previous = packed_row_bits(target, i64::from(x), i64::from(y) - 1, 0, 2);
                let reference_a = usize::from(packed_pixel(
                    reference,
                    reference_x + i64::from(reference_adaptive.x),
                    reference_y + i64::from(reference_adaptive.y),
                ));
                let reference_previous =
                    packed_row_bits(reference, reference_x, reference_y - 1, 0, 2);
                let reference_current = packed_row_bits(reference, reference_x, reference_y, -1, 3);
                let reference_next =
                    packed_row_bits(reference, reference_x, reference_y + 1, -1, 3);
                let mut context = (target_a << 2) | target_previous;
                context = (context << 1) | history & 1;
                context = (context << 1) | reference_a;
                context = (context << 2) | reference_previous;
                context = (context << 3) | reference_current;
                (context << 3) | reference_next
            }
            Self::Template1 => {
                let target_previous =
                    packed_row_bits(target, i64::from(x), i64::from(y) - 1, -1, 3);
                let reference_previous =
                    usize::from(packed_pixel(reference, reference_x, reference_y - 1));
                let reference_current = packed_row_bits(reference, reference_x, reference_y, -1, 3);
                let reference_next = packed_row_bits(reference, reference_x, reference_y + 1, 0, 2);
                let mut context = (target_previous << 1) | history & 1;
                context = (context << 1) | reference_previous;
                context = (context << 3) | reference_current;
                (context << 2) | reference_next
            }
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn refinement_prediction_cached(
    reference: &Bitmap,
    x: u32,
    y: u32,
    parameters: RefinementParameters,
) -> Option<bool> {
    let reference_x = i64::from(x) - i64::from(parameters.reference_dx);
    let reference_y = i64::from(y) - i64::from(parameters.reference_dy);
    let neighbourhood = (packed_row_bits(reference, reference_x, reference_y - 1, -1, 3) << 6)
        | (packed_row_bits(reference, reference_x, reference_y, -1, 3) << 3)
        | packed_row_bits(reference, reference_x, reference_y + 1, -1, 3);
    match neighbourhood {
        0 => Some(false),
        0x01ff => Some(true),
        _ => None,
    }
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
pub(crate) fn enforce_managed_memory(
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

#[cfg(test)]
fn word_raster_storage_bytes(width: u32, height: u32) -> Option<u64> {
    let stride_words = u64::from(width).checked_add(31)? / 32;
    stride_words.checked_mul(u64::from(height))?.checked_mul(4)
}

#[cfg(test)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
fn row_ring_storage_bytes(width: u32, rows: u32) -> Option<u64> {
    let stride_words = u64::from(width).checked_add(31)? / 32;
    stride_words
        .checked_mul(u64::from(rows.min(3)))?
        .checked_mul(4)
}

#[cfg(test)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
fn row_ring_fits_managed_memory(
    width: u32,
    rows: u32,
    context_bytes: u64,
    bitmap_bytes: u64,
    limits: &Limits,
) -> Result<bool, GenericRegionError> {
    let Some(row_bytes) = row_ring_storage_bytes(width, rows) else {
        return Err(integer_overflow());
    };
    let Some(managed) = context_bytes
        .checked_add(bitmap_bytes)
        .and_then(|bytes| bytes.checked_add(row_bytes))
    else {
        return Err(integer_overflow());
    };
    Ok(managed <= limits.max_decoder_managed_memory_bytes)
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
pub(crate) fn integer_overflow() -> GenericRegionError {
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
        crate::spec::bitmap::row_copy_result(old(bitmap), final(bitmap), source, destination),
        forall|x: u32| x < old(bitmap).width ==>
            crate::spec::bitmap::stored_pixel(final(bitmap), x, destination)
                == crate::spec::bitmap::stored_pixel(old(bitmap), x, source),
        out.is_ok(),
))]
#[allow(clippy::cast_possible_truncation)]
fn copy_bitmap_row(
    bitmap: &mut Bitmap,
    source: u32,
    destination: u32,
) -> Result<(), GenericRegionError> {
    bitmap.copy_row(source, destination)?;
    #[cfg(verus_keep_ghost)]
    production_generic_copied_row_pixels!(bitmap, source, destination);
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
            && bitmap.height == nominal_height
            && forall|index: int| 0 <= index < crate::views::bitmap_data(&bitmap).len() ==>
                crate::views::bitmap_data(&bitmap)[index]
                    == if index < crate::views::bitmap_data(&decoded).len() {
                        crate::views::bitmap_data(&decoded)[index]
                    } else { 0u8 },
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
        reveal(crate::spec::bitmap::zeroed_storage);
    }
    let byte_count = decoded.data.len();
    if byte_count > padded.data.len() {
        return Err(MalformedError::IntegerOverflow.into());
    }
    padded.copy_prefix_from(&decoded, byte_count)?;
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::views::bitmap_fields(&padded);
        crate::views::bitmap_fields(&decoded);
        reveal(crate::spec::bitmap::storage_valid);
        reveal(crate::spec::bitmap::prefix_copy_result);
    }
    Ok(padded)
}

#[path = "generic_row_ring.rs"]
mod generic_row_ring;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitmap::CombinationOperator;
    use std::collections::VecDeque;
    use std::hint::black_box;
    use std::time::{Duration, Instant};

    fn control_pixel(bitmap: &Bitmap, x: i128, row: i128) -> bool {
        if x < 0 || x >= i128::from(bitmap.width) || row < 0 || row >= i128::from(bitmap.height) {
            return false;
        }
        let x = usize::try_from(x).unwrap();
        let row = usize::try_from(row).unwrap();
        bitmap.data[row * bitmap.stride + x / 8] & (0x80 >> (x % 8)) != 0
    }

    fn control_window(bitmap: &Bitmap, center: i64, row: i64, left: i32, count: u8) -> usize {
        let start = i128::from(center) + i128::from(left);
        let end = start + i128::from(count);
        if !(1..=8).contains(&count)
            || row < 0
            || row >= i64::from(bitmap.height)
            || start < i128::from(i64::MIN)
            || start > i128::from(i64::MAX)
            || end < i128::from(i64::MIN)
            || end > i128::from(i64::MAX)
        {
            return 0;
        }
        (0..count).fold(0, |value, offset| {
            value * 2
                + usize::from(control_pixel(
                    bitmap,
                    start + i128::from(offset),
                    i128::from(row),
                ))
        })
    }

    fn control_malformed(error: GenericRegionError) -> MalformedError {
        match DecodeError::from(error) {
            DecodeError::Malformed(error) => error,
            other => panic!("expected a typed malformed projection: {other:?}"),
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn packed_rows_match_scalar_msb_windows_padding_and_numeric_edges() {
        let mut byte_cases = 0;
        let mut window_cases = 0;
        let mut extreme_cases = 0;
        for width in [0, 1, 7, 8, 9, 15, 16, 17] {
            let mut bitmap = Bitmap::blank(width, 3, &Limits::default()).unwrap();
            for (index, byte) in bitmap.data.iter_mut().enumerate() {
                *byte = u8::try_from(index % 256)
                    .unwrap()
                    .wrapping_mul(73)
                    .wrapping_add(u8::try_from(width).unwrap())
                    .wrapping_add(0xa5);
            }
            let before = bitmap.clone();
            for row in [-1, 0, 1, 2, 3] {
                let row_start = if (0..3).contains(&row) {
                    Some(usize::try_from(row).unwrap() * bitmap.stride)
                } else {
                    None
                };
                assert_eq!(packed_row_start(&bitmap, row), row_start);
                for center in -9..=i64::from(width) + 8 {
                    let byte = if center < 0 || center >= i64::from(width) || row_start.is_none() {
                        0
                    } else {
                        u8::try_from((0..8).fold(0_usize, |value, offset| {
                            value * 2
                                + usize::from(control_pixel(
                                    &bitmap,
                                    i128::from(center) + i128::from(offset),
                                    i128::from(row),
                                ))
                        }))
                        .unwrap()
                    };
                    assert_eq!(packed_row_byte(&bitmap, row_start, center), byte);
                    byte_cases += 1;
                    for left in [-8, -3, 0, 3, 8] {
                        for count in [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, u8::MAX] {
                            assert_eq!(
                                packed_row_bits(&bitmap, center, row, left, count),
                                control_window(&bitmap, center, row, left, count),
                            );
                            window_cases += 1;
                        }
                    }
                }
                for center in [i64::MIN, i64::MIN + 1, i64::MAX - 1, i64::MAX] {
                    for left in [i32::MIN, -1, 0, 1, i32::MAX] {
                        for count in [0, 1, 7, 8, 9, u8::MAX] {
                            assert_eq!(
                                packed_row_bits(&bitmap, center, row, left, count),
                                control_window(&bitmap, center, row, left, count),
                            );
                            extreme_cases += 1;
                        }
                    }
                }
            }
            for invalid_start in [None, Some(bitmap.data.len()), Some(usize::MAX)] {
                for x in [0, 8] {
                    assert_eq!(packed_row_byte(&bitmap, invalid_start, x), 0);
                }
            }
            assert_eq!(bitmap, before);
        }
        assert_eq!(byte_cases, 1085);
        assert_eq!(window_cases, 59_675);
        assert_eq!(extreme_cases, 4800);
    }

    #[test]
    fn probability_state_validation_matches_all_raw_fields_and_first_error() {
        assert!(validate_probability_states(&[]).is_ok());
        let mut cases = 0;
        let mut accepted = 0;
        for state_index in 0..=u8::MAX {
            for mps in 0..=u8::MAX {
                let candidate = ArithmeticContext { state_index, mps };
                let contexts = [
                    ArithmeticContext::new(),
                    candidate,
                    ArithmeticContext {
                        state_index: u8::MAX,
                        mps: u8::MAX,
                    },
                ];
                let valid = state_index < 47 && mps <= 1;
                for with_tail in [false, true] {
                    let slice = if with_tail {
                        &contexts[..]
                    } else {
                        &contexts[..2]
                    };
                    match validate_probability_states(slice) {
                        Ok(()) => {
                            assert!(valid && !with_tail);
                            accepted += 1;
                        }
                        Err(error) => {
                            let expected = if valid {
                                assert!(with_tail);
                                (u8::MAX, u8::MAX)
                            } else {
                                (state_index, mps)
                            };
                            assert_eq!(
                                control_malformed(error),
                                MalformedError::InvalidArithmeticContextState {
                                    state_index: expected.0,
                                    mps: expected.1,
                                }
                            );
                        }
                    }
                    assert_eq!(contexts[1].state_index(), state_index);
                    assert_eq!(contexts[1].mps(), mps);
                    cases += 1;
                }
            }
        }
        assert_eq!(cases, 131_072);
        assert_eq!(accepted, 94);
    }

    #[test]
    fn generic_error_trait_projection_and_eof_factory_preserve_fields() {
        for (error, expected) in [
            (
                GenericRegionError::ReservedFlags(u8::MAX),
                GenericRegionErrorKind::ReservedFlags(u8::MAX),
            ),
            (
                GenericRegionError::InvalidFlags(u8::MAX),
                GenericRegionErrorKind::InvalidFlags(u8::MAX),
            ),
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
                    code: u8::MAX,
                },
                GenericRegionErrorKind::UnexpectedArithmeticMarker {
                    offset: usize::MAX,
                    code: u8::MAX,
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
                    count: 0,
                },
                GenericRegionErrorKind::ContextIndex {
                    index: usize::MAX,
                    count: 0,
                },
            ),
        ] {
            assert_eq!(
                control_malformed(error),
                MalformedError::InvalidGenericRegion(expected)
            );
        }
        let mut cases = 0;
        for offset in [0, 1, 65_535, usize::MAX] {
            for needed in [0, 1, 65_535, usize::MAX] {
                for remaining in [0, 1, 65_535, usize::MAX] {
                    let fits = [offset, needed, remaining]
                        .into_iter()
                        .all(|value| value as u128 <= u128::from(u64::MAX));
                    let actual = control_malformed(unexpected_eof(offset, needed, remaining));
                    if fits {
                        assert_eq!(
                            actual,
                            MalformedError::UnexpectedEof {
                                offset: u64::try_from(offset).unwrap(),
                                needed: u64::try_from(needed).unwrap(),
                                remaining: u64::try_from(remaining).unwrap(),
                            }
                        );
                    } else {
                        assert_eq!(actual, MalformedError::IntegerOverflow);
                    }
                    cases += 1;
                }
            }
        }
        assert_eq!(cases, 64);
    }

    #[test]
    fn probability_failure_projection_preserves_context_and_decoder_errors() {
        let bytes = [0, 0];
        let decoder = ArithmeticDecoder::new(&bytes).unwrap();
        let before_position = decoder.byte_position();
        let before_marker = decoder.marker();
        let mut cases = 0;
        for signal in 0..=u8::MAX {
            for index in [0, 1, usize::MAX] {
                for context_count in [0, 37, usize::MAX] {
                    let actual = control_malformed(probability_signal_error(
                        &decoder,
                        index,
                        context_count,
                        signal,
                    ));
                    let expected = if signal == RASTER_SIGNAL_CONTEXT {
                        MalformedError::InvalidGenericRegion(GenericRegionErrorKind::ContextIndex {
                            index,
                            count: context_count,
                        })
                    } else {
                        match decoder.signal_error(signal).into_decode() {
                            DecodeError::Malformed(error) => error,
                            other => panic!("unexpected arithmetic diagnostic: {other:?}"),
                        }
                    };
                    assert_eq!(actual, expected);
                    assert_eq!(decoder.byte_position(), before_position);
                    assert_eq!(decoder.marker(), before_marker);
                    cases += 1;
                }
            }
        }
        assert_eq!(cases, 2304);
    }

    #[test]
    fn diagnostic_variants_preserve_fields_sources_and_formatter_failures() {
        use std::fmt::Write as _;

        struct RejectWrites;
        impl fmt::Write for RejectWrites {
            fn write_str(&mut self, _: &str) -> fmt::Result {
                Err(fmt::Error)
            }
        }

        for (error, message, delegated) in [
            (
                GenericRegionError::Decode(DecodeError::Malformed(MalformedError::IntegerOverflow)),
                "malformed JBIG2 input: input-derived integer overflow".to_owned(),
                true,
            ),
            (
                GenericRegionError::ReservedFlags(u8::MAX),
                "reserved generic-region flags are set: 0xff".to_owned(),
                false,
            ),
            (
                GenericRegionError::InvalidFlags(u8::MAX),
                "invalid generic-region flag combination: 0xff".to_owned(),
                false,
            ),
            (
                GenericRegionError::InvalidAdaptivePixel {
                    number: u8::MAX,
                    x: i32::MIN,
                    y: i32::MAX,
                },
                "adaptive pixel 255 at (-2147483648, 2147483647) is not an earlier target pixel"
                    .to_owned(),
                false,
            ),
            (
                GenericRegionError::InvalidSkipDimensions,
                "generic-region skip bitmap has the wrong dimensions".to_owned(),
                false,
            ),
            (
                GenericRegionError::InvalidArithmeticTerminator,
                "arithmetic region does not end in FF AC".to_owned(),
                false,
            ),
            (
                GenericRegionError::UnexpectedArithmeticMarker {
                    offset: usize::MAX,
                    code: 0xfe,
                },
                format!("unexpected arithmetic marker FF FE at byte {}", usize::MAX),
                false,
            ),
            (
                GenericRegionError::RowCountMismatch {
                    expected: u32::MAX,
                    actual: 0,
                },
                "terminal row count 0 differs from decoded row count 4294967295".to_owned(),
                false,
            ),
            (
                GenericRegionError::ContextIndex {
                    index: usize::MAX,
                    count: 0,
                },
                format!(
                    "context index {} is outside a statistics set of length 0",
                    usize::MAX
                ),
                false,
            ),
        ] {
            assert_eq!(error.to_string(), message);
            let source = std::error::Error::source(&error);
            assert_eq!(source.is_some(), delegated);
            if let Some(source) = source {
                assert!(matches!(
                    source.downcast_ref::<DecodeError>(),
                    Some(DecodeError::Malformed(MalformedError::IntegerOverflow)),
                ));
            }
            assert!(write!(&mut RejectWrites, "{error}").is_err());
        }
        let private_message = "private diagnostic message must not escape";
        let io = std::io::Error::new(std::io::ErrorKind::TimedOut, private_message);
        let error = GenericRegionError::from(DecodeError::from(io));
        assert_eq!(error.to_string(), "input/output error: TimedOut");
        assert!(!error.to_string().contains(private_message));
        assert!(matches!(
            std::error::Error::source(&error)
                .unwrap()
                .downcast_ref::<DecodeError>(),
            Some(DecodeError::Io(_)),
        ));
    }

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

    fn next_random(state: &mut u64) -> u32 {
        *state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (*state >> 32) as u32
    }

    fn random_bitmap(width: u32, height: u32, state: &mut u64) -> Bitmap {
        let limits = Limits::default();
        let mut bitmap = Bitmap::blank(width, height, &limits).expect("random bitmap");
        for y in 0..height {
            for x in 0..width {
                if next_random(state).is_multiple_of(4) {
                    bitmap.set_pixel(x, y, true).expect("in bounds");
                }
            }
        }
        bitmap
    }

    fn arithmetic_bytes(state: &mut u64) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(1_026);
        for _ in 0..1_024 {
            bytes.push((next_random(state) & 0x7f) as u8);
        }
        bytes.extend_from_slice(&ARITHMETIC_TERMINATOR);
        bytes
    }

    fn arithmetic_segment_payload(width: u32, height: u32, typical: bool, coded: &[u8]) -> Vec<u8> {
        let mut payload = region_prefix(width, height);
        payload.push(if typical { 0x08 } else { 0 });
        for offset in [
            TemplateOffset::new(3, -1),
            TemplateOffset::new(-3, -1),
            TemplateOffset::new(2, -2),
            TemplateOffset::new(-2, -2),
        ] {
            payload.push(offset.x.to_be_bytes()[3]);
            payload.push(offset.y.to_be_bytes()[3]);
        }
        payload.extend_from_slice(coded);
        payload
    }

    fn assert_decoder_state_eq(left: &ArithmeticDecoder<'_>, right: &ArithmeticDecoder<'_>) {
        assert_eq!(left.byte_position, right.byte_position);
        assert_eq!(left.current_byte, right.current_byte);
        assert_eq!(left.code, right.code);
        assert_eq!(left.interval, right.interval);
        assert_eq!(left.bit_count, right.bit_count);
        assert_eq!(left.marker, right.marker);
    }

    fn assert_word_template0_parity(
        width: u32,
        height: u32,
        rows: u32,
        extended: bool,
        typical: bool,
        coded: &[u8],
    ) {
        let limits = Limits::default();
        let header = generic_header(width, height, GenericTemplate::Template0, extended, typical);
        let mut word_decoder = ArithmeticDecoder::new(coded).expect("word decoder");
        let mut ring_decoder = ArithmeticDecoder::new(coded).expect("ring decoder");
        let mut byte_decoder = ArithmeticDecoder::new(coded).expect("byte decoder");
        let mut word_contexts =
            vec![ArithmeticContext::new(); generic_context_count(GenericTemplate::Template0)];
        let mut ring_contexts = word_contexts.clone();
        let mut byte_contexts = word_contexts.clone();
        let mut raster = GenericWordRaster::try_new(width, height).expect("word raster");
        let mut word_bitmap = Bitmap::blank(width, height, &limits).expect("word bitmap");
        let mut ring_bitmap = Bitmap::blank(width, height, &limits).expect("ring bitmap");
        let mut byte_bitmap = Bitmap::blank(width, height, &limits).expect("byte bitmap");
        let mut ring = GenericRowRing::try_new(width, rows, u64::MAX).expect("row ring");

        let word_result = decode_nominal_template0_words(
            &mut raster,
            rows,
            typical,
            &mut word_decoder,
            &mut word_contexts,
        );
        let ring_result = decode_nominal_template0_row_ring(
            &mut ring,
            &mut ring_bitmap,
            rows,
            typical,
            &mut ring_decoder,
            &mut ring_contexts,
        );
        let byte_result = decode_generic_with_oracle(
            &mut byte_bitmap,
            rows,
            GenericTemplate::Template0,
            extended,
            &header.adaptive,
            typical,
            None,
            &mut byte_decoder,
            &mut byte_contexts,
        );
        assert_eq!(
            word_result.as_ref().err().map(ToString::to_string),
            byte_result.as_ref().err().map(ToString::to_string)
        );
        assert_eq!(
            ring_result.as_ref().err().map(ToString::to_string),
            byte_result.as_ref().err().map(ToString::to_string)
        );
        if word_result.is_ok() {
            raster
                .materialize(&mut word_bitmap)
                .expect("materialize word raster");
            assert_eq!(word_bitmap, byte_bitmap);
            assert_eq!(ring_bitmap, byte_bitmap);
        }
        assert_eq!(word_contexts, byte_contexts);
        assert_eq!(ring_contexts, byte_contexts);
        assert_decoder_state_eq(&word_decoder, &byte_decoder);
        assert_decoder_state_eq(&ring_decoder, &byte_decoder);
    }

    fn median_duration(mut values: Vec<Duration>) -> Duration {
        values.sort_unstable();
        values[values.len() / 2]
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
    fn height_padding_preserves_decoded_bytes_and_zeroes_new_rows() {
        let limits = Limits::default();
        for width in [0, 1, 7, 8, 9, 63, 64, 65] {
            for height in [0, 1, 3] {
                let mut decoded = Bitmap::blank(width, height, &limits).unwrap();
                for (index, byte) in decoded.data.iter_mut().enumerate() {
                    *byte = u8::try_from(index % 251).unwrap().wrapping_add(17);
                }
                for nominal in [height, height + 1, height + 3] {
                    let result = pad_bitmap_height(decoded.clone(), nominal, &limits).unwrap();
                    assert_eq!(
                        (result.width(), result.height(), result.stride()),
                        (width, nominal, decoded.stride())
                    );
                    assert_eq!(&result.bytes()[..decoded.data.len()], decoded.bytes());
                    assert!(result.bytes()[decoded.data.len()..]
                        .iter()
                        .all(|byte| *byte == 0));
                }
            }
        }
        let decoded = Bitmap::blank(8, 2, &limits).unwrap();
        assert!(matches!(
            pad_bitmap_height(decoded.clone(), 1, &limits),
            Err(GenericRegionError::RowCountMismatch {
                expected: 1,
                actual: 2
            })
        ));
        let restricted = Limits {
            max_height: 2,
            ..limits
        };
        assert!(matches!(
            pad_bitmap_height(decoded, 3, &restricted),
            Err(GenericRegionError::Decode(DecodeError::LimitExceeded(
                crate::LimitExceeded {
                    kind: crate::LimitKind::Height,
                    limit: 2,
                    actual: 3,
                }
            )))
        ));
    }

    #[test]
    fn typical_row_copy_preserves_source_and_other_rows() {
        for width in [
            0, 1, 7, 8, 9, 63, 64, 65, 127, 128, 129, 511, 512, 513, 1017, 1023, 1024, 1025, 4096,
        ] {
            let mut original = Bitmap::blank(width, 4, &Limits::default()).unwrap();
            for (index, byte) in original.data.iter_mut().enumerate() {
                *byte = u8::try_from(index % 251)
                    .unwrap()
                    .wrapping_mul(37)
                    .wrapping_add(11);
            }
            for source in 0..4 {
                for destination in 0..4 {
                    let stride = original.stride();
                    let source_start = usize::try_from(source).unwrap() * stride;
                    let destination_start = usize::try_from(destination).unwrap() * stride;
                    let mut expected = original.bytes().to_vec();
                    expected[destination_start..destination_start + stride]
                        .copy_from_slice(&original.bytes()[source_start..source_start + stride]);
                    let mut actual = original.clone();
                    copy_bitmap_row(&mut actual, source, destination).unwrap();
                    assert_eq!(
                        actual.bytes(),
                        expected,
                        "{width}: {source} -> {destination}"
                    );
                    assert_eq!(
                        (actual.width(), actual.height(), actual.stride()),
                        (width, 4, stride)
                    );
                    for x in 0..width {
                        assert_eq!(actual.pixel(x, destination), original.pixel(x, source));
                    }
                }
            }
        }
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
    fn specialized_generic_kernels_match_scalar_oracle_randomized() {
        let limits = Limits::default();
        let templates = [
            GenericTemplate::Template0,
            GenericTemplate::Template1,
            GenericTemplate::Template2,
            GenericTemplate::Template3,
        ];
        let prior_offsets = [
            TemplateOffset::new(-1, 0),
            TemplateOffset::new(-4, 0),
            TemplateOffset::new(-7, -1),
            TemplateOffset::new(3, -1),
            TemplateOffset::new(7, -2),
        ];
        let mut state = 0x8d26_4f31_ba90_75ce;
        for case in 0..96_u32 {
            let template = templates[case as usize % templates.len()];
            let variant = case / u32::try_from(templates.len()).expect("four templates");
            let extended = template == GenericTemplate::Template0 && variant & 1 != 0;
            let width = 1 + next_random(&mut state) % 41;
            let height = 1 + next_random(&mut state) % 23;
            let rows = next_random(&mut state) % (height + 1);
            let typical = variant & 2 != 0;
            let mut adaptive = generic_header(width, height, template, extended, typical).adaptive;
            let adaptive_count = match (template, extended) {
                (GenericTemplate::Template0, true) => 12,
                (GenericTemplate::Template0, false) => 4,
                _ => 1,
            };
            if variant & 8 != 0 {
                for offset in adaptive.iter_mut().take(adaptive_count) {
                    *offset = prior_offsets[next_random(&mut state) as usize % prior_offsets.len()];
                }
            }
            let skip = (variant & 4 != 0).then(|| random_bitmap(width, height, &mut state));
            let coded = arithmetic_bytes(&mut state);
            let mut optimized_decoder = ArithmeticDecoder::new(&coded).expect("decoder");
            let mut oracle_decoder = ArithmeticDecoder::new(&coded).expect("decoder");
            let mut optimized_contexts =
                vec![ArithmeticContext::new(); generic_context_count(template)];
            let mut oracle_contexts = optimized_contexts.clone();
            let mut optimized = Bitmap::blank(width, height, &limits).expect("bitmap");
            let mut oracle = Bitmap::blank(width, height, &limits).expect("bitmap");

            decode_generic_with(
                &mut optimized,
                rows,
                template,
                extended,
                &adaptive,
                typical,
                skip.as_ref(),
                &mut optimized_decoder,
                &mut optimized_contexts,
            )
            .expect("optimized decode");
            decode_generic_with_oracle(
                &mut oracle,
                rows,
                template,
                extended,
                &adaptive,
                typical,
                skip.as_ref(),
                &mut oracle_decoder,
                &mut oracle_contexts,
            )
            .expect("oracle decode");

            assert_eq!(optimized, oracle, "bitmap mismatch in case {case}");
            assert_eq!(
                optimized_contexts, oracle_contexts,
                "context mismatch in case {case}"
            );
            assert_decoder_state_eq(&optimized_decoder, &oracle_decoder);
        }
    }

    #[test]
    fn word_template0_matches_byte_oracle_exhaustive_row_shapes() {
        let mut state = 0x942d_63a1_b806_f75c;
        for width in 0..=97_u32 {
            for height in 0..=6_u32 {
                for rows in 0..=height {
                    for mode in 0..4_u32 {
                        let extended = mode & 1 != 0;
                        let typical = mode & 2 != 0;
                        let coded = arithmetic_bytes(&mut state);
                        assert_word_template0_parity(
                            width, height, rows, extended, typical, &coded,
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn word_template0_matches_byte_oracle_randomized() {
        let mut state = 0x16cb_78d2_4af9_035e;
        for case in 0..256_u32 {
            let width = next_random(&mut state) % 258;
            let height = next_random(&mut state) % 19;
            let rows = if height == 0 {
                0
            } else {
                next_random(&mut state) % (height + 1)
            };
            let extended = case & 1 != 0;
            let typical = case & 2 != 0;
            let coded = arithmetic_bytes(&mut state);
            assert_word_template0_parity(width, height, rows, extended, typical, &coded);
        }
    }

    #[test]
    fn word_template0_matches_byte_oracle_on_entropy_error() {
        for coded in [
            vec![0x00, 0xff, 0xac],
            vec![0x7f, 0x00, 0xff, 0xac],
            vec![0x01, 0x02, 0x03, 0xff, 0xac],
        ] {
            assert_word_template0_parity(97, 11, 11, false, true, &coded);
        }
    }

    #[test]
    fn direct_bitmap_matches_owned_region_exhaustive_widths() {
        let limits = Limits::default();
        let mut state = 0xa314_6d29_8bf0_c572;
        for width in 1..=65_u32 {
            for height in 1..=4_u32 {
                for typical in [false, true] {
                    let coded = arithmetic_bytes(&mut state);
                    let payload = arithmetic_segment_payload(width, height, typical, &coded);
                    let mut scratch = MmrScratch::default();
                    let owned = decode_generic_region_segment_reused(
                        &payload,
                        false,
                        &limits,
                        &mut scratch,
                    )
                    .expect("owned decode");
                    let mut direct = Bitmap::blank(width, height, &limits).expect("direct target");
                    let metadata =
                        decode_generic_region_segment_into(&payload, false, &limits, &mut direct)
                            .expect("direct decode")
                            .expect("arithmetic route");
                    assert_eq!(direct, owned.bitmap);
                    assert_eq!(metadata.decoded_rows, owned.decoded_rows);
                    assert_eq!(metadata.consumed_bytes, owned.consumed_bytes);
                }
            }
        }
    }

    #[test]
    fn direct_bitmap_matches_owned_unknown_rows_randomized() {
        let limits = Limits::default();
        let mut state = 0x4bc7_9da2_105e_f638;
        for case in 0..128_u32 {
            let width = 1 + next_random(&mut state) % 129;
            let height = 1 + next_random(&mut state) % 17;
            let rows = next_random(&mut state) % (height + 1);
            let typical = case & 1 != 0;
            let mut coded = arithmetic_bytes(&mut state);
            coded.extend_from_slice(&rows.to_be_bytes());
            let payload = arithmetic_segment_payload(width, height, typical, &coded);
            let mut scratch = MmrScratch::default();
            let owned = decode_generic_region_segment_reused(&payload, true, &limits, &mut scratch)
                .expect("owned unknown-height decode");
            let mut direct = Bitmap::blank(width, height, &limits).expect("direct target");
            let metadata = decode_generic_region_segment_into(&payload, true, &limits, &mut direct)
                .expect("direct unknown-height decode")
                .expect("arithmetic route");
            assert_eq!(direct, owned.bitmap);
            assert_eq!(metadata.decoded_rows, rows);
            assert_eq!(metadata.consumed_bytes, owned.consumed_bytes);
        }
    }

    #[test]
    fn direct_bitmap_preserves_error_and_memory_limit_classification() {
        let width = 97;
        let height = 11;
        let limits = Limits::default();
        for coded in [
            vec![0x00, 0xff, 0xac],
            vec![0x7f, 0x00, 0xff, 0xac],
            vec![0x01, 0x02, 0x03, 0xff, 0xac],
        ] {
            let payload = arithmetic_segment_payload(width, height, false, &coded);
            let mut scratch = MmrScratch::default();
            let owned =
                decode_generic_region_segment_reused(&payload, false, &limits, &mut scratch);
            let mut direct = Bitmap::blank(width, height, &limits).expect("direct target");
            let direct = decode_generic_region_segment_into(&payload, false, &limits, &mut direct);
            assert_eq!(
                owned.as_ref().err().map(ToString::to_string),
                direct.as_ref().err().map(ToString::to_string)
            );
        }

        let mut state = 0x18e5_a720_64bc_93df;
        let coded = arithmetic_bytes(&mut state);
        let payload = arithmetic_segment_payload(width, height, false, &coded);
        let bitmap_bytes = bitmap_storage_bytes(width, height).expect("bitmap bytes");
        let context_bytes =
            context_storage_bytes(generic_context_count(GenericTemplate::Template0))
                .expect("context bytes");
        let limited = Limits {
            max_decoder_managed_memory_bytes: context_bytes + bitmap_bytes - 1,
            ..limits
        };
        let mut scratch = MmrScratch::default();
        let owned = decode_generic_region_segment_reused(&payload, false, &limited, &mut scratch);
        let mut direct = Bitmap::blank(width, height, &limits).expect("preallocated target");
        let direct = decode_generic_region_segment_into(&payload, false, &limited, &mut direct);
        assert_eq!(
            owned.as_ref().err().map(ToString::to_string),
            direct.as_ref().err().map(ToString::to_string)
        );
    }

    #[test]
    fn row_ring_preserves_continuous_decoder_and_retained_context_state() {
        let limits = Limits::default();
        let coded = arithmetic_bytes(&mut 0xb47e_93d0_2c51_a86f);
        let mut ring_decoder = ArithmeticDecoder::new(&coded).expect("ring decoder");
        let mut word_decoder = ArithmeticDecoder::new(&coded).expect("word decoder");
        let mut byte_decoder = ArithmeticDecoder::new(&coded).expect("byte decoder");
        let mut ring_contexts =
            vec![ArithmeticContext::new(); generic_context_count(GenericTemplate::Template0)];
        let mut word_contexts = ring_contexts.clone();
        let mut byte_contexts = ring_contexts.clone();

        for (width, height, rows, typical) in [(31, 5, 5, true), (65, 7, 4, false), (9, 3, 3, true)]
        {
            let header = generic_header(width, height, GenericTemplate::Template0, false, typical);
            let mut ring_bitmap = Bitmap::blank(width, height, &limits).expect("ring bitmap");
            let mut word_bitmap = Bitmap::blank(width, height, &limits).expect("word bitmap");
            let mut byte_bitmap = Bitmap::blank(width, height, &limits).expect("byte bitmap");
            let mut ring = GenericRowRing::try_new(width, rows, u64::MAX).expect("row ring");
            let mut raster = GenericWordRaster::try_new(width, height).expect("word raster");

            decode_nominal_template0_row_ring(
                &mut ring,
                &mut ring_bitmap,
                rows,
                typical,
                &mut ring_decoder,
                &mut ring_contexts,
            )
            .expect("row-ring decode");
            decode_nominal_template0_words(
                &mut raster,
                rows,
                typical,
                &mut word_decoder,
                &mut word_contexts,
            )
            .expect("word decode");
            raster
                .materialize(&mut word_bitmap)
                .expect("word materialization");
            decode_generic_with_oracle(
                &mut byte_bitmap,
                rows,
                GenericTemplate::Template0,
                false,
                &header.adaptive,
                typical,
                None,
                &mut byte_decoder,
                &mut byte_contexts,
            )
            .expect("byte decode");

            assert_eq!(ring_bitmap, byte_bitmap);
            assert_eq!(word_bitmap, byte_bitmap);
            assert_eq!(ring_contexts, byte_contexts);
            assert_eq!(word_contexts, byte_contexts);
            assert_decoder_state_eq(&ring_decoder, &byte_decoder);
            assert_decoder_state_eq(&word_decoder, &byte_decoder);
        }
    }

    #[test]
    fn nominal_template0_skip_uses_exact_byte_fallback() {
        let limits = Limits::default();
        let width = 67;
        let height = 8;
        let rows = 5;
        let header = generic_header(width, height, GenericTemplate::Template0, false, true);
        let mut state = 0xe534_18af_92c7_60bd;
        let coded = arithmetic_bytes(&mut state);
        let skip = random_bitmap(width, height, &mut state);
        let mut routed_decoder = ArithmeticDecoder::new(&coded).expect("routed decoder");
        let mut oracle_decoder = ArithmeticDecoder::new(&coded).expect("oracle decoder");
        let mut routed_contexts =
            GenericContexts::new(GenericTemplate::Template0, false, &limits).expect("contexts");
        let mut oracle_contexts = routed_contexts.contexts.clone();
        let mut work = 0;
        let routed = decode_generic_on(
            &mut routed_decoder,
            &mut routed_contexts,
            GenericArithmeticCall {
                width,
                height,
                adaptive: header.adaptive,
                typical_prediction: true,
                skip: Some(&skip),
            },
            rows,
            &limits,
            &mut work,
        )
        .expect("fallback decode");
        let mut oracle = Bitmap::blank(width, height, &limits).expect("oracle bitmap");
        decode_generic_with_oracle(
            &mut oracle,
            rows,
            GenericTemplate::Template0,
            false,
            &header.adaptive,
            true,
            Some(&skip),
            &mut oracle_decoder,
            &mut oracle_contexts,
        )
        .expect("oracle decode");

        assert_eq!(routed, oracle);
        assert_eq!(routed_contexts.contexts, oracle_contexts);
        assert_decoder_state_eq(&routed_decoder, &oracle_decoder);
        assert_eq!(
            work,
            generic_work(width, rows, true).expect("expected work")
        );
    }

    #[test]
    fn row_ring_peak_is_charged_released_and_low_memory_falls_back() {
        let width = 33;
        let height = 100;
        let context_bytes =
            context_storage_bytes(generic_context_count(GenericTemplate::Template0))
                .expect("context bytes");
        let bitmap_bytes = bitmap_storage_bytes(width, height).expect("bitmap bytes");
        let row_bytes = row_ring_storage_bytes(width, height).expect("row bytes");
        let raster_bytes = word_raster_storage_bytes(width, height).expect("word bytes");
        assert_eq!(bitmap_bytes, 500);
        assert_eq!(row_bytes, 24);
        assert_eq!(raster_bytes, 800);
        assert!(row_bytes < raster_bytes);

        let fallback_limits = Limits {
            max_decoder_managed_memory_bytes: context_bytes + bitmap_bytes,
            ..Limits::default()
        };
        assert!(!row_ring_fits_managed_memory(
            width,
            height,
            context_bytes,
            bitmap_bytes,
            &fallback_limits,
        )
        .expect("peak check"));
        let mut ring_limits = fallback_limits;
        ring_limits.max_decoder_managed_memory_bytes += row_bytes;
        assert!(row_ring_fits_managed_memory(
            width,
            height,
            context_bytes,
            bitmap_bytes,
            &ring_limits,
        )
        .expect("peak check"));
        let ring_charge =
            GenericRowRing::try_new(width, height, row_bytes).expect("exact charged capacity");
        assert_eq!(ring_charge.charged_bytes, row_bytes);
        drop(ring_charge);

        let header = generic_header(width, height, GenericTemplate::Template0, false, true);
        let coded = arithmetic_bytes(&mut 0x52c7_f931_0bae_846d);
        let mut fallback_decoder = ArithmeticDecoder::new(&coded).expect("fallback decoder");
        let mut ring_decoder = ArithmeticDecoder::new(&coded).expect("ring decoder");
        let mut fallback_contexts =
            GenericContexts::new(GenericTemplate::Template0, false, &fallback_limits)
                .expect("fallback contexts");
        let mut ring_contexts =
            GenericContexts::new(GenericTemplate::Template0, false, &ring_limits)
                .expect("ring contexts");
        let call = GenericArithmeticCall {
            width,
            height,
            adaptive: header.adaptive,
            typical_prediction: true,
            skip: None,
        };
        let mut fallback_work = 0;
        let mut ring_work = 0;
        let fallback = decode_generic_on(
            &mut fallback_decoder,
            &mut fallback_contexts,
            call,
            height,
            &fallback_limits,
            &mut fallback_work,
        )
        .expect("byte fallback");
        let ring = decode_generic_on(
            &mut ring_decoder,
            &mut ring_contexts,
            call,
            height,
            &ring_limits,
            &mut ring_work,
        )
        .expect("row-ring route");
        assert_eq!(ring, fallback);
        assert_eq!(ring_contexts.contexts, fallback_contexts.contexts);
        assert_decoder_state_eq(&ring_decoder, &fallback_decoder);
        assert_eq!(ring_work, fallback_work);
    }

    #[test]
    fn specialized_refinement_kernels_match_scalar_oracle_randomized() {
        let limits = Limits::default();
        let mut state = 0x71e4_c09b_325d_a86f;
        for case in 0..96_u32 {
            let template = if case & 1 == 0 {
                RefinementTemplate::Template0
            } else {
                RefinementTemplate::Template1
            };
            let width = 1 + next_random(&mut state) % 37;
            let height = 1 + next_random(&mut state) % 21;
            let reference_width = 1 + next_random(&mut state) % 43;
            let reference_height = 1 + next_random(&mut state) % 25;
            let reference = random_bitmap(reference_width, reference_height, &mut state);
            let adaptive = if case & 8 == 0 {
                [TemplateOffset::new(-1, -1); 2]
            } else {
                let target = [
                    TemplateOffset::new(-1, 0),
                    TemplateOffset::new(-5, -1),
                    TemplateOffset::new(4, -2),
                ][next_random(&mut state) as usize % 3];
                let reference = TemplateOffset::new(
                    i32::try_from(next_random(&mut state) % 11).expect("bounded") - 5,
                    i32::try_from(next_random(&mut state) % 7).expect("bounded") - 3,
                );
                [target, reference]
            };
            let parameters = RefinementParameters {
                reference_dx: i32::try_from(next_random(&mut state) % 9).expect("bounded") - 4,
                reference_dy: i32::try_from(next_random(&mut state) % 7).expect("bounded") - 3,
            };
            let typical = case & 2 != 0;
            let coded = arithmetic_bytes(&mut state);
            let mut optimized_decoder = ArithmeticDecoder::new(&coded).expect("decoder");
            let mut oracle_decoder = ArithmeticDecoder::new(&coded).expect("decoder");
            let mut optimized_contexts =
                vec![ArithmeticContext::new(); refinement_context_count(template)];
            let mut oracle_contexts = optimized_contexts.clone();
            let mut optimized = Bitmap::blank(width, height, &limits).expect("bitmap");
            let mut oracle = Bitmap::blank(width, height, &limits).expect("bitmap");

            decode_refinement_with(
                &mut optimized,
                &reference,
                template,
                adaptive,
                typical,
                parameters,
                &mut optimized_decoder,
                &mut optimized_contexts,
            )
            .expect("optimized decode");
            decode_refinement_with_oracle(
                &mut oracle,
                &reference,
                template,
                adaptive,
                typical,
                parameters,
                &mut oracle_decoder,
                &mut oracle_contexts,
            )
            .expect("oracle decode");

            assert_eq!(optimized, oracle, "bitmap mismatch in case {case}");
            assert_eq!(
                optimized_contexts, oracle_contexts,
                "context mismatch in case {case}"
            );
            assert_decoder_state_eq(&optimized_decoder, &oracle_decoder);
        }
    }

    #[test]
    #[ignore = "microbenchmark; run explicitly with --release --ignored --nocapture"]
    fn benchmark_specialized_generic_kernels_against_44a2b6e_oracle() {
        let limits = Limits::default();
        let width = 768;
        let height = 384;
        let coded = {
            let mut bytes = vec![0x55; 48_000];
            bytes.extend_from_slice(&ARITHMETIC_TERMINATOR);
            bytes
        };
        for template in [
            GenericTemplate::Template0,
            GenericTemplate::Template1,
            GenericTemplate::Template2,
            GenericTemplate::Template3,
        ] {
            let header = generic_header(width, height, template, false, true);
            let mut optimized_times = Vec::new();
            let mut oracle_times = Vec::new();
            for sample in 0..11 {
                let run = |optimized: bool| {
                    let mut bitmap = Bitmap::blank(width, height, &limits).expect("bitmap");
                    let mut decoder = ArithmeticDecoder::new(&coded).expect("decoder");
                    let mut contexts =
                        vec![ArithmeticContext::new(); generic_context_count(template)];
                    let start = Instant::now();
                    if optimized {
                        decode_generic_with(
                            &mut bitmap,
                            height,
                            template,
                            false,
                            &header.adaptive,
                            true,
                            None,
                            &mut decoder,
                            &mut contexts,
                        )
                        .expect("optimized decode");
                    } else {
                        decode_generic_with_oracle(
                            &mut bitmap,
                            height,
                            template,
                            false,
                            &header.adaptive,
                            true,
                            None,
                            &mut decoder,
                            &mut contexts,
                        )
                        .expect("oracle decode");
                    }
                    let elapsed = start.elapsed();
                    black_box((bitmap, contexts, decoder));
                    elapsed
                };
                if sample & 1 == 0 {
                    optimized_times.push(run(true));
                    oracle_times.push(run(false));
                } else {
                    oracle_times.push(run(false));
                    optimized_times.push(run(true));
                }
            }
            let optimized = median_duration(optimized_times);
            let oracle = median_duration(oracle_times);
            println!(
                "generic_{template:?} scalar_44a2b6e_median_ns={} specialized_median_ns={} speedup={:.3}",
                oracle.as_nanos(),
                optimized.as_nanos(),
                oracle.as_secs_f64() / optimized.as_secs_f64(),
            );
        }
    }

    #[test]
    #[ignore = "microbenchmark; run explicitly with --release --ignored --nocapture"]
    fn benchmark_specialized_refinement_kernels_against_44a2b6e_oracle() {
        let limits = Limits::default();
        let width = 512;
        let height = 384;
        let mut state = 0x914a_d27c_650e_b83f;
        let reference = random_bitmap(width + 5, height + 3, &mut state);
        let coded = {
            let mut bytes = vec![0x35; 32_000];
            bytes.extend_from_slice(&ARITHMETIC_TERMINATOR);
            bytes
        };
        for template in [RefinementTemplate::Template0, RefinementTemplate::Template1] {
            let adaptive = [TemplateOffset::new(-1, -1), TemplateOffset::new(-1, -1)];
            let parameters = RefinementParameters {
                reference_dx: -2,
                reference_dy: 1,
            };
            let mut optimized_times = Vec::new();
            let mut oracle_times = Vec::new();
            for sample in 0..11 {
                let run = |optimized: bool| {
                    let mut bitmap = Bitmap::blank(width, height, &limits).expect("bitmap");
                    let mut decoder = ArithmeticDecoder::new(&coded).expect("decoder");
                    let mut contexts =
                        vec![ArithmeticContext::new(); refinement_context_count(template)];
                    let start = Instant::now();
                    if optimized {
                        decode_refinement_with(
                            &mut bitmap,
                            &reference,
                            template,
                            adaptive,
                            true,
                            parameters,
                            &mut decoder,
                            &mut contexts,
                        )
                        .expect("optimized decode");
                    } else {
                        decode_refinement_with_oracle(
                            &mut bitmap,
                            &reference,
                            template,
                            adaptive,
                            true,
                            parameters,
                            &mut decoder,
                            &mut contexts,
                        )
                        .expect("oracle decode");
                    }
                    let elapsed = start.elapsed();
                    black_box((bitmap, contexts, decoder));
                    elapsed
                };
                if sample & 1 == 0 {
                    optimized_times.push(run(true));
                    oracle_times.push(run(false));
                } else {
                    oracle_times.push(run(false));
                    optimized_times.push(run(true));
                }
            }
            let optimized = median_duration(optimized_times);
            let oracle = median_duration(oracle_times);
            println!(
                "refinement_{template:?} scalar_44a2b6e_median_ns={} specialized_median_ns={} speedup={:.3}",
                oracle.as_nanos(),
                optimized.as_nanos(),
                oracle.as_secs_f64() / optimized.as_secs_f64(),
            );
        }
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

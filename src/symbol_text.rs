//! Symbol-dictionary and text-region decoding helpers.
//!
//! Shared types and entry points live here; dictionary decoding, text strips,
//! clipped placement, headers, Huffman tables, entropy adapters and bitmap
//! helpers each have a dedicated implementation module.

#[cfg(test)]
use std::collections::VecDeque;

use crate::arithmetic::{ArithmeticContext, ArithmeticDecoder, ArithmeticError, ArithmeticInteger};
use crate::bitmap::{Bitmap, CombinationOperator, RowBlit};
use crate::generic_region::{
    decode_generic_on, decode_refinement_on, GenericArithmeticCall, GenericContextSnapshot,
    GenericContexts, GenericRegionError, GenericTemplate, RefinementArithmeticCall,
    RefinementContextSnapshot, RefinementContexts, RefinementParameters, RefinementTemplate,
    TemplateOffset,
};
use crate::huffman::{HuffmanError, HuffmanLine, HuffmanTable, HuffmanValue, StandardHuffmanTable};
use crate::mmr::{decode_mmr, EofbPolicy, MmrError};
use crate::reader::{bit_reader, BitReader, ByteReader};
use crate::segment_data::{parse_region_info, RegionInfo};
use crate::{DecodeError, LimitExceeded, LimitKind, Limits, MalformedError, UnsupportedFeature};
#[cfg(verus_keep_ghost)]
use vstd::layout;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
use vstd::std_specs::convert::{FromSpec, FromSpecImpl};

#[cfg(verus_keep_ghost)]
#[rustfmt::skip]
#[path = "../specs/symbol_text.rs"]
pub(crate) mod symbol_text_spec;

#[cfg(verus_keep_ghost)]
#[rustfmt::skip]
#[path = "../proofs/shared/symbol_text.rs"]
mod symbol_text_shared;

#[cfg(verus_keep_ghost)]
#[rustfmt::skip]
#[path = "../proofs/spec_properties/symbol_text.rs"]
mod symbol_text_properties;

#[cfg(verus_keep_ghost)]
#[macro_use]
#[rustfmt::skip]
#[path = "../proofs/production_impl/symbol_text.rs"]
mod symbol_text_proofs;

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out as int == value as int,
))]
fn widen_u32_i64(value: u32) -> i64 {
    i64::from(value)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out as int == value as int,
))]
fn widen_u8_i64(value: u8) -> i64 {
    i64::from(value)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out as int == value as int,
))]
fn widen_u8_i32(value: u8) -> i32 {
    i32::from(value)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out == if value { 1i64 } else { 0i64 },
))]
// The pinned verifier needs the explicit Boolean relation instead of From<bool>.
#[allow(clippy::bool_to_int_with_if)]
fn widen_bool_i64(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

/// Internal symbol/text decoding failure.
#[derive(Debug)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) enum SymbolTextError {
    Decode(DecodeError),
    InvalidFlags(&'static str),
    InvalidTableAssociation(&'static str),
    InvalidCount(&'static str),
    InvalidDimensions,
    InvalidEntropyValue(&'static str),
    UnexpectedOutOfBand(&'static str),
    SymbolIdOutOfRange { id: u32, available: u32 },
    InvalidSymbolLengthRun,
    InvalidState(&'static str),
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl From<DecodeError> for SymbolTextError {
    fn from(error: DecodeError) -> Self {
        Self::Decode(error)
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl From<MalformedError> for SymbolTextError {
    fn from(error: MalformedError) -> Self {
        Self::Decode(error.into())
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl From<LimitExceeded> for SymbolTextError {
    fn from(error: LimitExceeded) -> Self {
        Self::Decode(error.into())
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl From<HuffmanError> for SymbolTextError {
    fn from(error: HuffmanError) -> Self {
        Self::Decode(error.into())
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl From<GenericRegionError> for SymbolTextError {
    fn from(error: GenericRegionError) -> Self {
        Self::Decode(error.into())
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl From<MmrError> for SymbolTextError {
    fn from(error: MmrError) -> Self {
        Self::Decode(error.into())
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl From<ArithmeticError> for SymbolTextError {
    fn from(error: ArithmeticError) -> Self {
        Self::Decode(error.into())
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl From<SymbolTextError> for DecodeError {
    fn from(error: SymbolTextError) -> Self {
        match error {
            SymbolTextError::Decode(error) => error,
            SymbolTextError::InvalidFlags(reason)
            | SymbolTextError::InvalidTableAssociation(reason)
            | SymbolTextError::InvalidCount(reason)
            | SymbolTextError::InvalidEntropyValue(reason)
            | SymbolTextError::UnexpectedOutOfBand(reason)
            | SymbolTextError::InvalidState(reason) => {
                Self::Malformed(MalformedError::InvalidSymbolText(reason))
            }
            SymbolTextError::InvalidDimensions => {
                Self::Malformed(MalformedError::InvalidSymbolDimensions)
            }
            SymbolTextError::SymbolIdOutOfRange { id, available } => {
                Self::Malformed(MalformedError::SymbolIdOutOfRange { id, available })
            }
            SymbolTextError::InvalidSymbolLengthRun => Self::Malformed(
                MalformedError::InvalidSymbolText("invalid symbol-code length run"),
            ),
        }
    }
}

#[cfg(verus_keep_ghost)]
verus! {

impl FromSpecImpl<DecodeError> for SymbolTextError {
    open spec fn obeys_from_spec() -> bool { true }

    closed spec fn from_spec(error: DecodeError) -> SymbolTextError {
        SymbolTextError::Decode(error)
    }
}

impl FromSpecImpl<MalformedError> for SymbolTextError {
    open spec fn obeys_from_spec() -> bool { true }

    closed spec fn from_spec(error: MalformedError) -> SymbolTextError {
        SymbolTextError::Decode(DecodeError::Malformed(error))
    }
}

impl FromSpecImpl<LimitExceeded> for SymbolTextError {
    open spec fn obeys_from_spec() -> bool { true }

    closed spec fn from_spec(error: LimitExceeded) -> SymbolTextError {
        SymbolTextError::Decode(DecodeError::LimitExceeded(error))
    }
}

impl FromSpecImpl<HuffmanError> for SymbolTextError {
    open spec fn obeys_from_spec() -> bool { true }

    closed spec fn from_spec(error: HuffmanError) -> SymbolTextError {
        SymbolTextError::Decode(<DecodeError as FromSpec<HuffmanError>>::from_spec(error))
    }
}

impl FromSpecImpl<GenericRegionError> for SymbolTextError {
    open spec fn obeys_from_spec() -> bool { true }

    closed spec fn from_spec(error: GenericRegionError) -> SymbolTextError {
        SymbolTextError::Decode(<DecodeError as FromSpec<GenericRegionError>>::from_spec(error))
    }
}

impl FromSpecImpl<MmrError> for SymbolTextError {
    open spec fn obeys_from_spec() -> bool { true }

    closed spec fn from_spec(error: MmrError) -> SymbolTextError {
        SymbolTextError::Decode(<DecodeError as FromSpec<MmrError>>::from_spec(error))
    }
}

impl FromSpecImpl<ArithmeticError> for SymbolTextError {
    open spec fn obeys_from_spec() -> bool { true }

    closed spec fn from_spec(error: ArithmeticError) -> SymbolTextError {
        SymbolTextError::Decode(<DecodeError as FromSpec<ArithmeticError>>::from_spec(error))
    }
}

// Project error policy, discharged by the executing From body through the pinned trait contract.
impl FromSpecImpl<SymbolTextError> for DecodeError {
    open spec fn obeys_from_spec() -> bool {
        true
    }

    closed spec fn from_spec(error: SymbolTextError) -> DecodeError {
        match error {
            SymbolTextError::Decode(error) => error,
            SymbolTextError::InvalidFlags(reason)
            | SymbolTextError::InvalidTableAssociation(reason)
            | SymbolTextError::InvalidCount(reason)
            | SymbolTextError::InvalidEntropyValue(reason)
            | SymbolTextError::UnexpectedOutOfBand(reason)
            | SymbolTextError::InvalidState(reason) =>
                DecodeError::Malformed(MalformedError::InvalidSymbolText(reason)),
            SymbolTextError::InvalidDimensions =>
                DecodeError::Malformed(MalformedError::InvalidSymbolDimensions),
            SymbolTextError::SymbolIdOutOfRange { id, available } =>
                DecodeError::Malformed(MalformedError::SymbolIdOutOfRange { id, available }),
            SymbolTextError::InvalidSymbolLengthRun => DecodeError::Malformed(
                MalformedError::InvalidSymbolText("invalid symbol-code length run")),
        }
    }
}

}

/// Result of decoding an arithmetic or Huffman integer field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) enum EntropyInteger {
    Value(i64),
    OutOfBand,
}

/// Integer field identities used to select independent arithmetic contexts or
/// configured Huffman tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) enum IntegerField {
    AggregationInstances,
    HeightDelta,
    WidthDelta,
    ExportRun,
    StripDelta,
    FirstS,
    SubsequentS,
    WithinStripT,
    RefinementWidth,
    RefinementHeight,
    RefinementX,
    RefinementY,
    RefinementSize,
    RefinementIndicator,
    /// The symbol dictionary's own collective-bitmap size (T.88 6.5.9
    /// `BMSIZE`), kept distinct from [`Self::RefinementSize`] because its
    /// Huffman table is independently selected by `SDHUFFBMSIZE`.
    DictionaryBitmapSize,
}

/// Total number of [`IntegerField`] variants, used to size per-field context
/// storage without depending on arithmetic.rs internals.
#[cfg_attr(verus_keep_ghost, verus_verify)]
const INTEGER_FIELD_COUNT: usize = 15;

impl IntegerField {
    /// A stable, dense index used to select independent context storage.
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out < INTEGER_FIELD_COUNT,
            out == symbol_text_spec::integer_field_slot(self),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    const fn slot(self) -> usize {
        self as usize
    }
}

/// Request supplied to an entropy implementation for symbol-ID decoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct SymbolIdRequest {
    pub(crate) available_symbols: u32,
    pub(crate) code_length: u8,
    pub(crate) huffman: bool,
}

/// Geometry passed to the generic refinement decoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct RefinementGeometry {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) reference_dx: i32,
    pub(crate) reference_dy: i32,
}

/// Complete generic-refinement request, excluding entropy-engine-private state.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct RefinementRequest<'a> {
    pub(crate) reference: &'a Bitmap,
    pub(crate) geometry: RefinementGeometry,
    pub(crate) template: u8,
    pub(crate) adaptive_pixels: [(i8, i8); 2],
    pub(crate) declared_data_size: Option<u32>,
}

/// Tracks abstract work and cumulative bitmap bytes across nested operations.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct ResourceTracker {
    pub(crate) work_units: u64,
    pub(crate) decoded_bitmap_bytes: u64,
}

impl ResourceTracker {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures symbol_text_spec::work_charge_effect(
            old(self), final(self), units, limits, out.is_ok(),
        ),
        match out {
            Ok(()) =>
                symbol_text_spec::work_units(final(self))
                    == symbol_text_spec::work_units(old(self)) + units
                && symbol_text_spec::work_units(final(self)) <= limits.max_work_units
                && symbol_text_spec::bitmap_bytes(final(self))
                    == symbol_text_spec::bitmap_bytes(old(self)),
            Err(_) => symbol_text_spec::bitmap_bytes(final(self))
                == symbol_text_spec::bitmap_bytes(old(self)),
        },
    ))]
    pub(crate) fn charge_work(
        &mut self,
        units: u64,
        limits: &Limits,
    ) -> Result<(), SymbolTextError> {
        #[cfg(verus_keep_ghost)]
        production_symbol_tracker_fields!(self);
        self.work_units = self
            .work_units
            .checked_add(units)
            .ok_or(MalformedError::IntegerOverflow)?;
        enforce_limit(LimitKind::Work, limits.max_work_units, self.work_units)?;
        Ok(())
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures symbol_text_spec::bitmap_charge_effect(
            old(self), final(self), width, height, limits, out.is_ok(),
        ),
        match out {
            Ok(()) =>
                symbol_text_spec::work_units(final(self))
                    == symbol_text_spec::work_units(old(self))
                && symbol_text_spec::bitmap_bytes(final(self))
                    == symbol_text_spec::bitmap_bytes(old(self))
                        + ((width as u64 + 7) / 8) * height as u64
                && symbol_text_spec::bitmap_bytes(final(self))
                    <= limits.max_cumulative_decoded_bitmap_bytes,
            Err(_) => symbol_text_spec::work_units(final(self))
                == symbol_text_spec::work_units(old(self)),
        },
    ))]
    pub(crate) fn charge_bitmap(
        &mut self,
        width: u32,
        height: u32,
        limits: &Limits,
    ) -> Result<(), SymbolTextError> {
        check_dimensions(width, height, limits)?;
        let stride = u64::from(width)
            .checked_add(7)
            .ok_or(MalformedError::IntegerOverflow)?
            / 8;
        let bytes = stride
            .checked_mul(u64::from(height))
            .ok_or(MalformedError::IntegerOverflow)?;
        self.decoded_bitmap_bytes = self
            .decoded_bitmap_bytes
            .checked_add(bytes)
            .ok_or(MalformedError::IntegerOverflow)?;
        enforce_limit(
            LimitKind::DecodedBitmapBytes,
            limits.max_cumulative_decoded_bitmap_bytes,
            self.decoded_bitmap_bytes,
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) enum TableChoice {
    Standard(StandardHuffmanTable),
    Custom,
}

/// Validated symbol-dictionary flag configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct SymbolDictionaryConfig {
    pub(crate) huffman: bool,
    pub(crate) refinement_aggregation: bool,
    pub(crate) height_delta: TableChoice,
    pub(crate) width_delta: TableChoice,
    pub(crate) bitmap_size: TableChoice,
    pub(crate) aggregation_instances: TableChoice,
    pub(crate) bitmap_context_used: bool,
    pub(crate) bitmap_context_retained: bool,
    pub(crate) template: u8,
    pub(crate) refinement_template: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) enum ReferenceCorner {
    BottomLeft,
    TopLeft,
    BottomRight,
    TopRight,
}

/// Validated text-region flag configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct TextRegionConfig {
    pub(crate) huffman: bool,
    pub(crate) refinement: bool,
    pub(crate) strip_size: u8,
    pub(crate) reference_corner: ReferenceCorner,
    pub(crate) transposed: bool,
    pub(crate) operator: CombinationOperator,
    pub(crate) default_pixel: bool,
    pub(crate) s_offset: i8,
    pub(crate) refinement_template: u8,
    pub(crate) huffman_choices: Option<TextHuffmanChoices>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct TextHuffmanChoices {
    pub(crate) first_s: TableChoice,
    pub(crate) subsequent_s: TableChoice,
    pub(crate) strip_delta: TableChoice,
    pub(crate) refinement_width: TableChoice,
    pub(crate) refinement_height: TableChoice,
    pub(crate) refinement_x: TableChoice,
    pub(crate) refinement_y: TableChoice,
    pub(crate) refinement_size: TableChoice,
}

/// Metadata accompanying a referred custom Huffman table.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct CustomHuffmanTable<'a> {
    pub(crate) table: &'a HuffmanTable,
    pub(crate) has_oob: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) enum TableParameter {
    DictionaryHeight,
    DictionaryWidth,
    DictionaryBitmapSize,
    DictionaryAggregationInstances,
    TextFirstS,
    TextSubsequentS,
    TextStripDelta,
    TextRefinementWidth,
    TextRefinementHeight,
    TextRefinementX,
    TextRefinementY,
    TextRefinementSize,
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) enum BoundHuffmanTable<'a> {
    Standard(StandardHuffmanTable),
    Custom(&'a HuffmanTable),
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct TableBinding<'a> {
    pub(crate) parameter: TableParameter,
    pub(crate) table: BoundHuffmanTable<'a>,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
const MAX_TABLE_BINDINGS: usize = 8;

#[derive(Debug, Clone, Copy)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
struct TableBindings<'a> {
    entries: [Option<TableBinding<'a>>; MAX_TABLE_BINDINGS],
    len: usize,
}

/// Cumulative height/width-class state for a symbol dictionary.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct DictionaryClassState {
    pub(crate) declared_new_symbols: u32,
    pub(crate) decoded_symbols: u32,
    pub(crate) height: u32,
    pub(crate) width: u32,
    pub(crate) total_width: u32,
    pub(crate) class_open: bool,
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(value) =>
            value as int == current as int + delta as int
            && value <= maximum,
        Err(_) => true,
    },
))]
fn checked_unsigned_delta(current: u32, delta: i64, maximum: u32) -> Result<u32, SymbolTextError> {
    let result = widen_u32_i64(current)
        .checked_add(delta)
        .ok_or(MalformedError::IntegerOverflow)?;
    let result = u32::try_from(result).map_err(|_error| SymbolTextError::InvalidDimensions)?;
    if result > maximum {
        return Err(SymbolTextError::InvalidDimensions);
    }
    Ok(result)
}

/// One decoded symbol-code-length run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) enum SymbolLengthRun {
    Literal(u8),
    RepeatPrevious(u8),
    Zeros(u16),
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires symbol_text_spec::codec_valid(old(codec)),
    ensures match out {
        Ok(_) => symbol_text_spec::codec_valid(final(codec)),
        Err(_) => true,
    },
))]
fn required_integer(
    codec: &mut SymbolCodec<'_>,
    field: IntegerField,
) -> Result<i64, SymbolTextError> {
    match codec.decode_integer(field)? {
        EntropyInteger::Value(value) => Ok(value),
        EntropyInteger::OutOfBand => Err(SymbolTextError::UnexpectedOutOfBand("required integer")),
    }
}

/// Physical top-left placement of one decoded symbol bitmap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct Placement {
    pub(crate) x: i64,
    pub(crate) y: i64,
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct StripState {
    pub(crate) cursor_s: i64,
    pub(crate) has_instance: bool,
}

/// Strip/S-coordinate state machine for one text region.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct TextRegionState {
    pub(crate) config: TextRegionConfig,
    pub(crate) declared_instances: u32,
    pub(crate) decoded_instances: u32,
    pub(crate) strip_t: i64,
    pub(crate) first_s: i64,
    pub(crate) strip: Option<StripState>,
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct PreparedPlacement<'a> {
    pub(crate) blit: RowBlit<'a>,
    pub(crate) source_y: u32,
    pub(crate) destination_y: u32,
    pub(crate) height: u32,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) const MAX_PLACEMENT_BATCH: usize = 16;

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct PlacementBatch<'a> {
    pub(crate) blits: Vec<RowBlit<'a>>,
    pub(crate) source_y: u32,
    pub(crate) destination_y: u32,
    pub(crate) height: u32,
    pub(crate) source_shift: u32,
    pub(crate) destination_shift: u32,
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out.is_ok() == (
        width <= limits.max_width
        && height <= limits.max_height
        && width as nat * height as nat <= limits.max_pixels_per_page
    ),
    match out {
        Ok(()) =>
            width <= limits.max_width
            && height <= limits.max_height
            && width as nat * height as nat <= limits.max_pixels_per_page,
        Err(_) => true,
    },
))]
fn check_dimensions(width: u32, height: u32, limits: &Limits) -> Result<(), SymbolTextError> {
    enforce_limit(
        LimitKind::Width,
        u64::from(limits.max_width),
        u64::from(width),
    )?;
    enforce_limit(
        LimitKind::Height,
        u64::from(limits.max_height),
        u64::from(height),
    )?;
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or(MalformedError::IntegerOverflow)?;
    enforce_limit(LimitKind::PagePixels, limits.max_pixels_per_page, pixels)?;
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) => actual <= limit,
        Err(_) => actual > limit,
    },
))]
fn enforce_limit(kind: LimitKind, limit: u64, actual: u64) -> Result<(), SymbolTextError> {
    if actual > limit {
        return Err(LimitExceeded {
            kind,
            limit,
            actual,
        }
        .into());
    }
    Ok(())
}

/// Complete, validated symbol-dictionary segment data header.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct SymbolDictionaryHeader {
    pub(crate) config: SymbolDictionaryConfig,
    pub(crate) adaptive: [TemplateOffset; 4],
    pub(crate) adaptive_count: u8,
    pub(crate) refinement_adaptive: [TemplateOffset; 2],
    pub(crate) exported_symbols: u32,
    pub(crate) new_symbols: u32,
}

/// Complete, validated text-region segment data header.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct TextRegionHeader {
    pub(crate) region: RegionInfo,
    pub(crate) config: TextRegionConfig,
    pub(crate) refinement_adaptive: [TemplateOffset; 2],
    pub(crate) number_of_instances: u32,
}

// ---------------------------------------------------------------------------
// Huffman table resolution.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
enum MaterializedHuffmanTable<'a> {
    Standard(StandardHuffmanTable),
    Custom(&'a HuffmanTable),
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
struct StandardHuffmanTables {
    a: Option<Box<HuffmanTable>>,
    b: Option<Box<HuffmanTable>>,
    c: Option<Box<HuffmanTable>>,
    d: Option<Box<HuffmanTable>>,
    e: Option<Box<HuffmanTable>>,
    f: Option<Box<HuffmanTable>>,
    g: Option<Box<HuffmanTable>>,
    h: Option<Box<HuffmanTable>>,
    i: Option<Box<HuffmanTable>>,
    j: Option<Box<HuffmanTable>>,
    k: Option<Box<HuffmanTable>>,
    l: Option<Box<HuffmanTable>>,
    m: Option<Box<HuffmanTable>>,
    n: Option<Box<HuffmanTable>>,
    o: Option<Box<HuffmanTable>>,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
struct HuffmanFieldChoices<'a> {
    aggregation_instances: Option<BoundHuffmanTable<'a>>,
    height_delta: Option<BoundHuffmanTable<'a>>,
    width_delta: Option<BoundHuffmanTable<'a>>,
    export_run: Option<BoundHuffmanTable<'a>>,
    strip_delta: Option<BoundHuffmanTable<'a>>,
    first_s: Option<BoundHuffmanTable<'a>>,
    subsequent_s: Option<BoundHuffmanTable<'a>>,
    refinement_width: Option<BoundHuffmanTable<'a>>,
    refinement_height: Option<BoundHuffmanTable<'a>>,
    refinement_x: Option<BoundHuffmanTable<'a>>,
    refinement_y: Option<BoundHuffmanTable<'a>>,
    refinement_size: Option<BoundHuffmanTable<'a>>,
    dictionary_bitmap_size: Option<BoundHuffmanTable<'a>>,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
enum HuffmanChoiceSet<'a> {
    Dictionary(HuffmanFieldChoices<'a>),
    TextRegion(HuffmanFieldChoices<'a>),
}

macro_rules! materialize_huffman_tables {
    ($choice_set:expr, $limits:expr) => {{
        let choice_set = $choice_set;
        let limits = $limits;
        let mut standard_tables = StandardHuffmanTables::empty();
        macro_rules! resolve {
            ($choice:expr) => {
                materialize_huffman_table($choice, &mut standard_tables, limits)?
            };
        }

        let tables = match choice_set {
            HuffmanChoiceSet::Dictionary(choices) => {
                let height_delta = resolve!(choices.height_delta);
                let width_delta = resolve!(choices.width_delta);
                let export_run = resolve!(choices.export_run);
                let dictionary_bitmap_size = resolve!(choices.dictionary_bitmap_size);
                let aggregation_instances = resolve!(choices.aggregation_instances);
                let refinement_x = resolve!(choices.refinement_x);
                let refinement_y = resolve!(choices.refinement_y);
                let refinement_size = resolve!(choices.refinement_size);
                HuffmanFieldTables {
                    standard_tables,
                    aggregation_instances,
                    height_delta,
                    width_delta,
                    export_run,
                    strip_delta: None,
                    first_s: None,
                    subsequent_s: None,
                    refinement_width: None,
                    refinement_height: None,
                    refinement_x,
                    refinement_y,
                    refinement_size,
                    dictionary_bitmap_size,
                }
            }
            HuffmanChoiceSet::TextRegion(choices) => {
                let first_s = resolve!(choices.first_s);
                let subsequent_s = resolve!(choices.subsequent_s);
                let strip_delta = resolve!(choices.strip_delta);
                let refinement_width = resolve!(choices.refinement_width);
                let refinement_height = resolve!(choices.refinement_height);
                let refinement_x = resolve!(choices.refinement_x);
                let refinement_y = resolve!(choices.refinement_y);
                let refinement_size = resolve!(choices.refinement_size);
                HuffmanFieldTables {
                    standard_tables,
                    aggregation_instances: None,
                    height_delta: None,
                    width_delta: None,
                    export_run: None,
                    strip_delta,
                    first_s,
                    subsequent_s,
                    refinement_width,
                    refinement_height,
                    refinement_x,
                    refinement_y,
                    refinement_size,
                    dictionary_bitmap_size: None,
                }
            }
        };
        Ok::<HuffmanFieldTables<'_>, SymbolTextError>(tables)
    }};
}

/// Resolved Huffman tables for every table-backed [`IntegerField`].
///
/// [`IntegerField::WithinStripT`] and [`IntegerField::RefinementIndicator`]
/// are intentionally absent: T.88 decodes both as raw fixed-width bits, never
/// through a Huffman table.
#[cfg_attr(verus_keep_ghost, verus_verify)]
struct HuffmanFieldTables<'a> {
    standard_tables: StandardHuffmanTables,
    aggregation_instances: Option<MaterializedHuffmanTable<'a>>,
    height_delta: Option<MaterializedHuffmanTable<'a>>,
    width_delta: Option<MaterializedHuffmanTable<'a>>,
    export_run: Option<MaterializedHuffmanTable<'a>>,
    strip_delta: Option<MaterializedHuffmanTable<'a>>,
    first_s: Option<MaterializedHuffmanTable<'a>>,
    subsequent_s: Option<MaterializedHuffmanTable<'a>>,
    refinement_width: Option<MaterializedHuffmanTable<'a>>,
    refinement_height: Option<MaterializedHuffmanTable<'a>>,
    refinement_x: Option<MaterializedHuffmanTable<'a>>,
    refinement_y: Option<MaterializedHuffmanTable<'a>>,
    refinement_size: Option<MaterializedHuffmanTable<'a>>,
    dictionary_bitmap_size: Option<MaterializedHuffmanTable<'a>>,
}

// ---------------------------------------------------------------------------
// Concrete entropy/refinement state.
//
// Each variant owns exactly one cursor. Integer, symbol-ID, generic-region,
// and refinement operations therefore advance the same cursor continuously
// without interior mutability or callback dispatch.
// ---------------------------------------------------------------------------

/// Per-[`IntegerField`] arithmetic probability contexts.
#[cfg_attr(verus_keep_ghost, verus_verify)]
struct IntegerContexts {
    slots: [[ArithmeticContext; 512]; INTEGER_FIELD_COUNT],
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
struct ArithmeticCodec<'a> {
    decoder: ArithmeticDecoder<'a>,
    integers: IntegerContexts,
    iaid: Vec<ArithmeticContext>,
    refinement: Option<RefinementContexts>,
    refinement_work: u64,
}

/// Concrete state for every symbol/text entropy mode.
#[cfg_attr(verus_keep_ghost, verus_verify)]
// Boxing either large variant would introduce an unaccounted infallible allocation.
#[allow(clippy::large_enum_variant)]
enum SymbolCodec<'a> {
    Arithmetic(ArithmeticCodec<'a>),
    Huffman(HuffmanCodec<'a>),
    #[cfg(test)]
    Scripted(ScriptedCodec),
}

#[cfg(test)]
struct ScriptedCodec {
    integers: VecDeque<EntropyInteger>,
    symbol_id: u32,
}

// ---------------------------------------------------------------------------
// Huffman entropy adapter.
// ---------------------------------------------------------------------------

/// The two-byte sequence that must terminate a bounded arithmetic block.
#[cfg_attr(verus_keep_ghost, verus_verify)]
const BOUNDED_ARITHMETIC_TERMINATOR: [u8; 2] = [0xff, 0xac];

#[cfg_attr(verus_keep_ghost, verus_verify)]
enum NormalizedArithmetic<'a> {
    Borrowed(&'a [u8]),
    Owned(Vec<u8>),
}

/// Owns the single Annex B cursor and any arithmetic-refinement state embedded
/// in that same Huffman-coded segment.
#[cfg_attr(verus_keep_ghost, verus_verify)]
struct HuffmanCodec<'a> {
    reader: BitReader<'a>,
    tables: HuffmanFieldTables<'a>,
    log_strip_size: u8,
    symbol_id_table: Option<HuffmanTable>,
    refinement: Option<RefinementContexts>,
    refinement_work: u64,
}

macro_rules! prepare_symbol_ids {
    ($codec:expr, $symbol_count:expr, $limits:expr) => {{
        let codec = $codec;
        let symbol_count = $symbol_count;
        let limits = $limits;
        let runcode_lines = read_symbol_id_runcode_lines(&mut codec.reader)?;
        let runcode_table = HuffmanTable::new(&runcode_lines, limits)?;
        let lengths =
            decode_symbol_id_code_lengths(&mut codec.reader, &runcode_table, symbol_count, limits)?;
        codec.symbol_id_table = Some(HuffmanTable::new_symbol_ids(&lengths, limits)?);
        byte_align(&mut codec.reader)?;
        Ok::<(), SymbolTextError>(())
    }};
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) enum SymbolCodebook<'a> {
    Contiguous(&'a [Bitmap]),
    Borrowed(&'a [&'a Bitmap]),
}

impl<'a> SymbolCodebook<'a> {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out as nat == symbol_text_spec::codebook_len(self),
    ))]
    pub(crate) fn len(self) -> usize {
        match self {
            Self::Contiguous(symbols) => symbols.len(),
            Self::Borrowed(symbols) => symbols.len(),
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires symbol_text_spec::codebook_valid(self),
        ensures match out {
            Some(bitmap) => crate::spec::bitmap::storage_valid(bitmap)
                && symbol_text_spec::codebook_selects(self, index as nat, bitmap),
            None => index as nat >= symbol_text_spec::codebook_len(self),
        },
    ))]
    pub(crate) fn get(self, index: usize) -> Option<&'a Bitmap> {
        match self {
            Self::Contiguous(symbols) => symbols.get(index),
            Self::Borrowed(symbols) => match symbols.get(index) {
                Some(bitmap) => Some(*bitmap),
                None => None,
            },
        }
    }
}

/// Looks up a symbol by ID across the imported and already-decoded-new
/// symbol collections, as used by T.88 6.5.8.2.2's single-instance
/// refinement/aggregate procedure.
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        symbol_text_spec::codebook_valid(imported),
        symbol_text_spec::bitmaps_valid(new_symbols@),
    ensures match out {
        Ok(bitmap) => crate::spec::bitmap::storage_valid(bitmap)
            && symbol_text_spec::symbol_id_selects(imported, new_symbols@, id, bitmap),
        Err(_) => true,
    },
    (symbol_text_spec::codebook_len(imported) + new_symbols@.len() <= usize::MAX
        && symbol_text_spec::codebook_len(imported) + new_symbols@.len() <= u32::MAX
        && (id as nat) < symbol_text_spec::codebook_len(imported) + new_symbols@.len())
        ==> out.is_ok(),
))]
fn symbol_by_id<'a>(
    imported: SymbolCodebook<'a>,
    new_symbols: &'a [Bitmap],
    id: u32,
) -> Result<&'a Bitmap, SymbolTextError> {
    let id_usize = usize::try_from(id).map_err(|_error| MalformedError::IntegerOverflow)?;
    let available = imported
        .len()
        .checked_add(new_symbols.len())
        .ok_or(MalformedError::IntegerOverflow)?;
    let available_u32 =
        u32::try_from(available).map_err(|_error| MalformedError::IntegerOverflow)?;
    if id_usize < imported.len() {
        imported
            .get(id_usize)
            .ok_or(SymbolTextError::SymbolIdOutOfRange {
                id,
                available: available_u32,
            })
    } else {
        let new_index = id_usize - imported.len();
        if new_index < new_symbols.len() {
            Ok(&new_symbols[new_index])
        } else {
            Err(SymbolTextError::SymbolIdOutOfRange {
                id,
                available: available_u32,
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Symbol-dictionary payload decoding (T.88 6.5).
// ---------------------------------------------------------------------------

/// Caller-retained generic/refinement probability state, carried across
/// symbol dictionaries that cooperatively share one evolving symbol set
/// (T.88 6.5.8.2.3's "bitmap coding context used"/"retained" flags).
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct DictionaryContexts {
    pub(crate) generic: Option<GenericContextSnapshot>,
    pub(crate) refinement: Option<RefinementContextSnapshot>,
}

/// Complete input to one symbol-dictionary segment payload decode.
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct SymbolDictionaryInput<'a> {
    pub(crate) payload: &'a [u8],
    pub(crate) imported_symbols: SymbolCodebook<'a>,
    pub(crate) custom_tables: &'a [&'a HuffmanTable],
    pub(crate) used_contexts: Option<&'a DictionaryContexts>,
    pub(crate) limits: &'a Limits,
}

/// Complete output of one symbol-dictionary segment payload decode.
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct SymbolDictionaryOutput {
    pub(crate) exported_symbols: Vec<Bitmap>,
    pub(crate) retained_contexts: Option<DictionaryContexts>,
    pub(crate) consumed_bytes: usize,
}

/// `(new/exported symbols, retained generic contexts, retained refinement
/// contexts, bytes consumed within the body)`.
type ArithmeticDictionaryResult = Result<
    (
        Vec<Bitmap>,
        Option<GenericContextSnapshot>,
        Option<RefinementContextSnapshot>,
        usize,
    ),
    SymbolTextError,
>;

// ---------------------------------------------------------------------------
// Text-region payload decoding (T.88 6.4, 7.4.3).
// ---------------------------------------------------------------------------

/// Complete input to one text-region segment payload decode.
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct TextRegionInput<'a> {
    pub(crate) payload: &'a [u8],
    pub(crate) symbols: SymbolCodebook<'a>,
    pub(crate) custom_tables: &'a [&'a HuffmanTable],
    pub(crate) limits: &'a Limits,
}

/// Complete output of one text-region segment payload decode.
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct TextRegionOutput {
    pub(crate) region: RegionInfo,
    pub(crate) bitmap: Bitmap,
    pub(crate) consumed_bytes: usize,
}

mod bitmap;
mod codec;
mod dictionary;
mod headers;
mod placement;
mod tables;
mod text;

#[allow(unused_imports)]
pub(crate) use codec::{checked_symbol_id, refinement_geometry, symbol_id_code_length};
pub(crate) use dictionary::decode_symbol_dictionary;
#[allow(unused_imports)]
pub(crate) use headers::{
    check_symbol_counts, parse_symbol_dictionary_flags, parse_symbol_dictionary_header,
    parse_text_region_flags, parse_text_region_header,
};
pub(crate) use placement::compose_placement;
pub(crate) use text::decode_text_region;

#[cfg(test)]
mod tests;

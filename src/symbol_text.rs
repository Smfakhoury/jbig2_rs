//! Symbol-dictionary and text-region decoding helpers.
//!
//! This module deliberately separates the normative state machines from the
//! entropy and generic-refinement engines.  Segment wiring can therefore bind
//! either arithmetic or Huffman integer decoders without duplicating the
//! bounds, placement, and progress checks implemented here.

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

impl From<DecodeError> for SymbolTextError {
    fn from(error: DecodeError) -> Self {
        Self::Decode(error)
    }
}

impl From<MalformedError> for SymbolTextError {
    fn from(error: MalformedError) -> Self {
        Self::Decode(error.into())
    }
}

impl From<LimitExceeded> for SymbolTextError {
    fn from(error: LimitExceeded) -> Self {
        Self::Decode(error.into())
    }
}

impl From<HuffmanError> for SymbolTextError {
    fn from(error: HuffmanError) -> Self {
        Self::Decode(error.into())
    }
}

impl From<GenericRegionError> for SymbolTextError {
    fn from(error: GenericRegionError) -> Self {
        Self::Decode(error.into())
    }
}

impl From<MmrError> for SymbolTextError {
    fn from(error: MmrError) -> Self {
        Self::Decode(error.into())
    }
}

impl From<ArithmeticError> for SymbolTextError {
    fn from(error: ArithmeticError) -> Self {
        Self::Decode(error.into())
    }
}

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
        ensures match out {
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
        ensures match out {
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

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(config) => symbol_text_spec::dictionary_config_safe(flags, config),
        Err(_) => true,
    },
))]
pub(crate) fn parse_symbol_dictionary_flags(
    flags: u16,
) -> Result<SymbolDictionaryConfig, SymbolTextError> {
    if flags & 0xe000 != 0 {
        return Err(SymbolTextError::InvalidFlags(
            "reserved symbol-dictionary flags",
        ));
    }
    let huffman = flags & 1 != 0;
    let refinement_aggregation = flags & 2 != 0;
    let height_selector = (flags / 4) % 4;
    let width_selector = (flags / 16) % 4;
    let dh = u8::try_from(height_selector).map_err(|_error| MalformedError::IntegerOverflow)?;
    let dw = u8::try_from(width_selector).map_err(|_error| MalformedError::IntegerOverflow)?;
    let bitmap_size_custom = flags & 0x0040 != 0;
    let aggregation_custom = flags & 0x0080 != 0;
    let bitmap_context_used = flags & 0x0100 != 0;
    let bitmap_context_retained = flags & 0x0200 != 0;
    let template_raw = (flags / 1_024) % 4;
    let refinement_template_raw = (flags / 4_096) % 2;
    let template = u8::try_from(template_raw).map_err(|_error| MalformedError::IntegerOverflow)?;
    let refinement_template =
        u8::try_from(refinement_template_raw).map_err(|_error| MalformedError::IntegerOverflow)?;

    if !huffman && (dh != 0 || dw != 0 || bitmap_size_custom || aggregation_custom) {
        return Err(SymbolTextError::InvalidFlags(
            "Huffman selections in arithmetic dictionary",
        ));
    }
    if huffman && template != 0 {
        return Err(SymbolTextError::InvalidFlags(
            "direct bitmap template in Huffman dictionary",
        ));
    }
    if !refinement_aggregation && (aggregation_custom || refinement_template != 0) {
        return Err(SymbolTextError::InvalidFlags(
            "aggregation fields without refinement aggregation",
        ));
    }
    if huffman && !refinement_aggregation && (bitmap_context_used || bitmap_context_retained) {
        return Err(SymbolTextError::InvalidFlags(
            "bitmap contexts in direct Huffman dictionary",
        ));
    }

    let config = SymbolDictionaryConfig {
        huffman,
        refinement_aggregation,
        height_delta: two_bit_choice(
            dh,
            StandardHuffmanTable::D,
            StandardHuffmanTable::E,
            "invalid height-delta table selection",
        )?,
        width_delta: two_bit_choice(
            dw,
            StandardHuffmanTable::B,
            StandardHuffmanTable::C,
            "invalid width-delta table selection",
        )?,
        bitmap_size: if bitmap_size_custom {
            TableChoice::Custom
        } else {
            TableChoice::Standard(StandardHuffmanTable::A)
        },
        aggregation_instances: if aggregation_custom {
            TableChoice::Custom
        } else {
            TableChoice::Standard(StandardHuffmanTable::A)
        },
        bitmap_context_used,
        bitmap_context_retained,
        template,
        refinement_template,
    };
    #[cfg(verus_keep_ghost)]
    proof! {
        reveal(symbol_text_spec::dictionary_config_safe);
        assert(config.template == template);
        assert(config.refinement_template == refinement_template);
        assert(symbol_text_spec::dictionary_config_safe(flags, config));
    }
    Ok(config)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(choice) => symbol_text_spec::two_bit_choice_result(
            encoded, first, second, choice
        ),
        Err(_) => encoded == 2 || encoded > 3,
    },
))]
fn two_bit_choice(
    encoded: u8,
    first: StandardHuffmanTable,
    second: StandardHuffmanTable,
    error: &'static str,
) -> Result<TableChoice, SymbolTextError> {
    match encoded {
        0 => Ok(TableChoice::Standard(first)),
        1 => Ok(TableChoice::Standard(second)),
        3 => Ok(TableChoice::Custom),
        _ => Err(SymbolTextError::InvalidFlags(error)),
    }
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

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(config) => symbol_text_spec::text_config_safe(
            flags, huffman_flags, colour_extension, config
        ),
        Err(_) => true,
    },
))]
pub(crate) fn parse_text_region_flags(
    flags: u16,
    huffman_flags: Option<u16>,
    colour_extension: bool,
) -> Result<TextRegionConfig, SymbolTextError> {
    if colour_extension {
        return Err(SymbolTextError::Decode(DecodeError::Unsupported(
            UnsupportedFeature::Colour,
        )));
    }
    let huffman = flags & 1 != 0;
    let refinement = flags & 2 != 0;
    let log_strips_raw = (flags / 4) % 4;
    let log_strips =
        u8::try_from(log_strips_raw).map_err(|_error| MalformedError::IntegerOverflow)?;
    let strip_size = match log_strips {
        0 => 1,
        1 => 2,
        2 => 4,
        _ => 8,
    };
    let corner = match (flags >> 4) & 3 {
        0 => ReferenceCorner::BottomLeft,
        1 => ReferenceCorner::TopLeft,
        2 => ReferenceCorner::BottomRight,
        3 => ReferenceCorner::TopRight,
        _ => return Err(SymbolTextError::InvalidFlags("invalid reference corner")),
    };
    let operator = match (flags >> 7) & 3 {
        0 => CombinationOperator::Or,
        1 => CombinationOperator::And,
        2 => CombinationOperator::Xor,
        3 => CombinationOperator::Xnor,
        _ => {
            return Err(SymbolTextError::InvalidFlags(
                "invalid text combination operator",
            ))
        }
    };
    let refinement_template_raw = (flags / 32_768) % 2;
    let refinement_template =
        u8::try_from(refinement_template_raw).map_err(|_error| MalformedError::IntegerOverflow)?;
    if !refinement && refinement_template != 0 {
        return Err(SymbolTextError::InvalidFlags(
            "refinement template without refinement",
        ));
    }
    let raw_offset_bits = (flags / 1_024) % 32;
    let raw_offset =
        i8::try_from(raw_offset_bits).map_err(|_error| MalformedError::IntegerOverflow)?;
    let s_offset = if raw_offset & 0x10 != 0 {
        raw_offset - 0x20
    } else {
        raw_offset
    };
    let choices = match (huffman, huffman_flags) {
        (true, Some(value)) => Some(parse_text_huffman_flags(value, refinement)?),
        (true, None) => return Err(SymbolTextError::InvalidFlags("missing text Huffman flags")),
        (false, Some(_)) => {
            return Err(SymbolTextError::InvalidFlags(
                "text Huffman flags in arithmetic region",
            ))
        }
        (false, None) => None,
    };

    let config = TextRegionConfig {
        huffman,
        refinement,
        strip_size,
        reference_corner: corner,
        transposed: flags & 0x0040 != 0,
        operator,
        default_pixel: flags & 0x0200 != 0,
        s_offset,
        refinement_template,
        huffman_choices: choices,
    };
    #[cfg(verus_keep_ghost)]
    proof! {
        reveal(symbol_text_spec::text_config_safe);
        assert(symbol_text_spec::text_config_safe(
            flags, huffman_flags, colour_extension, config
        ));
    }
    Ok(config)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(choices) => symbol_text_spec::text_huffman_choices_safe(
            flags, refinement, choices
        ),
        Err(_) => true,
    },
))]
fn parse_text_huffman_flags(
    flags: u16,
    refinement: bool,
) -> Result<TextHuffmanChoices, SymbolTextError> {
    if flags & 0x8000 != 0 {
        return Err(SymbolTextError::InvalidFlags("reserved text Huffman flag"));
    }
    let first_s = two_bit_choice(
        text_huffman_selector(flags, 0),
        StandardHuffmanTable::F,
        StandardHuffmanTable::G,
        "invalid first-S table selection",
    )?;
    let subsequent_s = four_way_choice(
        text_huffman_selector(flags, 2),
        StandardHuffmanTable::H,
        StandardHuffmanTable::I,
        StandardHuffmanTable::J,
    );
    let strip_delta = four_way_choice(
        text_huffman_selector(flags, 4),
        StandardHuffmanTable::K,
        StandardHuffmanTable::L,
        StandardHuffmanTable::M,
    );
    let refinement_width = refinement_choice(text_huffman_selector(flags, 6))?;
    let refinement_height = refinement_choice(text_huffman_selector(flags, 8))?;
    let refinement_x = refinement_choice(text_huffman_selector(flags, 10))?;
    let refinement_y = refinement_choice(text_huffman_selector(flags, 12))?;
    let refinement_size = if flags & 0x4000 != 0 {
        TableChoice::Custom
    } else {
        TableChoice::Standard(StandardHuffmanTable::A)
    };
    if !refinement && flags & 0x7fc0 != 0 {
        return Err(SymbolTextError::InvalidFlags(
            "refinement Huffman fields without refinement",
        ));
    }
    Ok(TextHuffmanChoices {
        first_s,
        subsequent_s,
        strip_delta,
        refinement_width,
        refinement_height,
        refinement_x,
        refinement_y,
        refinement_size,
    })
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires shift <= 12,
    ensures out <= u8::MAX,
))]
fn text_huffman_selector(flags: u16, shift: u32) -> u8 {
    let selected = (flags >> shift) & 3;
    selected as u8
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures symbol_text_spec::four_way_choice_result(
        encoded, first, second, third, out
    ),
))]
fn four_way_choice(
    encoded: u8,
    first: StandardHuffmanTable,
    second: StandardHuffmanTable,
    third: StandardHuffmanTable,
) -> TableChoice {
    match encoded {
        0 => TableChoice::Standard(first),
        1 => TableChoice::Standard(second),
        2 => TableChoice::Standard(third),
        _ => TableChoice::Custom,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(choice) => symbol_text_spec::refinement_choice_result(encoded, choice),
        Err(_) => encoded == 2 || encoded > 3,
    },
))]
fn refinement_choice(encoded: u8) -> Result<TableChoice, SymbolTextError> {
    two_bit_choice(
        encoded,
        StandardHuffmanTable::N,
        StandardHuffmanTable::O,
        "invalid refinement table selection",
    )
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

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out == (left == right),
))]
fn same_table_parameter(left: TableParameter, right: TableParameter) -> bool {
    matches!(
        (left, right),
        (
            TableParameter::DictionaryHeight,
            TableParameter::DictionaryHeight
        ) | (
            TableParameter::DictionaryWidth,
            TableParameter::DictionaryWidth
        ) | (
            TableParameter::DictionaryBitmapSize,
            TableParameter::DictionaryBitmapSize
        ) | (
            TableParameter::DictionaryAggregationInstances,
            TableParameter::DictionaryAggregationInstances
        ) | (TableParameter::TextFirstS, TableParameter::TextFirstS)
            | (
                TableParameter::TextSubsequentS,
                TableParameter::TextSubsequentS
            )
            | (
                TableParameter::TextStripDelta,
                TableParameter::TextStripDelta
            )
            | (
                TableParameter::TextRefinementWidth,
                TableParameter::TextRefinementWidth
            )
            | (
                TableParameter::TextRefinementHeight,
                TableParameter::TextRefinementHeight
            )
            | (
                TableParameter::TextRefinementX,
                TableParameter::TextRefinementX
            )
            | (
                TableParameter::TextRefinementY,
                TableParameter::TextRefinementY
            )
            | (
                TableParameter::TextRefinementSize,
                TableParameter::TextRefinementSize
            )
    )
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

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(bindings) => symbol_text_spec::table_bindings_valid(&bindings),
        Err(_) => true,
    },
))]
fn associate_dictionary_tables<'a>(
    config: &SymbolDictionaryConfig,
    custom: &'a [CustomHuffmanTable<'a>],
) -> Result<TableBindings<'a>, SymbolTextError> {
    if !config.huffman {
        if custom.is_empty() {
            return Ok(TableBindings {
                entries: [None; MAX_TABLE_BINDINGS],
                len: 0,
            });
        }
        return Err(SymbolTextError::InvalidTableAssociation(
            "custom tables on arithmetic dictionary",
        ));
    }
    let specifications = [
        (TableParameter::DictionaryHeight, config.height_delta, false),
        (TableParameter::DictionaryWidth, config.width_delta, true),
        (
            TableParameter::DictionaryBitmapSize,
            config.bitmap_size,
            false,
        ),
        (
            TableParameter::DictionaryAggregationInstances,
            config.aggregation_instances,
            false,
        ),
    ];
    bind_tables(&specifications, custom)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(bindings) => symbol_text_spec::table_bindings_valid(&bindings),
        Err(_) => true,
    },
))]
fn associate_text_tables<'a>(
    config: &TextRegionConfig,
    custom: &'a [CustomHuffmanTable<'a>],
) -> Result<TableBindings<'a>, SymbolTextError> {
    let Some(choices) = config.huffman_choices else {
        if custom.is_empty() {
            return Ok(TableBindings {
                entries: [None; MAX_TABLE_BINDINGS],
                len: 0,
            });
        }
        return Err(SymbolTextError::InvalidTableAssociation(
            "custom tables on arithmetic text region",
        ));
    };
    let specifications = [
        (TableParameter::TextFirstS, choices.first_s, false),
        (TableParameter::TextSubsequentS, choices.subsequent_s, true),
        (TableParameter::TextStripDelta, choices.strip_delta, false),
        (
            TableParameter::TextRefinementWidth,
            choices.refinement_width,
            false,
        ),
        (
            TableParameter::TextRefinementHeight,
            choices.refinement_height,
            false,
        ),
        (TableParameter::TextRefinementX, choices.refinement_x, false),
        (TableParameter::TextRefinementY, choices.refinement_y, false),
        (
            TableParameter::TextRefinementSize,
            choices.refinement_size,
            false,
        ),
    ];
    bind_tables(&specifications, custom)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(bindings) =>
            symbol_text_spec::table_bindings_valid(&bindings)
                && bindings.len == specifications@.len(),
        Err(_) => true,
    },
))]
fn bind_tables<'a>(
    specifications: &[(TableParameter, TableChoice, bool)],
    custom: &'a [CustomHuffmanTable<'a>],
) -> Result<TableBindings<'a>, SymbolTextError> {
    if specifications.len() > MAX_TABLE_BINDINGS {
        return Err(SymbolTextError::InvalidTableAssociation(
            "too many table selectors",
        ));
    }
    let mut custom_count = 0_usize;
    let mut specification_index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            specification_index <= specifications@.len(),
            custom_count <= specification_index,
        decreases specifications@.len() - specification_index,
    ))]
    while specification_index < specifications.len() {
        if matches!(specifications[specification_index].1, TableChoice::Custom) {
            custom_count += 1;
        }
        specification_index += 1;
    }
    if custom_count != custom.len() {
        return Err(SymbolTextError::InvalidTableAssociation(
            "custom table count does not match selectors",
        ));
    }
    let mut bindings = TableBindings {
        entries: [None; MAX_TABLE_BINDINGS],
        len: 0,
    };
    let mut custom_index = 0_usize;
    let mut specification_index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            specification_index <= specifications@.len(),
            specifications@.len() <= MAX_TABLE_BINDINGS,
            bindings.len == specification_index,
            symbol_text_spec::table_bindings_valid(&bindings),
            custom_index <= custom@.len(),
        decreases specifications@.len() - specification_index,
    ))]
    while specification_index < specifications.len() {
        let parameter = specifications[specification_index].0;
        let choice = specifications[specification_index].1;
        let requires_oob = specifications[specification_index].2;
        let table =
            match choice {
                TableChoice::Standard(table) => BoundHuffmanTable::Standard(table),
                TableChoice::Custom => {
                    let supplied = custom.get(custom_index).ok_or(
                        SymbolTextError::InvalidTableAssociation("missing custom table"),
                    )?;
                    if supplied.has_oob != requires_oob {
                        return Err(SymbolTextError::InvalidTableAssociation(
                            "custom table OOB capability is invalid",
                        ));
                    }
                    custom_index = custom_index
                        .checked_add(1)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    BoundHuffmanTable::Custom(supplied.table)
                }
            };
        bindings.entries[specification_index] = Some(TableBinding { parameter, table });
        bindings.len = specification_index + 1;
        specification_index += 1;
    }
    Ok(bindings)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(total) =>
            total as nat == imported as nat + new_symbols as nat
            && total <= limits.max_symbols
            && exported <= total,
        Err(_) => true,
    },
))]
pub(crate) fn check_symbol_counts(
    imported: u32,
    new_symbols: u32,
    exported: u32,
    limits: &Limits,
) -> Result<u32, SymbolTextError> {
    let total = imported
        .checked_add(new_symbols)
        .ok_or(MalformedError::IntegerOverflow)?;
    enforce_limit(
        LimitKind::Symbols,
        u64::from(limits.max_symbols),
        u64::from(total),
    )?;
    if exported > total {
        return Err(SymbolTextError::InvalidCount(
            "exported symbols exceed candidates",
        ));
    }
    Ok(total)
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

impl DictionaryClassState {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures match out {
            Ok(state) =>
                symbol_text_spec::dictionary_state_valid(&state)
                && state.declared_new_symbols == declared_new_symbols
                && state.decoded_symbols == 0
                && !state.class_open,
            Err(_) => true,
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn new(declared_new_symbols: u32, limits: &Limits) -> Result<Self, SymbolTextError> {
        enforce_limit(
            LimitKind::Symbols,
            u64::from(limits.max_symbols),
            u64::from(declared_new_symbols),
        )?;
        Ok(Self {
            declared_new_symbols,
            decoded_symbols: 0,
            height: 0,
            width: 0,
            total_width: 0,
            class_open: false,
        })
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires symbol_text_spec::dictionary_state_valid(old(self)),
        ensures match out {
            Ok(height) =>
                symbol_text_spec::dictionary_state_valid(final(self))
                && symbol_text_spec::work_units(final(resources))
                    == symbol_text_spec::work_units(old(resources)) + 1
                && symbol_text_spec::work_units(final(resources))
                    <= limits.max_work_units
                && final(self).class_open
                && final(self).declared_new_symbols == old(self).declared_new_symbols
                && final(self).height == height
                && final(self).decoded_symbols == old(self).decoded_symbols
                && final(self).width == 0
                && final(self).total_width == 0,
            Err(_) => true,
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn begin_height_class(
        &mut self,
        delta: i64,
        limits: &Limits,
        resources: &mut ResourceTracker,
    ) -> Result<u32, SymbolTextError> {
        if self.class_open {
            return Err(SymbolTextError::InvalidState("height class already open"));
        }
        if self.decoded_symbols == self.declared_new_symbols {
            return Err(SymbolTextError::InvalidState(
                "height class after final symbol",
            ));
        }
        resources.charge_work(1, limits)?;
        self.height = checked_unsigned_delta(self.height, delta, limits.max_height)?;
        self.width = 0;
        self.total_width = 0;
        self.class_open = true;
        Ok(self.height)
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires symbol_text_spec::dictionary_state_valid(old(self)),
        ensures match out {
            Ok((width, height)) =>
                symbol_text_spec::dictionary_state_valid(final(self))
                && symbol_text_spec::work_units(final(resources))
                    == symbol_text_spec::work_units(old(resources)) + 1
                && symbol_text_spec::work_units(final(resources))
                    <= limits.max_work_units
                && final(self).class_open
                && final(self).declared_new_symbols == old(self).declared_new_symbols
                && final(self).decoded_symbols == old(self).decoded_symbols + 1
                && final(self).decoded_symbols <= final(self).declared_new_symbols
                && width == final(self).width
                && height == final(self).height
                && final(self).total_width <= limits.max_width,
            Err(_) => true,
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn add_symbol(
        &mut self,
        width_delta: i64,
        limits: &Limits,
        resources: &mut ResourceTracker,
    ) -> Result<(u32, u32), SymbolTextError> {
        if !self.class_open {
            return Err(SymbolTextError::InvalidState("no open height class"));
        }
        if self.decoded_symbols >= self.declared_new_symbols {
            return Err(SymbolTextError::InvalidCount("too many new symbols"));
        }
        resources.charge_work(1, limits)?;
        self.width = checked_unsigned_delta(self.width, width_delta, limits.max_width)?;
        self.total_width = self
            .total_width
            .checked_add(self.width)
            .ok_or(MalformedError::IntegerOverflow)?;
        if self.total_width > limits.max_width {
            return Err(LimitExceeded {
                kind: LimitKind::Width,
                limit: u64::from(limits.max_width),
                actual: u64::from(self.total_width),
            }
            .into());
        }
        check_dimensions(self.total_width, self.height, limits)?;
        self.decoded_symbols = self
            .decoded_symbols
            .checked_add(1)
            .ok_or(MalformedError::IntegerOverflow)?;
        Ok((self.width, self.height))
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires symbol_text_spec::dictionary_state_valid(old(self)),
        ensures match out {
            Ok(total_width) =>
                symbol_text_spec::dictionary_state_valid(final(self))
                && !final(self).class_open
                && final(self).declared_new_symbols == old(self).declared_new_symbols
                && final(self).decoded_symbols == old(self).decoded_symbols
                && total_width == final(self).total_width,
            Err(_) => true,
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn finish_height_class(&mut self) -> Result<u32, SymbolTextError> {
        if !self.class_open {
            return Err(SymbolTextError::InvalidState("no open height class"));
        }
        self.class_open = false;
        Ok(self.total_width)
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires symbol_text_spec::dictionary_state_valid(&self),
        ensures match out {
            Ok(()) =>
                !self.class_open
                && self.decoded_symbols == self.declared_new_symbols,
            Err(_) => true,
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn finish(self) -> Result<(), SymbolTextError> {
        if self.class_open {
            return Err(SymbolTextError::InvalidState("height class still open"));
        }
        if self.decoded_symbols != self.declared_new_symbols {
            return Err(SymbolTextError::InvalidCount("new symbol count mismatch"));
        }
        Ok(())
    }
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

/// Decodes alternating export-flag runs, beginning with a zero run.
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        symbol_text_spec::codec_valid(old(codec)),
        symbol_text_spec::work_units(old(resources)) <= limits.max_work_units,
    ensures match out {
        Ok(exported) =>
            symbol_text_spec::codec_valid(final(codec))
                && exported@.len() == expected_exports
                && symbol_text_spec::export_indices_valid(
                    exported@, total_candidates
                )
                && symbol_text_spec::bitmap_bytes(final(resources))
                    == symbol_text_spec::bitmap_bytes(old(resources)),
        Err(_) => true,
    },
))]
fn decode_export_runs(
    codec: &mut SymbolCodec<'_>,
    total_candidates: u32,
    expected_exports: u32,
    limits: &Limits,
    resources: &mut ResourceTracker,
) -> Result<Vec<u32>, SymbolTextError> {
    if expected_exports > total_candidates {
        return Err(SymbolTextError::InvalidCount(
            "export count exceeds candidate count",
        ));
    }
    let capacity =
        usize::try_from(expected_exports).map_err(|_| MalformedError::IntegerOverflow)?;
    let requested_bytes = u64::from(expected_exports)
        .checked_mul(4)
        .ok_or(MalformedError::IntegerOverflow)?;
    let mut exported: Vec<u32> = Vec::new();
    exported
        .try_reserve(capacity)
        .map_err(|_| DecodeError::Allocation { requested_bytes })?;
    let mut index = 0_u32;
    let mut export_flag = false;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            symbol_text_spec::codec_valid(codec),
            symbol_text_spec::work_units(resources) <= limits.max_work_units,
            symbol_text_spec::work_units(resources)
                >= symbol_text_spec::work_units(old(resources)),
            symbol_text_spec::bitmap_bytes(resources)
                == symbol_text_spec::bitmap_bytes(old(resources)),
            index <= total_candidates,
            capacity == expected_exports,
            exported@.len() <= capacity,
            forall|offset: int| 0 <= offset < exported@.len()
                ==> #[trigger] exported@[offset] < index,
        decreases limits.max_work_units - symbol_text_spec::work_units(resources),
    ))]
    while index < total_candidates {
        resources.charge_work(1, limits)?;
        let run = match codec.decode_integer(IntegerField::ExportRun)? {
            EntropyInteger::Value(value) => u32::try_from(value)
                .map_err(|_| SymbolTextError::InvalidEntropyValue("negative export run"))?,
            EntropyInteger::OutOfBand => {
                return Err(SymbolTextError::UnexpectedOutOfBand("export run"))
            }
        };
        let end = index
            .checked_add(run)
            .ok_or(MalformedError::IntegerOverflow)?;
        if end > total_candidates {
            return Err(SymbolTextError::InvalidCount(
                "export run exceeds candidate count",
            ));
        }
        if export_flag {
            let mut candidate = index;
            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant
                    index <= candidate <= end,
                    end <= total_candidates,
                    exported@.len() <= capacity,
                    forall|offset: int| 0 <= offset < exported@.len()
                        ==> #[trigger] exported@[offset] < candidate,
                decreases end - candidate,
            ))]
            while candidate < end {
                if exported.len() >= capacity {
                    return Err(SymbolTextError::InvalidCount("too many exported symbols"));
                }
                exported.push(candidate);
                candidate = candidate
                    .checked_add(1)
                    .ok_or(MalformedError::IntegerOverflow)?;
            }
        }
        index = end;
        export_flag = !export_flag;
    }
    if exported.len() != capacity {
        return Err(SymbolTextError::InvalidCount(
            "exported-symbol popcount mismatch",
        ));
    }
    Ok(exported)
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
    requires symbol_text_spec::work_units(old(resources)) <= limits.max_work_units,
    ensures match out {
        Ok(lengths) =>
            lengths@.len() == symbol_count
                && forall|index: int| 0 <= index < lengths@.len()
                    ==> #[trigger] lengths@[index] <= 31
                && symbol_text_spec::work_units(final(resources))
                    <= limits.max_work_units
                && symbol_text_spec::bitmap_bytes(final(resources))
                    == symbol_text_spec::bitmap_bytes(old(resources)),
        Err(_) => true,
    },
))]
fn expand_symbol_code_lengths(
    symbol_count: u32,
    runs: &[SymbolLengthRun],
    limits: &Limits,
    resources: &mut ResourceTracker,
) -> Result<Vec<u8>, SymbolTextError> {
    enforce_limit(
        LimitKind::Symbols,
        u64::from(limits.max_symbols),
        u64::from(symbol_count),
    )?;
    let capacity = usize::try_from(symbol_count).map_err(|_| MalformedError::IntegerOverflow)?;
    let mut lengths: Vec<u8> = Vec::new();
    lengths
        .try_reserve(capacity)
        .map_err(|_| DecodeError::Allocation {
            requested_bytes: u64::from(symbol_count),
        })?;
    let mut run_index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            run_index <= runs@.len(),
            lengths@.len() <= capacity,
            symbol_text_spec::work_units(resources) <= limits.max_work_units,
            symbol_text_spec::bitmap_bytes(resources)
                == symbol_text_spec::bitmap_bytes(old(resources)),
            forall|index: int| 0 <= index < lengths@.len()
                ==> #[trigger] lengths@[index] <= 31,
        decreases runs@.len() - run_index,
    ))]
    while run_index < runs.len() {
        let run = runs[run_index];
        if lengths.len() == capacity {
            return Err(SymbolTextError::InvalidSymbolLengthRun);
        }
        resources.charge_work(1, limits)?;
        match run {
            SymbolLengthRun::Literal(length) if length <= 31 => {
                lengths.push(length);
            }
            SymbolLengthRun::Literal(_) => return Err(SymbolTextError::InvalidSymbolLengthRun),
            SymbolLengthRun::RepeatPrevious(count) if (3..=6).contains(&count) => {
                let previous = *lengths
                    .last()
                    .ok_or(SymbolTextError::InvalidSymbolLengthRun)?;
                extend_lengths(&mut lengths, capacity, previous, usize::from(count))?;
            }
            SymbolLengthRun::Zeros(count) if (3..=138).contains(&count) => {
                extend_lengths(&mut lengths, capacity, 0, usize::from(count))?;
            }
            SymbolLengthRun::RepeatPrevious(_) | SymbolLengthRun::Zeros(_) => {
                return Err(SymbolTextError::InvalidSymbolLengthRun)
            }
        }
        run_index += 1;
    }
    if lengths.len() != capacity {
        return Err(SymbolTextError::InvalidCount(
            "symbol code length count mismatch",
        ));
    }
    resources.charge_work(u64::from(symbol_count), limits)?;
    Ok(lengths)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        old(lengths)@.len() <= capacity,
        value <= 31,
        forall|i: int| 0 <= i < old(lengths)@.len()
            ==> #[trigger] old(lengths)@[i] <= 31,
    ensures match out {
        Ok(()) =>
            final(lengths)@.len() == old(lengths)@.len() + count
            && final(lengths)@.len() <= capacity
            && final(lengths)@.subrange(0, old(lengths)@.len() as int)
                == old(lengths)@
            && (forall|i: int| old(lengths)@.len() <= i < final(lengths)@.len()
                ==> final(lengths)@[i] == value)
            && (forall|i: int| 0 <= i < final(lengths)@.len()
                ==> #[trigger] final(lengths)@[i] <= 31),
        Err(_) => true,
    },
))]
fn extend_lengths(
    lengths: &mut Vec<u8>,
    capacity: usize,
    value: u8,
    count: usize,
) -> Result<(), SymbolTextError> {
    #[cfg(verus_keep_ghost)]
    proof_decl! {
        let ghost original = lengths@;
    }
    let end = lengths
        .len()
        .checked_add(count)
        .ok_or(MalformedError::IntegerOverflow)?;
    if end > capacity {
        return Err(SymbolTextError::InvalidSymbolLengthRun);
    }
    lengths.resize(end, value);
    #[cfg(verus_keep_ghost)]
    proof! {
        assert forall|i: int| 0 <= i < lengths@.len()
            implies #[trigger] lengths@[i] <= 31 by {
            if i < original.len() {
                assert(lengths@[i] == original[i]);
            } else {
                assert(lengths@[i] == value);
            }
        }
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(value) => value == id && value < available_symbols,
        Err(_) => id >= available_symbols,
    },
))]
pub(crate) fn checked_symbol_id(id: u32, available_symbols: u32) -> Result<u32, SymbolTextError> {
    if id >= available_symbols {
        return Err(SymbolTextError::SymbolIdOutOfRange {
            id,
            available: available_symbols,
        });
    }
    Ok(id)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires symbol_text_spec::codec_valid(old(codec)),
    ensures match out {
        Ok(id) =>
            symbol_text_spec::codec_valid(final(codec))
                && id < request.available_symbols,
        Err(_) => true,
    },
))]
fn decode_checked_symbol_id(
    codec: &mut SymbolCodec<'_>,
    request: SymbolIdRequest,
) -> Result<u32, SymbolTextError> {
    if request.available_symbols == 0 {
        return Err(SymbolTextError::InvalidCount(
            "symbol ID with no available symbols",
        ));
    }
    let id = codec.decode_symbol_id(request)?;
    checked_symbol_id(id, request.available_symbols)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(geometry) =>
            geometry.width as int == base_width as int + delta_width as int
            && geometry.height as int == base_height as int + delta_height as int
            && geometry.width <= limits.max_width
            && geometry.height <= limits.max_height
            && geometry.reference_dx as int
                == symbol_text_spec::floor_half(delta_width as int) + delta_x as int
            && geometry.reference_dy as int
                == symbol_text_spec::floor_half(delta_height as int) + delta_y as int,
        Err(_) => true,
    },
))]
pub(crate) fn refinement_geometry(
    base_width: u32,
    base_height: u32,
    delta_width: i64,
    delta_height: i64,
    delta_x: i64,
    delta_y: i64,
    limits: &Limits,
) -> Result<RefinementGeometry, SymbolTextError> {
    let width = checked_unsigned_delta(base_width, delta_width, limits.max_width)?;
    let height = checked_unsigned_delta(base_height, delta_height, limits.max_height)?;
    check_dimensions(width, height, limits)?;
    let reference_x_offset = floor_divide_by_two(delta_width)
        .checked_add(delta_x)
        .ok_or(MalformedError::IntegerOverflow)?;
    let reference_y_offset = floor_divide_by_two(delta_height)
        .checked_add(delta_y)
        .ok_or(MalformedError::IntegerOverflow)?;
    Ok(RefinementGeometry {
        width,
        height,
        reference_dx: i32::try_from(reference_x_offset)
            .map_err(|_error| MalformedError::IntegerOverflow)?,
        reference_dy: i32::try_from(reference_y_offset)
            .map_err(|_error| MalformedError::IntegerOverflow)?,
    })
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out as int == symbol_text_spec::floor_half(value as int),
))]
fn floor_divide_by_two(value: i64) -> i64 {
    value / 2 - widen_bool_i64(value < 0 && value % 2 != 0)
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        symbol_text_spec::codec_valid(old(codec)),
        crate::spec::bitmap::storage_valid(reference),
    ensures match out {
        Ok(bitmap) =>
            symbol_text_spec::codec_valid(final(codec))
                && crate::spec::bitmap::storage_valid(&bitmap)
                && symbol_text_spec::work_units(final(resources))
                    == symbol_text_spec::work_units(old(resources)),
        Err(_) => true,
    },
))]
fn decode_refined_bitmap(
    codec: &mut SymbolCodec<'_>,
    reference: &Bitmap,
    template: u8,
    adaptive_pixels: [(i8, i8); 2],
    huffman: bool,
    limits: &Limits,
    resources: &mut ResourceTracker,
) -> Result<Bitmap, SymbolTextError> {
    let delta_width = required_integer(codec, IntegerField::RefinementWidth)?;
    let delta_height = required_integer(codec, IntegerField::RefinementHeight)?;
    let delta_x = required_integer(codec, IntegerField::RefinementX)?;
    let delta_y = required_integer(codec, IntegerField::RefinementY)?;
    let declared_data_size = if huffman {
        let size = required_integer(codec, IntegerField::RefinementSize)?;
        Some(
            u32::try_from(size)
                .map_err(|_| SymbolTextError::InvalidEntropyValue("refinement size"))?,
        )
    } else {
        None
    };
    let geometry = refinement_geometry(
        reference.width(),
        reference.height(),
        delta_width,
        delta_height,
        delta_x,
        delta_y,
        limits,
    )?;
    resources.charge_bitmap(geometry.width, geometry.height, limits)?;
    let request = RefinementRequest {
        reference,
        geometry,
        template,
        adaptive_pixels,
        declared_data_size,
    };
    let bitmap = codec.decode_refinement(request, limits)?;
    if bitmap.width() != geometry.width || bitmap.height() != geometry.height {
        return Err(SymbolTextError::InvalidDimensions);
    }
    Ok(bitmap)
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

impl TextRegionState {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures match out {
            Ok(state) =>
                symbol_text_spec::text_state_valid(&state)
                && state.config == config
                && state.declared_instances == declared_instances
                && state.decoded_instances == 0
                && state.strip.is_none(),
            Err(_) => true,
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn new(
        config: TextRegionConfig,
        declared_instances: u32,
        initial_strip_value: i64,
        limits: &Limits,
    ) -> Result<Self, SymbolTextError> {
        enforce_limit(
            LimitKind::Work,
            limits.max_work_units,
            u64::from(declared_instances),
        )?;
        let scaled = initial_strip_value
            .checked_mul(widen_u8_i64(config.strip_size))
            .ok_or(MalformedError::IntegerOverflow)?;
        let strip_t = 0_i64
            .checked_sub(scaled)
            .ok_or(MalformedError::IntegerOverflow)?;
        Ok(Self {
            config,
            declared_instances,
            decoded_instances: 0,
            strip_t,
            first_s: 0,
            strip: None,
        })
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires symbol_text_spec::text_state_valid(old(self)),
        ensures match out {
            Ok(()) =>
                symbol_text_spec::text_state_valid(final(self))
                && final(self).strip.is_some()
                && final(self).decoded_instances == old(self).decoded_instances,
            Err(_) => true,
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn begin_strip(
        &mut self,
        strip_delta: i64,
        first_s_delta: i64,
    ) -> Result<(), SymbolTextError> {
        if self.strip.is_some() {
            return Err(SymbolTextError::InvalidState("strip already open"));
        }
        if self.decoded_instances >= self.declared_instances {
            return Err(SymbolTextError::InvalidState("strip after final instance"));
        }
        let scaled = strip_delta
            .checked_mul(widen_u8_i64(self.config.strip_size))
            .ok_or(MalformedError::IntegerOverflow)?;
        self.strip_t = self
            .strip_t
            .checked_add(scaled)
            .ok_or(MalformedError::IntegerOverflow)?;
        self.first_s = self
            .first_s
            .checked_add(first_s_delta)
            .ok_or(MalformedError::IntegerOverflow)?;
        self.strip = Some(StripState {
            cursor_s: self.first_s,
            has_instance: false,
        });
        Ok(())
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires symbol_text_spec::text_state_valid(old(self)),
        ensures match out {
            Ok(placement) =>
                symbol_text_spec::text_state_valid(final(self))
                && final(self).decoded_instances == old(self).decoded_instances + 1
                && final(self).strip.is_some()
                && symbol_text_spec::work_units(final(resources))
                    <= limits.max_work_units
                && symbol_text_spec::work_units(final(resources))
                    == symbol_text_spec::work_units(old(resources)) + 1
                && symbol_text_spec::placement_within_extents(placement, width, height),
            Err(_) => true,
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn place_first(
        &mut self,
        within_strip_t: u8,
        width: u32,
        height: u32,
        limits: &Limits,
        resources: &mut ResourceTracker,
    ) -> Result<Placement, SymbolTextError> {
        let strip = self
            .strip
            .as_ref()
            .ok_or(SymbolTextError::InvalidState("no open strip"))?;
        if strip.has_instance {
            return Err(SymbolTextError::InvalidState(
                "first instance already placed",
            ));
        }
        self.place_current(within_strip_t, width, height, limits, resources)
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires symbol_text_spec::text_state_valid(old(self)),
        ensures match out {
            Ok(placement) =>
                symbol_text_spec::text_state_valid(final(self))
                && final(self).decoded_instances == old(self).decoded_instances + 1
                && final(self).strip.is_some()
                && symbol_text_spec::work_units(final(resources))
                    <= limits.max_work_units
                && symbol_text_spec::work_units(final(resources))
                    == symbol_text_spec::work_units(old(resources)) + 1
                && symbol_text_spec::placement_within_extents(placement, width, height),
            Err(_) => true,
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn place_subsequent(
        &mut self,
        delta_s: i64,
        within_strip_t: u8,
        width: u32,
        height: u32,
        limits: &Limits,
        resources: &mut ResourceTracker,
    ) -> Result<Placement, SymbolTextError> {
        let strip = self
            .strip
            .as_mut()
            .ok_or(SymbolTextError::InvalidState("no open strip"))?;
        if !strip.has_instance {
            return Err(SymbolTextError::InvalidState(
                "subsequent instance before first",
            ));
        }
        strip.cursor_s = strip
            .cursor_s
            .checked_add(delta_s)
            .and_then(|value| value.checked_add(i64::from(self.config.s_offset)))
            .ok_or(MalformedError::IntegerOverflow)?;
        self.place_current(within_strip_t, width, height, limits, resources)
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires symbol_text_spec::text_state_valid(old(self)),
        ensures match out {
            Ok(placement) =>
                symbol_text_spec::text_state_valid(final(self))
                && final(self).decoded_instances == old(self).decoded_instances + 1
                && final(self).strip.is_some()
                && symbol_text_spec::work_units(final(resources))
                    <= limits.max_work_units
                && symbol_text_spec::work_units(final(resources))
                    == symbol_text_spec::work_units(old(resources)) + 1
                && symbol_text_spec::placement_within_extents(placement, width, height)
                && symbol_text_spec::placement_for_current(
                    old(self), within_strip_t, width, height, placement
                ),
            Err(_) => true,
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    fn place_current(
        &mut self,
        within_strip_t: u8,
        width: u32,
        height: u32,
        limits: &Limits,
        resources: &mut ResourceTracker,
    ) -> Result<Placement, SymbolTextError> {
        if width == 0 || height == 0 {
            return Err(SymbolTextError::InvalidDimensions);
        }
        if within_strip_t >= self.config.strip_size {
            return Err(SymbolTextError::InvalidEntropyValue(
                "within-strip T coordinate",
            ));
        }
        if self.decoded_instances >= self.declared_instances {
            return Err(SymbolTextError::InvalidCount("too many text instances"));
        }
        resources.charge_work(1, limits)?;
        let strip = self
            .strip
            .as_mut()
            .ok_or(SymbolTextError::InvalidState("no open strip"))?;
        let width_extent = widen_u32_i64(width) - 1;
        let height_extent = widen_u32_i64(height) - 1;
        if (!self.config.transposed
            && matches!(
                self.config.reference_corner,
                ReferenceCorner::TopRight | ReferenceCorner::BottomRight
            ))
            || (self.config.transposed
                && matches!(
                    self.config.reference_corner,
                    ReferenceCorner::BottomLeft | ReferenceCorner::BottomRight
                ))
        {
            let extent = if self.config.transposed {
                height_extent
            } else {
                width_extent
            };
            strip.cursor_s = strip
                .cursor_s
                .checked_add(extent)
                .ok_or(MalformedError::IntegerOverflow)?;
        }
        let symbol_s = strip.cursor_s;
        let symbol_t = self
            .strip_t
            .checked_add(widen_u8_i64(within_strip_t))
            .ok_or(MalformedError::IntegerOverflow)?;
        let (reference_x, reference_y) = if self.config.transposed {
            (symbol_t, symbol_s)
        } else {
            (symbol_s, symbol_t)
        };
        let x = match self.config.reference_corner {
            ReferenceCorner::TopLeft | ReferenceCorner::BottomLeft => reference_x,
            ReferenceCorner::TopRight | ReferenceCorner::BottomRight => reference_x
                .checked_sub(width_extent)
                .ok_or(MalformedError::IntegerOverflow)?,
        };
        let y = match self.config.reference_corner {
            ReferenceCorner::TopLeft | ReferenceCorner::TopRight => reference_y,
            ReferenceCorner::BottomLeft | ReferenceCorner::BottomRight => reference_y
                .checked_sub(height_extent)
                .ok_or(MalformedError::IntegerOverflow)?,
        };
        if (!self.config.transposed
            && matches!(
                self.config.reference_corner,
                ReferenceCorner::TopLeft | ReferenceCorner::BottomLeft
            ))
            || (self.config.transposed
                && matches!(
                    self.config.reference_corner,
                    ReferenceCorner::TopLeft | ReferenceCorner::TopRight
                ))
        {
            let extent = if self.config.transposed {
                height_extent
            } else {
                width_extent
            };
            strip.cursor_s = strip
                .cursor_s
                .checked_add(extent)
                .ok_or(MalformedError::IntegerOverflow)?;
        }
        strip.has_instance = true;
        self.decoded_instances = self
            .decoded_instances
            .checked_add(1)
            .ok_or(MalformedError::IntegerOverflow)?;
        Ok(Placement { x, y })
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires symbol_text_spec::text_state_valid(old(self)),
        ensures match out {
            Ok(()) =>
                symbol_text_spec::text_state_valid(final(self))
                && final(self).strip.is_none()
                && final(self).decoded_instances == old(self).decoded_instances,
            Err(_) => true,
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn end_strip(&mut self) -> Result<(), SymbolTextError> {
        let strip = self
            .strip
            .take()
            .ok_or(SymbolTextError::InvalidState("no open strip"))?;
        if !strip.has_instance {
            return Err(SymbolTextError::InvalidState("empty text strip"));
        }
        Ok(())
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires symbol_text_spec::text_state_valid(&self),
        ensures match out {
            Ok(()) =>
                self.strip.is_none()
                && self.decoded_instances == self.declared_instances,
            Err(_) => true,
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn finish(self) -> Result<(), SymbolTextError> {
        if self.strip.is_some() {
            return Err(SymbolTextError::InvalidState("text strip still open"));
        }
        if self.decoded_instances != self.declared_instances {
            return Err(SymbolTextError::InvalidCount(
                "text instance count mismatch",
            ));
        }
        Ok(())
    }
}

/// Combines a symbol bitmap at a signed top-left location with normative
/// clipping. Nonnegative origins use the existing checked bitmap composition
/// API; negative origins use an explicitly checked cropped loop.
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires value < 0,
    ensures match out {
        Ok(magnitude) =>
            value < 0
                && magnitude >= 0
                && magnitude as int == -(value as int),
        Err(_) => value == i64::MIN,
    },
))]
fn negative_magnitude(value: i64) -> Result<i64, SymbolTextError> {
    if value == i64::MIN {
        return Err(MalformedError::IntegerOverflow.into());
    }
    Ok(-value)
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

impl<'a> PlacementBatch<'a> {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out.blits@.len() == 0,
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    fn new() -> Self {
        Self {
            blits: Vec::new(),
            source_y: 0,
            destination_y: 0,
            height: 0,
            source_shift: 0,
            destination_shift: 0,
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures final(self).blits@ == old(self).blits@,
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    fn reserve(&mut self, count: usize, limits: &Limits) -> Result<(), SymbolTextError> {
        let capacity = count.min(MAX_PLACEMENT_BATCH);
        let requested_bytes = capacity
            .checked_mul(core::mem::size_of::<RowBlit<'_>>())
            .and_then(|bytes| u64::try_from(bytes).ok())
            .ok_or(MalformedError::IntegerOverflow)?;
        enforce_limit(
            LimitKind::Memory,
            limits.max_decoder_managed_memory_bytes,
            requested_bytes,
        )?;
        self.blits
            .try_reserve(capacity)
            .map_err(|_| DecodeError::Allocation { requested_bytes }.into())
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out ==> (
            self.blits@.len() < MAX_PLACEMENT_BATCH
            && (
                self.blits@.len() == 0
                || (
                    self.source_y == prepared.source_y
                    && self.destination_y == prepared.destination_y
                    && self.height == prepared.height
                    && self.source_shift == prepared.blit.source_x % 8
                    && self.destination_shift == prepared.blit.destination_x % 8
                )
            )
        ),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    fn compatible(&self, prepared: PreparedPlacement<'a>) -> bool {
        self.blits.is_empty()
            || (self.blits.len() < MAX_PLACEMENT_BATCH
                && self.source_y == prepared.source_y
                && self.destination_y == prepared.destination_y
                && self.height == prepared.height
                && self.source_shift == prepared.blit.source_x % 8
                && self.destination_shift == prepared.blit.destination_x % 8)
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            symbol_text_spec::placement_batch_valid(old(self), _destination),
            symbol_text_spec::prepared_placement_valid(prepared, _destination),
        ensures match out {
            Ok(()) => symbol_text_spec::placement_batch_valid(final(self), _destination),
            Err(_) => true,
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    fn push(
        &mut self,
        prepared: PreparedPlacement<'a>,
        _destination: &Bitmap,
    ) -> Result<(), SymbolTextError> {
        if !self.compatible(prepared) {
            return Err(SymbolTextError::InvalidState(
                "incompatible text placement batch",
            ));
        }
        if self.blits.is_empty() {
            self.source_y = prepared.source_y;
            self.destination_y = prepared.destination_y;
            self.height = prepared.height;
            self.source_shift = prepared.blit.source_x % 8;
            self.destination_shift = prepared.blit.destination_x % 8;
        }
        self.blits.push(prepared.blit);
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal(symbol_text_spec::placement_batch_valid);
            reveal(symbol_text_spec::prepared_placement_valid);
        }
        Ok(())
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        crate::spec::bitmap::storage_valid(old(destination)),
        symbol_text_spec::placement_batch_valid(old(batch), old(destination)),
    ensures match out {
        Ok(()) =>
            crate::spec::bitmap::storage_valid(final(destination))
            && final(destination).width == old(destination).width
            && final(destination).height == old(destination).height
            && crate::views::bitmap_width(final(destination))
                == crate::views::bitmap_width(old(destination))
            && crate::views::bitmap_height(final(destination))
                == crate::views::bitmap_height(old(destination))
            && crate::views::bitmap_stride(final(destination))
                == crate::views::bitmap_stride(old(destination))
            && crate::views::bitmap_data(final(destination)).len()
                == crate::views::bitmap_data(old(destination)).len()
            && final(batch).blits@.len() == 0
            && symbol_text_spec::placement_batch_valid(final(batch), final(destination)),
        Err(_) => true,
    },
        (old(batch).blits@.len() == 0 || old(batch).height == 0) ==> out.is_ok(),
        (old(batch).blits@.len() == 0 || old(batch).height == 0) ==>
            crate::views::bitmap_data(final(destination))
                == crate::views::bitmap_data(old(destination)),
))]
fn flush_placement_batch(
    destination: &mut Bitmap,
    batch: &mut PlacementBatch<'_>,
    operator: CombinationOperator,
) -> Result<(), SymbolTextError> {
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::views::bitmap_fields(destination);
        crate::views::bitmap_fields(old(destination));
    }
    if batch.blits.is_empty() {
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal(symbol_text_spec::placement_batch_valid);
            crate::views::bitmap_fields(destination);
            crate::views::bitmap_fields(old(destination));
        }
        return Ok(());
    }
    if batch.blits.len() == 1 {
        let blit = batch.blits[0];
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal(symbol_text_spec::placement_batch_valid);
            crate::views::bitmap_fields(blit.source);
        }
        destination.compose_region(
            blit.source,
            blit.source_x,
            batch.source_y,
            blit.destination_x,
            batch.destination_y,
            blit.width,
            batch.height,
            operator,
        )?;
        batch.blits.clear();
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal(symbol_text_spec::placement_batch_valid);
            crate::views::bitmap_fields(destination);
            crate::views::bitmap_fields(old(destination));
        }
        return Ok(());
    }
    destination.compose_row_batch_chunked(
        &batch.blits,
        batch.source_y,
        batch.destination_y,
        batch.height,
        operator,
    )?;
    batch.blits.clear();
    #[cfg(verus_keep_ghost)]
    proof! {
        reveal(symbol_text_spec::placement_batch_valid);
        crate::views::bitmap_fields(destination);
        crate::views::bitmap_fields(old(destination));
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        crate::spec::bitmap::storage_valid(destination),
        crate::spec::bitmap::storage_valid(source),
        symbol_text_spec::work_units(old(resources)) <= limits.max_work_units,
    ensures match out {
        Ok(Some(prepared)) =>
            symbol_text_spec::work_units(final(resources))
                <= limits.max_work_units
            && symbol_text_spec::bitmap_bytes(final(resources))
                == symbol_text_spec::bitmap_bytes(old(resources))
            && symbol_text_spec::prepared_placement_valid(prepared, destination)
            && prepared.blit.source_x as nat + prepared.blit.width as nat
                <= source.width as nat
            && prepared.source_y as nat + prepared.height as nat
                <= source.height as nat
            && prepared.blit.destination_x as nat + prepared.blit.width as nat
                <= destination.width as nat
            && prepared.destination_y as nat + prepared.height as nat
                <= destination.height as nat,
        Ok(None) =>
            symbol_text_spec::work_units(final(resources))
                <= limits.max_work_units
            && symbol_text_spec::bitmap_bytes(final(resources))
                == symbol_text_spec::bitmap_bytes(old(resources)),
        Err(_) => true,
    },
))]
fn prepare_placement<'a>(
    destination: &Bitmap,
    source: &'a Bitmap,
    placement: Placement,
    limits: &Limits,
    resources: &mut ResourceTracker,
) -> Result<Option<PreparedPlacement<'a>>, SymbolTextError> {
    let source_width = i64::from(source.width());
    let source_height = i64::from(source.height());
    let destination_width = i64::from(destination.width());
    let destination_height = i64::from(destination.height());
    let source_x = if placement.x < 0 {
        negative_magnitude(placement.x)?
    } else {
        0
    };
    let source_y = if placement.y < 0 {
        negative_magnitude(placement.y)?
    } else {
        0
    };
    let destination_x = placement.x.max(0);
    let destination_y = placement.y.max(0);
    let raw_overlap_width = source_width
        .checked_sub(source_x)
        .ok_or(MalformedError::IntegerOverflow)?
        .min(
            destination_width
                .checked_sub(destination_x)
                .ok_or(MalformedError::IntegerOverflow)?,
        );
    let overlap_width = if raw_overlap_width < 0 {
        0
    } else {
        raw_overlap_width
    };
    let raw_overlap_height = source_height
        .checked_sub(source_y)
        .ok_or(MalformedError::IntegerOverflow)?
        .min(
            destination_height
                .checked_sub(destination_y)
                .ok_or(MalformedError::IntegerOverflow)?,
        );
    let overlap_height = if raw_overlap_height < 0 {
        0
    } else {
        raw_overlap_height
    };
    let work = u64::try_from(overlap_width)
        .map_err(|_| MalformedError::IntegerOverflow)?
        .checked_mul(u64::try_from(overlap_height).map_err(|_| MalformedError::IntegerOverflow)?)
        .ok_or(MalformedError::IntegerOverflow)?;
    resources.charge_work(work, limits)?;
    if overlap_width == 0 || overlap_height == 0 {
        return Ok(None);
    }
    let prepared = PreparedPlacement {
        blit: RowBlit {
            source,
            source_x: u32::try_from(source_x).map_err(|_| MalformedError::IntegerOverflow)?,
            destination_x: u32::try_from(destination_x)
                .map_err(|_| MalformedError::IntegerOverflow)?,
            width: u32::try_from(overlap_width).map_err(|_| MalformedError::IntegerOverflow)?,
        },
        source_y: u32::try_from(source_y).map_err(|_| MalformedError::IntegerOverflow)?,
        destination_y: u32::try_from(destination_y).map_err(|_| MalformedError::IntegerOverflow)?,
        height: u32::try_from(overlap_height).map_err(|_| MalformedError::IntegerOverflow)?,
    };
    #[cfg(verus_keep_ghost)]
    proof! {
        reveal(symbol_text_spec::prepared_placement_valid);
        crate::views::bitmap_fields(source);
        crate::views::bitmap_fields(destination);
    }
    Ok(Some(prepared))
}

#[allow(clippy::too_many_lines)]
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        crate::spec::bitmap::storage_valid(old(destination)),
        crate::spec::bitmap::storage_valid(source),
        symbol_text_spec::work_units(old(resources)) <= limits.max_work_units,
    ensures match out {
        Ok(()) =>
            crate::spec::bitmap::storage_valid(final(destination))
                && crate::views::bitmap_width(final(destination))
                    == crate::views::bitmap_width(old(destination))
                && crate::views::bitmap_height(final(destination))
                    == crate::views::bitmap_height(old(destination))
                && symbol_text_spec::work_units(final(resources))
                    <= limits.max_work_units
                && symbol_text_spec::bitmap_bytes(final(resources))
                    == symbol_text_spec::bitmap_bytes(old(resources)),
        Err(_) => true,
    },
))]
pub(crate) fn compose_placement(
    destination: &mut Bitmap,
    source: &Bitmap,
    placement: Placement,
    operator: CombinationOperator,
    limits: &Limits,
    resources: &mut ResourceTracker,
) -> Result<(), SymbolTextError> {
    let Some(prepared) = prepare_placement(destination, source, placement, limits, resources)?
    else {
        return Ok(());
    };
    destination.compose_region(
        prepared.blit.source,
        prepared.blit.source_x,
        prepared.source_y,
        prepared.blit.destination_x,
        prepared.destination_y,
        prepared.blit.width,
        prepared.height,
        operator,
    )?;
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::views::bitmap_fields(destination);
        crate::views::bitmap_fields(old(destination));
        reveal(crate::spec::bitmap::storage_valid);
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
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

// ---------------------------------------------------------------------------
// Wire header parsing (T.88 7.4.3 symbol dictionary, 7.4.3 text region).
// ---------------------------------------------------------------------------

/// Reads one signed adaptive-pixel pair, as used by generic and refinement
/// AT flags.
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures
        -128 <= out,
        out <= 127,
        out == symbol_text_spec::signed_byte(value),
))]
fn signed_byte(value: u8) -> i32 {
    if value <= 127 {
        widen_u8_i32(value)
    } else {
        widen_u8_i32(value) - 256
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires old(reader).position <= old(reader).bytes@.len(),
    ensures
        final(reader).bytes@ == old(reader).bytes@,
        final(reader).position <= final(reader).bytes@.len(),
        match out {
            Ok(offset) =>
                final(reader).position == old(reader).position + 2
                && -128 <= offset.x <= 127
                && -128 <= offset.y <= 127,
            Err(_) => final(reader).position <= old(reader).position + 1,
        },
))]
fn read_at_pixel(reader: &mut ByteReader<'_>) -> Result<TemplateOffset, SymbolTextError> {
    let x = signed_byte(reader.read_u8()?);
    let y = signed_byte(reader.read_u8()?);
    Ok(TemplateOffset::new(x, y))
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

/// Parses a symbol-dictionary segment data header and returns its byte length.
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok((header, consumed)) =>
            consumed <= payload@.len()
            && header.adaptive_count <= 4
            && header.new_symbols <= limits.max_symbols
            && consumed as nat
                == 10 + 2 * header.adaptive_count as nat
                    + if header.config.refinement_aggregation
                        && header.config.refinement_template == 0
                    { 4nat } else { 0nat },
        Err(_) => true,
    },
))]
pub(crate) fn parse_symbol_dictionary_header(
    payload: &[u8],
    limits: &Limits,
) -> Result<(SymbolDictionaryHeader, usize), SymbolTextError> {
    let mut reader = ByteReader::new(payload);
    let flags = reader.read_u16()?;
    let config = parse_symbol_dictionary_flags(flags)?;

    let mut adaptive = [TemplateOffset::new(0, 0); 4];
    let adaptive_count: u8 = if config.huffman {
        0
    } else if config.template == 0 {
        4
    } else {
        1
    };
    let mut adaptive_index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            adaptive_count <= 4,
            adaptive_index <= adaptive_count as usize,
            reader.position <= payload@.len(),
            reader.bytes@ == payload@,
            reader.position == 2 + 2 * adaptive_index,
        decreases adaptive_count as usize - adaptive_index,
    ))]
    while adaptive_index < adaptive_count as usize {
        adaptive[adaptive_index] = read_at_pixel(&mut reader)?;
        adaptive_index += 1;
    }

    let mut refinement_adaptive = [TemplateOffset::new(0, 0); 2];
    if config.refinement_aggregation && config.refinement_template == 0 {
        refinement_adaptive[0] = read_at_pixel(&mut reader)?;
        refinement_adaptive[1] = read_at_pixel(&mut reader)?;
    }

    let exported_symbols = reader.read_u32()?;
    let new_symbols = reader.read_u32()?;
    enforce_limit(
        LimitKind::Symbols,
        u64::from(limits.max_symbols),
        u64::from(new_symbols),
    )?;

    Ok((
        SymbolDictionaryHeader {
            config,
            adaptive,
            adaptive_count,
            refinement_adaptive,
            exported_symbols,
            new_symbols,
        },
        reader.position(),
    ))
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

/// Parses a text-region segment data header and returns its byte length.
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok((header, _)) =>
            symbol_text_spec::valid_strip_size(header.config.strip_size),
        Err(_) => true,
    },
))]
pub(crate) fn parse_text_region_header(
    payload: &[u8],
    limits: &Limits,
) -> Result<(TextRegionHeader, usize), SymbolTextError> {
    let mut reader = ByteReader::new(payload);
    let region = parse_region_info(&mut reader, limits)?;
    let flags = reader.read_u16()?;
    let huffman = flags & 1 != 0;
    let huffman_flags = if huffman {
        Some(reader.read_u16()?)
    } else {
        None
    };
    let config = parse_text_region_flags(flags, huffman_flags, false)?;
    #[cfg(verus_keep_ghost)]
    proof! {
        reveal(symbol_text_spec::text_config_safe);
    }

    let mut refinement_adaptive = [TemplateOffset::new(0, 0); 2];
    if config.refinement && config.refinement_template == 0 {
        refinement_adaptive[0] = read_at_pixel(&mut reader)?;
        refinement_adaptive[1] = read_at_pixel(&mut reader)?;
    }

    let number_of_instances = reader.read_u32()?;
    enforce_limit(
        LimitKind::Work,
        limits.max_work_units,
        u64::from(number_of_instances),
    )?;

    Ok((
        TextRegionHeader {
            region,
            config,
            refinement_adaptive,
            number_of_instances,
        },
        reader.position(),
    ))
}

// ---------------------------------------------------------------------------
// Shared symbol code length.
// ---------------------------------------------------------------------------

/// The smallest `k` with `2^k >= symbol_count.max(1)` (T.88 "SBSYMCODELEN").
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(length) => length == symbol_text_spec::symbol_code_length_expected(symbol_count),
        Err(_) => false,
    },
))]
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn symbol_id_code_length(symbol_count: u32) -> Result<u8, SymbolTextError> {
    let length = if symbol_count <= 1 {
        0
    } else if symbol_count <= 2 {
        1
    } else if symbol_count <= 4 {
        2
    } else if symbol_count <= 8 {
        3
    } else if symbol_count <= 16 {
        4
    } else if symbol_count <= 32 {
        5
    } else if symbol_count <= 64 {
        6
    } else if symbol_count <= 128 {
        7
    } else if symbol_count <= 256 {
        8
    } else if symbol_count <= 512 {
        9
    } else if symbol_count <= 1_024 {
        10
    } else if symbol_count <= 2_048 {
        11
    } else if symbol_count <= 4_096 {
        12
    } else if symbol_count <= 8_192 {
        13
    } else if symbol_count <= 16_384 {
        14
    } else if symbol_count <= 32_768 {
        15
    } else if symbol_count <= 65_536 {
        16
    } else if symbol_count <= 131_072 {
        17
    } else if symbol_count <= 262_144 {
        18
    } else if symbol_count <= 524_288 {
        19
    } else if symbol_count <= 1_048_576 {
        20
    } else if symbol_count <= 2_097_152 {
        21
    } else if symbol_count <= 4_194_304 {
        22
    } else if symbol_count <= 8_388_608 {
        23
    } else if symbol_count <= 16_777_216 {
        24
    } else if symbol_count <= 33_554_432 {
        25
    } else if symbol_count <= 67_108_864 {
        26
    } else if symbol_count <= 134_217_728 {
        27
    } else if symbol_count <= 268_435_456 {
        28
    } else if symbol_count <= 536_870_912 {
        29
    } else if symbol_count <= 1_073_741_824 {
        30
    } else if symbol_count <= 2_147_483_648 {
        31
    } else {
        32
    };
    Ok(length)
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

impl StandardHuffmanTables {
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    const fn empty() -> Self {
        Self {
            a: None,
            b: None,
            c: None,
            d: None,
            e: None,
            f: None,
            g: None,
            h: None,
            i: None,
            j: None,
            k: None,
            l: None,
            m: None,
            n: None,
            o: None,
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn materialize(
        &mut self,
        table: StandardHuffmanTable,
        limits: &Limits,
    ) -> Result<(), HuffmanError> {
        if self.table(table).is_none() {
            let materialized = Some(Box::new(HuffmanTable::standard(table, limits)?));
            match table {
                StandardHuffmanTable::A => self.a = materialized,
                StandardHuffmanTable::B => self.b = materialized,
                StandardHuffmanTable::C => self.c = materialized,
                StandardHuffmanTable::D => self.d = materialized,
                StandardHuffmanTable::E => self.e = materialized,
                StandardHuffmanTable::F => self.f = materialized,
                StandardHuffmanTable::G => self.g = materialized,
                StandardHuffmanTable::H => self.h = materialized,
                StandardHuffmanTable::I => self.i = materialized,
                StandardHuffmanTable::J => self.j = materialized,
                StandardHuffmanTable::K => self.k = materialized,
                StandardHuffmanTable::L => self.l = materialized,
                StandardHuffmanTable::M => self.m = materialized,
                StandardHuffmanTable::N => self.n = materialized,
                StandardHuffmanTable::O => self.o = materialized,
            }
        }
        Ok(())
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn table(&self, table: StandardHuffmanTable) -> Option<&HuffmanTable> {
        macro_rules! get {
            ($field:expr) => {
                match $field {
                    Some(table) => Some(table),
                    None => None,
                }
            };
        }
        match table {
            StandardHuffmanTable::A => get!(&self.a),
            StandardHuffmanTable::B => get!(&self.b),
            StandardHuffmanTable::C => get!(&self.c),
            StandardHuffmanTable::D => get!(&self.d),
            StandardHuffmanTable::E => get!(&self.e),
            StandardHuffmanTable::F => get!(&self.f),
            StandardHuffmanTable::G => get!(&self.g),
            StandardHuffmanTable::H => get!(&self.h),
            StandardHuffmanTable::I => get!(&self.i),
            StandardHuffmanTable::J => get!(&self.j),
            StandardHuffmanTable::K => get!(&self.k),
            StandardHuffmanTable::L => get!(&self.l),
            StandardHuffmanTable::M => get!(&self.m),
            StandardHuffmanTable::N => get!(&self.n),
            StandardHuffmanTable::O => get!(&self.o),
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn materialize_huffman_table<'a>(
    choice: Option<BoundHuffmanTable<'a>>,
    standard: &mut StandardHuffmanTables,
    limits: &Limits,
) -> Result<Option<MaterializedHuffmanTable<'a>>, SymbolTextError> {
    match choice {
        None => Ok(None),
        Some(BoundHuffmanTable::Standard(table)) => {
            standard.materialize(table, limits)?;
            Ok(Some(MaterializedHuffmanTable::Standard(table)))
        }
        Some(BoundHuffmanTable::Custom(table)) => Ok(Some(MaterializedHuffmanTable::Custom(table))),
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires symbol_text_spec::table_bindings_valid(bindings),
    ensures
        out.is_some() == exists|i: int|
            0 <= i < bindings.len
            && match #[trigger] bindings.entries[i] {
                Some(binding) => binding.parameter == parameter,
                None => false,
            },
))]
fn find_table_binding<'a>(
    bindings: &TableBindings<'a>,
    parameter: TableParameter,
) -> Option<BoundHuffmanTable<'a>> {
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index <= bindings.len,
            bindings.len <= MAX_TABLE_BINDINGS,
            symbol_text_spec::table_bindings_valid(bindings),
            forall|i: int| 0 <= i < index
                ==> match #[trigger] bindings.entries[i] {
                    Some(binding) => binding.parameter != parameter,
                    None => true,
                },
        decreases bindings.len - index,
    ))]
    while index < bindings.len {
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(symbol_text_spec::table_bindings_valid(bindings));
            reveal(symbol_text_spec::table_bindings_valid);
            assert(index < MAX_TABLE_BINDINGS);
            assert(bindings.entries[index as int].is_some() == (index < bindings.len));
            assert(bindings.entries[index as int].is_some());
        }
        let binding = bindings.entries[index]?;
        if same_table_parameter(binding.parameter, parameter) {
            return Some(binding.table);
        }
        index += 1;
    }
    None
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

impl HuffmanFieldChoices<'_> {
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    const fn empty() -> Self {
        Self {
            aggregation_instances: None,
            height_delta: None,
            width_delta: None,
            export_run: None,
            strip_delta: None,
            first_s: None,
            subsequent_s: None,
            refinement_width: None,
            refinement_height: None,
            refinement_x: None,
            refinement_y: None,
            refinement_size: None,
            dictionary_bitmap_size: None,
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires symbol_text_spec::table_bindings_valid(bindings),
    ensures match out {
        Ok(choices) =>
            config.huffman
                && choices.height_delta.is_some()
                && choices.width_delta.is_some()
                && choices.export_run.is_some()
                && choices.dictionary_bitmap_size.is_some()
                && (config.refinement_aggregation ==> (
                    choices.aggregation_instances.is_some()
                        && choices.refinement_x.is_some()
                        && choices.refinement_y.is_some()
                        && choices.refinement_size.is_some()
                )),
        Err(_) => true,
    },
))]
fn select_dictionary_tables<'a>(
    config: &SymbolDictionaryConfig,
    bindings: &TableBindings<'a>,
) -> Result<HuffmanFieldChoices<'a>, SymbolTextError> {
    if !config.huffman {
        return Err(SymbolTextError::InvalidTableAssociation(
            "dictionary choices require Huffman mode",
        ));
    }
    let mut choices = HuffmanFieldChoices::empty();
    choices.height_delta = Some(
        find_table_binding(bindings, TableParameter::DictionaryHeight).ok_or(
            SymbolTextError::InvalidTableAssociation("missing height table"),
        )?,
    );
    choices.width_delta = Some(
        find_table_binding(bindings, TableParameter::DictionaryWidth).ok_or(
            SymbolTextError::InvalidTableAssociation("missing width table"),
        )?,
    );
    choices.dictionary_bitmap_size = Some(
        find_table_binding(bindings, TableParameter::DictionaryBitmapSize).ok_or(
            SymbolTextError::InvalidTableAssociation("missing bitmap-size table"),
        )?,
    );
    choices.export_run = Some(BoundHuffmanTable::Standard(StandardHuffmanTable::A));
    if config.refinement_aggregation {
        choices.aggregation_instances = Some(
            find_table_binding(bindings, TableParameter::DictionaryAggregationInstances).ok_or(
                SymbolTextError::InvalidTableAssociation("missing aggregation-instances table"),
            )?,
        );
        choices.refinement_x = Some(BoundHuffmanTable::Standard(StandardHuffmanTable::O));
        choices.refinement_y = Some(BoundHuffmanTable::Standard(StandardHuffmanTable::O));
        choices.refinement_size = Some(BoundHuffmanTable::Standard(StandardHuffmanTable::A));
    }
    Ok(choices)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires symbol_text_spec::table_bindings_valid(bindings),
    ensures match out {
        Ok(choices) =>
            choices.first_s.is_some()
                && choices.subsequent_s.is_some()
                && choices.strip_delta.is_some()
                && (refinement ==> (
                    choices.refinement_width.is_some()
                        && choices.refinement_height.is_some()
                        && choices.refinement_x.is_some()
                        && choices.refinement_y.is_some()
                        && choices.refinement_size.is_some()
                )),
        Err(_) => true,
    },
))]
fn select_text_tables<'a>(
    bindings: &TableBindings<'a>,
    refinement: bool,
) -> Result<HuffmanFieldChoices<'a>, SymbolTextError> {
    let mut choices = HuffmanFieldChoices::empty();
    choices.first_s = Some(
        find_table_binding(bindings, TableParameter::TextFirstS)
            .ok_or(SymbolTextError::InvalidTableAssociation("missing FS table"))?,
    );
    choices.subsequent_s = Some(
        find_table_binding(bindings, TableParameter::TextSubsequentS)
            .ok_or(SymbolTextError::InvalidTableAssociation("missing DS table"))?,
    );
    choices.strip_delta = Some(
        find_table_binding(bindings, TableParameter::TextStripDelta)
            .ok_or(SymbolTextError::InvalidTableAssociation("missing DT table"))?,
    );
    if refinement {
        choices.refinement_width = Some(
            find_table_binding(bindings, TableParameter::TextRefinementWidth).ok_or(
                SymbolTextError::InvalidTableAssociation("missing RDW table"),
            )?,
        );
        choices.refinement_height = Some(
            find_table_binding(bindings, TableParameter::TextRefinementHeight).ok_or(
                SymbolTextError::InvalidTableAssociation("missing RDH table"),
            )?,
        );
        choices.refinement_x = Some(
            find_table_binding(bindings, TableParameter::TextRefinementX).ok_or(
                SymbolTextError::InvalidTableAssociation("missing RDX table"),
            )?,
        );
        choices.refinement_y = Some(
            find_table_binding(bindings, TableParameter::TextRefinementY).ok_or(
                SymbolTextError::InvalidTableAssociation("missing RDY table"),
            )?,
        );
        choices.refinement_size = Some(
            find_table_binding(bindings, TableParameter::TextRefinementSize).ok_or(
                SymbolTextError::InvalidTableAssociation("missing RSIZE table"),
            )?,
        );
    }
    Ok(choices)
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

impl HuffmanFieldTables<'_> {
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn table_for(&self, field: IntegerField) -> Option<&HuffmanTable> {
        let resolved = match field {
            IntegerField::AggregationInstances => self.aggregation_instances.as_ref(),
            IntegerField::HeightDelta => self.height_delta.as_ref(),
            IntegerField::WidthDelta => self.width_delta.as_ref(),
            IntegerField::ExportRun => self.export_run.as_ref(),
            IntegerField::StripDelta => self.strip_delta.as_ref(),
            IntegerField::FirstS => self.first_s.as_ref(),
            IntegerField::SubsequentS => self.subsequent_s.as_ref(),
            IntegerField::RefinementWidth => self.refinement_width.as_ref(),
            IntegerField::RefinementHeight => self.refinement_height.as_ref(),
            IntegerField::RefinementX => self.refinement_x.as_ref(),
            IntegerField::RefinementY => self.refinement_y.as_ref(),
            IntegerField::RefinementSize => self.refinement_size.as_ref(),
            IntegerField::DictionaryBitmapSize => self.dictionary_bitmap_size.as_ref(),
            IntegerField::WithinStripT | IntegerField::RefinementIndicator => None,
        };
        match resolved {
            Some(MaterializedHuffmanTable::Standard(table)) => self.standard_tables.table(*table),
            Some(MaterializedHuffmanTable::Custom(table)) => Some(table),
            None => None,
        }
    }
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

impl IntegerContexts {
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn new() -> Self {
        Self {
            slots: [[ArithmeticContext::new(); 512]; INTEGER_FIELD_COUNT],
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
struct ArithmeticCodec<'a> {
    decoder: ArithmeticDecoder<'a>,
    integers: IntegerContexts,
    iaid: Vec<ArithmeticContext>,
    refinement: Option<RefinementContexts>,
    refinement_work: u64,
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(codec) => symbol_text_spec::arithmetic_codec_valid(&codec),
        Err(_) => true,
    },
))]
fn new_arithmetic_codec<'a>(
    bytes: &'a [u8],
    iaid_code_length: u8,
    refinement_template: Option<RefinementTemplate>,
    limits: &Limits,
) -> Result<ArithmeticCodec<'a>, SymbolTextError> {
    if u32::from(iaid_code_length) >= usize::BITS {
        return Err(MalformedError::IntegerOverflow.into());
    }
    let count = 1_usize << iaid_code_length;
    let requested_bytes = count
        .checked_mul(core::mem::size_of::<ArithmeticContext>())
        .and_then(|bytes| u64::try_from(bytes).ok())
        .ok_or(MalformedError::IntegerOverflow)?;
    enforce_limit(
        LimitKind::Memory,
        limits.max_decoder_managed_memory_bytes,
        requested_bytes,
    )?;
    let mut iaid = Vec::new();
    iaid.try_reserve(count)
        .map_err(|_| DecodeError::Allocation { requested_bytes })?;
    iaid.resize(count, ArithmeticContext::new());
    Ok(ArithmeticCodec {
        decoder: ArithmeticDecoder::new(bytes)?,
        integers: IntegerContexts {
            slots: [[ArithmeticContext::new(); 512]; INTEGER_FIELD_COUNT],
        },
        iaid,
        refinement: match refinement_template {
            Some(template) => Some(RefinementContexts::new(template, limits)?),
            None => None,
        },
        refinement_work: 0,
    })
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

impl ArithmeticCodec<'_> {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires crate::arithmetic::arithmetic_spec::decoder_valid(&old(self).decoder),
            symbol_text_spec::integer_contexts_valid(&old(self).integers),
        ensures
            crate::arithmetic::arithmetic_spec::decoder_state_valid(&final(self).decoder),
            symbol_text_spec::integer_contexts_valid(&final(self).integers),
            final(self).iaid@ == old(self).iaid@,
            final(self).refinement == old(self).refinement,
            final(self).refinement_work == old(self).refinement_work,
            out == symbol_text_spec::arithmetic_integer_result(
                crate::arithmetic::arithmetic_spec::integer_state(
                    old(self).decoder,
                    old(self).integers.slots[symbol_text_spec::integer_field_slot(field) as int]@,
                ).0,
            ),
            final(self).decoder == crate::arithmetic::arithmetic_spec::integer_state(
                old(self).decoder,
                old(self).integers.slots[symbol_text_spec::integer_field_slot(field) as int]@,
            ).1,
            final(self).integers.slots[symbol_text_spec::integer_field_slot(field) as int]@
                == crate::arithmetic::arithmetic_spec::integer_state(
                    old(self).decoder,
                    old(self).integers.slots[symbol_text_spec::integer_field_slot(field) as int]@,
                ).2,
            forall|slot: int| 0 <= slot < INTEGER_FIELD_COUNT
                && slot != symbol_text_spec::integer_field_slot(field) ==>
                    final(self).integers.slots[slot]@ == old(self).integers.slots[slot]@,
            match out {
                Ok(_) => crate::arithmetic::arithmetic_spec::decoder_valid(
                    &final(self).decoder
                ),
                Err(_) => true,
            },
    ))]
    fn decode_integer(&mut self, field: IntegerField) -> Result<EntropyInteger, SymbolTextError> {
        let slot = field.slot();
        let decoded = match self.decoder.decode_integer(&mut self.integers.slots[slot]) {
            Ok(decoded) => decoded,
            Err(error) => return Err(SymbolTextError::Decode(error.into_decode())),
        };
        match decoded {
            ArithmeticInteger::Value(value) => Ok(EntropyInteger::Value(value)),
            ArithmeticInteger::OutOfBand => Ok(EntropyInteger::OutOfBand),
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            crate::arithmetic::arithmetic_spec::decoder_valid(&old(self).decoder),
            symbol_text_spec::arithmetic_contexts_valid(old(self).iaid@),
        ensures
            crate::arithmetic::arithmetic_spec::decoder_state_valid(&final(self).decoder),
            symbol_text_spec::arithmetic_contexts_valid(final(self).iaid@),
            final(self).integers.slots@ == old(self).integers.slots@,
            final(self).refinement == old(self).refinement,
            final(self).refinement_work == old(self).refinement_work,
            out == symbol_text_spec::arithmetic_symbol_id_result(
                crate::arithmetic::arithmetic_spec::iaid_state(
                    old(self).decoder, old(self).iaid@, request.code_length,
                ).0,
            ),
            final(self).decoder == crate::arithmetic::arithmetic_spec::iaid_state(
                old(self).decoder, old(self).iaid@, request.code_length,
            ).1,
            final(self).iaid@ == crate::arithmetic::arithmetic_spec::iaid_state(
                old(self).decoder, old(self).iaid@, request.code_length,
            ).2,
            match out {
                Ok(_) => crate::arithmetic::arithmetic_spec::decoder_valid(
                    &final(self).decoder
                ),
                Err(_) => true,
            },
    ))]
    fn decode_symbol_id(&mut self, request: SymbolIdRequest) -> Result<u32, SymbolTextError> {
        match self
            .decoder
            .decode_iaid(request.code_length, &mut self.iaid)
        {
            Ok(value) => Ok(value),
            Err(error) => Err(SymbolTextError::Decode(error.into_decode())),
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            crate::arithmetic::arithmetic_spec::decoder_valid(&old(self).decoder),
            crate::spec::bitmap::storage_valid(request.reference),
        ensures match out {
            Ok(bitmap) =>
                crate::spec::bitmap::storage_valid(&bitmap)
                    && crate::arithmetic::arithmetic_spec::decoder_valid(
                        &final(self).decoder
                    )
                    && final(self).integers.slots@ == old(self).integers.slots@
                    && final(self).iaid@ == old(self).iaid@,
            Err(_) => true,
        },
    ))]
    fn decode_refinement(
        &mut self,
        request: RefinementRequest<'_>,
        limits: &Limits,
    ) -> Result<Bitmap, SymbolTextError> {
        let adaptive = [
            TemplateOffset::new(
                i32::from(request.adaptive_pixels[0].0),
                i32::from(request.adaptive_pixels[0].1),
            ),
            TemplateOffset::new(
                i32::from(request.adaptive_pixels[1].0),
                i32::from(request.adaptive_pixels[1].1),
            ),
        ];
        let call = RefinementArithmeticCall {
            width: request.geometry.width,
            height: request.geometry.height,
            adaptive,
            typical_prediction: false,
        };
        let parameters = RefinementParameters {
            reference_dx: request.geometry.reference_dx,
            reference_dy: request.geometry.reference_dy,
        };
        let contexts = self
            .refinement
            .as_mut()
            .ok_or(SymbolTextError::InvalidState(
                "missing arithmetic refinement contexts",
            ))?;
        Ok(decode_refinement_on(
            &mut self.decoder,
            contexts,
            call,
            request.reference,
            parameters,
            limits,
            &mut self.refinement_work,
        )?)
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            crate::arithmetic::arithmetic_spec::decoder_valid(&old(self).decoder),
            match call.skip {
                Some(mask) => crate::spec::bitmap::storage_valid(mask),
                None => true,
            },
        ensures match out {
            Ok(bitmap) =>
                crate::spec::bitmap::storage_valid(&bitmap)
                    && crate::arithmetic::arithmetic_spec::decoder_valid(
                        &final(self).decoder
                    )
                    && final(self).integers.slots@ == old(self).integers.slots@
                    && final(self).iaid@ == old(self).iaid@,
            Err(_) => true,
        },
    ))]
    fn decode_generic(
        &mut self,
        contexts: &mut GenericContexts,
        call: GenericArithmeticCall<'_>,
        height: u32,
        limits: &Limits,
        cumulative_work: &mut u64,
    ) -> Result<Bitmap, SymbolTextError> {
        Ok(decode_generic_on(
            &mut self.decoder,
            contexts,
            call,
            height,
            limits,
            cumulative_work,
        )?)
    }
}

// ---------------------------------------------------------------------------
// Huffman entropy adapter.
// ---------------------------------------------------------------------------

/// The two-byte sequence that must terminate a bounded arithmetic block.
#[cfg_attr(verus_keep_ghost, verus_verify)]
const BOUNDED_ARITHMETIC_TERMINATOR: [u8; 2] = [0xff, 0xac];

/// Validates that a bounded arithmetic slice ends in `FF AC` and contains no
/// earlier marker, mirroring `generic_region`'s equivalent (private) check
/// for the Huffman-declared-size refinement bitmaps embedded in symbol
/// dictionaries and text regions (T.88 6.5.8.2.2, 6.4.11).
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) =>
            data@.len() >= 2
            && data@[data@.len() - 2] == 0xff
            && data@[data@.len() - 1] == 0xac
            && forall|i: int| 0 <= i < data@.len() - 2 ==> !(
                #[trigger] data@[i] == 0xff && data@[i + 1] > 0x8f
            ),
        Err(_) => true,
    },
))]
fn validate_bounded_arithmetic_terminator(data: &[u8]) -> Result<(), SymbolTextError> {
    if data.len() < 2
        || data[data.len() - 2] != BOUNDED_ARITHMETIC_TERMINATOR[0]
        || data[data.len() - 1] != BOUNDED_ARITHMETIC_TERMINATOR[1]
    {
        return Err(SymbolTextError::InvalidState(
            "embedded arithmetic refinement data is missing its FF AC terminator",
        ));
    }

    let final_offset = data
        .len()
        .checked_sub(2)
        .ok_or(MalformedError::IntegerOverflow)?;
    let mut offset = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            final_offset == data@.len() - 2,
            offset <= data@.len() - 1,
            forall|i: int| 0 <= i < offset ==> (
                !(#[trigger] data@[i] == 0xff && data@[i + 1] > 0x8f)
                || i == final_offset
            ),
        decreases data@.len() - 1 - offset,
    ))]
    while offset < data.len() - 1 {
        if data[offset] == 0xff && data[offset + 1] > 0x8f && offset != final_offset {
            return Err(SymbolTextError::InvalidState(
                "premature arithmetic marker in embedded refinement data",
            ));
        }
        offset += 1;
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
enum NormalizedArithmetic<'a> {
    Borrowed(&'a [u8]),
    Owned(Vec<u8>),
}

impl NormalizedArithmetic<'_> {
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn bytes(&self) -> &[u8] {
        match self {
            Self::Borrowed(bytes) => bytes,
            Self::Owned(bytes) => bytes,
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn len(&self) -> usize {
        self.bytes().len()
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn normalized_bounded_arithmetic<'a>(
    data: &'a [u8],
    limits: &Limits,
) -> Result<NormalizedArithmetic<'a>, SymbolTextError> {
    if data.ends_with(&BOUNDED_ARITHMETIC_TERMINATOR) {
        validate_bounded_arithmetic_terminator(data)?;
        return Ok(NormalizedArithmetic::Borrowed(data));
    }
    if data.last() != Some(&BOUNDED_ARITHMETIC_TERMINATOR[0]) {
        return Err(SymbolTextError::InvalidState(
            "bounded arithmetic data has no terminal marker",
        ));
    }
    let requested_bytes = u64::try_from(data.len())
        .map_err(|_| MalformedError::IntegerOverflow)?
        .checked_add(1)
        .ok_or(MalformedError::IntegerOverflow)?;
    enforce_limit(
        LimitKind::Memory,
        limits.max_decoder_managed_memory_bytes,
        requested_bytes,
    )?;
    let mut normalized = Vec::new();
    normalized
        .try_reserve(
            data.len()
                .checked_add(1)
                .ok_or(MalformedError::IntegerOverflow)?,
        )
        .map_err(|_| DecodeError::Allocation { requested_bytes })?;
    normalized.extend_from_slice(data);
    normalized.push(BOUNDED_ARITHMETIC_TERMINATOR[1]);
    validate_bounded_arithmetic_terminator(&normalized)?;
    Ok(NormalizedArithmetic::Owned(normalized))
}

/// Validates that any marker the decoder actually observed is the declared
/// final `FF AC`, after a successful bounded decode.
#[cfg_attr(verus_keep_ghost, verus_verify)]
fn validate_bounded_arithmetic_marker(
    decoder: &ArithmeticDecoder<'_>,
    data_len: usize,
) -> Result<(), SymbolTextError> {
    if let Some(marker) = decoder.marker() {
        let expected_offset = data_len
            .checked_sub(2)
            .ok_or(MalformedError::IntegerOverflow)?;
        if marker.code != BOUNDED_ARITHMETIC_TERMINATOR[1] || marker.offset != expected_offset {
            return Err(SymbolTextError::InvalidState(
                "unexpected arithmetic marker in embedded refinement data",
            ));
        }
    }
    Ok(())
}

/// Rounds a bit reader forward to the next byte boundary.
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires old(reader).bit_position <= old(reader).bytes@.len() * 8,
    ensures
        final(reader).bytes@ == old(reader).bytes@,
        match out {
            Ok(()) =>
                final(reader).bit_position >= old(reader).bit_position
                && final(reader).bit_position <= final(reader).bytes@.len() * 8
                && final(reader).bit_position % 8 == 0
                && final(reader).bit_position < old(reader).bit_position + 8,
            Err(_) => final(reader).bit_position == old(reader).bit_position,
        },
))]
fn byte_align(reader: &mut BitReader<'_>) -> Result<(), SymbolTextError> {
    let total_bits = reader
        .bytes
        .len()
        .checked_mul(8)
        .ok_or(MalformedError::IntegerOverflow)?;
    let aligned = reader
        .bit_position
        .checked_add(7)
        .ok_or(MalformedError::IntegerOverflow)?
        / 8
        * 8;
    if aligned > total_bits {
        let needed = aligned
            .checked_sub(reader.bit_position)
            .and_then(|bits| u64::try_from(bits).ok())
            .ok_or(MalformedError::IntegerOverflow)?;
        let remaining = total_bits
            .checked_sub(reader.bit_position)
            .and_then(|bits| u64::try_from(bits).ok())
            .ok_or(MalformedError::IntegerOverflow)?;
        return Err(MalformedError::UnexpectedEof {
            offset: u64::try_from(reader.bit_position)
                .map_err(|_error| MalformedError::IntegerOverflow)?,
            needed,
            remaining,
        }
        .into());
    }
    reader.bit_position = aligned;
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires crate::huffman::huffman_spec::reader_valid(old(reader)),
    ensures crate::huffman::huffman_spec::reader_valid(final(reader)),
))]
fn read_symbol_id_runcode_lines(
    reader: &mut BitReader<'_>,
) -> Result<[HuffmanLine; 35], SymbolTextError> {
    const RUNCODE_COUNT: usize = 35;
    let mut runcode_lines = [HuffmanLine::normal(0, 0, 0); RUNCODE_COUNT];
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index <= RUNCODE_COUNT,
            crate::huffman::huffman_spec::reader_valid(reader),
        decreases RUNCODE_COUNT - index,
    ))]
    while index < RUNCODE_COUNT {
        let prefix_len =
            u8::try_from(reader.read_bits(4)?).map_err(|_| MalformedError::IntegerOverflow)?;
        let value = i32::try_from(index).map_err(|_| MalformedError::IntegerOverflow)?;
        runcode_lines[index] = HuffmanLine::normal(value, prefix_len, 0);
        index += 1;
    }
    Ok(runcode_lines)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires crate::huffman::huffman_spec::reader_valid(old(reader)),
    ensures crate::huffman::huffman_spec::reader_valid(final(reader)),
))]
fn decode_symbol_id_code_lengths(
    reader: &mut BitReader<'_>,
    runcode_table: &HuffmanTable,
    symbol_count: u32,
    limits: &Limits,
) -> Result<Vec<u8>, SymbolTextError> {
    let capacity = usize::try_from(symbol_count).map_err(|_| MalformedError::IntegerOverflow)?;
    let mut lengths: Vec<u8> = Vec::new();
    lengths
        .try_reserve(capacity)
        .map_err(|_| DecodeError::Allocation {
            requested_bytes: u64::from(symbol_count),
        })?;
    let mut resources = ResourceTracker {
        work_units: 0,
        decoded_bitmap_bytes: 0,
    };
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            lengths@.len() <= capacity,
            forall|index: int| 0 <= index < lengths@.len()
                ==> #[trigger] lengths@[index] <= 31,
            symbol_text_spec::work_units(&resources) <= limits.max_work_units,
            crate::huffman::huffman_spec::reader_valid(reader),
        decreases capacity - lengths@.len(),
    ))]
    while lengths.len() < capacity {
        resources.charge_work(1, limits)?;
        let code = match runcode_table.decode(reader)? {
            HuffmanValue::Integer(value) => value,
            HuffmanValue::OutOfBand => {
                return Err(SymbolTextError::UnexpectedOutOfBand("symbol ID run code"))
            }
        };
        match code {
            0..=31 => {
                let length = u8::try_from(code).map_err(|_| MalformedError::IntegerOverflow)?;
                lengths.push(length);
            }
            32 => {
                let count = u8::try_from(
                    reader
                        .read_bits(2)?
                        .checked_add(3)
                        .ok_or(MalformedError::IntegerOverflow)?,
                )
                .map_err(|_| MalformedError::IntegerOverflow)?;
                let previous = *lengths
                    .last()
                    .ok_or(SymbolTextError::InvalidSymbolLengthRun)?;
                extend_lengths(&mut lengths, capacity, previous, usize::from(count))?;
            }
            33 => {
                let count = u16::try_from(
                    reader
                        .read_bits(3)?
                        .checked_add(3)
                        .ok_or(MalformedError::IntegerOverflow)?,
                )
                .map_err(|_| MalformedError::IntegerOverflow)?;
                extend_lengths(&mut lengths, capacity, 0, usize::from(count))?;
            }
            34 => {
                let count = u16::try_from(
                    reader
                        .read_bits(7)?
                        .checked_add(11)
                        .ok_or(MalformedError::IntegerOverflow)?,
                )
                .map_err(|_| MalformedError::IntegerOverflow)?;
                extend_lengths(&mut lengths, capacity, 0, usize::from(count))?;
            }
            _ => return Err(SymbolTextError::InvalidSymbolLengthRun),
        }
    }
    resources.charge_work(u64::from(symbol_count), limits)?;
    Ok(lengths)
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

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(codec) => crate::huffman::huffman_spec::reader_valid(&codec.reader),
        Err(_) => true,
    },
))]
fn new_huffman_codec<'a>(
    bytes: &'a [u8],
    tables: HuffmanFieldTables<'a>,
    log_strip_size: u8,
    refinement_template: Option<RefinementTemplate>,
    limits: &Limits,
) -> Result<HuffmanCodec<'a>, SymbolTextError> {
    if bytes.len() > usize::MAX / 8 {
        return Err(MalformedError::IntegerOverflow.into());
    }
    Ok(HuffmanCodec {
        reader: bit_reader(bytes),
        tables,
        log_strip_size,
        symbol_id_table: None,
        refinement: match refinement_template {
            Some(template) => Some(RefinementContexts::new(template, limits)?),
            None => None,
        },
        refinement_work: 0,
    })
}

impl HuffmanCodec<'_> {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires crate::huffman::huffman_spec::reader_valid(&old(self).reader),
        ensures crate::huffman::huffman_spec::reader_valid(&final(self).reader),
    ))]
    fn read_raw_bits(&mut self, count: u8) -> Result<u32, SymbolTextError> {
        Ok(self.reader.read_bits(count)?)
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires crate::huffman::huffman_spec::reader_valid(&old(self).reader),
        ensures crate::huffman::huffman_spec::reader_valid(&final(self).reader),
    ))]
    fn decode_integer(&mut self, field: IntegerField) -> Result<EntropyInteger, SymbolTextError> {
        match field {
            IntegerField::WithinStripT => Ok(EntropyInteger::Value(i64::from(
                self.read_raw_bits(self.log_strip_size)?,
            ))),
            IntegerField::RefinementIndicator => {
                Ok(EntropyInteger::Value(i64::from(self.read_raw_bits(1)?)))
            }
            _ => {
                let table = self.tables.table_for(field).ok_or(
                    SymbolTextError::InvalidTableAssociation("no Huffman table bound for field"),
                )?;
                match table.decode(&mut self.reader)? {
                    HuffmanValue::Integer(value) => Ok(EntropyInteger::Value(i64::from(value))),
                    HuffmanValue::OutOfBand => Ok(EntropyInteger::OutOfBand),
                }
            }
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires crate::huffman::huffman_spec::reader_valid(&old(self).reader),
        ensures crate::huffman::huffman_spec::reader_valid(&final(self).reader),
    ))]
    fn decode_symbol_id(&mut self, request: SymbolIdRequest) -> Result<u32, SymbolTextError> {
        // When a symbol-ID Huffman table has been prepared (T.88 7.4.3.1.7,
        // normal text-region decoding), decode through it. Otherwise this is
        // the symbol dictionary's own single-instance aggregate refinement ID
        // (T.88 6.5.8.2.2), which T.88 specifies as a literal fixed-width
        // field of `code_length` bits, not a Huffman code.
        if let Some(table) = self.symbol_id_table.as_ref() {
            match table.decode(&mut self.reader)? {
                HuffmanValue::Integer(value) => u32::try_from(value)
                    .map_err(|_| SymbolTextError::InvalidEntropyValue("symbol ID")),
                HuffmanValue::OutOfBand => Err(SymbolTextError::UnexpectedOutOfBand("symbol ID")),
            }
        } else {
            self.read_raw_bits(request.code_length)
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            crate::huffman::huffman_spec::reader_valid(&old(self).reader),
            crate::spec::bitmap::storage_valid(request.reference),
        ensures
            crate::huffman::huffman_spec::reader_valid(&final(self).reader),
            match out {
                Ok(bitmap) => crate::spec::bitmap::storage_valid(&bitmap),
                Err(_) => true,
            },
    ))]
    fn decode_refinement(
        &mut self,
        request: RefinementRequest<'_>,
        limits: &Limits,
    ) -> Result<Bitmap, SymbolTextError> {
        let declared_size = request
            .declared_data_size
            .ok_or(SymbolTextError::InvalidState(
                "Huffman refinement bitmap is missing its declared byte size",
            ))?;
        byte_align(&mut self.reader)?;
        let byte_offset = self
            .reader
            .bit_position
            .checked_div(8)
            .ok_or(MalformedError::IntegerOverflow)?;
        let size = usize::try_from(declared_size).map_err(|_| MalformedError::IntegerOverflow)?;
        let end = byte_offset
            .checked_add(size)
            .ok_or(MalformedError::IntegerOverflow)?;
        let remaining = if byte_offset <= self.reader.bytes.len() {
            self.reader.bytes.len() - byte_offset
        } else {
            0
        };
        let slice =
            self.reader
                .bytes
                .get(byte_offset..end)
                .ok_or(MalformedError::UnexpectedEof {
                    offset: u64::try_from(byte_offset)
                        .map_err(|_| MalformedError::IntegerOverflow)?,
                    needed: declared_size.into(),
                    remaining: u64::try_from(remaining)
                        .map_err(|_| MalformedError::IntegerOverflow)?,
                })?;
        let normalized = normalized_bounded_arithmetic(slice, limits)?;

        let adaptive = [
            TemplateOffset::new(
                i32::from(request.adaptive_pixels[0].0),
                i32::from(request.adaptive_pixels[0].1),
            ),
            TemplateOffset::new(
                i32::from(request.adaptive_pixels[1].0),
                i32::from(request.adaptive_pixels[1].1),
            ),
        ];
        let call = RefinementArithmeticCall {
            width: request.geometry.width,
            height: request.geometry.height,
            adaptive,
            typical_prediction: false,
        };
        let parameters = RefinementParameters {
            reference_dx: request.geometry.reference_dx,
            reference_dy: request.geometry.reference_dy,
        };
        let contexts = self
            .refinement
            .as_mut()
            .ok_or(SymbolTextError::InvalidState(
                "missing Huffman refinement contexts",
            ))?;
        let mut decoder = ArithmeticDecoder::new(normalized.bytes())?;
        let bitmap = decode_refinement_on(
            &mut decoder,
            contexts,
            call,
            request.reference,
            parameters,
            limits,
            &mut self.refinement_work,
        )?;
        validate_bounded_arithmetic_marker(&decoder, normalized.len())?;

        let advanced_bits = end.checked_mul(8).ok_or(MalformedError::IntegerOverflow)?;
        self.reader.bit_position = advanced_bits;
        Ok(bitmap)
    }
}

impl<'a> SymbolCodec<'a> {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires symbol_text_spec::codec_valid(old(self)),
        ensures match out {
            Ok(_) => symbol_text_spec::codec_valid(final(self)),
            Err(_) => true,
        },
    ))]
    fn decode_integer(&mut self, field: IntegerField) -> Result<EntropyInteger, SymbolTextError> {
        match self {
            Self::Arithmetic(codec) => codec.decode_integer(field),
            Self::Huffman(codec) => codec.decode_integer(field),
            #[cfg(test)]
            Self::Scripted(codec) => codec
                .integers
                .pop_front()
                .ok_or(SymbolTextError::InvalidState("missing scripted integer")),
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires symbol_text_spec::codec_valid(old(self)),
        ensures match out {
            Ok(_) => symbol_text_spec::codec_valid(final(self)),
            Err(_) => true,
        },
    ))]
    fn decode_symbol_id(&mut self, request: SymbolIdRequest) -> Result<u32, SymbolTextError> {
        match self {
            Self::Arithmetic(codec) => codec.decode_symbol_id(request),
            Self::Huffman(codec) => codec.decode_symbol_id(request),
            #[cfg(test)]
            Self::Scripted(codec) => Ok(codec.symbol_id),
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            symbol_text_spec::codec_valid(old(self)),
            crate::spec::bitmap::storage_valid(request.reference),
        ensures match out {
            Ok(bitmap) =>
                symbol_text_spec::codec_valid(final(self))
                    && crate::spec::bitmap::storage_valid(&bitmap),
            Err(_) => true,
        },
    ))]
    fn decode_refinement(
        &mut self,
        request: RefinementRequest<'_>,
        limits: &Limits,
    ) -> Result<Bitmap, SymbolTextError> {
        match self {
            Self::Arithmetic(codec) => codec.decode_refinement(request, limits),
            Self::Huffman(codec) => codec.decode_refinement(request, limits),
            #[cfg(test)]
            Self::Scripted(_) => Ok(Bitmap::blank(
                request.geometry.width,
                request.geometry.height,
                limits,
            )?),
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires symbol_text_spec::codec_valid(old(self)),
        ensures match out {
            Ok(codec) =>
                crate::huffman::huffman_spec::reader_valid(&codec.reader),
            Err(_) => true,
        },
    ))]
    fn huffman_mut(&mut self) -> Result<&mut HuffmanCodec<'a>, SymbolTextError> {
        match self {
            Self::Huffman(codec) => Ok(codec),
            Self::Arithmetic(_) => Err(SymbolTextError::InvalidState(
                "Huffman cursor requested from arithmetic state",
            )),
            #[cfg(test)]
            Self::Scripted(_) => Err(SymbolTextError::InvalidState(
                "Huffman cursor requested from scripted state",
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// Small bitmap and template-mapping helpers shared by the dictionary and
// text-region decoders.
// ---------------------------------------------------------------------------

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures symbol_text_spec::generic_template_matches(value, out),
))]
fn generic_template_from_u8(value: u8) -> GenericTemplate {
    match value {
        0 => GenericTemplate::Template0,
        1 => GenericTemplate::Template1,
        2 => GenericTemplate::Template2,
        _ => GenericTemplate::Template3,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures symbol_text_spec::refinement_template_matches(value, out),
))]
fn refinement_template_from_u8(value: u8) -> RefinementTemplate {
    if value == 0 {
        RefinementTemplate::Template0
    } else {
        RefinementTemplate::Template1
    }
}

/// Constructs a bitmap directly from exactly `stride * height` packed bytes,
/// as used by T.88 6.5.9's uncompressed collective bitmap.
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(bitmap) =>
            crate::spec::bitmap::storage_valid(&bitmap)
            && bitmap.width == width
            && bitmap.height == height,
        Err(_) => true,
    },
))]
#[allow(clippy::too_many_lines)]
fn bitmap_from_packed_bytes(
    width: u32,
    height: u32,
    bytes: &[u8],
    limits: &Limits,
) -> Result<Bitmap, SymbolTextError> {
    check_dimensions(width, height, limits)?;
    let stride = u64::from(width)
        .checked_add(7)
        .ok_or(MalformedError::IntegerOverflow)?
        / 8;
    let length = stride
        .checked_mul(u64::from(height))
        .ok_or(MalformedError::IntegerOverflow)?;
    enforce_limit(
        LimitKind::Memory,
        limits.max_decoder_managed_memory_bytes,
        length,
    )?;
    let Ok(length_usize) = usize::try_from(length) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    if bytes.len() < length_usize {
        let Ok(remaining) = u64::try_from(bytes.len()) else {
            return Err(MalformedError::IntegerOverflow.into());
        };
        return Err(MalformedError::UnexpectedEof {
            offset: 0,
            needed: length,
            remaining,
        }
        .into());
    }
    if width % 8 != 0 {
        let unused = 8 - (width % 8);
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(1 <= unused <= 7);
        }
        let padding_mask = match unused {
            1 => 0x01,
            2 => 0x03,
            3 => 0x07,
            4 => 0x0f,
            5 => 0x1f,
            6 => 0x3f,
            7 => 0x7f,
            _ => return Err(MalformedError::IntegerOverflow.into()),
        };
        let Ok(stride_usize) = usize::try_from(stride) else {
            return Err(MalformedError::IntegerOverflow.into());
        };
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                row <= height,
                length_usize <= bytes@.len(),
                length_usize as nat == stride_usize as nat * height as nat,
                stride_usize > 0,
            decreases height - row,
        ))]
        for row in 0..height {
            let Ok(row) = usize::try_from(row) else {
                return Err(MalformedError::IntegerOverflow.into());
            };
            let row_offset = row
                .checked_mul(stride_usize)
                .ok_or(MalformedError::IntegerOverflow)?;
            let stride_minus_one = stride_usize
                .checked_sub(1)
                .ok_or(MalformedError::IntegerOverflow)?;
            let final_byte = row_offset
                .checked_add(stride_minus_one)
                .ok_or(MalformedError::IntegerOverflow)?;
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(row < height as usize);
                assert(row_offset as nat == row as nat * stride_usize as nat);
                assert(stride_minus_one as nat + 1 == stride_usize as nat);
                assert(final_byte as nat
                    == row_offset as nat + stride_minus_one as nat);
                vstd::arithmetic::mul::lemma_mul_inequality(
                    row as int + 1,
                    height as int,
                    stride_usize as int,
                );
                vstd::arithmetic::mul::lemma_mul_is_distributive_add_other_way(
                    stride_usize as int,
                    row as int,
                    1,
                );
                vstd::arithmetic::mul::lemma_mul_is_commutative(
                    height as int,
                    stride_usize as int,
                );
                assert(final_byte as nat + 1
                    == (row as nat + 1) * stride_usize as nat);
                assert(final_byte < bytes@.len());
            }
            if bytes[final_byte] & padding_mask != 0 {
                return Err(SymbolTextError::InvalidState(
                    "raw collective bitmap has nonzero padding",
                ));
            }
        }
    }
    let mut data = Vec::new();
    if data.try_reserve(length_usize).is_err() {
        return Err(DecodeError::Allocation {
            requested_bytes: length,
        }
        .into());
    }
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index <= length_usize,
            length_usize <= bytes@.len(),
            data@ == bytes@.subrange(0, index as int),
        decreases length_usize - index,
    ))]
    while index < length_usize {
        data.push(bytes[index]);
        index += 1;
    }
    let Ok(stride) = usize::try_from(stride) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    let bitmap = Bitmap {
        width,
        height,
        stride,
        data,
    };
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::views::bitmap_fields(&bitmap);
        reveal(crate::spec::bitmap::storage_valid);
        reveal(crate::spec::bitmap::stride_for_width);
    }
    Ok(bitmap)
}

/// Crops a fixed-height column range out of a wider collective bitmap, used
/// to split T.88 6.5.9 height-class collective bitmaps into their individual
/// symbols.
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires crate::spec::bitmap::storage_valid(source),
    ensures match out {
        Ok(bitmap) =>
            crate::spec::bitmap::storage_valid(&bitmap)
            && crate::views::bitmap_width(&bitmap) == width
            && crate::views::bitmap_height(&bitmap)
                == crate::views::bitmap_height(source),
        Err(_) => true,
    },
))]
fn crop_bitmap_columns(
    source: &Bitmap,
    x_start: u32,
    width: u32,
    limits: &Limits,
) -> Result<Bitmap, SymbolTextError> {
    let height = source.height();
    let mut cropped = Bitmap::blank(width, height, limits)?;
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::views::bitmap_fields(&cropped);
        assert(cropped.width == width);
        assert(cropped.height == height);
    }
    let source_end = x_start
        .checked_add(width)
        .ok_or(MalformedError::IntegerOverflow)?;
    if source_end > source.width() {
        return Err(MalformedError::IntegerOverflow.into());
    }
    cropped.compose_region(
        source,
        x_start,
        0,
        0,
        0,
        width,
        height,
        CombinationOperator::Replace,
    )?;
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::views::bitmap_fields(&cropped);
        reveal(crate::spec::bitmap::storage_valid);
        assert(crate::spec::bitmap::storage_valid(&cropped));
        assert(crate::views::bitmap_width(&cropped) == width);
        assert(crate::views::bitmap_height(&cropped) == height);
    }
    Ok(cropped)
}

/// Deep-copies a bitmap's packed bytes under fallible allocation.
///
/// [`Bitmap`] intentionally has no infallible [`Clone`]; exported symbols
/// must still become independently owned bitmaps, so this performs the copy
/// explicitly through its crate-visible fields.
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires crate::spec::bitmap::storage_valid(source),
    ensures match out {
        Ok(bitmap) =>
            crate::spec::bitmap::storage_valid(&bitmap)
            && bitmap.width == source.width
            && bitmap.height == source.height
            && bitmap.stride == source.stride
            && bitmap.data@ == source.data@,
        Err(_) => true,
    },
))]
fn clone_bitmap(source: &Bitmap, limits: &Limits) -> Result<Bitmap, SymbolTextError> {
    let Ok(requested_bytes) = u64::try_from(source.data.len()) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    enforce_limit(
        LimitKind::Memory,
        limits.max_decoder_managed_memory_bytes,
        requested_bytes,
    )?;
    let mut data = Vec::new();
    if data.try_reserve(source.data.len()).is_err() {
        return Err(DecodeError::Allocation { requested_bytes }.into());
    }
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index <= source.data@.len(),
            data@ == source.data@.subrange(0, index as int),
        decreases source.data@.len() - index,
    ))]
    while index < source.data.len() {
        data.push(source.data[index]);
        index += 1;
    }
    let bitmap = Bitmap {
        width: source.width,
        height: source.height,
        stride: source.stride,
        data,
    };
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::views::bitmap_fields(&bitmap);
        crate::views::bitmap_fields(source);
        reveal(crate::spec::bitmap::storage_valid);
    }
    Ok(bitmap)
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
            Some(bitmap) => crate::spec::bitmap::storage_valid(bitmap),
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
        Ok(bitmap) => crate::spec::bitmap::storage_valid(bitmap),
        Err(_) => true,
    },
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

/// Wraps caller-supplied custom Huffman tables, deriving each table's
/// out-of-band capability with [`HuffmanTable::has_oob`] rather than trusting
/// a caller-supplied flag.
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(wrapped) => wrapped@.len() == custom_tables@.len(),
        Err(_) => true,
    },
))]
fn wrap_custom_tables<'a>(
    custom_tables: &'a [&'a HuffmanTable],
) -> Result<Vec<CustomHuffmanTable<'a>>, SymbolTextError> {
    let mut wrapped = Vec::new();
    let Some(requested_bytes) = custom_tables
        .len()
        .checked_mul(core::mem::size_of::<CustomHuffmanTable<'_>>())
    else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    let Ok(requested_bytes) = u64::try_from(requested_bytes) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    if wrapped.try_reserve(custom_tables.len()).is_err() {
        return Err(DecodeError::Allocation { requested_bytes }.into());
    }
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index <= custom_tables@.len(),
            wrapped@.len() == index,
        decreases custom_tables@.len() - index,
    ))]
    while index < custom_tables.len() {
        let table = custom_tables[index];
        wrapped.push(CustomHuffmanTable {
            table,
            has_oob: table.has_oob(),
        });
        index += 1;
    }
    Ok(wrapped)
}

/// The base-two logarithm of a strip size that is always a power of two
/// (T.88 7.4.3.1.1's two-bit `LOGSBSTRIPS` field).
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires symbol_text_spec::valid_strip_size(strip_size),
    ensures symbol_text_spec::strip_log_valid(strip_size, out),
))]
fn log2_strip_size(strip_size: u8) -> u8 {
    if strip_size <= 1 {
        0
    } else if strip_size <= 3 {
        1
    } else if strip_size <= 7 {
        2
    } else if strip_size <= 15 {
        3
    } else if strip_size <= 31 {
        4
    } else if strip_size <= 63 {
        5
    } else if strip_size <= 127 {
        6
    } else {
        7
    }
}

/// Decodes one T.88 6.5.8.2.2 single-instance refinement/aggregate symbol
/// bitmap, shared by the arithmetic and Huffman symbol-dictionary decoders.
///
/// Unlike the text-region per-instance refinement procedure (handled by the
/// existing [`decode_refined_bitmap`] helper), this procedure never decodes a
/// width or height delta: the symbol's dimensions were already fixed by the
/// enclosing height-class loop, so only the symbol ID and reference offset
/// are decoded before invoking the shared generic-refinement callback.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        symbol_text_spec::codec_valid(old(codec)),
        symbol_text_spec::codebook_valid(imported),
        symbol_text_spec::bitmaps_valid(new_symbols@),
    ensures match out {
        Ok(bitmap) =>
            symbol_text_spec::codec_valid(final(codec))
                && crate::spec::bitmap::storage_valid(&bitmap)
                && crate::views::bitmap_width(&bitmap) == width
                && crate::views::bitmap_height(&bitmap) == height
                && symbol_text_spec::work_units(final(resources))
                    == symbol_text_spec::work_units(old(resources)),
        Err(_) => true,
    },
))]
fn decode_dictionary_single_instance_refinement(
    codec: &mut SymbolCodec<'_>,
    imported: SymbolCodebook<'_>,
    new_symbols: &[Bitmap],
    code_length: u8,
    width: u32,
    height: u32,
    refinement_template: u8,
    refinement_adaptive: [(i8, i8); 2],
    huffman: bool,
    limits: &Limits,
    resources: &mut ResourceTracker,
) -> Result<Bitmap, SymbolTextError> {
    let available = imported
        .len()
        .checked_add(new_symbols.len())
        .and_then(|count| u32::try_from(count).ok())
        .ok_or(MalformedError::IntegerOverflow)?;
    let id = decode_checked_symbol_id(
        codec,
        SymbolIdRequest {
            available_symbols: available,
            code_length,
            huffman,
        },
    )?;
    let reference_x = required_integer(codec, IntegerField::RefinementX)?;
    let reference_y = required_integer(codec, IntegerField::RefinementY)?;
    let declared_size = if huffman {
        let size = required_integer(codec, IntegerField::RefinementSize)?;
        Some(
            u32::try_from(size)
                .map_err(|_| SymbolTextError::InvalidEntropyValue("refinement size"))?,
        )
    } else {
        None
    };
    let reference = symbol_by_id(imported, new_symbols, id)?;
    check_dimensions(width, height, limits)?;
    resources.charge_bitmap(width, height, limits)?;
    let geometry = RefinementGeometry {
        width,
        height,
        reference_dx: i32::try_from(reference_x).map_err(|_| MalformedError::IntegerOverflow)?,
        reference_dy: i32::try_from(reference_y).map_err(|_| MalformedError::IntegerOverflow)?,
    };
    let request = RefinementRequest {
        reference,
        geometry,
        template: refinement_template,
        adaptive_pixels: refinement_adaptive,
        declared_data_size: declared_size,
    };
    let bitmap = codec.decode_refinement(request, limits)?;
    if bitmap.width() != width || bitmap.height() != height {
        return Err(SymbolTextError::InvalidDimensions);
    }
    Ok(bitmap)
}

/// Decodes one complete text-region bitmap (T.88 6.4), shared by top-level
/// text-region segments and the nested multiple-instance aggregate
/// procedure invoked from a symbol dictionary (T.88 6.5.8.2.1).
///
/// `code_length` is the fixed IAID bit width for the whole enclosing segment;
/// `available_symbols` is the (possibly
/// smaller, for a nested aggregate call still in progress) slice used both
/// to validate decoded IDs and to look up referenced bitmaps.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        symbol_text_spec::codec_valid(old(codec)),
        symbol_text_spec::codebook_valid(available_symbols),
        symbol_text_spec::work_units(old(resources)) <= limits.max_work_units,
    ensures match out {
        Ok(bitmap) =>
            symbol_text_spec::codec_valid(final(codec))
                && crate::spec::bitmap::storage_valid(&bitmap)
                && crate::views::bitmap_width(&bitmap) == width
                && crate::views::bitmap_height(&bitmap) == height
                && symbol_text_spec::work_units(final(resources))
                    <= limits.max_work_units,
        Err(_) => true,
    },
))]
fn decode_text_region_body(
    codec: &mut SymbolCodec<'_>,
    available_symbols: SymbolCodebook<'_>,
    code_length: u8,
    width: u32,
    height: u32,
    number_of_instances: u32,
    config: &TextRegionConfig,
    refinement_adaptive: [(i8, i8); 2],
    limits: &Limits,
    resources: &mut ResourceTracker,
) -> Result<Bitmap, SymbolTextError> {
    resources.charge_bitmap(width, height, limits)?;
    let mut bitmap = Bitmap::filled(width, height, config.default_pixel, limits)?;
    let mut placement_batch = PlacementBatch::new();
    if !config.refinement {
        placement_batch.reserve(
            usize::try_from(number_of_instances).map_err(|_| MalformedError::IntegerOverflow)?,
            limits,
        )?;
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        reveal(symbol_text_spec::placement_batch_valid);
    }

    let initial_strip = required_integer(codec, IntegerField::StripDelta)?;
    let mut state = TextRegionState::new(*config, number_of_instances, initial_strip, limits)?;

    let available_count =
        u32::try_from(available_symbols.len()).map_err(|_| MalformedError::IntegerOverflow)?;

    let mut decoded = 0_u32;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            decoded <= number_of_instances,
            state.decoded_instances == decoded,
            state.strip.is_none(),
            symbol_text_spec::text_state_valid(&state),
            symbol_text_spec::codec_valid(codec),
            symbol_text_spec::codebook_valid(available_symbols),
            crate::spec::bitmap::storage_valid(&bitmap),
            crate::views::bitmap_width(&bitmap) == width,
            crate::views::bitmap_height(&bitmap) == height,
            symbol_text_spec::placement_batch_valid(&placement_batch, &bitmap),
            symbol_text_spec::work_units(resources) <= limits.max_work_units,
        decreases number_of_instances - decoded,
    ))]
    while decoded < number_of_instances {
        let strip_start = decoded;
        let strip_delta = required_integer(codec, IntegerField::StripDelta)?;
        let first_s_delta = required_integer(codec, IntegerField::FirstS)?;
        state.begin_strip(strip_delta, first_s_delta)?;
        let mut first = true;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                decoded <= number_of_instances,
                strip_start <= decoded,
                strip_start < number_of_instances,
                first ==> decoded == strip_start,
                !first ==> decoded > strip_start,
                state.decoded_instances == decoded,
                state.strip.is_some(),
                symbol_text_spec::text_state_valid(&state),
                symbol_text_spec::codec_valid(codec),
                symbol_text_spec::codebook_valid(available_symbols),
                crate::spec::bitmap::storage_valid(&bitmap),
                crate::views::bitmap_width(&bitmap) == width,
                crate::views::bitmap_height(&bitmap) == height,
                symbol_text_spec::placement_batch_valid(&placement_batch, &bitmap),
                symbol_text_spec::work_units(resources) <= limits.max_work_units,
            decreases number_of_instances - decoded,
        ))]
        loop {
            if decoded >= number_of_instances {
                #[cfg(verus_keep_ghost)]
                proof! {
                    assert(decoded > strip_start);
                }
                break;
            }
            let delta_s = if first {
                None
            } else {
                match codec.decode_integer(IntegerField::SubsequentS)? {
                    EntropyInteger::Value(value) => Some(value),
                    EntropyInteger::OutOfBand => {
                        #[cfg(verus_keep_ghost)]
                        proof! {
                            assert(decoded > strip_start);
                        }
                        break;
                    }
                }
            };
            let within_strip_t = if config.strip_size > 1 {
                match codec.decode_integer(IntegerField::WithinStripT)? {
                    EntropyInteger::Value(value) => u8::try_from(value).map_err(|_| {
                        SymbolTextError::InvalidEntropyValue("within-strip T coordinate")
                    })?,
                    EntropyInteger::OutOfBand => {
                        return Err(SymbolTextError::UnexpectedOutOfBand(
                            "within-strip T coordinate",
                        ))
                    }
                }
            } else {
                0
            };
            let symbol_id = decode_checked_symbol_id(
                codec,
                SymbolIdRequest {
                    available_symbols: available_count,
                    code_length,
                    huffman: config.huffman,
                },
            )?;
            let base_symbol = available_symbols
                .get(usize::try_from(symbol_id).map_err(|_| MalformedError::IntegerOverflow)?)
                .ok_or(SymbolTextError::SymbolIdOutOfRange {
                    id: symbol_id,
                    available: available_count,
                })?;

            let refined_storage;
            let placed_bitmap: &Bitmap = if config.refinement {
                let indicator = match codec.decode_integer(IntegerField::RefinementIndicator)? {
                    EntropyInteger::Value(value) => value,
                    EntropyInteger::OutOfBand => {
                        return Err(SymbolTextError::UnexpectedOutOfBand("refinement indicator"))
                    }
                };
                if indicator != 0 {
                    refined_storage = Some(decode_refined_bitmap(
                        codec,
                        base_symbol,
                        config.refinement_template,
                        refinement_adaptive,
                        config.huffman,
                        limits,
                        resources,
                    )?);
                    refined_storage.as_ref().unwrap_or(base_symbol)
                } else {
                    base_symbol
                }
            } else {
                base_symbol
            };

            let symbol_width = placed_bitmap.width();
            let symbol_height = placed_bitmap.height();
            let placement = match delta_s {
                None => state.place_first(
                    within_strip_t,
                    symbol_width,
                    symbol_height,
                    limits,
                    resources,
                )?,
                Some(delta) => state.place_subsequent(
                    delta,
                    within_strip_t,
                    symbol_width,
                    symbol_height,
                    limits,
                    resources,
                )?,
            };
            if config.refinement {
                flush_placement_batch(&mut bitmap, &mut placement_batch, config.operator)?;
                compose_placement(
                    &mut bitmap,
                    placed_bitmap,
                    placement,
                    config.operator,
                    limits,
                    resources,
                )?;
            } else if let Some(prepared) =
                prepare_placement(&bitmap, base_symbol, placement, limits, resources)?
            {
                if !placement_batch.compatible(prepared) {
                    flush_placement_batch(&mut bitmap, &mut placement_batch, config.operator)?;
                }
                placement_batch.push(prepared, &bitmap)?;
            }

            decoded = decoded
                .checked_add(1)
                .ok_or(MalformedError::IntegerOverflow)?;
            first = false;
        }
        if decoded == strip_start {
            return Err(SymbolTextError::InvalidState("empty text strip"));
        }
        flush_placement_batch(&mut bitmap, &mut placement_batch, config.operator)?;
        state.end_strip()?;
    }
    flush_placement_batch(&mut bitmap, &mut placement_batch, config.operator)?;
    state.finish()?;
    Ok(bitmap)
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

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        symbol_text_spec::codec_valid(old(codec)),
        symbol_text_spec::codebook_valid(imported),
        symbol_text_spec::bitmaps_valid(new_symbols@),
        symbol_text_spec::work_units(old(resources)) <= limits.max_work_units,
        header.adaptive_count <= 4,
    ensures match out {
        Ok(bitmap) =>
            symbol_text_spec::codec_valid(final(codec))
                && crate::spec::bitmap::storage_valid(&bitmap)
                && crate::views::bitmap_width(&bitmap) == width
                && crate::views::bitmap_height(&bitmap) == height
                && symbol_text_spec::work_units(final(resources))
                    <= limits.max_work_units
                && symbol_text_spec::work_units(final(resources))
                    >= symbol_text_spec::work_units(old(resources)),
        Err(_) => true,
    },
))]
fn decode_one_new_symbol(
    header: &SymbolDictionaryHeader,
    codec: &mut SymbolCodec<'_>,
    imported: SymbolCodebook<'_>,
    new_symbols: &[Bitmap],
    generic_contexts: Option<&mut GenericContexts>,
    generic_work: &mut u64,
    aggregation_instances: Option<u32>,
    width: u32,
    height: u32,
    code_length: u8,
    refinement_adaptive: [(i8, i8); 2],
    limits: &Limits,
    resources: &mut ResourceTracker,
) -> Result<Bitmap, SymbolTextError> {
    let config = header.config;
    if config.refinement_aggregation {
        let instances = aggregation_instances.ok_or(SymbolTextError::InvalidState(
            "missing aggregation-instance count",
        ))?;
        if instances == 0 {
            return Err(SymbolTextError::InvalidCount(
                "zero-instance aggregate symbol",
            ));
        }
        if instances > 1 {
            let capacity = imported
                .len()
                .checked_add(new_symbols.len())
                .ok_or(MalformedError::IntegerOverflow)?;
            let requested_bytes = capacity
                .checked_mul(core::mem::size_of::<&Bitmap>())
                .and_then(|bytes| u64::try_from(bytes).ok())
                .ok_or(MalformedError::IntegerOverflow)?;
            let mut symbol_refs: Vec<&Bitmap> = Vec::new();
            symbol_refs
                .try_reserve(capacity)
                .map_err(|_| DecodeError::Allocation { requested_bytes })?;
            let mut imported_index = 0_usize;
            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant
                    imported_index <= symbol_text_spec::codebook_len(imported),
                    symbol_refs@.len() == imported_index,
                    symbol_text_spec::codebook_valid(imported),
                    symbol_text_spec::bitmap_refs_valid(symbol_refs@),
                decreases symbol_text_spec::codebook_len(imported) - imported_index,
            ))]
            while imported_index < imported.len() {
                symbol_refs.push(imported.get(imported_index).ok_or(
                    SymbolTextError::InvalidState("missing imported aggregate symbol"),
                )?);
                imported_index += 1;
            }
            let mut new_index = 0_usize;
            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant
                    new_index <= new_symbols@.len(),
                    symbol_refs@.len()
                        == symbol_text_spec::codebook_len(imported) + new_index,
                    symbol_text_spec::bitmaps_valid(new_symbols@),
                    symbol_text_spec::bitmap_refs_valid(symbol_refs@),
                decreases new_symbols@.len() - new_index,
            ))]
            while new_index < new_symbols.len() {
                symbol_refs.push(&new_symbols[new_index]);
                new_index += 1;
            }
            let nested_config = TextRegionConfig {
                huffman: config.huffman,
                refinement: true,
                strip_size: 1,
                reference_corner: ReferenceCorner::TopLeft,
                transposed: false,
                operator: CombinationOperator::Or,
                default_pixel: false,
                s_offset: 0,
                refinement_template: config.refinement_template,
                huffman_choices: None,
            };
            let work_before = resources.work_units;
            let bitmap = decode_text_region_body(
                codec,
                SymbolCodebook::Borrowed(&symbol_refs),
                code_length,
                width,
                height,
                instances,
                &nested_config,
                refinement_adaptive,
                limits,
                resources,
            )?;
            if resources.work_units < work_before {
                return Err(SymbolTextError::InvalidState(
                    "resource work counter decreased",
                ));
            }
            Ok(bitmap)
        } else {
            if aggregation_instances.is_some() {
                return Err(SymbolTextError::InvalidState(
                    "aggregation count on direct-coded symbol",
                ));
            }
            decode_dictionary_single_instance_refinement(
                codec,
                imported,
                new_symbols,
                code_length,
                width,
                height,
                config.refinement_template,
                refinement_adaptive,
                config.huffman,
                limits,
                resources,
            )
        }
    } else {
        resources.charge_bitmap(width, height, limits)?;
        let contexts = generic_contexts.ok_or(SymbolTextError::InvalidState(
            "missing generic contexts for a direct-coded dictionary",
        ))?;
        let mut full_adaptive = [TemplateOffset::new(0, 0); 12];
        full_adaptive[..usize::from(header.adaptive_count)]
            .copy_from_slice(&header.adaptive[..usize::from(header.adaptive_count)]);
        let call = GenericArithmeticCall {
            width,
            height,
            adaptive: full_adaptive,
            typical_prediction: false,
            skip: None,
        };
        let bitmap = match codec {
            SymbolCodec::Arithmetic(codec) => {
                codec.decode_generic(contexts, call, height, limits, generic_work)
            }
            SymbolCodec::Huffman(_) => Err(SymbolTextError::InvalidState(
                "direct dictionary bitmap requires arithmetic state",
            )),
            #[cfg(test)]
            SymbolCodec::Scripted(_) => Err(SymbolTextError::InvalidState(
                "direct dictionary bitmap requires arithmetic state",
            )),
        }?;
        if bitmap.width() != width || bitmap.height() != height {
            return Err(SymbolTextError::InvalidDimensions);
        }
        Ok(bitmap)
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires crate::huffman::huffman_spec::reader_valid(old(reader)),
    ensures
        crate::huffman::huffman_spec::reader_valid(final(reader)),
        match out {
            Ok(bitmap) => crate::spec::bitmap::storage_valid(&bitmap),
            Err(_) => true,
        },
))]
fn decode_collective_bitmap(
    reader: &mut BitReader<'_>,
    bitmap_size: u32,
    width: u32,
    height: u32,
    limits: &Limits,
) -> Result<Bitmap, SymbolTextError> {
    byte_align(reader)?;
    let byte_offset = reader
        .bit_position
        .checked_div(8)
        .ok_or(MalformedError::IntegerOverflow)?;
    let length = if bitmap_size == 0 {
        (u64::from(width)
            .checked_add(7)
            .ok_or(MalformedError::IntegerOverflow)?
            / 8)
        .checked_mul(u64::from(height))
        .ok_or(MalformedError::IntegerOverflow)?
    } else {
        u64::from(bitmap_size)
    };
    let length_usize = usize::try_from(length).map_err(|_| MalformedError::IntegerOverflow)?;
    let end = byte_offset
        .checked_add(length_usize)
        .ok_or(MalformedError::IntegerOverflow)?;
    let remaining = if byte_offset <= reader.bytes.len() {
        reader.bytes.len() - byte_offset
    } else {
        0
    };
    let source = reader
        .bytes
        .get(byte_offset..end)
        .ok_or(MalformedError::UnexpectedEof {
            offset: u64::try_from(byte_offset).map_err(|_| MalformedError::IntegerOverflow)?,
            needed: length,
            remaining: u64::try_from(remaining).map_err(|_| MalformedError::IntegerOverflow)?,
        })?;
    let bitmap = if bitmap_size == 0 {
        bitmap_from_packed_bytes(width, height, source, limits)?
    } else {
        decode_mmr(source, width, height, limits, EofbPolicy::Optional)?
    };
    reader.bit_position = end.checked_mul(8).ok_or(MalformedError::IntegerOverflow)?;
    Ok(bitmap)
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

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        symbol_text_spec::codebook_valid(imported),
        header.adaptive_count <= 4,
        total_symbols as nat
            == symbol_text_spec::codebook_len(imported) + header.new_symbols as nat,
    ensures match out {
        Ok((exported, _, _, consumed)) =>
            symbol_text_spec::bitmaps_valid(exported@)
                && consumed == body@.len(),
        Err(_) => true,
    },
))]
fn decode_symbol_dictionary_arithmetic(
    header: &SymbolDictionaryHeader,
    imported: SymbolCodebook<'_>,
    body: &[u8],
    code_length: u8,
    total_symbols: u32,
    refinement_adaptive: [(i8, i8); 2],
    used_contexts: Option<&DictionaryContexts>,
    limits: &Limits,
) -> ArithmeticDictionaryResult {
    let config = header.config;
    let normalized = normalized_bounded_arithmetic(body, limits)?;
    let refinement_template = config
        .refinement_aggregation
        .then(|| refinement_template_from_u8(config.refinement_template));
    let mut codec = SymbolCodec::Arithmetic(new_arithmetic_codec(
        normalized.bytes(),
        code_length,
        refinement_template,
        limits,
    )?);
    let mut resources = ResourceTracker {
        work_units: 0,
        decoded_bitmap_bytes: 0,
    };

    let mut generic_contexts = if config.refinement_aggregation {
        None
    } else {
        let mut contexts =
            GenericContexts::new(generic_template_from_u8(config.template), false, limits)?;
        if let Some(used) = used_contexts.and_then(|used| used.generic.as_ref()) {
            contexts.restore(used, limits)?;
        }
        Some(contexts)
    };
    let mut generic_work = 0_u64;

    if config.refinement_aggregation {
        if let Some(used) = used_contexts.and_then(|used| used.refinement.as_ref()) {
            let SymbolCodec::Arithmetic(arithmetic) = &mut codec else {
                return Err(SymbolTextError::InvalidState(
                    "arithmetic dictionary has non-arithmetic state",
                ));
            };
            let contexts = arithmetic
                .refinement
                .as_mut()
                .ok_or(SymbolTextError::InvalidState(
                    "missing arithmetic refinement contexts",
                ))?;
            contexts.restore(used, limits)?;
        }
    }

    let new_symbol_capacity =
        usize::try_from(header.new_symbols).map_err(|_| MalformedError::IntegerOverflow)?;
    let mut new_symbols: Vec<Bitmap> = Vec::new();
    new_symbols
        .try_reserve(new_symbol_capacity)
        .map_err(|_| DecodeError::Allocation {
            requested_bytes: u64::from(header.new_symbols),
        })?;

    let mut class_state = DictionaryClassState::new(header.new_symbols, limits)?;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            new_symbols@.len() <= header.new_symbols as nat,
            new_symbol_capacity == header.new_symbols as nat,
            class_state.decoded_symbols as nat == new_symbols@.len(),
            class_state.declared_new_symbols == header.new_symbols,
            !class_state.class_open,
            header.adaptive_count <= 4,
            symbol_text_spec::dictionary_state_valid(&class_state),
            symbol_text_spec::codebook_valid(imported),
            symbol_text_spec::bitmaps_valid(new_symbols@),
            symbol_text_spec::codec_valid(&codec),
            symbol_text_spec::work_units(&resources) <= limits.max_work_units,
        decreases limits.max_work_units - symbol_text_spec::work_units(&resources),
    ))]
    while u32::try_from(new_symbols.len()).map_err(|_| MalformedError::IntegerOverflow)?
        < header.new_symbols
    {
        #[cfg_attr(not(verus_keep_ghost), allow(unused_variables))]
        let class_start_work = resources.work_units;
        let height_delta = required_integer(&mut codec, IntegerField::HeightDelta)?;
        class_state.begin_height_class(height_delta, limits, &mut resources)?;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                new_symbols@.len() <= header.new_symbols as nat,
                new_symbol_capacity == header.new_symbols as nat,
                class_state.decoded_symbols as nat == new_symbols@.len(),
                class_state.declared_new_symbols == header.new_symbols,
                class_state.class_open,
                header.adaptive_count <= 4,
                symbol_text_spec::dictionary_state_valid(&class_state),
                symbol_text_spec::codebook_valid(imported),
                symbol_text_spec::bitmaps_valid(new_symbols@),
                symbol_text_spec::codec_valid(&codec),
                symbol_text_spec::work_units(&resources)
                    <= limits.max_work_units,
                resources.work_units > class_start_work,
            decreases limits.max_work_units
                - symbol_text_spec::work_units(&resources),
        ))]
        while let EntropyInteger::Value(width_delta) =
            codec.decode_integer(IntegerField::WidthDelta)?
        {
            let (width, height) = class_state.add_symbol(width_delta, limits, &mut resources)?;
            let aggregation_instances = if config.refinement_aggregation {
                let instances = required_integer(&mut codec, IntegerField::AggregationInstances)?;
                Some(
                    u32::try_from(instances).map_err(|_| {
                        SymbolTextError::InvalidEntropyValue("aggregation instances")
                    })?,
                )
            } else {
                None
            };
            let bitmap = decode_one_new_symbol(
                header,
                &mut codec,
                imported,
                &new_symbols,
                generic_contexts.as_mut(),
                &mut generic_work,
                aggregation_instances,
                width,
                height,
                code_length,
                refinement_adaptive,
                limits,
                &mut resources,
            )?;
            if new_symbols.len() >= new_symbol_capacity {
                return Err(SymbolTextError::InvalidCount("too many new symbols"));
            }
            new_symbols.push(bitmap);
        }
        class_state.finish_height_class()?;
    }
    class_state.finish()?;

    let exported = decode_export_runs(
        &mut codec,
        total_symbols,
        header.exported_symbols,
        limits,
        &mut resources,
    )?;

    let SymbolCodec::Arithmetic(arithmetic) = &codec else {
        return Err(SymbolTextError::InvalidState(
            "arithmetic dictionary has non-arithmetic state",
        ));
    };
    validate_bounded_arithmetic_marker(&arithmetic.decoder, normalized.len())?;

    let retained_generic = match generic_contexts.as_ref() {
        Some(contexts) => Some(contexts.try_snapshot(limits)?),
        None => None,
    };
    let retained_refinement = match arithmetic.refinement.as_ref() {
        Some(contexts) => Some(contexts.try_snapshot(limits)?),
        None => None,
    };

    let exported_symbols = assemble_exported_symbols(imported, &new_symbols, &exported, limits)?;
    Ok((
        exported_symbols,
        retained_generic,
        retained_refinement,
        body.len(),
    ))
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        symbol_text_spec::codebook_valid(imported),
        header.adaptive_count <= 4,
        total_symbols as nat
            == symbol_text_spec::codebook_len(imported) + header.new_symbols as nat,
    ensures match out {
        Ok((exported, _, consumed)) =>
            symbol_text_spec::bitmaps_valid(exported@)
                && consumed == body@.len(),
        Err(_) => true,
    },
))]
fn decode_symbol_dictionary_huffman(
    header: &SymbolDictionaryHeader,
    imported: SymbolCodebook<'_>,
    body: &[u8],
    code_length: u8,
    total_symbols: u32,
    refinement_adaptive: [(i8, i8); 2],
    custom_tables: &[&HuffmanTable],
    limits: &Limits,
) -> Result<(Vec<Bitmap>, Option<RefinementContextSnapshot>, usize), SymbolTextError> {
    let config = header.config;
    let custom_wrapped = wrap_custom_tables(custom_tables)?;
    let bindings = associate_dictionary_tables(&config, &custom_wrapped)?;
    let choices = select_dictionary_tables(&config, &bindings)?;
    let tables = materialize_huffman_tables!(HuffmanChoiceSet::Dictionary(choices), limits)?;
    let refinement_template = config
        .refinement_aggregation
        .then(|| refinement_template_from_u8(config.refinement_template));
    let mut codec = SymbolCodec::Huffman(new_huffman_codec(
        body,
        tables,
        0,
        refinement_template,
        limits,
    )?);
    let mut resources = ResourceTracker {
        work_units: 0,
        decoded_bitmap_bytes: 0,
    };

    let new_symbol_capacity =
        usize::try_from(header.new_symbols).map_err(|_| MalformedError::IntegerOverflow)?;
    let mut new_symbols: Vec<Bitmap> = Vec::new();
    new_symbols
        .try_reserve(new_symbol_capacity)
        .map_err(|_| DecodeError::Allocation {
            requested_bytes: u64::from(header.new_symbols),
        })?;

    let mut class_state = DictionaryClassState::new(header.new_symbols, limits)?;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            new_symbols@.len() <= header.new_symbols as nat,
            new_symbol_capacity == header.new_symbols as nat,
            class_state.decoded_symbols as nat == new_symbols@.len(),
            class_state.declared_new_symbols == header.new_symbols,
            !class_state.class_open,
            header.adaptive_count <= 4,
            symbol_text_spec::dictionary_state_valid(&class_state),
            symbol_text_spec::codebook_valid(imported),
            symbol_text_spec::bitmaps_valid(new_symbols@),
            symbol_text_spec::codec_valid(&codec),
            symbol_text_spec::work_units(&resources) <= limits.max_work_units,
        decreases limits.max_work_units - symbol_text_spec::work_units(&resources),
    ))]
    while u32::try_from(new_symbols.len()).map_err(|_| MalformedError::IntegerOverflow)?
        < header.new_symbols
    {
        #[cfg_attr(not(verus_keep_ghost), allow(unused_variables))]
        let class_start_work = resources.work_units;
        let height_delta = required_integer(&mut codec, IntegerField::HeightDelta)?;
        let height = class_state.begin_height_class(height_delta, limits, &mut resources)?;

        if config.refinement_aggregation {
            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant
                    new_symbols@.len() <= header.new_symbols as nat,
                    new_symbol_capacity == header.new_symbols as nat,
                    class_state.decoded_symbols as nat == new_symbols@.len(),
                    class_state.declared_new_symbols == header.new_symbols,
                    class_state.class_open,
                    header.adaptive_count <= 4,
                    symbol_text_spec::dictionary_state_valid(&class_state),
                    symbol_text_spec::codebook_valid(imported),
                    symbol_text_spec::bitmaps_valid(new_symbols@),
                    symbol_text_spec::codec_valid(&codec),
                    symbol_text_spec::work_units(&resources)
                        <= limits.max_work_units,
                    resources.work_units > class_start_work,
                decreases limits.max_work_units
                    - symbol_text_spec::work_units(&resources),
            ))]
            while let EntropyInteger::Value(width_delta) =
                codec.decode_integer(IntegerField::WidthDelta)?
            {
                let (width, height) =
                    class_state.add_symbol(width_delta, limits, &mut resources)?;
                let instances = required_integer(&mut codec, IntegerField::AggregationInstances)?;
                let instances = u32::try_from(instances)
                    .map_err(|_| SymbolTextError::InvalidEntropyValue("aggregation instances"))?;
                if instances > 1 {
                    let SymbolCodec::Huffman(huffman) = &mut codec else {
                        return Err(SymbolTextError::InvalidState(
                            "Huffman dictionary has non-Huffman state",
                        ));
                    };
                    prepare_symbol_ids!(huffman, total_symbols, limits)?;
                    #[cfg(verus_keep_ghost)]
                    proof! {
                        reveal(symbol_text_spec::codec_valid);
                    }
                }
                let bitmap = decode_one_new_symbol(
                    header,
                    &mut codec,
                    imported,
                    &new_symbols,
                    None,
                    &mut 0,
                    Some(instances),
                    width,
                    height,
                    code_length,
                    refinement_adaptive,
                    limits,
                    &mut resources,
                )?;
                if new_symbols.len() >= new_symbol_capacity {
                    return Err(SymbolTextError::InvalidCount("too many new symbols"));
                }
                new_symbols.push(bitmap);
            }
        } else {
            // T.88 6.5.9: collect widths for one height class, then decode a
            // single collective bitmap and split it by recorded width.
            let mut widths: Vec<u32> = Vec::new();
            let decoded_count =
                u32::try_from(new_symbols.len()).map_err(|_| MalformedError::IntegerOverflow)?;
            let remaining = header
                .new_symbols
                .checked_sub(decoded_count)
                .and_then(|count| usize::try_from(count).ok())
                .ok_or(MalformedError::IntegerOverflow)?;
            let requested_width_bytes = remaining
                .checked_mul(core::mem::size_of::<u32>())
                .and_then(|bytes| u64::try_from(bytes).ok())
                .ok_or(MalformedError::IntegerOverflow)?;
            widths
                .try_reserve(remaining)
                .map_err(|_| DecodeError::Allocation {
                    requested_bytes: requested_width_bytes,
                })?;
            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant
                    widths@.len() <= remaining,
                    new_symbols@.len() + widths@.len()
                        == class_state.decoded_symbols as nat,
                    class_state.declared_new_symbols == header.new_symbols,
                    class_state.decoded_symbols <= class_state.declared_new_symbols,
                    class_state.class_open,
                    symbol_text_spec::dictionary_state_valid(&class_state),
                    symbol_text_spec::work_units(&resources)
                        <= limits.max_work_units,
                    resources.work_units > class_start_work,
                    symbol_text_spec::codec_valid(&codec),
                decreases limits.max_work_units
                    - symbol_text_spec::work_units(&resources),
            ))]
            while let EntropyInteger::Value(width_delta) =
                codec.decode_integer(IntegerField::WidthDelta)?
            {
                let (width, _) = class_state.add_symbol(width_delta, limits, &mut resources)?;
                if widths.len() >= remaining {
                    return Err(SymbolTextError::InvalidCount("too many new symbols"));
                }
                widths.push(width);
            }
            let mut total_width = 0_u32;
            let mut width_index = 0_usize;
            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant width_index <= widths@.len(),
                decreases widths@.len() - width_index,
            ))]
            while width_index < widths.len() {
                total_width = total_width
                    .checked_add(widths[width_index])
                    .ok_or(MalformedError::IntegerOverflow)?;
                width_index += 1;
            }
            let bitmap_size = match codec.decode_integer(IntegerField::DictionaryBitmapSize)? {
                EntropyInteger::Value(value) => u32::try_from(value)
                    .map_err(|_| SymbolTextError::InvalidEntropyValue("bitmap size"))?,
                EntropyInteger::OutOfBand => {
                    return Err(SymbolTextError::UnexpectedOutOfBand(
                        "collective bitmap size",
                    ))
                }
            };
            let SymbolCodec::Huffman(huffman) = &mut codec else {
                return Err(SymbolTextError::InvalidState(
                    "Huffman dictionary has non-Huffman state",
                ));
            };
            let collective = decode_collective_bitmap(
                &mut huffman.reader,
                bitmap_size,
                total_width,
                height,
                limits,
            )?;
            #[cfg(verus_keep_ghost)]
            proof! {
                reveal(symbol_text_spec::codec_valid);
            }
            resources.charge_bitmap(total_width, height, limits)?;

            if widths.len() == 1 {
                if new_symbols.len() >= new_symbol_capacity {
                    return Err(SymbolTextError::InvalidCount("too many new symbols"));
                }
                new_symbols.push(collective);
            } else {
                let mut x_start = 0_u32;
                #[cfg_attr(not(verus_keep_ghost), allow(unused_variables))]
                let symbols_before = new_symbols.len();
                let mut crop_index = 0_usize;
                #[cfg_attr(verus_keep_ghost, verus_spec(
                    invariant
                        crop_index <= widths@.len(),
                        new_symbols@.len() == symbols_before + crop_index,
                        symbol_text_spec::bitmaps_valid(new_symbols@),
                        crate::spec::bitmap::storage_valid(&collective),
                    decreases widths@.len() - crop_index,
                ))]
                while crop_index < widths.len() {
                    let width = widths[crop_index];
                    let cropped = crop_bitmap_columns(&collective, x_start, width, limits)?;
                    if new_symbols.len() >= new_symbol_capacity {
                        return Err(SymbolTextError::InvalidCount("too many new symbols"));
                    }
                    new_symbols.push(cropped);
                    x_start = x_start
                        .checked_add(width)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    crop_index += 1;
                }
            }
        }
        class_state.finish_height_class()?;
    }
    class_state.finish()?;

    let exported = decode_export_runs(
        &mut codec,
        total_symbols,
        header.exported_symbols,
        limits,
        &mut resources,
    )?;

    let consumed_within_body = codec
        .huffman_mut()?
        .reader
        .bit_position
        .checked_add(7)
        .ok_or(MalformedError::IntegerOverflow)?
        / 8;
    if consumed_within_body != body.len() {
        return Err(SymbolTextError::InvalidState(
            "Huffman symbol dictionary did not exactly consume its segment data",
        ));
    }

    let SymbolCodec::Huffman(huffman) = &codec else {
        return Err(SymbolTextError::InvalidState(
            "Huffman dictionary has non-Huffman state",
        ));
    };
    let retained_refinement = match huffman.refinement.as_ref() {
        Some(contexts) => Some(contexts.try_snapshot(limits)?),
        None => None,
    };
    let exported_symbols = assemble_exported_symbols(imported, &new_symbols, &exported, limits)?;
    Ok((exported_symbols, retained_refinement, consumed_within_body))
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        symbol_text_spec::codebook_valid(imported),
        symbol_text_spec::bitmaps_valid(new_symbols@),
        forall|index: int| 0 <= index < exported_indices@.len()
            ==> (#[trigger] exported_indices@[index] as nat)
                < symbol_text_spec::codebook_len(imported) + (new_symbols@).len(),
    ensures match out {
        Ok(exported) =>
            exported@.len() == exported_indices@.len()
                && symbol_text_spec::bitmaps_valid(exported@),
        Err(_) => true,
    },
))]
fn assemble_exported_symbols(
    imported: SymbolCodebook<'_>,
    new_symbols: &[Bitmap],
    exported_indices: &[u32],
    limits: &Limits,
) -> Result<Vec<Bitmap>, SymbolTextError> {
    let mut exported_symbols: Vec<Bitmap> = Vec::new();
    let requested_bytes = exported_indices
        .len()
        .checked_mul(core::mem::size_of::<Bitmap>())
        .and_then(|bytes| u64::try_from(bytes).ok())
        .ok_or(MalformedError::IntegerOverflow)?;
    exported_symbols
        .try_reserve(exported_indices.len())
        .map_err(|_| DecodeError::Allocation { requested_bytes })?;
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index <= exported_indices@.len(),
            exported_symbols@.len() == index,
            symbol_text_spec::codebook_valid(imported),
            symbol_text_spec::bitmaps_valid(new_symbols@),
            symbol_text_spec::bitmaps_valid(exported_symbols@),
            forall|offset: int| 0 <= offset < exported_indices@.len()
                ==> (#[trigger] exported_indices@[offset] as nat)
                    < symbol_text_spec::codebook_len(imported) + (new_symbols@).len(),
        decreases exported_indices@.len() - index,
    ))]
    while index < exported_indices.len() {
        let symbol_id = exported_indices[index];
        let source = symbol_by_id(imported, new_symbols, symbol_id)?;
        exported_symbols.push(clone_bitmap(source, limits)?);
        index += 1;
    }
    Ok(exported_symbols)
}

/// Decodes one complete symbol-dictionary segment payload (T.88 6.5, 7.4.3).
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires symbol_text_spec::codebook_valid(input.imported_symbols),
    ensures match out {
        Ok(output) =>
            symbol_text_spec::bitmaps_valid(output.exported_symbols@)
                && output.consumed_bytes <= input.payload@.len(),
        Err(_) => true,
    },
))]
pub(crate) fn decode_symbol_dictionary(
    input: &SymbolDictionaryInput<'_>,
) -> Result<SymbolDictionaryOutput, SymbolTextError> {
    let limits = input.limits;
    let (header, header_len) = parse_symbol_dictionary_header(input.payload, limits)?;
    let config = header.config;

    let imported_count =
        u32::try_from(input.imported_symbols.len()).map_err(|_| MalformedError::IntegerOverflow)?;
    let total_symbols = check_symbol_counts(
        imported_count,
        header.new_symbols,
        header.exported_symbols,
        limits,
    )?;
    let code_length = symbol_id_code_length(total_symbols)?;

    let uses_generic_contexts = !config.huffman && !config.refinement_aggregation;
    let uses_refinement_contexts = config.refinement_aggregation;
    if config.bitmap_context_used {
        let used = input
            .used_contexts
            .ok_or(SymbolTextError::InvalidState("missing retained contexts"))?;
        if uses_generic_contexts && used.generic.is_none() {
            return Err(SymbolTextError::InvalidState(
                "missing retained generic contexts",
            ));
        }
        if uses_refinement_contexts && used.refinement.is_none() {
            return Err(SymbolTextError::InvalidState(
                "missing retained refinement contexts",
            ));
        }
    } else if input.used_contexts.is_some() {
        return Err(SymbolTextError::InvalidState(
            "unexpected retained contexts without the context-used flag",
        ));
    }

    let body = input
        .payload
        .get(header_len..)
        .ok_or(MalformedError::IntegerOverflow)?;

    let refinement_adaptive: [(i8, i8); 2] = [
        (
            i8::try_from(header.refinement_adaptive[0].x)
                .map_err(|_| MalformedError::IntegerOverflow)?,
            i8::try_from(header.refinement_adaptive[0].y)
                .map_err(|_| MalformedError::IntegerOverflow)?,
        ),
        (
            i8::try_from(header.refinement_adaptive[1].x)
                .map_err(|_| MalformedError::IntegerOverflow)?,
            i8::try_from(header.refinement_adaptive[1].y)
                .map_err(|_| MalformedError::IntegerOverflow)?,
        ),
    ];

    let (exported_symbols, retained_generic, retained_refinement, consumed_within_body) =
        if config.huffman {
            let (exported, retained_refinement, consumed) = decode_symbol_dictionary_huffman(
                &header,
                input.imported_symbols,
                body,
                code_length,
                total_symbols,
                refinement_adaptive,
                input.custom_tables,
                limits,
            )?;
            (exported, None, retained_refinement, consumed)
        } else {
            if !input.custom_tables.is_empty() {
                return Err(SymbolTextError::InvalidTableAssociation(
                    "custom tables on an arithmetic symbol dictionary",
                ));
            }
            let (exported, retained_generic, retained_refinement, consumed) =
                decode_symbol_dictionary_arithmetic(
                    &header,
                    input.imported_symbols,
                    body,
                    code_length,
                    total_symbols,
                    refinement_adaptive,
                    input.used_contexts,
                    limits,
                )?;
            (exported, retained_generic, retained_refinement, consumed)
        };

    let retained_contexts = if config.bitmap_context_retained {
        Some(DictionaryContexts {
            generic: retained_generic,
            refinement: retained_refinement,
        })
    } else {
        None
    };

    let consumed_bytes = header_len
        .checked_add(consumed_within_body)
        .ok_or(MalformedError::IntegerOverflow)?;
    Ok(SymbolDictionaryOutput {
        exported_symbols,
        retained_contexts,
        consumed_bytes,
    })
}

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

/// Decodes one complete text-region segment payload (T.88 6.4, 7.4.3).
#[allow(clippy::too_many_lines)]
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires symbol_text_spec::codebook_valid(input.symbols),
    ensures match out {
        Ok(output) =>
            crate::spec::bitmap::storage_valid(&output.bitmap)
                && output.consumed_bytes <= input.payload@.len(),
        Err(_) => true,
    },
))]
pub(crate) fn decode_text_region(
    input: &TextRegionInput<'_>,
) -> Result<TextRegionOutput, SymbolTextError> {
    let limits = input.limits;
    let (header, header_len) = parse_text_region_header(input.payload, limits)?;
    let config = header.config;
    let body = input
        .payload
        .get(header_len..)
        .ok_or(MalformedError::IntegerOverflow)?;

    let symbol_count =
        u32::try_from(input.symbols.len()).map_err(|_| MalformedError::IntegerOverflow)?;
    let code_length = symbol_id_code_length(symbol_count)?;

    let refinement_adaptive: [(i8, i8); 2] = [
        (
            i8::try_from(header.refinement_adaptive[0].x)
                .map_err(|_| MalformedError::IntegerOverflow)?,
            i8::try_from(header.refinement_adaptive[0].y)
                .map_err(|_| MalformedError::IntegerOverflow)?,
        ),
        (
            i8::try_from(header.refinement_adaptive[1].x)
                .map_err(|_| MalformedError::IntegerOverflow)?,
            i8::try_from(header.refinement_adaptive[1].y)
                .map_err(|_| MalformedError::IntegerOverflow)?,
        ),
    ];
    let mut resources = ResourceTracker {
        work_units: 0,
        decoded_bitmap_bytes: 0,
    };
    let refinement_template = refinement_template_from_u8(config.refinement_template);

    let (bitmap, consumed_within_body) = if config.huffman {
        let custom_wrapped = wrap_custom_tables(input.custom_tables)?;
        let bindings = associate_text_tables(&config, &custom_wrapped)?;
        let choices = select_text_tables(&bindings, config.refinement)?;
        let tables = materialize_huffman_tables!(HuffmanChoiceSet::TextRegion(choices), limits)?;
        let log_strip_size = log2_strip_size(config.strip_size);
        let mut codec = SymbolCodec::Huffman(new_huffman_codec(
            body,
            tables,
            log_strip_size,
            if config.refinement {
                Some(refinement_template)
            } else {
                None
            },
            limits,
        )?);
        let SymbolCodec::Huffman(huffman) = &mut codec else {
            return Err(SymbolTextError::InvalidState(
                "Huffman text region has non-Huffman state",
            ));
        };
        prepare_symbol_ids!(huffman, symbol_count, limits)?;
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal(symbol_text_spec::codec_valid);
        }

        let bitmap = decode_text_region_body(
            &mut codec,
            input.symbols,
            code_length,
            header.region.width,
            header.region.height,
            header.number_of_instances,
            &config,
            refinement_adaptive,
            limits,
            &mut resources,
        )?;

        let consumed_within_body = codec
            .huffman_mut()?
            .reader
            .bit_position
            .checked_add(7)
            .ok_or(MalformedError::IntegerOverflow)?
            / 8;
        if consumed_within_body != body.len() {
            return Err(SymbolTextError::InvalidState(
                "Huffman text region did not exactly consume its segment data",
            ));
        }
        (bitmap, consumed_within_body)
    } else {
        if !input.custom_tables.is_empty() {
            return Err(SymbolTextError::InvalidTableAssociation(
                "custom tables on an arithmetic text region",
            ));
        }
        let normalized = normalized_bounded_arithmetic(body, limits)?;
        let mut codec = SymbolCodec::Arithmetic(new_arithmetic_codec(
            normalized.bytes(),
            code_length,
            if config.refinement {
                Some(refinement_template)
            } else {
                None
            },
            limits,
        )?);

        let bitmap = decode_text_region_body(
            &mut codec,
            input.symbols,
            code_length,
            header.region.width,
            header.region.height,
            header.number_of_instances,
            &config,
            refinement_adaptive,
            limits,
            &mut resources,
        )?;

        let SymbolCodec::Arithmetic(arithmetic) = &codec else {
            return Err(SymbolTextError::InvalidState(
                "arithmetic text region has non-arithmetic state",
            ));
        };
        validate_bounded_arithmetic_marker(&arithmetic.decoder, normalized.len())?;
        (bitmap, body.len())
    };

    let consumed_bytes = header_len
        .checked_add(consumed_within_body)
        .ok_or(MalformedError::IntegerOverflow)?;
    Ok(TextRegionOutput {
        region: header.region,
        bitmap,
        consumed_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::hint::black_box;
    use std::time::{Duration, Instant};

    fn arithmetic_codec_fixture(
        bytes: &[u8],
        iaid_count: usize,
        salt: usize,
    ) -> ArithmeticCodec<'_> {
        ArithmeticCodec {
            decoder: ArithmeticDecoder::new(bytes).expect("arithmetic constructor"),
            integers: IntegerContexts {
                slots: std::array::from_fn(|slot| {
                    std::array::from_fn(|index| {
                        ArithmeticContext::from_state(
                            u8::try_from((index * 17 + slot + salt) % 47).unwrap(),
                            u8::try_from((index / 2 + slot + salt) % 2).unwrap(),
                        )
                        .unwrap()
                    })
                }),
            },
            iaid: (0..iaid_count)
                .map(|index| {
                    ArithmeticContext::from_state(
                        u8::try_from((index * 17 + salt) % 47).unwrap(),
                        u8::try_from((index / 2 + salt) % 2).unwrap(),
                    )
                    .unwrap()
                })
                .collect(),
            refinement: None,
            refinement_work: 73,
        }
    }

    fn assert_arithmetic_forwarding<T: std::fmt::Debug + PartialEq>(
        actual: Result<T, SymbolTextError>,
        expected: Result<T, SymbolTextError>,
    ) -> bool {
        match (actual, expected) {
            (Ok(actual), Ok(expected)) => {
                assert_eq!(actual, expected);
                true
            }
            (
                Err(SymbolTextError::Decode(DecodeError::Malformed(actual))),
                Err(SymbolTextError::Decode(DecodeError::Malformed(expected))),
            ) => {
                assert_eq!(actual, expected);
                false
            }
            (actual, expected) => {
                panic!("wrong arithmetic forwarding: {actual:?}, expected {expected:?}")
            }
        }
    }

    fn assert_arithmetic_cursor_and_continuation(
        actual: &mut ArithmeticDecoder<'_>,
        expected: &mut ArithmeticDecoder<'_>,
        success: bool,
    ) {
        assert_eq!(actual.byte_position(), expected.byte_position());
        assert_eq!(actual.marker(), expected.marker());
        if success {
            let mut actual_context = ArithmeticContext::new();
            let mut expected_context = ArithmeticContext::new();
            for _ in 0..64 {
                let signal = actual.decode_signal(&mut actual_context);
                assert_eq!(signal, expected.decode_signal(&mut expected_context));
                assert_eq!(actual_context, expected_context);
                assert_eq!(actual.byte_position(), expected.byte_position());
                assert_eq!(actual.marker(), expected.marker());
                if signal > 1 {
                    break;
                }
            }
        }
    }

    #[test]
    fn arithmetic_integer_caller_matches_direct_procedure_and_preserves_other_banks() {
        let fields = [
            IntegerField::AggregationInstances,
            IntegerField::HeightDelta,
            IntegerField::WidthDelta,
            IntegerField::ExportRun,
            IntegerField::StripDelta,
            IntegerField::FirstS,
            IntegerField::SubsequentS,
            IntegerField::WithinStripT,
            IntegerField::RefinementWidth,
            IntegerField::RefinementHeight,
            IntegerField::RefinementX,
            IntegerField::RefinementY,
            IntegerField::RefinementSize,
            IntegerField::RefinementIndicator,
            IntegerField::DictionaryBitmapSize,
        ];
        let samples: [&[u8]; 6] = [
            &[0, 0],
            &[0xff, 0x90],
            &[0; 64],
            &[0x13; 64],
            &[0xaa; 64],
            &[0xff, 0x8f, 0, 0],
        ];
        let mut outcomes = [0; 2];
        for (slot, field) in fields.into_iter().enumerate() {
            assert_eq!(field.slot(), slot);
            for bytes in samples {
                for salt in [0, 1, 23, 46] {
                    let mut actual = arithmetic_codec_fixture(bytes, 7, salt);
                    let before = actual.integers.slots;
                    let before_iaid = actual.iaid.clone();
                    let mut expected_bank = before[slot];
                    let mut expected_decoder = ArithmeticDecoder::new(bytes).unwrap();
                    let expected = expected_decoder
                        .decode_integer(&mut expected_bank)
                        .map(|value| match value {
                            ArithmeticInteger::Value(value) => EntropyInteger::Value(value),
                            ArithmeticInteger::OutOfBand => EntropyInteger::OutOfBand,
                        })
                        .map_err(SymbolTextError::from);
                    let success =
                        assert_arithmetic_forwarding(actual.decode_integer(field), expected);
                    outcomes[usize::from(success)] += 1;
                    assert_eq!(actual.integers.slots[slot], expected_bank);
                    for (index, bank) in before.iter().enumerate() {
                        if index != slot {
                            assert_eq!(actual.integers.slots[index], *bank);
                        }
                    }
                    assert_eq!(actual.iaid, before_iaid);
                    assert!(actual.refinement.is_none());
                    assert_eq!(actual.refinement_work, 73);
                    assert_arithmetic_cursor_and_continuation(
                        &mut actual.decoder,
                        &mut expected_decoder,
                        success,
                    );
                }
            }
        }
        assert_eq!(outcomes.iter().sum::<usize>(), 360);
        assert!(outcomes.into_iter().all(|count| count > 0));
    }

    #[test]
    fn arithmetic_iaid_caller_forwards_every_length_and_preserves_integer_banks() {
        let samples: [&[u8]; 6] = [
            &[0, 0],
            &[0xff, 0x90],
            &[0; 64],
            &[0x13; 64],
            &[0xaa; 64],
            &[0xff, 0x8f, 0, 0],
        ];
        let mut outcomes = [0; 2];
        for length in 0..=u8::MAX {
            for count in [0, 1, 2, 3, 4, 16, 32, 35] {
                for bytes in samples {
                    let mut actual =
                        arithmetic_codec_fixture(bytes, count, usize::from(length) % 47);
                    let before = actual.integers.slots;
                    let mut expected_bank = actual.iaid.clone();
                    let mut expected_decoder = ArithmeticDecoder::new(bytes).unwrap();
                    let expected = expected_decoder
                        .decode_iaid(length, &mut expected_bank)
                        .map_err(SymbolTextError::from);
                    let request = SymbolIdRequest {
                        available_symbols: 0,
                        code_length: length,
                        huffman: false,
                    };
                    let success =
                        assert_arithmetic_forwarding(actual.decode_symbol_id(request), expected);
                    outcomes[usize::from(success)] += 1;
                    assert_eq!(actual.iaid, expected_bank);
                    assert_eq!(actual.integers.slots, before);
                    assert!(actual.refinement.is_none());
                    assert_eq!(actual.refinement_work, 73);
                    assert_arithmetic_cursor_and_continuation(
                        &mut actual.decoder,
                        &mut expected_decoder,
                        success,
                    );
                }
            }
        }
        assert_eq!(outcomes.iter().sum::<usize>(), 12_288);
        assert!(outcomes.into_iter().all(|count| count > 0));
    }

    #[test]
    fn symbol_code_length_is_minimal_at_every_u32_power_boundary() {
        let boundaries = (0..u32::BITS).flat_map(|shift| {
            let power = 1_u32 << shift;
            [power - 1, power, power + 1]
        });
        for count in (0..=65_536).chain(boundaries).chain([u32::MAX]) {
            let length = symbol_id_code_length(count).expect("bounded code length");
            let expected = u8::try_from(u32::BITS - count.saturating_sub(1).leading_zeros())
                .expect("at most 32 bits");
            assert_eq!(length, expected, "symbol count {count}");
            assert!((1_u64 << length) >= u64::from(count.max(1)));
            if length != 0 {
                assert!((1_u64 << (length - 1)) < u64::from(count));
            }
        }
    }

    #[test]
    fn packed_symbol_storage_matches_pixels_and_rejects_each_dirty_row() {
        let limits = Limits::default();
        for width in 0_u32..=33 {
            for height in [0_u32, 1, 3] {
                let stride = usize::try_from(width.div_ceil(8)).unwrap();
                let length = stride * usize::try_from(height).unwrap();
                for seed in [0_usize, 0x55, 0xaa, 0xff] {
                    let mut bytes: Vec<_> = (0..length)
                        .map(|index| u8::try_from((index * 73 + seed) % 256).unwrap())
                        .collect();
                    if width % 8 != 0 {
                        for row in bytes.chunks_exact_mut(stride) {
                            row[stride - 1] &= 0xff << (8 - width % 8);
                        }
                    }
                    let expected = bytes.clone();
                    bytes.extend_from_slice(&[0xff, 0x93]);
                    let input = bytes.clone();
                    let bitmap = bitmap_from_packed_bytes(width, height, &bytes, &limits)
                        .expect("packed storage");
                    assert_eq!(
                        (bitmap.width, bitmap.height, bitmap.stride),
                        (width, height, stride)
                    );
                    assert_eq!(bitmap.data, expected);
                    assert_eq!(bytes, input);
                    for y in 0..height {
                        for x in 0..width {
                            let index = usize::try_from(y).unwrap() * stride
                                + usize::try_from(x / 8).unwrap();
                            assert_eq!(
                                bitmap.pixel(x, y),
                                Some(expected[index] & (0x80 >> (x % 8)) != 0),
                            );
                        }
                    }
                    for cut in 0..length {
                        assert!(matches!(
                            bitmap_from_packed_bytes(width, height, &bytes[..cut], &limits),
                            Err(SymbolTextError::Decode(DecodeError::Malformed(
                                MalformedError::UnexpectedEof {
                                    offset: 0, needed, remaining,
                                }
                            ))) if needed == length as u64 && remaining == cut as u64
                        ));
                    }
                    if width % 8 != 0 {
                        for y in 0..usize::try_from(height).unwrap() {
                            let mut dirty = expected.clone();
                            dirty[(y + 1) * stride - 1] |= 1;
                            assert!(matches!(
                                bitmap_from_packed_bytes(width, height, &dirty, &limits),
                                Err(SymbolTextError::InvalidState(
                                    "raw collective bitmap has nonzero padding"
                                ))
                            ));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn packed_symbol_limits_are_inclusive_and_precede_payload_checks() {
        let inclusive = Limits {
            max_width: 9,
            max_height: 3,
            max_pixels_per_page: 27,
            max_decoder_managed_memory_bytes: 6,
            ..Limits::default()
        };
        let bytes = [0xff, 0x80, 0x33, 0, 0, 0];
        assert_eq!(
            bitmap_from_packed_bytes(9, 3, &bytes, &inclusive)
                .expect("inclusive limits")
                .data,
            bytes,
        );
        let failures = [
            (LimitKind::Width, 8, 9),
            (LimitKind::Height, 2, 3),
            (LimitKind::PagePixels, 26, 27),
            (LimitKind::Memory, 5, 6),
        ];
        for rejected in 1_u8..16 {
            let limits = Limits {
                max_width: 9 - u32::from(rejected & 1 != 0),
                max_height: 3 - u32::from(rejected & 2 != 0),
                max_pixels_per_page: 27 - u64::from(rejected & 4 != 0),
                max_decoder_managed_memory_bytes: 6 - u64::from(rejected & 8 != 0),
                ..inclusive
            };
            let expected = failures
                .iter()
                .enumerate()
                .find(|(index, _)| rejected & (1 << index) != 0)
                .unwrap()
                .1;
            assert!(matches!(
                bitmap_from_packed_bytes(9, 3, &[], &limits),
                Err(SymbolTextError::Decode(DecodeError::LimitExceeded(
                    LimitExceeded { kind, limit, actual }
                ))) if (kind, limit, actual) == *expected
            ));
        }
        let limits = Limits {
            max_width: u32::MAX,
            max_height: u32::MAX,
            max_pixels_per_page: u64::MAX,
            max_decoder_managed_memory_bytes: 0,
            ..Limits::default()
        };
        let needed = u64::from(u32::MAX).div_ceil(8) * u64::from(u32::MAX);
        assert!(matches!(
            bitmap_from_packed_bytes(u32::MAX, u32::MAX, &[], &limits),
            Err(SymbolTextError::Decode(DecodeError::LimitExceeded(
                LimitExceeded { kind: LimitKind::Memory, limit: 0, actual }
            ))) if actual == needed
        ));
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn export_run_errors_preserve_classification_work_and_unread_input() {
        let cases = [
            (
                vec![EntropyInteger::Value(1)],
                1,
                2,
                10,
                0,
                1,
                SymbolTextError::InvalidCount("export count exceeds candidate count"),
            ),
            (
                vec![EntropyInteger::Value(-1)],
                3,
                0,
                10,
                1,
                0,
                SymbolTextError::InvalidEntropyValue("negative export run"),
            ),
            (
                vec![EntropyInteger::Value(i64::from(u32::MAX) + 1)],
                3,
                0,
                10,
                1,
                0,
                SymbolTextError::InvalidEntropyValue("negative export run"),
            ),
            (
                vec![EntropyInteger::OutOfBand],
                3,
                0,
                10,
                1,
                0,
                SymbolTextError::UnexpectedOutOfBand("export run"),
            ),
            (
                vec![EntropyInteger::Value(4)],
                3,
                0,
                10,
                1,
                0,
                SymbolTextError::InvalidCount("export run exceeds candidate count"),
            ),
            (
                vec![EntropyInteger::Value(0), EntropyInteger::Value(2)],
                3,
                1,
                10,
                2,
                0,
                SymbolTextError::InvalidCount("too many exported symbols"),
            ),
            (
                vec![EntropyInteger::Value(3)],
                3,
                1,
                10,
                1,
                0,
                SymbolTextError::InvalidCount("exported-symbol popcount mismatch"),
            ),
            (
                vec![
                    EntropyInteger::Value(1),
                    EntropyInteger::Value(i64::from(u32::MAX)),
                ],
                u32::MAX,
                0,
                10,
                2,
                0,
                SymbolTextError::Decode(DecodeError::Malformed(MalformedError::IntegerOverflow)),
            ),
            (
                vec![],
                1,
                0,
                10,
                1,
                0,
                SymbolTextError::InvalidState("missing scripted integer"),
            ),
            (
                vec![EntropyInteger::Value(1)],
                1,
                0,
                0,
                1,
                1,
                SymbolTextError::Decode(DecodeError::LimitExceeded(LimitExceeded {
                    kind: LimitKind::Work,
                    limit: 0,
                    actual: 1,
                })),
            ),
        ];
        for (integers, candidates, expected_exports, max_work_units, work, unread, expected) in
            cases
        {
            let mut codec = SymbolCodec::Scripted(ScriptedCodec {
                integers: integers.into(),
                symbol_id: 0,
            });
            let mut resources = ResourceTracker {
                work_units: 0,
                decoded_bitmap_bytes: 17,
            };
            let limits = Limits {
                max_work_units,
                ..Limits::default()
            };
            let error = decode_export_runs(
                &mut codec,
                candidates,
                expected_exports,
                &limits,
                &mut resources,
            )
            .expect_err("invalid export run");
            match (error, expected) {
                (
                    SymbolTextError::InvalidCount(actual),
                    SymbolTextError::InvalidCount(expected),
                )
                | (
                    SymbolTextError::InvalidEntropyValue(actual),
                    SymbolTextError::InvalidEntropyValue(expected),
                )
                | (
                    SymbolTextError::UnexpectedOutOfBand(actual),
                    SymbolTextError::UnexpectedOutOfBand(expected),
                )
                | (
                    SymbolTextError::InvalidState(actual),
                    SymbolTextError::InvalidState(expected),
                ) => assert_eq!(actual, expected),
                (
                    SymbolTextError::Decode(DecodeError::Malformed(actual)),
                    SymbolTextError::Decode(DecodeError::Malformed(expected)),
                ) => assert_eq!(actual, expected),
                (
                    SymbolTextError::Decode(DecodeError::LimitExceeded(actual)),
                    SymbolTextError::Decode(DecodeError::LimitExceeded(expected)),
                ) => assert_eq!(actual, expected),
                (actual, expected) => {
                    panic!("wrong export error: {actual:?}, expected {expected:?}")
                }
            }
            assert_eq!(
                resources,
                ResourceTracker {
                    work_units: work,
                    decoded_bitmap_bytes: 17
                }
            );
            let SymbolCodec::Scripted(scripted) = codec else {
                panic!("scripted codec")
            };
            assert_eq!(scripted.integers.len(), unread);
        }
    }

    #[test]
    fn export_runs_match_every_small_selection_with_exact_work_and_input_frames() {
        for count in 0_u32..=9 {
            for selected in 0_u32..(1 << count) {
                let expected: Vec<_> = (0..count)
                    .filter(|index| selected & (1 << index) != 0)
                    .collect();
                let mut integers = VecDeque::new();
                let mut flag = false;
                let mut run = 0;
                for index in 0..count {
                    let next = selected & (1 << index) != 0;
                    if next != flag {
                        integers.push_back(EntropyInteger::Value(run));
                        flag = next;
                        run = 0;
                    }
                    run += 1;
                }
                if count != 0 {
                    integers.push_back(EntropyInteger::Value(run));
                }
                let work = u64::try_from(integers.len()).unwrap() + 5;
                integers.push_back(EntropyInteger::Value(-73));
                let mut codec = SymbolCodec::Scripted(ScriptedCodec {
                    integers,
                    symbol_id: 0,
                });
                let mut resources = ResourceTracker {
                    work_units: 5,
                    decoded_bitmap_bytes: 17,
                };
                let limits = Limits {
                    max_work_units: work,
                    ..Limits::default()
                };
                let result = decode_export_runs(
                    &mut codec,
                    count,
                    selected.count_ones(),
                    &limits,
                    &mut resources,
                )
                .expect("scalar selection");
                assert_eq!(result, expected);
                assert_eq!(
                    resources,
                    ResourceTracker {
                        work_units: work,
                        decoded_bitmap_bytes: 17
                    }
                );
                let SymbolCodec::Scripted(scripted) = codec else {
                    panic!("scripted codec")
                };
                assert_eq!(
                    scripted.integers,
                    VecDeque::from([EntropyInteger::Value(-73)])
                );
            }
        }
    }

    #[test]
    fn lossless_byte_and_boolean_bridges_match_standard_conversions() {
        for value in 0..=u8::MAX {
            assert_eq!(widen_u8_i64(value), i64::from(value));
            assert_eq!(widen_u8_i32(value), i32::from(value));
        }
        for value in [false, true] {
            assert_eq!(widen_bool_i64(value), i64::from(value));
        }
    }

    #[test]
    fn full_u32_widening_matches_standard_conversion() {
        for value in [
            0,
            1,
            255,
            256,
            65_535,
            65_536,
            0x7fff_ffff,
            1_u32 << 31,
            u32::MAX - 1,
            u32::MAX,
        ] {
            assert_eq!(widen_u32_i64(value), i64::from(value));
        }
        let mut value = 0x1020_3040_u32;
        for _ in 0..4096 {
            value = value.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            assert_eq!(widen_u32_i64(value), i64::from(value));
        }
    }

    fn scalar_compose_placement_oracle(
        destination: &mut Bitmap,
        source: &Bitmap,
        placement: Placement,
        operator: CombinationOperator,
        limits: &Limits,
        resources: &mut ResourceTracker,
    ) -> Result<(), SymbolTextError> {
        let source_x = if placement.x < 0 {
            negative_magnitude(placement.x)?
        } else {
            0
        };
        let source_y = if placement.y < 0 {
            negative_magnitude(placement.y)?
        } else {
            0
        };
        let destination_x = placement.x.max(0);
        let destination_y = placement.y.max(0);
        let overlap_width = (i64::from(source.width()) - source_x)
            .min(i64::from(destination.width()) - destination_x)
            .max(0);
        let overlap_height = (i64::from(source.height()) - source_y)
            .min(i64::from(destination.height()) - destination_y)
            .max(0);
        resources.charge_work(
            u64::try_from(overlap_width)
                .map_err(|_| MalformedError::IntegerOverflow)?
                .checked_mul(
                    u64::try_from(overlap_height).map_err(|_| MalformedError::IntegerOverflow)?,
                )
                .ok_or(MalformedError::IntegerOverflow)?,
            limits,
        )?;
        for row in 0..overlap_height {
            for column in 0..overlap_width {
                let sx = u32::try_from(source_x + column)
                    .map_err(|_| MalformedError::IntegerOverflow)?;
                let sy =
                    u32::try_from(source_y + row).map_err(|_| MalformedError::IntegerOverflow)?;
                let dx = u32::try_from(destination_x + column)
                    .map_err(|_| MalformedError::IntegerOverflow)?;
                let dy = u32::try_from(destination_y + row)
                    .map_err(|_| MalformedError::IntegerOverflow)?;
                let incoming = source
                    .pixel(sx, sy)
                    .ok_or(MalformedError::IntegerOverflow)?;
                let current = destination
                    .pixel(dx, dy)
                    .ok_or(MalformedError::IntegerOverflow)?;
                destination.set_pixel(dx, dy, operator.apply(current, incoming))?;
            }
        }
        Ok(())
    }

    fn batched_compose_placement(
        destination: &mut Bitmap,
        placements: &[(&Bitmap, Placement)],
        operator: CombinationOperator,
        limits: &Limits,
        resources: &mut ResourceTracker,
    ) -> Result<(), SymbolTextError> {
        let mut batch = PlacementBatch::new();
        batch.reserve(placements.len(), limits)?;
        for &(source, placement) in placements {
            if let Some(prepared) =
                prepare_placement(destination, source, placement, limits, resources)?
            {
                if !batch.compatible(prepared) {
                    flush_placement_batch(destination, &mut batch, operator)?;
                }
                batch.push(prepared, destination)?;
            }
        }
        flush_placement_batch(destination, &mut batch, operator)
    }

    fn median_duration(mut values: Vec<Duration>) -> Duration {
        values.sort_unstable();
        values[values.len() / 2]
    }

    fn text_config(corner: ReferenceCorner, transposed: bool, strip_size: u8) -> TextRegionConfig {
        TextRegionConfig {
            huffman: false,
            refinement: false,
            strip_size,
            reference_corner: corner,
            transposed,
            operator: CombinationOperator::Or,
            default_pixel: false,
            s_offset: 0,
            refinement_template: 0,
            huffman_choices: None,
        }
    }

    #[test]
    fn validates_symbol_dictionary_flags() {
        let config = parse_symbol_dictionary_flags(1).expect("valid Huffman flags");
        assert!(config.huffman);
        assert_eq!(
            config.height_delta,
            TableChoice::Standard(StandardHuffmanTable::D)
        );
        assert!(matches!(
            parse_symbol_dictionary_flags(1 | (2 << 2)),
            Err(SymbolTextError::InvalidFlags(_))
        ));
        assert!(matches!(
            parse_symbol_dictionary_flags(0xe000),
            Err(SymbolTextError::InvalidFlags(_))
        ));
    }

    #[test]
    fn validates_text_flags_and_signed_offset() {
        let flags = 1 | (2 << 2) | (3 << 4) | (0x1d << 10);
        let config = parse_text_region_flags(flags, Some(0), false).expect("valid text flags");
        assert_eq!(config.strip_size, 4);
        assert_eq!(config.reference_corner, ReferenceCorner::TopRight);
        assert_eq!(config.s_offset, -3);
        assert!(matches!(
            parse_text_region_flags(flags, Some(0x8000), false),
            Err(SymbolTextError::InvalidFlags(_))
        ));
    }

    #[test]
    fn associates_custom_tables_in_normative_order_and_checks_oob() {
        let limits = Limits::default();
        let table =
            HuffmanTable::standard(StandardHuffmanTable::A, &limits).expect("standard table");
        let config = parse_symbol_dictionary_flags(1 | (3 << 2) | (3 << 4)).expect("valid flags");
        let supplied = [
            CustomHuffmanTable {
                table: &table,
                has_oob: false,
            },
            CustomHuffmanTable {
                table: &table,
                has_oob: true,
            },
        ];
        let bindings = associate_dictionary_tables(&config, &supplied).expect("valid associations");
        assert_eq!(
            bindings.entries[0].expect("height").parameter,
            TableParameter::DictionaryHeight
        );
        assert_eq!(
            bindings.entries[1].expect("width").parameter,
            TableParameter::DictionaryWidth
        );

        let invalid = [
            CustomHuffmanTable {
                table: &table,
                has_oob: true,
            },
            supplied[1],
        ];
        assert!(matches!(
            associate_dictionary_tables(&config, &invalid),
            Err(SymbolTextError::InvalidTableAssociation(_))
        ));
    }

    #[test]
    fn checks_cumulative_dictionary_dimensions_and_counts() {
        let limits = Limits {
            max_width: 8,
            max_height: 8,
            ..Limits::default()
        };
        let mut resources = ResourceTracker::default();
        let mut state = DictionaryClassState::new(2, &limits).expect("state");
        assert_eq!(
            state
                .begin_height_class(2, &limits, &mut resources)
                .expect("height"),
            2
        );
        assert_eq!(
            state
                .add_symbol(2, &limits, &mut resources)
                .expect("first symbol"),
            (2, 2)
        );
        assert_eq!(
            state
                .add_symbol(1, &limits, &mut resources)
                .expect("second symbol"),
            (3, 2)
        );
        assert_eq!(state.finish_height_class().expect("finish class"), 5);
        state.finish().expect("exact symbol count");

        let mut state = DictionaryClassState::new(1, &limits).expect("state");
        assert!(matches!(
            state.begin_height_class(-1, &limits, &mut resources),
            Err(SymbolTextError::InvalidDimensions)
        ));
    }

    #[test]
    fn decodes_export_runs_including_zero_length_toggles() {
        let limits = Limits::default();
        let mut resources = ResourceTracker::default();
        let mut codec = SymbolCodec::Scripted(ScriptedCodec {
            integers: VecDeque::from([
                EntropyInteger::Value(2),
                EntropyInteger::Value(1),
                EntropyInteger::Value(2),
            ]),
            symbol_id: 0,
        });
        let exported = decode_export_runs(&mut codec, 5, 1, &limits, &mut resources)
            .expect("valid export runs");
        assert_eq!(exported, vec![2]);

        let mut resources = ResourceTracker::default();
        let mut codec = SymbolCodec::Scripted(ScriptedCodec {
            integers: VecDeque::from([
                EntropyInteger::Value(0),
                EntropyInteger::Value(0),
                EntropyInteger::Value(5),
            ]),
            symbol_id: 0,
        });
        let exported = decode_export_runs(&mut codec, 5, 0, &limits, &mut resources)
            .expect("zero runs make input progress");
        assert!(exported.is_empty());
    }

    #[test]
    fn bounds_nonprogressing_export_runs_by_work_limit() {
        let limits = Limits {
            max_work_units: 2,
            ..Limits::default()
        };
        let mut resources = ResourceTracker::default();
        let mut codec = SymbolCodec::Scripted(ScriptedCodec {
            integers: VecDeque::from([
                EntropyInteger::Value(0),
                EntropyInteger::Value(0),
                EntropyInteger::Value(0),
            ]),
            symbol_id: 0,
        });
        let result = decode_export_runs(&mut codec, 1, 0, &limits, &mut resources);
        assert!(matches!(
            result,
            Err(SymbolTextError::Decode(DecodeError::LimitExceeded(
                LimitExceeded {
                    kind: LimitKind::Work,
                    ..
                }
            )))
        ));
    }

    #[test]
    fn expands_and_validates_symbol_length_runs() {
        let limits = Limits::default();
        let mut resources = ResourceTracker::default();
        let lengths = expand_symbol_code_lengths(
            8,
            &[
                SymbolLengthRun::Literal(3),
                SymbolLengthRun::RepeatPrevious(3),
                SymbolLengthRun::Zeros(4),
            ],
            &limits,
            &mut resources,
        )
        .expect("valid runs");
        assert_eq!(lengths, vec![3, 3, 3, 3, 0, 0, 0, 0]);

        let mut resources = ResourceTracker::default();
        assert!(matches!(
            expand_symbol_code_lengths(
                4,
                &[SymbolLengthRun::RepeatPrevious(3)],
                &limits,
                &mut resources
            ),
            Err(SymbolTextError::InvalidSymbolLengthRun)
        ));
        let mut resources = ResourceTracker::default();
        assert!(matches!(
            expand_symbol_code_lengths(4, &[SymbolLengthRun::Zeros(5)], &limits, &mut resources),
            Err(SymbolTextError::InvalidSymbolLengthRun)
        ));
    }

    #[test]
    fn builds_symbol_id_huffman_table_from_lengths() {
        let limits = Limits::default();
        let table = HuffmanTable::new_symbol_ids(&[1, 3, 3], &limits);
        assert!(table.is_ok());
        let invalid = HuffmanTable::new_symbol_ids(&[1, 1, 1], &limits);
        assert!(invalid.is_err());
    }

    #[test]
    fn exported_symbols_match_exact_clone_oracle() {
        let limits = Limits::default();
        let imported = Bitmap {
            width: 8,
            height: 1,
            stride: 1,
            data: vec![0x11],
        };
        let new0 = Bitmap {
            width: 8,
            height: 1,
            stride: 1,
            data: vec![0x22],
        };
        let new1 = Bitmap {
            width: 8,
            height: 1,
            stride: 1,
            data: vec![0x33],
        };
        let imported_pointer = imported.data.as_ptr();
        let new0_pointer = new0.data.as_ptr();
        let expected = vec![
            clone_bitmap(&imported, &limits).expect("imported clone"),
            clone_bitmap(&new0, &limits).expect("new clone oracle"),
        ];
        let new_symbols = vec![new0, new1];
        let output = assemble_exported_symbols(
            SymbolCodebook::Borrowed(&[&imported]),
            &new_symbols,
            &[0, 1],
            &limits,
        )
        .expect("assembled symbols");
        assert_eq!(output, expected);
        assert_ne!(output[0].data.as_ptr(), imported_pointer);
        assert_ne!(output[1].data.as_ptr(), new0_pointer);
        assert!(output.capacity() >= 2);
    }

    #[test]
    fn sparse_new_exports_preserve_scalar_order() {
        let limits = Limits::default();
        let mut symbols = Vec::new();
        for value in 0_u8..8 {
            symbols.push(Bitmap {
                width: 8,
                height: 1,
                stride: 1,
                data: vec![value],
            });
        }
        let output = assemble_exported_symbols(
            SymbolCodebook::Contiguous(&[]),
            &symbols,
            &[0, 3, 7],
            &limits,
        )
        .expect("sparse exports");
        assert_eq!(
            output
                .iter()
                .map(|bitmap| bitmap.data[0])
                .collect::<Vec<_>>(),
            vec![0, 3, 7]
        );
    }

    #[test]
    fn borrowed_symbol_buffer_reuses_pointer_capacity() {
        let limits = Limits::default();
        let symbols = [
            Bitmap::blank(8, 1, &limits).expect("symbol"),
            Bitmap::blank(8, 1, &limits).expect("symbol"),
        ];
        let mut borrowed = Vec::new();
        borrowed
            .try_reserve(symbols.len())
            .expect("pointer reserve");
        for symbol in &symbols {
            borrowed.push(symbol);
        }
        let pointer = borrowed.as_ptr();
        let capacity = borrowed.capacity();
        borrowed.clear();
        for symbol in &symbols {
            borrowed.push(symbol);
        }
        assert_eq!(borrowed.as_ptr(), pointer);
        assert_eq!(borrowed.capacity(), capacity);
    }

    #[test]
    fn computes_strip_placement_and_reference_corners() {
        let limits = Limits::default();
        let mut resources = ResourceTracker::default();
        let mut state = TextRegionState::new(
            text_config(ReferenceCorner::TopRight, false, 4),
            1,
            2,
            &limits,
        )
        .expect("state");
        state.begin_strip(3, 2).expect("strip");
        let placement = state
            .place_first(2, 3, 2, &limits, &mut resources)
            .expect("placement");
        assert_eq!(placement, Placement { x: 2, y: 6 });
        state.end_strip().expect("end strip");
        state.finish().expect("finish region");

        let mut resources = ResourceTracker::default();
        let mut state = TextRegionState::new(
            text_config(ReferenceCorner::BottomLeft, true, 1),
            1,
            0,
            &limits,
        )
        .expect("state");
        state.begin_strip(4, 7).expect("strip");
        let placement = state
            .place_first(0, 3, 2, &limits, &mut resources)
            .expect("placement");
        assert_eq!(placement, Placement { x: 4, y: 7 });
    }

    #[test]
    fn clips_negative_symbol_placement() {
        let limits = Limits::default();
        let source = Bitmap::filled(3, 2, true, &limits).expect("source");
        let mut destination = Bitmap::blank(3, 3, &limits).expect("destination");
        let mut resources = ResourceTracker::default();
        compose_placement(
            &mut destination,
            &source,
            Placement { x: -1, y: -1 },
            CombinationOperator::Or,
            &limits,
            &mut resources,
        )
        .expect("clipped composition");
        assert_eq!(destination.pixel(0, 0), Some(true));
        assert_eq!(destination.pixel(1, 0), Some(true));
        assert_eq!(destination.pixel(2, 0), Some(false));
        assert_eq!(destination.pixel(0, 1), Some(false));
    }

    #[test]
    fn clipped_symbol_placement_matches_scalar_oracle() {
        let limits = Limits::default();
        let operators = [
            CombinationOperator::Or,
            CombinationOperator::And,
            CombinationOperator::Xor,
            CombinationOperator::Xnor,
            CombinationOperator::Replace,
        ];
        for source_width in 0..=17 {
            for destination_width in 0..=17 {
                let source = Bitmap::filled(source_width, 3, true, &limits).expect("source");
                for x in -20..=20 {
                    for y in -4..=4 {
                        for operator in operators {
                            let mut expected = Bitmap::filled(destination_width, 4, false, &limits)
                                .expect("destination");
                            let mut actual = expected.clone();
                            let mut expected_resources = ResourceTracker::default();
                            let mut actual_resources = ResourceTracker::default();
                            scalar_compose_placement_oracle(
                                &mut expected,
                                &source,
                                Placement { x, y },
                                operator,
                                &limits,
                                &mut expected_resources,
                            )
                            .expect("scalar placement");
                            compose_placement(
                                &mut actual,
                                &source,
                                Placement { x, y },
                                operator,
                                &limits,
                                &mut actual_resources,
                            )
                            .expect("chunk placement");
                            assert_eq!(actual, expected);
                            assert_eq!(actual_resources, expected_resources);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn placement_batches_match_scalar_for_alignments_and_operators() {
        let limits = Limits::default();
        let operators = [
            CombinationOperator::Or,
            CombinationOperator::And,
            CombinationOperator::Xor,
            CombinationOperator::Xnor,
            CombinationOperator::Replace,
        ];
        for alignment in 0_i64..8 {
            for width in 1..=16 {
                let source = Bitmap::filled(width, 3, true, &limits).expect("source");
                let placements = [
                    (&source, Placement { x: alignment, y: 1 }),
                    (
                        &source,
                        Placement {
                            x: alignment + i64::from(width / 2),
                            y: 1,
                        },
                    ),
                    (
                        &source,
                        Placement {
                            x: alignment + i64::from(width),
                            y: 1,
                        },
                    ),
                ];
                for operator in operators {
                    let mut expected = Bitmap::filled(64, 6, false, &limits).expect("destination");
                    let mut actual = expected.clone();
                    let mut expected_resources = ResourceTracker::default();
                    let mut actual_resources = ResourceTracker::default();
                    for &(source, placement) in &placements {
                        scalar_compose_placement_oracle(
                            &mut expected,
                            source,
                            placement,
                            operator,
                            &limits,
                            &mut expected_resources,
                        )
                        .expect("scalar placement");
                    }
                    batched_compose_placement(
                        &mut actual,
                        &placements,
                        operator,
                        &limits,
                        &mut actual_resources,
                    )
                    .expect("batched placement");
                    assert_eq!(actual, expected);
                    assert_eq!(actual_resources, expected_resources);
                }
            }
        }
    }

    #[test]
    fn empty_placement_batch_flush_preserves_all_storage_and_metadata() {
        let limits = Limits::default();
        for width in [0, 1, 7, 8, 9, 65] {
            for height in [0, 1, 3] {
                for operator in [
                    CombinationOperator::Or,
                    CombinationOperator::And,
                    CombinationOperator::Xor,
                    CombinationOperator::Xnor,
                    CombinationOperator::Replace,
                ] {
                    let original = Bitmap::filled(width, height, true, &limits).expect("bitmap");
                    let mut destination = original.clone();
                    let mut batch = PlacementBatch {
                        blits: Vec::with_capacity(MAX_PLACEMENT_BATCH),
                        source_y: u32::MAX,
                        destination_y: u32::MAX,
                        height: u32::MAX,
                        source_shift: 7,
                        destination_shift: 7,
                    };
                    let capacity = batch.blits.capacity();
                    flush_placement_batch(&mut destination, &mut batch, operator)
                        .expect("empty flush succeeds without consulting unused geometry");
                    assert_eq!(destination, original);
                    assert!(batch.blits.is_empty());
                    assert_eq!(batch.blits.capacity(), capacity);
                    assert_eq!(
                        (
                            batch.source_y,
                            batch.destination_y,
                            batch.height,
                            batch.source_shift,
                            batch.destination_shift,
                        ),
                        (u32::MAX, u32::MAX, u32::MAX, 7, 7),
                    );
                }
            }
        }
    }

    #[test]
    fn zero_height_placement_batches_clear_pending_entries_without_mutation() {
        let limits = Limits::default();
        let source = Bitmap::filled(129, 3, true, &limits).expect("source");
        let original = Bitmap::filled(139, 4, true, &limits).expect("destination");
        for count in [1, 2, MAX_PLACEMENT_BATCH] {
            for width in [0, 1, 7, 8, 64] {
                for operator in [
                    CombinationOperator::Or,
                    CombinationOperator::And,
                    CombinationOperator::Xor,
                    CombinationOperator::Xnor,
                    CombinationOperator::Replace,
                ] {
                    let mut destination = original.clone();
                    let mut batch = PlacementBatch {
                        blits: (0..count)
                            .map(|index| RowBlit {
                                source: &source,
                                source_x: 3,
                                destination_x: 5 + u32::try_from(index % 4)
                                    .expect("bounded column")
                                    * 8,
                                width,
                            })
                            .collect(),
                        source_y: source.height(),
                        destination_y: destination.height(),
                        height: 0,
                        source_shift: 3,
                        destination_shift: 5,
                    };
                    let capacity = batch.blits.capacity();
                    for _ in 0..2 {
                        flush_placement_batch(&mut destination, &mut batch, operator)
                            .expect("zero-height and subsequent empty flush succeed");
                        assert_eq!(destination, original);
                        assert!(batch.blits.is_empty());
                        assert_eq!(batch.blits.capacity(), capacity);
                        assert_eq!(
                            (
                                batch.source_y,
                                batch.destination_y,
                                batch.height,
                                batch.source_shift,
                                batch.destination_shift,
                            ),
                            (3, 4, 0, 3, 5),
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn randomized_placement_batches_match_scalar_oracle() {
        let limits = Limits::default();
        let operators = [
            CombinationOperator::Or,
            CombinationOperator::And,
            CombinationOperator::Xor,
            CombinationOperator::Xnor,
            CombinationOperator::Replace,
        ];
        let mut random = 0x51f1_5eed_u32;
        for case in 0..512 {
            let mut sources = Vec::new();
            for _ in 0..8 {
                random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let width = random % 24 + 1;
                random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let height = random % 5 + 1;
                let mut bitmap = Bitmap::blank(width, height, &limits).expect("source");
                for byte in &mut bitmap.data {
                    random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    *byte = u8::try_from(random >> 24).expect("random byte");
                }
                sources.push(bitmap);
            }
            let mut placements = Vec::new();
            for source in &sources {
                random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let x = i64::from(random % 52) - 10;
                random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let y = i64::from(random % 14) - 4;
                placements.push((source, Placement { x, y }));
            }
            let operator = operators[case % operators.len()];
            let mut expected = Bitmap::blank(41, 9, &limits).expect("destination");
            let mut actual = expected.clone();
            let mut expected_resources = ResourceTracker::default();
            let mut actual_resources = ResourceTracker::default();
            for &(source, placement) in &placements {
                scalar_compose_placement_oracle(
                    &mut expected,
                    source,
                    placement,
                    operator,
                    &limits,
                    &mut expected_resources,
                )
                .expect("scalar placement");
            }
            batched_compose_placement(
                &mut actual,
                &placements,
                operator,
                &limits,
                &mut actual_resources,
            )
            .expect("batched placement");
            assert_eq!(actual, expected, "case {case}");
            assert_eq!(actual_resources, expected_resources, "case {case}");
        }
    }

    #[test]
    #[ignore = "microbenchmark; run explicitly with --release --ignored --nocapture"]
    fn benchmark_clipped_text_route_against_6333205_scalar_oracle() {
        let limits = Limits::default();
        let source = Bitmap::filled(1024, 512, true, &limits).expect("source");
        let original = Bitmap::blank(1024, 512, &limits).expect("destination");
        let placement = Placement { x: -3, y: -2 };
        let mut chunk_times = Vec::new();
        let mut scalar_times = Vec::new();
        for sample in 0..11 {
            let mut chunk = original.clone();
            let mut scalar = original.clone();
            let mut chunk_resources = ResourceTracker::default();
            let mut scalar_resources = ResourceTracker::default();
            if sample % 2 == 0 {
                let start = Instant::now();
                compose_placement(
                    &mut chunk,
                    &source,
                    placement,
                    CombinationOperator::Or,
                    &limits,
                    &mut chunk_resources,
                )
                .expect("chunk placement");
                chunk_times.push(start.elapsed());
                black_box(chunk.bytes());

                let start = Instant::now();
                scalar_compose_placement_oracle(
                    &mut scalar,
                    &source,
                    placement,
                    CombinationOperator::Or,
                    &limits,
                    &mut scalar_resources,
                )
                .expect("scalar placement");
                scalar_times.push(start.elapsed());
                black_box(scalar.bytes());
            } else {
                let start = Instant::now();
                scalar_compose_placement_oracle(
                    &mut scalar,
                    &source,
                    placement,
                    CombinationOperator::Or,
                    &limits,
                    &mut scalar_resources,
                )
                .expect("scalar placement");
                scalar_times.push(start.elapsed());
                black_box(scalar.bytes());

                let start = Instant::now();
                compose_placement(
                    &mut chunk,
                    &source,
                    placement,
                    CombinationOperator::Or,
                    &limits,
                    &mut chunk_resources,
                )
                .expect("chunk placement");
                chunk_times.push(start.elapsed());
                black_box(chunk.bytes());
            }
            assert_eq!(chunk, scalar);
            assert_eq!(chunk_resources, scalar_resources);
        }
        let chunk = median_duration(chunk_times);
        let scalar = median_duration(scalar_times);
        println!(
            "text scalar_6333205_median_ns={} chunk_median_ns={} speedup={:.3}",
            scalar.as_nanos(),
            chunk.as_nanos(),
            scalar.as_secs_f64() / chunk.as_secs_f64(),
        );
    }

    #[test]
    #[ignore = "performance microbenchmark"]
    fn benchmark_row_aligned_text_placement_batch() {
        let limits = Limits::default();
        let source = Bitmap::filled(8, 1, true, &limits).expect("source");
        let original = Bitmap::blank(32_768, 1, &limits).expect("destination");
        let placements = (0..4_096)
            .map(|index| {
                (
                    &source,
                    Placement {
                        x: i64::from(index * 8),
                        y: 0,
                    },
                )
            })
            .collect::<Vec<_>>();
        let mut batched_times = Vec::new();
        let mut sequential_times = Vec::new();
        for sample in 0..11 {
            let mut batched = original.clone();
            let mut sequential = original.clone();
            let mut batched_resources = ResourceTracker::default();
            let mut sequential_resources = ResourceTracker::default();
            for batched_first in [sample % 2 == 0, sample % 2 != 0] {
                let start = Instant::now();
                if batched_first {
                    batched_compose_placement(
                        &mut batched,
                        &placements,
                        CombinationOperator::Or,
                        &limits,
                        &mut batched_resources,
                    )
                    .expect("batched placements");
                    batched_times.push(start.elapsed());
                    black_box(batched.bytes());
                } else {
                    for &(source, placement) in &placements {
                        compose_placement(
                            &mut sequential,
                            source,
                            placement,
                            CombinationOperator::Or,
                            &limits,
                            &mut sequential_resources,
                        )
                        .expect("sequential placement");
                    }
                    sequential_times.push(start.elapsed());
                    black_box(sequential.bytes());
                }
            }
            assert_eq!(batched, sequential);
            assert_eq!(batched_resources, sequential_resources);
        }
        let batched = median_duration(batched_times);
        let sequential = median_duration(sequential_times);
        println!(
            "text-batch sequential_median_ns={} batched_median_ns={} speedup={:.3}",
            sequential.as_nanos(),
            batched.as_nanos(),
            sequential.as_secs_f64() / batched.as_secs_f64(),
        );
    }

    #[test]
    fn refinement_geometry_uses_mathematical_floor() {
        let geometry =
            refinement_geometry(5, 7, -3, 1, 3, -1, &Limits::default()).expect("valid refinement");
        assert_eq!(
            geometry,
            RefinementGeometry {
                width: 2,
                height: 8,
                reference_dx: 1,
                reference_dy: -1,
            }
        );
    }

    #[test]
    fn refinement_state_receives_checked_geometry() {
        let limits = Limits::default();
        let reference = Bitmap::blank(5, 7, &limits).expect("reference");
        let mut codec = SymbolCodec::Scripted(ScriptedCodec {
            integers: VecDeque::from([
                EntropyInteger::Value(-3),
                EntropyInteger::Value(1),
                EntropyInteger::Value(3),
                EntropyInteger::Value(-1),
                EntropyInteger::Value(4),
            ]),
            symbol_id: 0,
        });
        let mut resources = ResourceTracker::default();
        let bitmap = decode_refined_bitmap(
            &mut codec,
            &reference,
            0,
            [(0, 0); 2],
            true,
            &limits,
            &mut resources,
        )
        .expect("refinement");
        assert_eq!((bitmap.width(), bitmap.height()), (2, 8));
    }

    #[test]
    fn rejects_unused_fixed_length_symbol_ids() {
        let error = checked_symbol_id(7, 5).expect_err("ID 7 is outside five symbols");
        assert!(matches!(
            error,
            SymbolTextError::SymbolIdOutOfRange {
                id: 7,
                available: 5
            }
        ));
    }

    #[test]
    fn repeated_standard_table_materialization_is_reused() {
        let limits = Limits::default();
        let mut tables = StandardHuffmanTables::empty();
        tables
            .materialize(StandardHuffmanTable::A, &limits)
            .expect("first table A");
        let first = tables
            .table(StandardHuffmanTable::A)
            .expect("materialized table") as *const HuffmanTable;
        tables
            .materialize(StandardHuffmanTable::A, &limits)
            .expect("second table A");
        let second =
            tables.table(StandardHuffmanTable::A).expect("reused table") as *const HuffmanTable;
        assert_eq!(first, second);
    }

    #[test]
    fn concrete_empty_dictionary_and_arithmetic_text_segments_decode() {
        const ANNEX_H_ENCODED: [u8; 30] = [
            0x84, 0xc7, 0x3b, 0xfc, 0xe1, 0xa1, 0x43, 0x04, 0x02, 0x20, 0x00, 0x00, 0x41, 0x0d,
            0xbb, 0x86, 0xf4, 0x31, 0x7f, 0xff, 0x88, 0xff, 0x37, 0x47, 0x1a, 0xdb, 0x6a, 0xdf,
            0xff, 0xac,
        ];
        let limits = Limits::default();
        let custom: [&HuffmanTable; 0] = [];

        let mut arithmetic_dictionary = vec![0x00, 0x00];
        arithmetic_dictionary.extend_from_slice(&[3, 0xff, 0xfd, 0xff, 2, 0xfe, 0xfe, 0xfe]);
        arithmetic_dictionary.extend_from_slice(&0_u32.to_be_bytes());
        arithmetic_dictionary.extend_from_slice(&0_u32.to_be_bytes());
        arithmetic_dictionary.extend_from_slice(&[0xff, 0xac]);
        let arithmetic_output = decode_symbol_dictionary(&SymbolDictionaryInput {
            payload: &arithmetic_dictionary,
            imported_symbols: SymbolCodebook::Contiguous(&[]),
            custom_tables: &custom,
            used_contexts: None,
            limits: &limits,
        })
        .expect("empty arithmetic dictionary decodes");
        assert!(arithmetic_output.exported_symbols.is_empty());
        assert_eq!(
            arithmetic_output.consumed_bytes,
            arithmetic_dictionary.len()
        );

        let mut huffman_dictionary = vec![0x00, 0x01];
        huffman_dictionary.extend_from_slice(&0_u32.to_be_bytes());
        huffman_dictionary.extend_from_slice(&0_u32.to_be_bytes());
        let huffman_output = decode_symbol_dictionary(&SymbolDictionaryInput {
            payload: &huffman_dictionary,
            imported_symbols: SymbolCodebook::Contiguous(&[]),
            custom_tables: &custom,
            used_contexts: None,
            limits: &limits,
        })
        .expect("empty Huffman dictionary decodes");
        assert!(huffman_output.exported_symbols.is_empty());
        assert_eq!(huffman_output.consumed_bytes, huffman_dictionary.len());

        let mut arithmetic_text = vec![0_u8; 17];
        arithmetic_text[3] = 1;
        arithmetic_text[7] = 1;
        arithmetic_text.extend_from_slice(&0_u16.to_be_bytes());
        arithmetic_text.extend_from_slice(&0_u32.to_be_bytes());
        arithmetic_text.extend_from_slice(&ANNEX_H_ENCODED);
        let arithmetic_text_output = decode_text_region(&TextRegionInput {
            payload: &arithmetic_text,
            symbols: SymbolCodebook::Contiguous(&[]),
            custom_tables: &custom,
            limits: &limits,
        })
        .expect("empty arithmetic text region decodes");
        assert_eq!(arithmetic_text_output.bitmap.bytes(), &[0]);
    }

    #[test]
    #[ignore = "performance microbenchmark"]
    #[allow(clippy::cast_precision_loss)]
    fn benchmark_reused_standard_huffman_materialization() {
        use std::hint::black_box;
        use std::time::Instant;

        fn choices() -> HuffmanFieldChoices<'static> {
            let mut choices = HuffmanFieldChoices::empty();
            choices.first_s = Some(BoundHuffmanTable::Standard(StandardHuffmanTable::A));
            choices.subsequent_s = Some(BoundHuffmanTable::Standard(StandardHuffmanTable::A));
            choices.strip_delta = Some(BoundHuffmanTable::Standard(StandardHuffmanTable::A));
            choices.refinement_width = Some(BoundHuffmanTable::Standard(StandardHuffmanTable::O));
            choices.refinement_height = Some(BoundHuffmanTable::Standard(StandardHuffmanTable::O));
            choices.refinement_x = Some(BoundHuffmanTable::Standard(StandardHuffmanTable::O));
            choices.refinement_y = Some(BoundHuffmanTable::Standard(StandardHuffmanTable::O));
            choices.refinement_size = Some(BoundHuffmanTable::Standard(StandardHuffmanTable::A));
            choices
        }

        let limits = Limits::default();
        let mut reused_samples = Vec::new();
        let mut repeated_samples = Vec::new();
        for round in 0..9 {
            for repeated in [round % 2 != 0, round % 2 == 0] {
                let start = Instant::now();
                for _ in 0..64 {
                    if repeated {
                        let tables = vec![
                            Box::new(
                                HuffmanTable::standard(StandardHuffmanTable::A, &limits)
                                    .expect("table A"),
                            ),
                            Box::new(
                                HuffmanTable::standard(StandardHuffmanTable::A, &limits)
                                    .expect("table A"),
                            ),
                            Box::new(
                                HuffmanTable::standard(StandardHuffmanTable::A, &limits)
                                    .expect("table A"),
                            ),
                            Box::new(
                                HuffmanTable::standard(StandardHuffmanTable::O, &limits)
                                    .expect("table O"),
                            ),
                            Box::new(
                                HuffmanTable::standard(StandardHuffmanTable::O, &limits)
                                    .expect("table O"),
                            ),
                            Box::new(
                                HuffmanTable::standard(StandardHuffmanTable::O, &limits)
                                    .expect("table O"),
                            ),
                            Box::new(
                                HuffmanTable::standard(StandardHuffmanTable::O, &limits)
                                    .expect("table O"),
                            ),
                            Box::new(
                                HuffmanTable::standard(StandardHuffmanTable::A, &limits)
                                    .expect("table A"),
                            ),
                        ];
                        black_box(tables);
                    } else {
                        let tables = (|| -> Result<_, SymbolTextError> {
                            materialize_huffman_tables!(
                                HuffmanChoiceSet::TextRegion(choices()),
                                &limits
                            )
                        })()
                        .expect("materialized tables");
                        black_box(tables);
                    }
                }
                let elapsed = start.elapsed().as_nanos();
                if repeated {
                    repeated_samples.push(elapsed);
                } else {
                    reused_samples.push(elapsed);
                }
            }
        }
        reused_samples.sort_unstable();
        repeated_samples.sort_unstable();
        let reused = reused_samples[reused_samples.len() / 2];
        let repeated = repeated_samples[repeated_samples.len() / 2];
        let gain = (repeated - reused) as f64 * 100.0 / repeated as f64;
        eprintln!(
            "huffman-materialization: repeated={repeated}ns reused={reused}ns gain={gain:.2}%"
        );
    }

    #[test]
    #[ignore = "performance microbenchmark"]
    #[allow(clippy::cast_precision_loss)]
    fn benchmark_borrowed_dictionary_text_codebook() {
        let limits = Limits::default();
        let mut symbols = Vec::new();
        for value in 0_u16..2_048 {
            let mut bitmap = Bitmap::blank(128, 8, &limits).expect("symbol");
            bitmap.data.fill(u8::try_from(value & 0xff).expect("byte"));
            symbols.push(bitmap);
        }
        let mut borrowed_samples = Vec::new();
        let mut cloned_samples = Vec::new();
        let mut borrowed = Vec::new();
        borrowed
            .try_reserve(symbols.len())
            .expect("borrowed codebook capacity");
        for round in 0..9 {
            for cloned in [round % 2 != 0, round % 2 == 0] {
                let start = Instant::now();
                for _ in 0..32 {
                    if cloned {
                        let mut owned = Vec::new();
                        owned.try_reserve(symbols.len()).expect("owned codebook");
                        for symbol in &symbols {
                            owned.push(clone_bitmap(symbol, &limits).expect("symbol clone"));
                        }
                        black_box(owned);
                    } else {
                        borrowed.clear();
                        for symbol in &symbols {
                            borrowed.push(symbol);
                        }
                        black_box(&borrowed);
                    }
                }
                let elapsed = start.elapsed().as_nanos();
                if cloned {
                    cloned_samples.push(elapsed);
                } else {
                    borrowed_samples.push(elapsed);
                }
            }
        }
        borrowed_samples.sort_unstable();
        cloned_samples.sort_unstable();
        let borrowed = borrowed_samples[borrowed_samples.len() / 2];
        let cloned = cloned_samples[cloned_samples.len() / 2];
        let gain = (cloned - borrowed) as f64 * 100.0 / cloned as f64;
        eprintln!("symbol-codebook: cloned={cloned}ns borrowed={borrowed}ns gain={gain:.2}%");
    }
}

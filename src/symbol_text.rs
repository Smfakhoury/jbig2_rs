//! Symbol-dictionary and text-region decoding helpers.
//!
//! This module deliberately separates the normative state machines from the
//! entropy and generic-refinement engines.  Segment wiring can therefore bind
//! either arithmetic or Huffman integer decoders without duplicating the
//! bounds, placement, and progress checks implemented here.

use core::cell::RefCell;
use std::borrow::Cow;

use crate::arithmetic::{ArithmeticContext, ArithmeticDecoder, ArithmeticError, ArithmeticInteger};
use crate::bitmap::{Bitmap, CombinationOperator};
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
use vstd::prelude::*;

#[cfg(verus_keep_ghost)]
#[rustfmt::skip]
#[path = "../specs/symbol_text.rs"]
mod symbol_text_spec;

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
    #[cfg(verus_keep_ghost)]
    {
        value as i64
    }
    #[cfg(not(verus_keep_ghost))]
    {
        i64::from(value)
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out as int == value as int,
))]
fn widen_u8_i64(value: u8) -> i64 {
    #[cfg(verus_keep_ghost)]
    {
        value as i64
    }
    #[cfg(not(verus_keep_ghost))]
    {
        i64::from(value)
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out as int == value as int,
))]
fn widen_u8_i32(value: u8) -> i32 {
    #[cfg(verus_keep_ghost)]
    {
        value as i32
    }
    #[cfg(not(verus_keep_ghost))]
    {
        i32::from(value)
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out == if value { 1i64 } else { 0i64 },
))]
fn widen_bool_i64(value: bool) -> i64 {
    #[cfg(verus_keep_ghost)]
    {
        if value {
            1
        } else {
            0
        }
    }
    #[cfg(not(verus_keep_ghost))]
    {
        i64::from(value)
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

/// Entropy operations needed by the symbol and text state machines.
pub(crate) trait EntropyDecoder {
    fn decode_integer(&mut self, field: IntegerField) -> Result<EntropyInteger, SymbolTextError>;

    fn decode_symbol_id(&mut self, request: SymbolIdRequest) -> Result<u32, SymbolTextError>;

    /// Prepares symbol-ID decoding for `symbol_count` available symbols.
    ///
    /// Huffman implementations must build the T.88 7.4.3.1.7 run-length coded
    /// symbol-ID table from the current bit position (and byte-align
    /// afterwards) before any [`Self::decode_symbol_id`] call. Arithmetic
    /// implementations have no equivalent per-call setup, so the default
    /// implementation is a no-op.
    fn prepare_symbol_ids(
        &mut self,
        symbol_count: u32,
        limits: &Limits,
    ) -> Result<(), SymbolTextError> {
        let _ = (symbol_count, limits);
        Ok(())
    }
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
#[derive(Debug)]
pub(crate) struct RefinementRequest<'a> {
    pub(crate) reference: &'a Bitmap,
    pub(crate) geometry: RefinementGeometry,
    pub(crate) template: u8,
    pub(crate) adaptive_pixels: [(i8, i8); 2],
    pub(crate) declared_data_size: Option<u32>,
}

/// Callback boundary for the arithmetic generic-refinement decoder.
///
/// When `declared_data_size` is present, the implementation must constrain its
/// input to exactly that many bytes and reject under- or over-consumption.
pub(crate) trait GenericRefinementDecoder {
    fn decode_refinement(
        &mut self,
        request: RefinementRequest<'_>,
        limits: &Limits,
    ) -> Result<Bitmap, SymbolTextError>;
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

pub(crate) fn associate_dictionary_tables<'a>(
    config: &SymbolDictionaryConfig,
    custom: &'a [CustomHuffmanTable<'a>],
) -> Result<Vec<TableBinding<'a>>, SymbolTextError> {
    if !config.huffman {
        if custom.is_empty() {
            return Ok(Vec::new());
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

pub(crate) fn associate_text_tables<'a>(
    config: &TextRegionConfig,
    custom: &'a [CustomHuffmanTable<'a>],
) -> Result<Vec<TableBinding<'a>>, SymbolTextError> {
    let Some(choices) = config.huffman_choices else {
        if custom.is_empty() {
            return Ok(Vec::new());
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

fn bind_tables<'a>(
    specifications: &[(TableParameter, TableChoice, bool)],
    custom: &'a [CustomHuffmanTable<'a>],
) -> Result<Vec<TableBinding<'a>>, SymbolTextError> {
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
    let requested_bytes = specifications
        .len()
        .checked_mul(core::mem::size_of::<TableBinding<'_>>())
        .and_then(|bytes| u64::try_from(bytes).ok())
        .ok_or(MalformedError::IntegerOverflow)?;
    let mut bindings = Vec::new();
    bindings
        .try_reserve_exact(specifications.len())
        .map_err(|_error| DecodeError::Allocation { requested_bytes })?;
    let mut custom_index = 0_usize;
    let mut specification_index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            specification_index <= specifications@.len(),
            bindings@.len() == specification_index,
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
        bindings.push(TableBinding { parameter, table });
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
                && final(self).class_open
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
                && final(self).class_open
                && final(self).decoded_symbols == old(self).decoded_symbols + 1
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
pub(crate) fn decode_export_runs<F>(
    total_candidates: u32,
    expected_exports: u32,
    limits: &Limits,
    resources: &mut ResourceTracker,
    mut next_run: F,
) -> Result<Vec<u32>, SymbolTextError>
where
    F: FnMut() -> Result<EntropyInteger, SymbolTextError>,
{
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
    let mut exported = Vec::new();
    exported
        .try_reserve_exact(capacity)
        .map_err(|_| DecodeError::Allocation { requested_bytes })?;
    let mut index = 0_u32;
    let mut export_flag = false;
    while index < total_candidates {
        resources.charge_work(1, limits)?;
        let run = match next_run()? {
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

pub(crate) fn expand_symbol_code_lengths<I>(
    symbol_count: u32,
    runs: I,
    limits: &Limits,
    resources: &mut ResourceTracker,
) -> Result<Vec<u8>, SymbolTextError>
where
    I: IntoIterator<Item = SymbolLengthRun>,
{
    enforce_limit(
        LimitKind::Symbols,
        u64::from(limits.max_symbols),
        u64::from(symbol_count),
    )?;
    let capacity = usize::try_from(symbol_count).map_err(|_| MalformedError::IntegerOverflow)?;
    let mut lengths = Vec::new();
    lengths
        .try_reserve_exact(capacity)
        .map_err(|_| DecodeError::Allocation {
            requested_bytes: u64::from(symbol_count),
        })?;
    for run in runs {
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
    requires old(lengths)@.len() <= capacity,
    ensures match out {
        Ok(()) =>
            final(lengths)@.len() == old(lengths)@.len() + count
            && final(lengths)@.len() <= capacity
            && final(lengths)@.subrange(0, old(lengths)@.len() as int)
                == old(lengths)@
            && forall|i: int| old(lengths)@.len() <= i < final(lengths)@.len()
                ==> final(lengths)@[i] == value,
        Err(_) => true,
    },
))]
fn extend_lengths(
    lengths: &mut Vec<u8>,
    capacity: usize,
    value: u8,
    count: usize,
) -> Result<(), SymbolTextError> {
    let end = lengths
        .len()
        .checked_add(count)
        .ok_or(MalformedError::IntegerOverflow)?;
    if end > capacity {
        return Err(SymbolTextError::InvalidSymbolLengthRun);
    }
    lengths.resize(end, value);
    Ok(())
}

pub(crate) fn build_symbol_id_table(
    lengths: &[u8],
    limits: &Limits,
) -> Result<HuffmanTable, SymbolTextError> {
    let count = u32::try_from(lengths.len()).map_err(|_error| MalformedError::IntegerOverflow)?;
    enforce_limit(
        LimitKind::Symbols,
        u64::from(limits.max_symbols),
        u64::from(count),
    )?;
    let requested_bytes = lengths
        .len()
        .checked_mul(core::mem::size_of::<HuffmanLine>())
        .and_then(|bytes| u64::try_from(bytes).ok())
        .ok_or(MalformedError::IntegerOverflow)?;
    let mut lines = Vec::new();
    lines
        .try_reserve_exact(lengths.len())
        .map_err(|_error| DecodeError::Allocation { requested_bytes })?;
    for (index, &length) in lengths.iter().enumerate() {
        let value = i32::try_from(index).map_err(|_error| MalformedError::IntegerOverflow)?;
        lines.push(HuffmanLine::normal(value, length, 0));
    }
    Ok(HuffmanTable::new(&lines, limits)?)
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

pub(crate) fn decode_checked_symbol_id<E: EntropyDecoder>(
    entropy: &mut E,
    request: SymbolIdRequest,
) -> Result<u32, SymbolTextError> {
    if request.available_symbols == 0 {
        return Err(SymbolTextError::InvalidCount(
            "symbol ID with no available symbols",
        ));
    }
    let id = entropy.decode_symbol_id(request)?;
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

#[allow(clippy::too_many_arguments)]
pub(crate) fn decode_refined_bitmap<E, R>(
    entropy: &mut E,
    refinement: &mut R,
    reference: &Bitmap,
    template: u8,
    adaptive_pixels: [(i8, i8); 2],
    huffman: bool,
    limits: &Limits,
    resources: &mut ResourceTracker,
) -> Result<Bitmap, SymbolTextError>
where
    E: EntropyDecoder,
    R: GenericRefinementDecoder,
{
    let delta_width = required_integer(entropy, IntegerField::RefinementWidth)?;
    let delta_height = required_integer(entropy, IntegerField::RefinementHeight)?;
    let delta_x = required_integer(entropy, IntegerField::RefinementX)?;
    let delta_y = required_integer(entropy, IntegerField::RefinementY)?;
    let declared_data_size = if huffman {
        let size = required_integer(entropy, IntegerField::RefinementSize)?;
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
    let bitmap = refinement.decode_refinement(
        RefinementRequest {
            reference,
            geometry,
            template,
            adaptive_pixels,
            declared_data_size,
        },
        limits,
    )?;
    if bitmap.width() != geometry.width || bitmap.height() != geometry.height {
        return Err(SymbolTextError::InvalidDimensions);
    }
    Ok(bitmap)
}

fn required_integer<E: EntropyDecoder>(
    entropy: &mut E,
    field: IntegerField,
) -> Result<i64, SymbolTextError> {
    match entropy.decode_integer(field)? {
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
#[allow(clippy::too_many_lines)]
pub(crate) fn compose_placement(
    destination: &mut Bitmap,
    source: &Bitmap,
    placement: Placement,
    operator: CombinationOperator,
    limits: &Limits,
    resources: &mut ResourceTracker,
) -> Result<(), SymbolTextError> {
    let source_width = i64::from(source.width());
    let source_height = i64::from(source.height());
    let destination_width = i64::from(destination.width());
    let destination_height = i64::from(destination.height());
    let source_x = if placement.x < 0 {
        placement
            .x
            .checked_neg()
            .ok_or(MalformedError::IntegerOverflow)?
    } else {
        0
    };
    let source_y = if placement.y < 0 {
        placement
            .y
            .checked_neg()
            .ok_or(MalformedError::IntegerOverflow)?
    } else {
        0
    };
    let destination_x = placement.x.max(0);
    let destination_y = placement.y.max(0);
    let overlap_width = source_width
        .checked_sub(source_x)
        .ok_or(MalformedError::IntegerOverflow)?
        .min(
            destination_width
                .checked_sub(destination_x)
                .ok_or(MalformedError::IntegerOverflow)?,
        )
        .max(0);
    let overlap_height = source_height
        .checked_sub(source_y)
        .ok_or(MalformedError::IntegerOverflow)?
        .min(
            destination_height
                .checked_sub(destination_y)
                .ok_or(MalformedError::IntegerOverflow)?,
        )
        .max(0);
    let work = u64::try_from(overlap_width)
        .map_err(|_| MalformedError::IntegerOverflow)?
        .checked_mul(u64::try_from(overlap_height).map_err(|_| MalformedError::IntegerOverflow)?)
        .ok_or(MalformedError::IntegerOverflow)?;
    resources.charge_work(work, limits)?;
    if overlap_width == 0 || overlap_height == 0 {
        return Ok(());
    }
    if placement.x >= 0 && placement.y >= 0 {
        let x = u32::try_from(placement.x).map_err(|_| MalformedError::IntegerOverflow)?;
        let y = u32::try_from(placement.y).map_err(|_| MalformedError::IntegerOverflow)?;
        destination.compose(source, x, y, operator)?;
        return Ok(());
    }

    let mut row = 0_i64;
    while row < overlap_height {
        let mut column = 0_i64;
        while column < overlap_width {
            let sx = u32::try_from(
                source_x
                    .checked_add(column)
                    .ok_or(MalformedError::IntegerOverflow)?,
            )
            .map_err(|_| MalformedError::IntegerOverflow)?;
            let sy = u32::try_from(
                source_y
                    .checked_add(row)
                    .ok_or(MalformedError::IntegerOverflow)?,
            )
            .map_err(|_| MalformedError::IntegerOverflow)?;
            let dx = u32::try_from(
                destination_x
                    .checked_add(column)
                    .ok_or(MalformedError::IntegerOverflow)?,
            )
            .map_err(|_| MalformedError::IntegerOverflow)?;
            let dy = u32::try_from(
                destination_y
                    .checked_add(row)
                    .ok_or(MalformedError::IntegerOverflow)?,
            )
            .map_err(|_| MalformedError::IntegerOverflow)?;
            let incoming = source
                .pixel(sx, sy)
                .ok_or(MalformedError::IntegerOverflow)?;
            let current = destination
                .pixel(dx, dy)
                .ok_or(MalformedError::IntegerOverflow)?;
            set_pixel_checked(
                destination,
                dx,
                dy,
                apply_operator(operator, current, incoming),
            )?;
            column = column
                .checked_add(1)
                .ok_or(MalformedError::IntegerOverflow)?;
        }
        row = row.checked_add(1).ok_or(MalformedError::IntegerOverflow)?;
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out == crate::spec::bitmap::combine(operator, destination, source),
))]
fn apply_operator(operator: CombinationOperator, destination: bool, source: bool) -> bool {
    match operator {
        CombinationOperator::Or => destination || source,
        CombinationOperator::And => destination && source,
        CombinationOperator::Xor => destination != source,
        CombinationOperator::Xnor => destination == source,
        CombinationOperator::Replace => source,
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn set_pixel_checked(
    bitmap: &mut Bitmap,
    x: u32,
    y: u32,
    value: bool,
) -> Result<(), SymbolTextError> {
    if x >= bitmap.width || y >= bitmap.height {
        return Err(MalformedError::IntegerOverflow.into());
    }
    let byte_index = usize::try_from(y)
        .map_err(|_error| MalformedError::IntegerOverflow)?
        .checked_mul(bitmap.stride)
        .and_then(|offset| offset.checked_add(usize::try_from(x / 8).ok()?))
        .ok_or(MalformedError::IntegerOverflow)?;
    let bit = 7 - (x % 8);
    if byte_index >= bitmap.data.len() {
        return Err(MalformedError::IntegerOverflow.into());
    }
    let byte = &mut bitmap.data[byte_index];
    let mask = 1_u8 << bit;
    if value {
        *byte |= mask;
    } else {
        *byte &= !mask;
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
pub(crate) struct TextRegionHeader {
    pub(crate) region: RegionInfo,
    pub(crate) config: TextRegionConfig,
    pub(crate) refinement_adaptive: [TemplateOffset; 2],
    pub(crate) number_of_instances: u32,
}

/// Parses a text-region segment data header and returns its byte length.
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

/// A Huffman table resolved from either a freshly built standard table or a
/// caller-supplied custom table.
///
/// The owned standard-table variant is boxed: standard tables are several
/// kilobytes of fixed-size canonical-code arrays, and boxing avoids forcing
/// every `ResolvedTable` (most of which are small borrows of caller-owned
/// custom tables) to reserve that much inline space.
#[cfg_attr(verus_keep_ghost, verus_verify)]
enum ResolvedTable<'a> {
    Owned(Box<HuffmanTable>),
    Borrowed(&'a HuffmanTable),
}

impl ResolvedTable<'_> {
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn table(&self) -> &HuffmanTable {
        match self {
            Self::Owned(table) => table,
            Self::Borrowed(table) => table,
        }
    }
}

/// Builds a standard table and wraps it as an owned, boxed resolved table.
fn owned_standard_table<'a>(
    table: StandardHuffmanTable,
    limits: &Limits,
) -> Result<ResolvedTable<'a>, SymbolTextError> {
    Ok(ResolvedTable::Owned(Box::new(HuffmanTable::standard(
        table, limits,
    )?)))
}

fn resolve_bound_table<'a>(
    bound: BoundHuffmanTable<'a>,
    limits: &Limits,
) -> Result<ResolvedTable<'a>, SymbolTextError> {
    match bound {
        BoundHuffmanTable::Standard(table) => owned_standard_table(table, limits),
        BoundHuffmanTable::Custom(table) => Ok(ResolvedTable::Borrowed(table)),
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures
        out.is_some() == exists|i: int|
            0 <= i < bindings@.len()
            && bindings@[i].parameter == parameter,
))]
fn find_table_binding<'a>(
    bindings: &[TableBinding<'a>],
    parameter: TableParameter,
) -> Option<BoundHuffmanTable<'a>> {
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index <= bindings@.len(),
            forall|i: int| 0 <= i < index
                ==> bindings@[i].parameter != parameter,
        decreases bindings@.len() - index,
    ))]
    while index < bindings.len() {
        let binding = bindings[index];
        if same_table_parameter(binding.parameter, parameter) {
            return Some(binding.table);
        }
        index += 1;
    }
    None
}

/// Resolved Huffman tables for every table-backed [`IntegerField`].
///
/// [`IntegerField::WithinStripT`] and [`IntegerField::RefinementIndicator`]
/// are intentionally absent: T.88 decodes both as raw fixed-width bits, never
/// through a Huffman table.
#[cfg_attr(verus_keep_ghost, verus_verify)]
struct HuffmanFieldTables<'a> {
    aggregation_instances: Option<ResolvedTable<'a>>,
    height_delta: Option<ResolvedTable<'a>>,
    width_delta: Option<ResolvedTable<'a>>,
    export_run: Option<ResolvedTable<'a>>,
    strip_delta: Option<ResolvedTable<'a>>,
    first_s: Option<ResolvedTable<'a>>,
    subsequent_s: Option<ResolvedTable<'a>>,
    refinement_width: Option<ResolvedTable<'a>>,
    refinement_height: Option<ResolvedTable<'a>>,
    refinement_x: Option<ResolvedTable<'a>>,
    refinement_y: Option<ResolvedTable<'a>>,
    refinement_size: Option<ResolvedTable<'a>>,
    dictionary_bitmap_size: Option<ResolvedTable<'a>>,
}

impl<'a> HuffmanFieldTables<'a> {
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

    /// Builds the table set for a Huffman symbol dictionary's own fields
    /// (height/width/bitmap-size/aggregation-instances/export-run).
    /// Refinement fields, when relevant, use T.88's hardcoded standard tables
    /// (O for X/Y, A for the declared bitmap size) because the symbol
    /// dictionary header has no selector bits for them.
    fn for_dictionary(
        config: &SymbolDictionaryConfig,
        bindings: &[TableBinding<'a>],
        limits: &Limits,
    ) -> Result<Self, SymbolTextError> {
        let mut tables = Self::empty();
        tables.height_delta = Some(resolve_bound_table(
            find_table_binding(bindings, TableParameter::DictionaryHeight).ok_or(
                SymbolTextError::InvalidTableAssociation("missing height table"),
            )?,
            limits,
        )?);
        tables.width_delta = Some(resolve_bound_table(
            find_table_binding(bindings, TableParameter::DictionaryWidth).ok_or(
                SymbolTextError::InvalidTableAssociation("missing width table"),
            )?,
            limits,
        )?);
        tables.export_run = Some(owned_standard_table(StandardHuffmanTable::A, limits)?);
        tables.dictionary_bitmap_size = Some(resolve_bound_table(
            find_table_binding(bindings, TableParameter::DictionaryBitmapSize).ok_or(
                SymbolTextError::InvalidTableAssociation("missing bitmap-size table"),
            )?,
            limits,
        )?);
        if config.refinement_aggregation {
            tables.aggregation_instances = Some(resolve_bound_table(
                find_table_binding(bindings, TableParameter::DictionaryAggregationInstances)
                    .ok_or(SymbolTextError::InvalidTableAssociation(
                        "missing aggregation-instances table",
                    ))?,
                limits,
            )?);
            tables.refinement_x = Some(owned_standard_table(StandardHuffmanTable::O, limits)?);
            tables.refinement_y = Some(owned_standard_table(StandardHuffmanTable::O, limits)?);
            tables.refinement_size = Some(owned_standard_table(StandardHuffmanTable::A, limits)?);
        }
        Ok(tables)
    }

    /// Builds the hardcoded table set used while decoding a nested aggregate
    /// text region from within a Huffman symbol dictionary (T.88 6.5.8.2,
    /// Table 17): FS/DS/DT/RDW/RDH use the first of their normal selector
    /// choices because the symbol dictionary header carries no selector bits
    /// for them, and RDX/RDY/RSIZE match [`Self::for_dictionary`].
    fn for_nested_aggregate(limits: &Limits) -> Result<Self, SymbolTextError> {
        Ok(Self {
            aggregation_instances: None,
            height_delta: None,
            width_delta: None,
            export_run: None,
            strip_delta: Some(owned_standard_table(StandardHuffmanTable::K, limits)?),
            first_s: Some(owned_standard_table(StandardHuffmanTable::F, limits)?),
            subsequent_s: Some(owned_standard_table(StandardHuffmanTable::H, limits)?),
            refinement_width: Some(owned_standard_table(StandardHuffmanTable::O, limits)?),
            refinement_height: Some(owned_standard_table(StandardHuffmanTable::O, limits)?),
            refinement_x: Some(owned_standard_table(StandardHuffmanTable::O, limits)?),
            refinement_y: Some(owned_standard_table(StandardHuffmanTable::O, limits)?),
            refinement_size: Some(owned_standard_table(StandardHuffmanTable::A, limits)?),
            dictionary_bitmap_size: None,
        })
    }

    /// Builds the table set for a top-level Huffman text-region segment.
    fn for_text_region(
        bindings: &[TableBinding<'a>],
        refinement: bool,
        limits: &Limits,
    ) -> Result<Self, SymbolTextError> {
        let mut tables = Self::empty();
        tables.first_s = Some(resolve_bound_table(
            find_table_binding(bindings, TableParameter::TextFirstS)
                .ok_or(SymbolTextError::InvalidTableAssociation("missing FS table"))?,
            limits,
        )?);
        tables.subsequent_s = Some(resolve_bound_table(
            find_table_binding(bindings, TableParameter::TextSubsequentS)
                .ok_or(SymbolTextError::InvalidTableAssociation("missing DS table"))?,
            limits,
        )?);
        tables.strip_delta = Some(resolve_bound_table(
            find_table_binding(bindings, TableParameter::TextStripDelta)
                .ok_or(SymbolTextError::InvalidTableAssociation("missing DT table"))?,
            limits,
        )?);
        if refinement {
            tables.refinement_width = Some(resolve_bound_table(
                find_table_binding(bindings, TableParameter::TextRefinementWidth).ok_or(
                    SymbolTextError::InvalidTableAssociation("missing RDW table"),
                )?,
                limits,
            )?);
            tables.refinement_height = Some(resolve_bound_table(
                find_table_binding(bindings, TableParameter::TextRefinementHeight).ok_or(
                    SymbolTextError::InvalidTableAssociation("missing RDH table"),
                )?,
                limits,
            )?);
            tables.refinement_x = Some(resolve_bound_table(
                find_table_binding(bindings, TableParameter::TextRefinementX).ok_or(
                    SymbolTextError::InvalidTableAssociation("missing RDX table"),
                )?,
                limits,
            )?);
            tables.refinement_y = Some(resolve_bound_table(
                find_table_binding(bindings, TableParameter::TextRefinementY).ok_or(
                    SymbolTextError::InvalidTableAssociation("missing RDY table"),
                )?,
                limits,
            )?);
            tables.refinement_size = Some(resolve_bound_table(
                find_table_binding(bindings, TableParameter::TextRefinementSize).ok_or(
                    SymbolTextError::InvalidTableAssociation("missing RSIZE table"),
                )?,
                limits,
            )?);
        }
        Ok(tables)
    }

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
        resolved.map(ResolvedTable::table)
    }
}

// ---------------------------------------------------------------------------
// Arithmetic entropy adapter.
//
// `ArithmeticEntropy` and `ArithmeticRefinement` each borrow the same
// continuous `ArithmeticDecoder` through a shared `RefCell`. The borrows are
// always scoped to a single decode call and never overlap in time, so the
// interior mutability never panics; it exists only so the two independent
// roles (integer/IAID decoding and generic-refinement decoding) can be two
// distinct Rust values while still driving one caller-owned bitstream
// cursor, exactly as `decode_refined_bitmap`'s two type parameters require.
// ---------------------------------------------------------------------------

/// Per-[`IntegerField`] arithmetic probability contexts.
#[cfg_attr(verus_keep_ghost, verus_verify)]
struct IntegerContexts {
    slots: [[ArithmeticContext; 512]; INTEGER_FIELD_COUNT],
}

impl IntegerContexts {
    fn new() -> Self {
        Self {
            slots: [[ArithmeticContext::new(); 512]; INTEGER_FIELD_COUNT],
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn slot_mut(&mut self, field: IntegerField) -> &mut [ArithmeticContext; 512] {
        &mut self.slots[field.slot()]
    }
}

/// Decodes Annex A integers and IAID symbol IDs from one shared, caller-owned
/// [`ArithmeticDecoder`].
struct ArithmeticEntropy<'a, 'b> {
    decoder: &'b RefCell<ArithmeticDecoder<'a>>,
    integers: IntegerContexts,
    iaid: Vec<ArithmeticContext>,
}

impl<'a, 'b> ArithmeticEntropy<'a, 'b> {
    fn new(
        decoder: &'b RefCell<ArithmeticDecoder<'a>>,
        iaid_code_length: u8,
        limits: &Limits,
    ) -> Result<Self, SymbolTextError> {
        let count = 1_usize
            .checked_shl(u32::from(iaid_code_length))
            .ok_or(MalformedError::IntegerOverflow)?;
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
        iaid.try_reserve_exact(count)
            .map_err(|_| DecodeError::Allocation { requested_bytes })?;
        iaid.resize(count, ArithmeticContext::new());
        Ok(Self {
            decoder,
            integers: IntegerContexts::new(),
            iaid,
        })
    }
}

impl EntropyDecoder for ArithmeticEntropy<'_, '_> {
    fn decode_integer(&mut self, field: IntegerField) -> Result<EntropyInteger, SymbolTextError> {
        let mut decoder = self.decoder.borrow_mut();
        let slot = self.integers.slot_mut(field);
        match decoder.decode_integer(slot)? {
            ArithmeticInteger::Value(value) => Ok(EntropyInteger::Value(value)),
            ArithmeticInteger::OutOfBand => Ok(EntropyInteger::OutOfBand),
        }
    }

    fn decode_symbol_id(&mut self, request: SymbolIdRequest) -> Result<u32, SymbolTextError> {
        let mut decoder = self.decoder.borrow_mut();
        Ok(decoder.decode_iaid(request.code_length, &mut self.iaid)?)
    }
}

/// Decodes generic-refinement bitmaps from the same shared, caller-owned
/// [`ArithmeticDecoder`] used by a sibling [`ArithmeticEntropy`].
struct ArithmeticRefinement<'a, 'b> {
    decoder: &'b RefCell<ArithmeticDecoder<'a>>,
    contexts: RefinementContexts,
    cumulative_work: u64,
}

impl<'a, 'b> ArithmeticRefinement<'a, 'b> {
    fn new(
        decoder: &'b RefCell<ArithmeticDecoder<'a>>,
        template: RefinementTemplate,
        limits: &Limits,
    ) -> Result<Self, SymbolTextError> {
        Ok(Self {
            decoder,
            contexts: RefinementContexts::new(template, limits)?,
            cumulative_work: 0,
        })
    }
}

impl GenericRefinementDecoder for ArithmeticRefinement<'_, '_> {
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
        let mut decoder = self.decoder.borrow_mut();
        Ok(decode_refinement_on(
            &mut decoder,
            &mut self.contexts,
            call,
            request.reference,
            parameters,
            limits,
            &mut self.cumulative_work,
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

fn normalized_bounded_arithmetic<'a>(
    data: &'a [u8],
    limits: &Limits,
) -> Result<Cow<'a, [u8]>, SymbolTextError> {
    if data.ends_with(&BOUNDED_ARITHMETIC_TERMINATOR) {
        validate_bounded_arithmetic_terminator(data)?;
        return Ok(Cow::Borrowed(data));
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
        .try_reserve_exact(
            data.len()
                .checked_add(1)
                .ok_or(MalformedError::IntegerOverflow)?,
        )
        .map_err(|_| DecodeError::Allocation { requested_bytes })?;
    normalized.extend_from_slice(data);
    normalized.push(BOUNDED_ARITHMETIC_TERMINATOR[1]);
    validate_bounded_arithmetic_terminator(&normalized)?;
    Ok(Cow::Owned(normalized))
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

/// Builds the T.88 7.4.3.1.7 symbol-ID Huffman table from the current bit
/// position and leaves the reader byte-aligned immediately afterward.
///
/// This reuses the crate's existing run-length expansion
/// ([`expand_symbol_code_lengths`]) and canonical-table construction
/// ([`build_symbol_id_table`]) helpers; only the normative run-code
/// bitstream syntax is decoded here.
fn build_symbol_id_runcode_table(
    reader: &mut BitReader<'_>,
    symbol_count: u32,
    limits: &Limits,
) -> Result<HuffmanTable, SymbolTextError> {
    const RUNCODE_COUNT: usize = 35;
    let mut runcode_lines = Vec::new();
    let requested_bytes = RUNCODE_COUNT
        .checked_mul(core::mem::size_of::<HuffmanLine>())
        .and_then(|bytes| u64::try_from(bytes).ok())
        .ok_or(MalformedError::IntegerOverflow)?;
    runcode_lines
        .try_reserve_exact(RUNCODE_COUNT)
        .map_err(|_| DecodeError::Allocation { requested_bytes })?;
    for index in 0..RUNCODE_COUNT {
        let prefix_len =
            u8::try_from(reader.read_bits(4)?).map_err(|_| MalformedError::IntegerOverflow)?;
        let value = i32::try_from(index).map_err(|_| MalformedError::IntegerOverflow)?;
        runcode_lines.push(HuffmanLine::normal(value, prefix_len, 0));
    }
    let runcode_table = HuffmanTable::new(&runcode_lines, limits)?;

    let capacity = usize::try_from(symbol_count).map_err(|_| MalformedError::IntegerOverflow)?;
    let mut runs = Vec::new();
    runs.try_reserve_exact(capacity)
        .map_err(|_| DecodeError::Allocation {
            requested_bytes: u64::from(symbol_count),
        })?;
    let mut decoded_estimate = 0_u32;
    while decoded_estimate < symbol_count {
        let code = match runcode_table.decode(reader)? {
            HuffmanValue::Integer(value) => value,
            HuffmanValue::OutOfBand => {
                return Err(SymbolTextError::UnexpectedOutOfBand("symbol ID run code"))
            }
        };
        let run = match code {
            0..=31 => {
                let length = u8::try_from(code).map_err(|_| MalformedError::IntegerOverflow)?;
                decoded_estimate = decoded_estimate
                    .checked_add(1)
                    .ok_or(MalformedError::IntegerOverflow)?;
                SymbolLengthRun::Literal(length)
            }
            32 => {
                let count = u8::try_from(
                    reader
                        .read_bits(2)?
                        .checked_add(3)
                        .ok_or(MalformedError::IntegerOverflow)?,
                )
                .map_err(|_| MalformedError::IntegerOverflow)?;
                decoded_estimate = decoded_estimate
                    .checked_add(u32::from(count))
                    .ok_or(MalformedError::IntegerOverflow)?;
                SymbolLengthRun::RepeatPrevious(count)
            }
            33 => {
                let count = u16::try_from(
                    reader
                        .read_bits(3)?
                        .checked_add(3)
                        .ok_or(MalformedError::IntegerOverflow)?,
                )
                .map_err(|_| MalformedError::IntegerOverflow)?;
                decoded_estimate = decoded_estimate
                    .checked_add(u32::from(count))
                    .ok_or(MalformedError::IntegerOverflow)?;
                SymbolLengthRun::Zeros(count)
            }
            34 => {
                let count = u16::try_from(
                    reader
                        .read_bits(7)?
                        .checked_add(11)
                        .ok_or(MalformedError::IntegerOverflow)?,
                )
                .map_err(|_| MalformedError::IntegerOverflow)?;
                decoded_estimate = decoded_estimate
                    .checked_add(u32::from(count))
                    .ok_or(MalformedError::IntegerOverflow)?;
                SymbolLengthRun::Zeros(count)
            }
            _ => return Err(SymbolTextError::InvalidSymbolLengthRun),
        };
        if runs.len() >= capacity {
            return Err(SymbolTextError::InvalidSymbolLengthRun);
        }
        runs.push(run);
    }

    let mut resources = ResourceTracker::default();
    let lengths = expand_symbol_code_lengths(symbol_count, runs, limits, &mut resources)?;
    let table = build_symbol_id_table(&lengths, limits)?;
    byte_align(reader)?;
    Ok(table)
}

/// Decodes Annex B integers and the text-region symbol-ID table from a
/// shared, caller-owned bit reader.
struct HuffmanEntropy<'a, 'b> {
    reader: &'b RefCell<BitReader<'a>>,
    tables: HuffmanFieldTables<'a>,
    log_strip_size: u8,
    symbol_id_table: Option<HuffmanTable>,
}

impl<'a, 'b> HuffmanEntropy<'a, 'b> {
    fn new(
        reader: &'b RefCell<BitReader<'a>>,
        tables: HuffmanFieldTables<'a>,
        log_strip_size: u8,
    ) -> Self {
        Self {
            reader,
            tables,
            log_strip_size,
            symbol_id_table: None,
        }
    }

    fn read_raw_bits(&self, count: u8) -> Result<u32, SymbolTextError> {
        let mut reader = self.reader.borrow_mut();
        Ok(reader.read_bits(count)?)
    }
}

impl EntropyDecoder for HuffmanEntropy<'_, '_> {
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
                let mut reader = self.reader.borrow_mut();
                match table.decode(&mut reader)? {
                    HuffmanValue::Integer(value) => Ok(EntropyInteger::Value(i64::from(value))),
                    HuffmanValue::OutOfBand => Ok(EntropyInteger::OutOfBand),
                }
            }
        }
    }

    fn decode_symbol_id(&mut self, request: SymbolIdRequest) -> Result<u32, SymbolTextError> {
        // When a symbol-ID Huffman table has been prepared (T.88 7.4.3.1.7,
        // normal text-region decoding), decode through it. Otherwise this is
        // the symbol dictionary's own single-instance aggregate refinement ID
        // (T.88 6.5.8.2.2), which T.88 specifies as a literal fixed-width
        // field of `code_length` bits, not a Huffman code.
        if let Some(table) = self.symbol_id_table.as_ref() {
            let mut reader = self.reader.borrow_mut();
            match table.decode(&mut reader)? {
                HuffmanValue::Integer(value) => u32::try_from(value)
                    .map_err(|_| SymbolTextError::InvalidEntropyValue("symbol ID")),
                HuffmanValue::OutOfBand => Err(SymbolTextError::UnexpectedOutOfBand("symbol ID")),
            }
        } else {
            self.read_raw_bits(request.code_length)
        }
    }

    fn prepare_symbol_ids(
        &mut self,
        symbol_count: u32,
        limits: &Limits,
    ) -> Result<(), SymbolTextError> {
        let mut reader = self.reader.borrow_mut();
        self.symbol_id_table = Some(build_symbol_id_runcode_table(
            &mut reader,
            symbol_count,
            limits,
        )?);
        Ok(())
    }
}

/// Decodes generic-refinement bitmaps embedded in a Huffman-coded bitstream.
///
/// Per T.88 6.5.8.2.2 and 6.4.11, a Huffman-declared refinement bitmap is
/// always arithmetically coded using a *fresh* arithmetic decoder bounded to
/// exactly the declared byte count, located at the next byte boundary after
/// the preceding Huffman-coded fields. The persistent `GR` probability
/// contexts, however, are shared across every refinement bitmap decoded in
/// one segment, matching the generic-refinement statistics lifetime used by
/// the fully arithmetic path.
struct HuffmanRefinement<'a, 'b> {
    reader: &'b RefCell<BitReader<'a>>,
    contexts: RefinementContexts,
    cumulative_work: u64,
}

impl<'a, 'b> HuffmanRefinement<'a, 'b> {
    fn new(
        reader: &'b RefCell<BitReader<'a>>,
        template: RefinementTemplate,
        limits: &Limits,
    ) -> Result<Self, SymbolTextError> {
        Ok(Self {
            reader,
            contexts: RefinementContexts::new(template, limits)?,
            cumulative_work: 0,
        })
    }
}

impl GenericRefinementDecoder for HuffmanRefinement<'_, '_> {
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
        let mut reader = self.reader.borrow_mut();
        byte_align(&mut reader)?;
        let byte_offset = reader
            .bit_position
            .checked_div(8)
            .ok_or(MalformedError::IntegerOverflow)?;
        let size = usize::try_from(declared_size).map_err(|_| MalformedError::IntegerOverflow)?;
        let end = byte_offset
            .checked_add(size)
            .ok_or(MalformedError::IntegerOverflow)?;
        let remaining = if byte_offset <= reader.bytes.len() {
            reader.bytes.len() - byte_offset
        } else {
            0
        };
        let slice = reader
            .bytes
            .get(byte_offset..end)
            .ok_or(MalformedError::UnexpectedEof {
                offset: u64::try_from(byte_offset).map_err(|_| MalformedError::IntegerOverflow)?,
                needed: declared_size.into(),
                remaining: u64::try_from(remaining).map_err(|_| MalformedError::IntegerOverflow)?,
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
        let mut decoder = ArithmeticDecoder::new(normalized.as_ref())?;
        let bitmap = decode_refinement_on(
            &mut decoder,
            &mut self.contexts,
            call,
            request.reference,
            parameters,
            limits,
            &mut self.cumulative_work,
        )?;
        validate_bounded_arithmetic_marker(&decoder, normalized.len())?;

        let advanced_bits = end.checked_mul(8).ok_or(MalformedError::IntegerOverflow)?;
        reader.bit_position = advanced_bits;
        Ok(bitmap)
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
    let length_usize = usize::try_from(length).map_err(|_| MalformedError::IntegerOverflow)?;
    let source = bytes
        .get(..length_usize)
        .ok_or(MalformedError::UnexpectedEof {
            offset: 0,
            needed: length,
            remaining: u64::try_from(bytes.len()).map_err(|_| MalformedError::IntegerOverflow)?,
        })?;
    if width % 8 != 0 {
        let unused = 8 - (width % 8);
        let padding_mask =
            u8::try_from((1_u16 << unused) - 1).map_err(|_| MalformedError::IntegerOverflow)?;
        let stride_usize = usize::try_from(stride).map_err(|_| MalformedError::IntegerOverflow)?;
        for row in 0..height {
            let final_byte = usize::try_from(row)
                .map_err(|_| MalformedError::IntegerOverflow)?
                .checked_mul(stride_usize)
                .and_then(|offset| offset.checked_add(stride_usize.checked_sub(1)?))
                .ok_or(MalformedError::IntegerOverflow)?;
            if source
                .get(final_byte)
                .is_none_or(|byte| byte & padding_mask != 0)
            {
                return Err(SymbolTextError::InvalidState(
                    "raw collective bitmap has nonzero padding",
                ));
            }
        }
    }
    let mut data = Vec::new();
    data.try_reserve_exact(length_usize)
        .map_err(|_| DecodeError::Allocation {
            requested_bytes: length,
        })?;
    data.extend_from_slice(source);
    Ok(Bitmap {
        width,
        height,
        stride: usize::try_from(stride).map_err(|_| MalformedError::IntegerOverflow)?,
        data,
    })
}

/// Crops a fixed-height column range out of a wider collective bitmap, used
/// to split T.88 6.5.9 height-class collective bitmaps into their individual
/// symbols.
fn crop_bitmap_columns(
    source: &Bitmap,
    x_start: u32,
    width: u32,
    limits: &Limits,
) -> Result<Bitmap, SymbolTextError> {
    let height = source.height();
    let mut cropped = Bitmap::blank(width, height, limits)?;
    for y in 0..height {
        for x in 0..width {
            let column = x_start
                .checked_add(x)
                .ok_or(MalformedError::IntegerOverflow)?;
            let value = source
                .pixel(column, y)
                .ok_or(MalformedError::IntegerOverflow)?;
            cropped.set_pixel(x, y, value)?;
        }
    }
    Ok(cropped)
}

/// Deep-copies a bitmap's packed bytes under fallible allocation.
///
/// [`Bitmap`] intentionally has no infallible [`Clone`]; exported symbols
/// must still become independently owned bitmaps, so this performs the copy
/// explicitly through its crate-visible fields.
fn clone_bitmap(source: &Bitmap, limits: &Limits) -> Result<Bitmap, SymbolTextError> {
    let requested_bytes =
        u64::try_from(source.data.len()).map_err(|_| MalformedError::IntegerOverflow)?;
    enforce_limit(
        LimitKind::Memory,
        limits.max_decoder_managed_memory_bytes,
        requested_bytes,
    )?;
    let mut data = Vec::new();
    data.try_reserve_exact(source.data.len())
        .map_err(|_| DecodeError::Allocation { requested_bytes })?;
    data.extend_from_slice(&source.data);
    Ok(Bitmap {
        width: source.width,
        height: source.height,
        stride: source.stride,
        data,
    })
}

/// Looks up a symbol by ID across the imported and already-decoded-new
/// symbol collections, as used by T.88 6.5.8.2.2's single-instance
/// refinement/aggregate procedure.
#[cfg_attr(verus_keep_ghost, verus_verify)]
fn symbol_by_id<'a>(
    imported: &'a [Bitmap],
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
        Ok(&imported[id_usize])
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
fn wrap_custom_tables<'a>(
    custom_tables: &'a [&'a HuffmanTable],
) -> Result<Vec<CustomHuffmanTable<'a>>, SymbolTextError> {
    let mut wrapped = Vec::new();
    let requested_bytes = custom_tables
        .len()
        .checked_mul(core::mem::size_of::<CustomHuffmanTable<'_>>())
        .and_then(|bytes| u64::try_from(bytes).ok())
        .ok_or(MalformedError::IntegerOverflow)?;
    wrapped
        .try_reserve_exact(custom_tables.len())
        .map_err(|_| DecodeError::Allocation { requested_bytes })?;
    for &table in custom_tables {
        wrapped.push(CustomHuffmanTable {
            table,
            has_oob: table.has_oob(),
        });
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
#[allow(clippy::too_many_arguments)]
fn decode_dictionary_single_instance_refinement<E, R>(
    entropy: &mut E,
    refinement: &mut R,
    imported: &[Bitmap],
    new_symbols: &[Bitmap],
    code_length: u8,
    width: u32,
    height: u32,
    refinement_template: u8,
    refinement_adaptive: [(i8, i8); 2],
    huffman: bool,
    limits: &Limits,
    resources: &mut ResourceTracker,
) -> Result<Bitmap, SymbolTextError>
where
    E: EntropyDecoder,
    R: GenericRefinementDecoder,
{
    let available = imported
        .len()
        .checked_add(new_symbols.len())
        .and_then(|count| u32::try_from(count).ok())
        .ok_or(MalformedError::IntegerOverflow)?;
    let id = decode_checked_symbol_id(
        entropy,
        SymbolIdRequest {
            available_symbols: available,
            code_length,
            huffman,
        },
    )?;
    let reference_x = required_integer(entropy, IntegerField::RefinementX)?;
    let reference_y = required_integer(entropy, IntegerField::RefinementY)?;
    let declared_size = if huffman {
        let size = required_integer(entropy, IntegerField::RefinementSize)?;
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
    let bitmap = refinement.decode_refinement(
        RefinementRequest {
            reference,
            geometry,
            template: refinement_template,
            adaptive_pixels: refinement_adaptive,
            declared_data_size: declared_size,
        },
        limits,
    )?;
    if bitmap.width() != width || bitmap.height() != height {
        return Err(SymbolTextError::InvalidDimensions);
    }
    Ok(bitmap)
}

/// Decodes one complete text-region bitmap (T.88 6.4), shared by top-level
/// text-region segments and the nested multiple-instance aggregate
/// procedure invoked from a symbol dictionary (T.88 6.5.8.2.1).
///
/// `code_length` and `table_symbol_count` are the *fixed* IAID bit width and
/// Huffman symbol-ID table size for the whole enclosing segment (the
/// declared total symbol count); `available_symbols` is the (possibly
/// smaller, for a nested aggregate call still in progress) slice used both
/// to validate decoded IDs and to look up referenced bitmaps.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn decode_text_region_body<E, R>(
    entropy: &mut E,
    refinement: &mut R,
    available_symbols: &[&Bitmap],
    code_length: u8,
    table_symbol_count: u32,
    width: u32,
    height: u32,
    number_of_instances: u32,
    config: &TextRegionConfig,
    refinement_adaptive: [(i8, i8); 2],
    limits: &Limits,
    resources: &mut ResourceTracker,
) -> Result<Bitmap, SymbolTextError>
where
    E: EntropyDecoder,
    R: GenericRefinementDecoder,
{
    entropy.prepare_symbol_ids(table_symbol_count, limits)?;
    resources.charge_bitmap(width, height, limits)?;
    let mut bitmap = Bitmap::filled(width, height, config.default_pixel, limits)?;

    let initial_strip = required_integer(entropy, IntegerField::StripDelta)?;
    let mut state = TextRegionState::new(*config, number_of_instances, initial_strip, limits)?;

    let available_count =
        u32::try_from(available_symbols.len()).map_err(|_| MalformedError::IntegerOverflow)?;

    let mut decoded = 0_u32;
    while decoded < number_of_instances {
        let strip_delta = required_integer(entropy, IntegerField::StripDelta)?;
        let first_s_delta = required_integer(entropy, IntegerField::FirstS)?;
        state.begin_strip(strip_delta, first_s_delta)?;
        let mut first = true;
        loop {
            if decoded >= number_of_instances {
                break;
            }
            let delta_s = if first {
                None
            } else {
                match entropy.decode_integer(IntegerField::SubsequentS)? {
                    EntropyInteger::Value(value) => Some(value),
                    EntropyInteger::OutOfBand => break,
                }
            };
            let within_strip_t = if config.strip_size > 1 {
                match entropy.decode_integer(IntegerField::WithinStripT)? {
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
                entropy,
                SymbolIdRequest {
                    available_symbols: available_count,
                    code_length,
                    huffman: config.huffman,
                },
            )?;
            let base_symbol = *available_symbols
                .get(usize::try_from(symbol_id).map_err(|_| MalformedError::IntegerOverflow)?)
                .ok_or(SymbolTextError::SymbolIdOutOfRange {
                    id: symbol_id,
                    available: available_count,
                })?;

            let refined_storage;
            let placed_bitmap: &Bitmap = if config.refinement {
                let indicator = match entropy.decode_integer(IntegerField::RefinementIndicator)? {
                    EntropyInteger::Value(value) => value,
                    EntropyInteger::OutOfBand => {
                        return Err(SymbolTextError::UnexpectedOutOfBand("refinement indicator"))
                    }
                };
                if indicator != 0 {
                    refined_storage = Some(decode_refined_bitmap(
                        entropy,
                        refinement,
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
            compose_placement(
                &mut bitmap,
                placed_bitmap,
                placement,
                config.operator,
                limits,
                resources,
            )?;

            decoded = decoded
                .checked_add(1)
                .ok_or(MalformedError::IntegerOverflow)?;
            first = false;
        }
        state.end_strip()?;
    }
    state.finish()?;
    Ok(bitmap)
}

// ---------------------------------------------------------------------------
// Symbol-dictionary payload decoding (T.88 6.5).
// ---------------------------------------------------------------------------

/// Caller-retained generic/refinement probability state, carried across
/// symbol dictionaries that cooperatively share one evolving symbol set
/// (T.88 6.5.8.2.3's "bitmap coding context used"/"retained" flags).
pub(crate) struct DictionaryContexts {
    pub(crate) generic: Option<GenericContextSnapshot>,
    pub(crate) refinement: Option<RefinementContextSnapshot>,
}

/// Complete input to one symbol-dictionary segment payload decode.
pub(crate) struct SymbolDictionaryInput<'a> {
    pub(crate) payload: &'a [u8],
    pub(crate) imported_symbols: &'a [Bitmap],
    pub(crate) custom_tables: &'a [&'a HuffmanTable],
    pub(crate) used_contexts: Option<&'a DictionaryContexts>,
    pub(crate) limits: &'a Limits,
}

/// Complete output of one symbol-dictionary segment payload decode.
pub(crate) struct SymbolDictionaryOutput {
    pub(crate) exported_symbols: Vec<Bitmap>,
    pub(crate) retained_contexts: Option<DictionaryContexts>,
    pub(crate) consumed_bytes: usize,
}

#[allow(clippy::too_many_arguments)]
fn decode_one_new_symbol<E, R>(
    header: &SymbolDictionaryHeader,
    entropy: &mut E,
    refinement: Option<&mut R>,
    imported: &[Bitmap],
    new_symbols: &[Bitmap],
    generic_contexts: Option<&mut GenericContexts>,
    generic_decoder: Option<&RefCell<ArithmeticDecoder<'_>>>,
    generic_work: &mut u64,
    width: u32,
    height: u32,
    code_length: u8,
    total_symbols: u32,
    refinement_adaptive: [(i8, i8); 2],
    limits: &Limits,
    resources: &mut ResourceTracker,
) -> Result<Bitmap, SymbolTextError>
where
    E: EntropyDecoder,
    R: GenericRefinementDecoder,
{
    let config = header.config;
    if config.refinement_aggregation {
        let refinement = refinement.ok_or(SymbolTextError::InvalidState(
            "missing refinement adapter for an aggregate dictionary",
        ))?;
        let instances = required_integer(entropy, IntegerField::AggregationInstances)?;
        let instances = u32::try_from(instances)
            .map_err(|_| SymbolTextError::InvalidEntropyValue("aggregation instances"))?;
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
                .try_reserve_exact(capacity)
                .map_err(|_| DecodeError::Allocation { requested_bytes })?;
            symbol_refs.extend(imported.iter());
            symbol_refs.extend(new_symbols.iter());
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
            decode_text_region_body(
                entropy,
                refinement,
                &symbol_refs,
                code_length,
                total_symbols,
                width,
                height,
                instances,
                &nested_config,
                refinement_adaptive,
                limits,
                resources,
            )
        } else {
            decode_dictionary_single_instance_refinement(
                entropy,
                refinement,
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
        let decoder_cell = generic_decoder.ok_or(SymbolTextError::InvalidState(
            "missing arithmetic decoder for a direct-coded dictionary",
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
        let mut decoder = decoder_cell.borrow_mut();
        Ok(decode_generic_on(
            &mut decoder,
            contexts,
            call,
            height,
            limits,
            generic_work,
        )?)
    }
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
fn decode_symbol_dictionary_arithmetic(
    header: &SymbolDictionaryHeader,
    imported: &[Bitmap],
    body: &[u8],
    code_length: u8,
    total_symbols: u32,
    refinement_adaptive: [(i8, i8); 2],
    used_contexts: Option<&DictionaryContexts>,
    limits: &Limits,
) -> ArithmeticDictionaryResult {
    let config = header.config;
    let normalized = normalized_bounded_arithmetic(body, limits)?;
    let decoder_cell = RefCell::new(ArithmeticDecoder::new(normalized.as_ref())?);
    let mut entropy = ArithmeticEntropy::new(&decoder_cell, code_length, limits)?;
    let mut resources = ResourceTracker::default();

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

    let mut refinement = if config.refinement_aggregation {
        let mut refine = ArithmeticRefinement::new(
            &decoder_cell,
            refinement_template_from_u8(config.refinement_template),
            limits,
        )?;
        if let Some(used) = used_contexts.and_then(|used| used.refinement.as_ref()) {
            refine.contexts.restore(used, limits)?;
        }
        Some(refine)
    } else {
        None
    };

    let new_symbol_capacity =
        usize::try_from(header.new_symbols).map_err(|_| MalformedError::IntegerOverflow)?;
    let mut new_symbols: Vec<Bitmap> = Vec::new();
    new_symbols
        .try_reserve_exact(new_symbol_capacity)
        .map_err(|_| DecodeError::Allocation {
            requested_bytes: u64::from(header.new_symbols),
        })?;

    let mut class_state = DictionaryClassState::new(header.new_symbols, limits)?;
    while u32::try_from(new_symbols.len()).map_err(|_| MalformedError::IntegerOverflow)?
        < header.new_symbols
    {
        let height_delta = required_integer(&mut entropy, IntegerField::HeightDelta)?;
        class_state.begin_height_class(height_delta, limits, &mut resources)?;
        while let EntropyInteger::Value(width_delta) =
            entropy.decode_integer(IntegerField::WidthDelta)?
        {
            let (width, height) = class_state.add_symbol(width_delta, limits, &mut resources)?;
            let bitmap = decode_one_new_symbol(
                header,
                &mut entropy,
                refinement.as_mut(),
                imported,
                &new_symbols,
                generic_contexts.as_mut(),
                Some(&decoder_cell),
                &mut generic_work,
                width,
                height,
                code_length,
                total_symbols,
                refinement_adaptive,
                limits,
                &mut resources,
            )?;
            new_symbols.push(bitmap);
        }
        class_state.finish_height_class()?;
    }
    class_state.finish()?;

    let exported = decode_export_runs(
        total_symbols,
        header.exported_symbols,
        limits,
        &mut resources,
        || entropy.decode_integer(IntegerField::ExportRun),
    )?;

    {
        let decoder = decoder_cell.borrow();
        validate_bounded_arithmetic_marker(&decoder, normalized.len())?;
    }

    let retained_generic = generic_contexts
        .as_ref()
        .map(|contexts| contexts.try_snapshot(limits))
        .transpose()?;
    let retained_refinement = refinement
        .as_ref()
        .map(|refine| refine.contexts.try_snapshot(limits))
        .transpose()?;

    let exported_symbols = assemble_exported_symbols(imported, &new_symbols, &exported, limits)?;
    Ok((
        exported_symbols,
        retained_generic,
        retained_refinement,
        body.len(),
    ))
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn decode_symbol_dictionary_huffman(
    header: &SymbolDictionaryHeader,
    imported: &[Bitmap],
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
    let tables = HuffmanFieldTables::for_dictionary(&config, &bindings, limits)?;
    let reader_cell = RefCell::new(bit_reader(body));
    let mut entropy = HuffmanEntropy::new(&reader_cell, tables, 0);
    let mut resources = ResourceTracker::default();

    let mut refinement = if config.refinement_aggregation {
        Some(HuffmanRefinement::new(
            &reader_cell,
            refinement_template_from_u8(config.refinement_template),
            limits,
        )?)
    } else {
        None
    };

    let new_symbol_capacity =
        usize::try_from(header.new_symbols).map_err(|_| MalformedError::IntegerOverflow)?;
    let mut new_symbols: Vec<Bitmap> = Vec::new();
    new_symbols
        .try_reserve_exact(new_symbol_capacity)
        .map_err(|_| DecodeError::Allocation {
            requested_bytes: u64::from(header.new_symbols),
        })?;

    let mut class_state = DictionaryClassState::new(header.new_symbols, limits)?;
    while u32::try_from(new_symbols.len()).map_err(|_| MalformedError::IntegerOverflow)?
        < header.new_symbols
    {
        let height_delta = required_integer(&mut entropy, IntegerField::HeightDelta)?;
        let height = class_state.begin_height_class(height_delta, limits, &mut resources)?;

        if config.refinement_aggregation {
            while let EntropyInteger::Value(width_delta) =
                entropy.decode_integer(IntegerField::WidthDelta)?
            {
                let (width, height) =
                    class_state.add_symbol(width_delta, limits, &mut resources)?;
                let bitmap = decode_one_new_symbol(
                    header,
                    &mut entropy,
                    refinement.as_mut(),
                    imported,
                    &new_symbols,
                    None,
                    None,
                    &mut 0,
                    width,
                    height,
                    code_length,
                    total_symbols,
                    refinement_adaptive,
                    limits,
                    &mut resources,
                )?;
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
                .try_reserve_exact(remaining)
                .map_err(|_| DecodeError::Allocation {
                    requested_bytes: requested_width_bytes,
                })?;
            while let EntropyInteger::Value(width_delta) =
                entropy.decode_integer(IntegerField::WidthDelta)?
            {
                let (width, _) = class_state.add_symbol(width_delta, limits, &mut resources)?;
                widths.push(width);
            }
            let total_width: u32 = widths
                .iter()
                .copied()
                .try_fold(0_u32, u32::checked_add)
                .ok_or(MalformedError::IntegerOverflow)?;
            let bitmap_size = match entropy.decode_integer(IntegerField::DictionaryBitmapSize)? {
                EntropyInteger::Value(value) => u32::try_from(value)
                    .map_err(|_| SymbolTextError::InvalidEntropyValue("bitmap size"))?,
                EntropyInteger::OutOfBand => {
                    return Err(SymbolTextError::UnexpectedOutOfBand(
                        "collective bitmap size",
                    ))
                }
            };
            {
                let mut reader = reader_cell.borrow_mut();
                byte_align(&mut reader)?;
            }
            let collective = if bitmap_size == 0 {
                let mut reader = reader_cell.borrow_mut();
                let byte_offset = reader
                    .bit_position
                    .checked_div(8)
                    .ok_or(MalformedError::IntegerOverflow)?;
                let stride = u64::from(total_width)
                    .checked_add(7)
                    .ok_or(MalformedError::IntegerOverflow)?
                    / 8;
                let length = stride
                    .checked_mul(u64::from(height))
                    .ok_or(MalformedError::IntegerOverflow)?;
                let length_usize =
                    usize::try_from(length).map_err(|_| MalformedError::IntegerOverflow)?;
                let end = byte_offset
                    .checked_add(length_usize)
                    .ok_or(MalformedError::IntegerOverflow)?;
                let remaining = if byte_offset <= reader.bytes.len() {
                    reader.bytes.len() - byte_offset
                } else {
                    0
                };
                let source =
                    reader
                        .bytes
                        .get(byte_offset..end)
                        .ok_or(MalformedError::UnexpectedEof {
                            offset: u64::try_from(byte_offset)
                                .map_err(|_| MalformedError::IntegerOverflow)?,
                            needed: length,
                            remaining: u64::try_from(remaining)
                                .map_err(|_| MalformedError::IntegerOverflow)?,
                        })?;
                let bitmap = bitmap_from_packed_bytes(total_width, height, source, limits)?;
                reader.bit_position = end.checked_mul(8).ok_or(MalformedError::IntegerOverflow)?;
                bitmap
            } else {
                let mut reader = reader_cell.borrow_mut();
                let byte_offset = reader
                    .bit_position
                    .checked_div(8)
                    .ok_or(MalformedError::IntegerOverflow)?;
                let size =
                    usize::try_from(bitmap_size).map_err(|_| MalformedError::IntegerOverflow)?;
                let end = byte_offset
                    .checked_add(size)
                    .ok_or(MalformedError::IntegerOverflow)?;
                let remaining = if byte_offset <= reader.bytes.len() {
                    reader.bytes.len() - byte_offset
                } else {
                    0
                };
                let source =
                    reader
                        .bytes
                        .get(byte_offset..end)
                        .ok_or(MalformedError::UnexpectedEof {
                            offset: u64::try_from(byte_offset)
                                .map_err(|_| MalformedError::IntegerOverflow)?,
                            needed: bitmap_size.into(),
                            remaining: u64::try_from(remaining)
                                .map_err(|_| MalformedError::IntegerOverflow)?,
                        })?;
                let bitmap = decode_mmr(source, total_width, height, limits, EofbPolicy::Optional)?;
                reader.bit_position = end.checked_mul(8).ok_or(MalformedError::IntegerOverflow)?;
                bitmap
            };
            resources.charge_bitmap(total_width, height, limits)?;

            if widths.len() == 1 {
                new_symbols.push(collective);
            } else {
                let mut x_start = 0_u32;
                for width in widths {
                    let cropped = crop_bitmap_columns(&collective, x_start, width, limits)?;
                    new_symbols.push(cropped);
                    x_start = x_start
                        .checked_add(width)
                        .ok_or(MalformedError::IntegerOverflow)?;
                }
            }
        }
        class_state.finish_height_class()?;
    }
    class_state.finish()?;

    let exported = decode_export_runs(
        total_symbols,
        header.exported_symbols,
        limits,
        &mut resources,
        || entropy.decode_integer(IntegerField::ExportRun),
    )?;

    let consumed_within_body = {
        let reader = reader_cell.borrow();
        reader
            .bit_position
            .checked_add(7)
            .ok_or(MalformedError::IntegerOverflow)?
            / 8
    };
    if consumed_within_body != body.len() {
        return Err(SymbolTextError::InvalidState(
            "Huffman symbol dictionary did not exactly consume its segment data",
        ));
    }

    let retained_refinement = refinement
        .as_ref()
        .map(|refine| refine.contexts.try_snapshot(limits))
        .transpose()?;
    let exported_symbols = assemble_exported_symbols(imported, &new_symbols, &exported, limits)?;
    Ok((exported_symbols, retained_refinement, consumed_within_body))
}

fn assemble_exported_symbols(
    imported: &[Bitmap],
    new_symbols: &[Bitmap],
    exported_indices: &[u32],
    limits: &Limits,
) -> Result<Vec<Bitmap>, SymbolTextError> {
    let mut exported_symbols = Vec::new();
    let requested_bytes = exported_indices
        .len()
        .checked_mul(core::mem::size_of::<Bitmap>())
        .and_then(|bytes| u64::try_from(bytes).ok())
        .ok_or(MalformedError::IntegerOverflow)?;
    exported_symbols
        .try_reserve_exact(exported_indices.len())
        .map_err(|_| DecodeError::Allocation { requested_bytes })?;
    for &index in exported_indices {
        let source = symbol_by_id(imported, new_symbols, index)?;
        exported_symbols.push(clone_bitmap(source, limits)?);
    }
    Ok(exported_symbols)
}

/// Decodes one complete symbol-dictionary segment payload (T.88 6.5, 7.4.3).
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
pub(crate) struct TextRegionInput<'a> {
    pub(crate) payload: &'a [u8],
    pub(crate) symbols: &'a [Bitmap],
    pub(crate) custom_tables: &'a [&'a HuffmanTable],
    pub(crate) limits: &'a Limits,
}

/// Complete output of one text-region segment payload decode.
pub(crate) struct TextRegionOutput {
    pub(crate) region: RegionInfo,
    pub(crate) bitmap: Bitmap,
    pub(crate) consumed_bytes: usize,
}

/// Decodes one complete text-region segment payload (T.88 6.4, 7.4.3).
#[allow(clippy::too_many_lines)]
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

    let mut symbol_refs: Vec<&Bitmap> = Vec::new();
    let requested_bytes = input
        .symbols
        .len()
        .checked_mul(core::mem::size_of::<&Bitmap>())
        .and_then(|bytes| u64::try_from(bytes).ok())
        .ok_or(MalformedError::IntegerOverflow)?;
    symbol_refs
        .try_reserve_exact(input.symbols.len())
        .map_err(|_| DecodeError::Allocation { requested_bytes })?;
    symbol_refs.extend(input.symbols.iter());

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
    let mut resources = ResourceTracker::default();
    let refinement_template = refinement_template_from_u8(config.refinement_template);

    let (bitmap, consumed_within_body) = if config.huffman {
        let custom_wrapped = wrap_custom_tables(input.custom_tables)?;
        let bindings = associate_text_tables(&config, &custom_wrapped)?;
        let tables = HuffmanFieldTables::for_text_region(&bindings, config.refinement, limits)?;
        let reader_cell = RefCell::new(bit_reader(body));
        let log_strip_size = log2_strip_size(config.strip_size);
        let mut entropy = HuffmanEntropy::new(&reader_cell, tables, log_strip_size);
        let mut refine = HuffmanRefinement::new(&reader_cell, refinement_template, limits)?;

        let bitmap = decode_text_region_body(
            &mut entropy,
            &mut refine,
            &symbol_refs,
            code_length,
            symbol_count,
            header.region.width,
            header.region.height,
            header.number_of_instances,
            &config,
            refinement_adaptive,
            limits,
            &mut resources,
        )?;

        let consumed_within_body = {
            let reader = reader_cell.borrow();
            reader
                .bit_position
                .checked_add(7)
                .ok_or(MalformedError::IntegerOverflow)?
                / 8
        };
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
        let decoder_cell = RefCell::new(ArithmeticDecoder::new(normalized.as_ref())?);
        let mut entropy = ArithmeticEntropy::new(&decoder_cell, code_length, limits)?;
        let mut refine = ArithmeticRefinement::new(&decoder_cell, refinement_template, limits)?;

        let bitmap = decode_text_region_body(
            &mut entropy,
            &mut refine,
            &symbol_refs,
            code_length,
            symbol_count,
            header.region.width,
            header.region.height,
            header.number_of_instances,
            &config,
            refinement_adaptive,
            limits,
            &mut resources,
        )?;

        {
            let decoder = decoder_cell.borrow();
            validate_bounded_arithmetic_marker(&decoder, normalized.len())?;
        }
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
    use std::collections::VecDeque;

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
        assert_eq!(bindings[0].parameter, TableParameter::DictionaryHeight);
        assert_eq!(bindings[1].parameter, TableParameter::DictionaryWidth);

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
        let mut runs = VecDeque::from([
            EntropyInteger::Value(2),
            EntropyInteger::Value(1),
            EntropyInteger::Value(2),
        ]);
        let exported = decode_export_runs(5, 1, &limits, &mut resources, || {
            runs.pop_front()
                .ok_or(SymbolTextError::InvalidState("missing scripted run"))
        })
        .expect("valid export runs");
        assert_eq!(exported, vec![2]);

        let mut resources = ResourceTracker::default();
        let mut runs = VecDeque::from([
            EntropyInteger::Value(0),
            EntropyInteger::Value(0),
            EntropyInteger::Value(5),
        ]);
        let exported = decode_export_runs(5, 0, &limits, &mut resources, || {
            runs.pop_front()
                .ok_or(SymbolTextError::InvalidState("missing scripted run"))
        })
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
        let result = decode_export_runs(1, 0, &limits, &mut resources, || {
            Ok(EntropyInteger::Value(0))
        });
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
            [
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
                [SymbolLengthRun::RepeatPrevious(3)],
                &limits,
                &mut resources
            ),
            Err(SymbolTextError::InvalidSymbolLengthRun)
        ));
        let mut resources = ResourceTracker::default();
        assert!(matches!(
            expand_symbol_code_lengths(4, [SymbolLengthRun::Zeros(5)], &limits, &mut resources),
            Err(SymbolTextError::InvalidSymbolLengthRun)
        ));
    }

    #[test]
    fn builds_symbol_id_huffman_table_from_lengths() {
        let limits = Limits::default();
        let table = build_symbol_id_table(&[1, 3, 3], &limits);
        assert!(table.is_ok());
        assert!(build_symbol_id_table(&[1, 1, 1], &limits).is_err());
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

    struct ScriptedEntropy {
        integers: VecDeque<EntropyInteger>,
        symbol_id: u32,
    }

    impl EntropyDecoder for ScriptedEntropy {
        fn decode_integer(
            &mut self,
            _field: IntegerField,
        ) -> Result<EntropyInteger, SymbolTextError> {
            self.integers
                .pop_front()
                .ok_or(SymbolTextError::InvalidState("missing integer"))
        }

        fn decode_symbol_id(&mut self, _request: SymbolIdRequest) -> Result<u32, SymbolTextError> {
            Ok(self.symbol_id)
        }
    }

    struct BlankRefinement;

    impl GenericRefinementDecoder for BlankRefinement {
        fn decode_refinement(
            &mut self,
            request: RefinementRequest<'_>,
            limits: &Limits,
        ) -> Result<Bitmap, SymbolTextError> {
            Ok(Bitmap::blank(
                request.geometry.width,
                request.geometry.height,
                limits,
            )?)
        }
    }

    #[test]
    fn refinement_callback_receives_checked_geometry() {
        let limits = Limits::default();
        let reference = Bitmap::blank(5, 7, &limits).expect("reference");
        let mut entropy = ScriptedEntropy {
            integers: VecDeque::from([
                EntropyInteger::Value(-3),
                EntropyInteger::Value(1),
                EntropyInteger::Value(3),
                EntropyInteger::Value(-1),
                EntropyInteger::Value(4),
            ]),
            symbol_id: 0,
        };
        let mut decoder = BlankRefinement;
        let mut resources = ResourceTracker::default();
        let bitmap = decode_refined_bitmap(
            &mut entropy,
            &mut decoder,
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
        let mut entropy = ScriptedEntropy {
            integers: VecDeque::new(),
            symbol_id: 7,
        };
        let error = decode_checked_symbol_id(
            &mut entropy,
            SymbolIdRequest {
                available_symbols: 5,
                code_length: 3,
                huffman: false,
            },
        )
        .expect_err("ID 7 is outside five symbols");
        assert!(matches!(
            error,
            SymbolTextError::SymbolIdOutOfRange {
                id: 7,
                available: 5
            }
        ));
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
            imported_symbols: &[],
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
            imported_symbols: &[],
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
            symbols: &[],
            custom_tables: &custom,
            limits: &limits,
        })
        .expect("empty arithmetic text region decodes");
        assert_eq!(arithmetic_text_output.bitmap.bytes(), &[0]);
    }
}

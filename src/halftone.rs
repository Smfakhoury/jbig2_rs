use crate::bitmap::CombinationOperator;
use crate::reader::ByteReader;
use crate::{
    Bitmap, DecodeError, HalftoneErrorKind, LimitExceeded, LimitKind, Limits, MalformedError,
};
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg(verus_keep_ghost)]
#[rustfmt::skip]
#[path = "../specs/halftone.rs"]
mod halftone_spec;

#[cfg(verus_keep_ghost)]
#[rustfmt::skip]
#[path = "../proofs/spec_properties/halftone.rs"]
mod halftone_properties;

#[cfg(verus_keep_ghost)]
#[rustfmt::skip]
#[path = "../proofs/shared/halftone.rs"]
mod halftone_shared;

#[cfg(verus_keep_ghost)]
#[macro_use]
#[rustfmt::skip]
#[path = "../proofs/production_impl/halftone.rs"]
mod halftone_proofs;

/// Errors specific to pattern-dictionary and halftone decoding.
///
/// The structural variants need corresponding public [`MalformedError`]
/// variants when this module is wired into the segment dispatcher.
#[derive(Debug)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) enum HalftoneError {
    Decode(DecodeError),
    InvalidPatternDictionaryFlags(u8),
    InvalidPatternDimensions {
        width: u8,
        height: u8,
    },
    InvalidHalftoneFlags(u8),
    InvalidHalftoneConfiguration,
    InvalidBitmapDimensions {
        expected_width: u32,
        expected_height: u32,
        actual_width: u32,
        actual_height: u32,
    },
    InvalidGrayBitDepth(u8),
    EmptyPatternDictionary,
    InconsistentPatternDimensions,
    InvalidPatternIndex {
        index: u32,
        pattern_count: u64,
    },
}

impl From<DecodeError> for HalftoneError {
    fn from(error: DecodeError) -> Self {
        Self::Decode(error)
    }
}

impl From<MalformedError> for HalftoneError {
    fn from(error: MalformedError) -> Self {
        Self::Decode(error.into())
    }
}

/// The coding mode and generic-region template parameters for one bitmap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) enum GenericRegionCoding {
    Arithmetic {
        template: u8,
        adaptive_pixels: [(i32, i32); 4],
    },
    Mmr {
        /// Gray planes require EOFB; a known-length collective bitmap does not.
        require_eofb: bool,
    },
}

/// A generic-region decode requested by the pattern or halftone procedures.
#[derive(Debug, Clone, Copy)]
pub(crate) struct GenericRegionRequest<'a> {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) coding: GenericRegionCoding,
    pub(crate) typical_prediction: bool,
    pub(crate) extended_template: bool,
    pub(crate) skip: Option<&'a Bitmap>,
}

/// Callback boundary for the generic arithmetic and MMR bitmap decoders.
///
/// Implementations must return exactly the requested dimensions. Arithmetic
/// implementations must preserve their generic-region probability state
/// across successive gray-plane calls; the segment dispatcher is responsible
/// for the single segment-level reset before entering this module.
pub(crate) trait GenericRegionDecoder {
    fn decode_generic_region(
        &mut self,
        request: GenericRegionRequest<'_>,
    ) -> Result<Bitmap, DecodeError>;
}

impl<F> GenericRegionDecoder for F
where
    F: for<'a> FnMut(GenericRegionRequest<'a>) -> Result<Bitmap, DecodeError>,
{
    fn decode_generic_region(
        &mut self,
        request: GenericRegionRequest<'_>,
    ) -> Result<Bitmap, DecodeError> {
        self(request)
    }
}

/// A cumulative work meter shared by all stages of one decode operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct WorkBudget {
    limit: u64,
    used: u64,
}

impl WorkBudget {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures
            halftone_spec::work_limit(&out) == limits.max_work_units,
            halftone_spec::work_used(&out) == 0,
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) const fn new(limits: &Limits) -> Self {
        let out = Self {
            limit: limits.max_work_units,
            used: 0,
        };
        #[cfg(verus_keep_ghost)]
        proof! {
            halftone_spec::work_fields(&out);
        }
        out
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == halftone_spec::work_used(self),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) const fn used(&self) -> u64 {
        #[cfg(verus_keep_ghost)]
        proof! {
            halftone_spec::work_fields(self);
        }
        self.used
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures match out {
            Ok(()) =>
                halftone_spec::work_limit(final(self))
                    == halftone_spec::work_limit(old(self))
                && halftone_spec::work_used(final(self))
                    == halftone_spec::work_used(old(self)) + units
                && halftone_spec::work_used(final(self))
                    <= halftone_spec::work_limit(final(self)),
            Err(_) =>
                halftone_spec::work_limit(final(self))
                    == halftone_spec::work_limit(old(self))
                && halftone_spec::work_used(final(self))
                    == halftone_spec::work_used(old(self)),
        },
    ))]
    fn charge(&mut self, units: u64) -> Result<(), HalftoneError> {
        #[cfg(verus_keep_ghost)]
        proof! {
            halftone_spec::work_fields(self);
        }
        let used = self
            .used
            .checked_add(units)
            .ok_or(MalformedError::IntegerOverflow)?;
        if used > self.limit {
            #[cfg(verus_keep_ghost)]
            proof! {
                halftone_spec::work_fields(self);
            }
            return Err(LimitExceeded {
                kind: LimitKind::Work,
                limit: self.limit,
                actual: used,
            }
            .into());
        }
        self.used = used;
        #[cfg(verus_keep_ghost)]
        proof! {
            halftone_spec::work_fields(self);
        }
        Ok(())
    }
}

impl From<LimitExceeded> for HalftoneError {
    fn from(error: LimitExceeded) -> Self {
        Self::Decode(error.into())
    }
}

impl From<HalftoneError> for DecodeError {
    fn from(error: HalftoneError) -> Self {
        let kind = match error {
            HalftoneError::Decode(error) => return error,
            HalftoneError::InvalidPatternDictionaryFlags(flags) => {
                HalftoneErrorKind::InvalidPatternDictionaryFlags(flags)
            }
            HalftoneError::InvalidPatternDimensions { width, height } => {
                HalftoneErrorKind::InvalidPatternDimensions { width, height }
            }
            HalftoneError::InvalidHalftoneFlags(flags) => {
                HalftoneErrorKind::InvalidHalftoneFlags(flags)
            }
            HalftoneError::InvalidHalftoneConfiguration => HalftoneErrorKind::InvalidConfiguration,
            HalftoneError::InvalidBitmapDimensions {
                expected_width,
                expected_height,
                actual_width,
                actual_height,
            } => HalftoneErrorKind::InvalidBitmapDimensions {
                expected_width,
                expected_height,
                actual_width,
                actual_height,
            },
            HalftoneError::InvalidGrayBitDepth(depth) => {
                HalftoneErrorKind::InvalidGrayBitDepth(depth)
            }
            HalftoneError::EmptyPatternDictionary => HalftoneErrorKind::EmptyPatternDictionary,
            HalftoneError::InconsistentPatternDimensions => {
                HalftoneErrorKind::InconsistentPatternDimensions
            }
            HalftoneError::InvalidPatternIndex {
                index,
                pattern_count,
            } => HalftoneErrorKind::InvalidPatternIndex {
                index,
                pattern_count,
            },
        };
        Self::Malformed(MalformedError::InvalidHalftone(kind))
    }
}

/// Parsed seven-byte pattern-dictionary data header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct PatternDictionaryHeader {
    pub(crate) mmr: bool,
    pub(crate) template: u8,
    pub(crate) pattern_width: u8,
    pub(crate) pattern_height: u8,
    pub(crate) gray_max: u32,
}

impl PatternDictionaryHeader {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out as nat == halftone_spec::pattern_count(self),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn pattern_count(self) -> u64 {
        u64::from(self.gray_max) + 1
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures
        out == halftone_spec::pattern_flags_valid(flags),
        out ==> (flags & 1 == 0 || (flags / 2) % 4 == 0),
))]
#[allow(clippy::verbose_bit_mask)]
const fn pattern_flags_valid(flags: u8) -> bool {
    flags & 0xf8 == 0 && (flags & 1 == 0 || (flags / 2) % 4 == 0)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out == (flags / 2) % 4, out <= 3,
))]
fn template_from_flags(flags: u8) -> u8 {
    (flags / 2) % 4
}

/// Parses and validates the fixed pattern-dictionary data header.
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires old(reader).position <= old(reader).bytes@.len(),
    ensures match out {
        Ok(header) =>
            header.pattern_width > 0
            && header.pattern_height > 0
            && header.template <= 3
            && (!header.mmr || header.template == 0),
        Err(_) => true,
    },
))]
pub(crate) fn parse_pattern_dictionary_header(
    reader: &mut ByteReader<'_>,
) -> Result<PatternDictionaryHeader, HalftoneError> {
    let flags = reader.read_u8()?;
    let mmr = flags & 1 != 0;
    let template = template_from_flags(flags);
    if !pattern_flags_valid(flags) {
        return Err(HalftoneError::InvalidPatternDictionaryFlags(flags));
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(!mmr || template == 0);
    }

    let pattern_width = reader.read_u8()?;
    let pattern_height = reader.read_u8()?;
    if pattern_width == 0 || pattern_height == 0 {
        return Err(HalftoneError::InvalidPatternDimensions {
            width: pattern_width,
            height: pattern_height,
        });
    }

    let gray_max = reader.read_u32()?;
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(pattern_width > 0);
        assert(pattern_height > 0);
        assert(template <= 3);
        assert(!mmr || template == 0);
    }
    Ok(PatternDictionaryHeader {
        mmr,
        template,
        pattern_width,
        pattern_height,
        gray_max,
    })
}

/// Parsed 21-byte halftone-specific header following region information.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct HalftoneHeader {
    pub(crate) mmr: bool,
    pub(crate) template: u8,
    pub(crate) enable_skip: bool,
    pub(crate) operator: CombinationOperator,
    pub(crate) default_pixel: bool,
    pub(crate) grid_width: u32,
    pub(crate) grid_height: u32,
    pub(crate) grid_x: i32,
    pub(crate) grid_y: i32,
    pub(crate) grid_vector_x: u16,
    pub(crate) grid_vector_y: u16,
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures
        out == halftone_spec::halftone_flags_valid(flags),
        out ==> (
            flags & 1 == 0
            || ((flags / 2) % 4 == 0 && flags & 8 == 0)
        ),
))]
#[allow(clippy::verbose_bit_mask)]
const fn halftone_flags_valid(flags: u8) -> bool {
    flags & 1 == 0 || ((flags / 2) % 4 == 0 && flags & 8 == 0)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out == halftone_spec::operator_for_code(code),
))]
const fn combination_operator(code: u8) -> Option<CombinationOperator> {
    match code {
        0 => Some(CombinationOperator::Or),
        1 => Some(CombinationOperator::And),
        2 => Some(CombinationOperator::Xor),
        3 => Some(CombinationOperator::Xnor),
        4 => Some(CombinationOperator::Replace),
        _ => None,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out as int == halftone_spec::signed_u32(value),
))]
#[allow(clippy::cast_possible_wrap, clippy::checked_conversions)]
fn signed_grid_origin(value: u32) -> i32 {
    if value <= i32::MAX as u32 {
        value as i32
    } else {
        let magnitude = u32::MAX - value;
        -1 - magnitude as i32
    }
}

/// Parses and validates the halftone-specific header.
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires old(reader).position <= old(reader).bytes@.len(),
    ensures match out {
        Ok(header) =>
            header.template <= 3
            && (!header.mmr
                || (header.template == 0 && !header.enable_skip)),
        Err(_) => true,
    },
))]
pub(crate) fn parse_halftone_header(
    reader: &mut ByteReader<'_>,
) -> Result<HalftoneHeader, HalftoneError> {
    let flags = reader.read_u8()?;
    let mmr = flags & 1 != 0;
    let template = template_from_flags(flags);
    let enable_skip = flags & 8 != 0;
    if !halftone_flags_valid(flags) {
        return Err(HalftoneError::InvalidHalftoneFlags(flags));
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(!mmr || (template == 0 && !enable_skip));
    }
    let operator_code = (flags >> 4) & 7;
    let operator = combination_operator(operator_code)
        .ok_or(MalformedError::InvalidCombinationOperator(operator_code))?;

    let grid_width = reader.read_u32()?;
    let grid_height = reader.read_u32()?;
    let grid_x = signed_grid_origin(reader.read_u32()?);
    let grid_y = signed_grid_origin(reader.read_u32()?);
    let grid_vector_x = reader.read_u16()?;
    let grid_vector_y = reader.read_u16()?;
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(template <= 3);
        assert(!mmr || (template == 0 && !enable_skip));
    }
    Ok(HalftoneHeader {
        mmr,
        template,
        enable_skip,
        operator,
        default_pixel: flags & 0x80 != 0,
        grid_width,
        grid_height,
        grid_x,
        grid_y,
        grid_vector_x,
        grid_vector_y,
    })
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Some(width) =>
            width as nat == pattern_count as nat * pattern_width as nat
            && pattern_count as nat * pattern_width as nat <= u32::MAX as nat,
        None =>
            pattern_count as nat * pattern_width as nat > u32::MAX as nat,
    },
))]
fn checked_collective_width(pattern_count: u64, pattern_width: u8) -> Option<u32> {
    let width = pattern_count.checked_mul(u64::from(pattern_width))?;
    u32::try_from(width).ok()
}

/// Decodes and slices a pattern dictionary's collective bitmap.
pub(crate) fn decode_pattern_dictionary<D: GenericRegionDecoder>(
    header: PatternDictionaryHeader,
    decoder: &mut D,
    limits: &Limits,
    work: &mut WorkBudget,
) -> Result<Vec<Bitmap>, HalftoneError> {
    validate_pattern_header(header)?;
    let pattern_count = header.pattern_count();
    enforce_limit(
        LimitKind::Symbols,
        u64::from(limits.max_symbols),
        pattern_count,
    )?;
    let collective_width = checked_collective_width(pattern_count, header.pattern_width)
        .ok_or(MalformedError::IntegerOverflow)?;
    enforce_bitmap_dimensions(collective_width, u32::from(header.pattern_height), limits)?;
    let pixels = u64::from(collective_width)
        .checked_mul(u64::from(header.pattern_height))
        .ok_or(MalformedError::IntegerOverflow)?;
    work.charge(pixels)?;

    let coding = if header.mmr {
        GenericRegionCoding::Mmr {
            require_eofb: false,
        }
    } else {
        GenericRegionCoding::Arithmetic {
            template: header.template,
            adaptive_pixels: pattern_adaptive_pixels(header.pattern_width),
        }
    };
    let collective = decoder.decode_generic_region(GenericRegionRequest {
        width: collective_width,
        height: u32::from(header.pattern_height),
        coding,
        typical_prediction: false,
        extended_template: false,
        skip: None,
    })?;
    require_bitmap_dimensions(
        &collective,
        collective_width,
        u32::from(header.pattern_height),
    )?;
    slice_collective_bitmap(&collective, header, limits, work)
}

/// Slices a checked collective bitmap into owned fixed-size patterns.
pub(crate) fn slice_collective_bitmap(
    collective: &Bitmap,
    header: PatternDictionaryHeader,
    limits: &Limits,
    work: &mut WorkBudget,
) -> Result<Vec<Bitmap>, HalftoneError> {
    validate_pattern_header(header)?;
    let pattern_count = header.pattern_count();
    enforce_limit(
        LimitKind::Symbols,
        u64::from(limits.max_symbols),
        pattern_count,
    )?;
    let expected_width = checked_collective_width(pattern_count, header.pattern_width)
        .ok_or(MalformedError::IntegerOverflow)?;
    require_bitmap_dimensions(collective, expected_width, u32::from(header.pattern_height))?;

    let pattern_bytes = bitmap_bytes(
        u32::from(header.pattern_width),
        u32::from(header.pattern_height),
    )?;
    let retained_pattern_bytes = pattern_bytes
        .checked_mul(pattern_count)
        .ok_or(MalformedError::IntegerOverflow)?;
    let vector_bytes = pattern_count
        .checked_mul(
            u64::try_from(core::mem::size_of::<Bitmap>())
                .map_err(|_| MalformedError::IntegerOverflow)?,
        )
        .ok_or(MalformedError::IntegerOverflow)?;
    let collective_bytes =
        u64::try_from(collective.bytes().len()).map_err(|_| MalformedError::IntegerOverflow)?;
    let total_memory =
        checked_memory_sum(collective_bytes, retained_pattern_bytes, vector_bytes, 0)
            .ok_or(MalformedError::IntegerOverflow)?;
    enforce_limit(
        LimitKind::Memory,
        limits.max_decoder_managed_memory_bytes,
        total_memory,
    )?;

    let total_pixels = pattern_count
        .checked_mul(u64::from(header.pattern_width))
        .and_then(|value| value.checked_mul(u64::from(header.pattern_height)))
        .ok_or(MalformedError::IntegerOverflow)?;
    work.charge(total_pixels)?;

    let count = usize::try_from(pattern_count).map_err(|_| MalformedError::IntegerOverflow)?;
    let mut patterns = Vec::new();
    patterns
        .try_reserve_exact(count)
        .map_err(|_| DecodeError::Allocation {
            requested_bytes: vector_bytes,
        })?;
    for pattern_index in 0..pattern_count {
        let mut pattern = Bitmap::blank(
            u32::from(header.pattern_width),
            u32::from(header.pattern_height),
            limits,
        )?;
        let source_x = pattern_index
            .checked_mul(u64::from(header.pattern_width))
            .ok_or(MalformedError::IntegerOverflow)?;
        let source_x = u32::try_from(source_x).map_err(|_| MalformedError::IntegerOverflow)?;
        for y in 0..u32::from(header.pattern_height) {
            for x in 0..u32::from(header.pattern_width) {
                #[cfg(verus_keep_ghost)]
                production_slice_coordinate!(
                    pattern_index,
                    pattern_count,
                    x,
                    header,
                    source_x,
                    expected_width
                );
                let value = collective
                    .pixel(
                        source_x
                            .checked_add(x)
                            .ok_or(MalformedError::IntegerOverflow)?,
                        y,
                    )
                    .ok_or(MalformedError::IntegerOverflow)?;
                pattern.set_pixel(x, y, value)?;
            }
        }
        patterns.push(pattern);
    }
    Ok(patterns)
}

/// Returns the number of gray planes needed for a pattern count.
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires 1 <= pattern_count <= 4_294_967_296,
    ensures out == halftone_spec::gray_depth_expected(pattern_count),
))]
fn gray_bit_depth_value(pattern_count: u64) -> u8 {
    if pattern_count <= 1 {
        0
    } else if pattern_count <= 2 {
        1
    } else if pattern_count <= 4 {
        2
    } else if pattern_count <= 8 {
        3
    } else if pattern_count <= 16 {
        4
    } else if pattern_count <= 32 {
        5
    } else if pattern_count <= 64 {
        6
    } else if pattern_count <= 128 {
        7
    } else if pattern_count <= 256 {
        8
    } else if pattern_count <= 512 {
        9
    } else if pattern_count <= 1_024 {
        10
    } else if pattern_count <= 2_048 {
        11
    } else if pattern_count <= 4_096 {
        12
    } else if pattern_count <= 8_192 {
        13
    } else if pattern_count <= 16_384 {
        14
    } else if pattern_count <= 32_768 {
        15
    } else if pattern_count <= 65_536 {
        16
    } else if pattern_count <= 131_072 {
        17
    } else if pattern_count <= 262_144 {
        18
    } else if pattern_count <= 524_288 {
        19
    } else if pattern_count <= 1_048_576 {
        20
    } else if pattern_count <= 2_097_152 {
        21
    } else if pattern_count <= 4_194_304 {
        22
    } else if pattern_count <= 8_388_608 {
        23
    } else if pattern_count <= 16_777_216 {
        24
    } else if pattern_count <= 33_554_432 {
        25
    } else if pattern_count <= 67_108_864 {
        26
    } else if pattern_count <= 134_217_728 {
        27
    } else if pattern_count <= 268_435_456 {
        28
    } else if pattern_count <= 536_870_912 {
        29
    } else if pattern_count <= 1_073_741_824 {
        30
    } else if pattern_count <= 2_147_483_648 {
        31
    } else {
        32
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(depth) =>
            1 <= pattern_count <= 4_294_967_296
            && depth == halftone_spec::gray_depth_expected(pattern_count),
        Err(_) => pattern_count == 0 || pattern_count > 4_294_967_296,
    },
))]
pub(crate) fn gray_bit_depth(pattern_count: u64) -> Result<u8, HalftoneError> {
    if pattern_count == 0 || pattern_count > (u64::from(u32::MAX) + 1) {
        return Err(MalformedError::IntegerOverflow.into());
    }
    if pattern_count == 1 {
        #[cfg(verus_keep_ghost)]
        production_one_pattern_zero_plane!();
    }
    Ok(gray_bit_depth_value(pattern_count))
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out == halftone_spec::gray_binary_bit(coded, previous),
))]
const fn gray_to_binary_bit(coded: bool, previous: Option<bool>) -> bool {
    match previous {
        Some(bit) => coded != bit,
        None => coded,
    }
}

/// Decodes Gray-coded planes and reconstructs unsigned pattern indexes.
#[allow(clippy::too_many_arguments)]
pub(crate) fn decode_gray_planes<D: GenericRegionDecoder>(
    width: u32,
    height: u32,
    bit_depth: u8,
    mmr: bool,
    template: u8,
    skip: Option<&Bitmap>,
    decoder: &mut D,
    limits: &Limits,
    work: &mut WorkBudget,
) -> Result<Vec<u32>, HalftoneError> {
    if bit_depth > 32 {
        return Err(HalftoneError::InvalidGrayBitDepth(bit_depth));
    }
    if template > 3 || (mmr && (template != 0 || skip.is_some())) {
        return Err(HalftoneError::InvalidHalftoneConfiguration);
    }
    enforce_bitmap_dimensions(width, height, limits)?;
    if let Some(skip_bitmap) = skip {
        require_bitmap_dimensions(skip_bitmap, width, height)?;
    }

    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or(MalformedError::IntegerOverflow)?;
    let value_bytes = pixels
        .checked_mul(4)
        .ok_or(MalformedError::IntegerOverflow)?;
    let plane_bytes = bitmap_bytes(width, height)?;
    let concurrent_plane_count = u64::from(bit_depth != 0) + u64::from(bit_depth > 1);
    let skip_bytes = if skip.is_some() { plane_bytes } else { 0 };
    let concurrent_plane_bytes = plane_bytes
        .checked_mul(concurrent_plane_count)
        .ok_or(MalformedError::IntegerOverflow)?;
    let managed_bytes = checked_memory_sum(value_bytes, concurrent_plane_bytes, skip_bytes, 0)
        .ok_or(MalformedError::IntegerOverflow)?;
    enforce_limit(
        LimitKind::Memory,
        limits.max_decoder_managed_memory_bytes,
        managed_bytes,
    )?;

    let pixel_count = usize::try_from(pixels).map_err(|_| MalformedError::IntegerOverflow)?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(pixel_count)
        .map_err(|_| DecodeError::Allocation {
            requested_bytes: value_bytes,
        })?;
    values.resize(pixel_count, 0);
    if bit_depth == 0 {
        return Ok(values);
    }

    let plane_work = pixels
        .checked_mul(u64::from(bit_depth))
        .and_then(|value| value.checked_mul(2))
        .ok_or(MalformedError::IntegerOverflow)?;
    work.charge(plane_work)?;

    let coding = if mmr {
        GenericRegionCoding::Mmr { require_eofb: true }
    } else {
        GenericRegionCoding::Arithmetic {
            template,
            adaptive_pixels: gray_adaptive_pixels(template),
        }
    };
    let mut more_significant: Option<Bitmap> = None;
    for plane_index in (0..bit_depth).rev() {
        let mut plane = decoder.decode_generic_region(GenericRegionRequest {
            width,
            height,
            coding,
            typical_prediction: false,
            extended_template: false,
            skip,
        })?;
        require_bitmap_dimensions(&plane, width, height)?;
        for y in 0..height {
            for x in 0..width {
                let coded = plane.pixel(x, y).ok_or(MalformedError::IntegerOverflow)?;
                let previous = if let Some(previous) = more_significant.as_ref() {
                    Some(
                        previous
                            .pixel(x, y)
                            .ok_or(MalformedError::IntegerOverflow)?,
                    )
                } else {
                    None
                };
                #[cfg(verus_keep_ghost)]
                production_gray_binary_step!(coded, previous);
                let true_value = gray_to_binary_bit(coded, previous);
                plane.set_pixel(x, y, true_value)?;
                if true_value {
                    let offset = pixel_offset(width, x, y)?;
                    let value = values
                        .get_mut(offset)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    *value |= 1_u32
                        .checked_shl(u32::from(plane_index))
                        .ok_or(MalformedError::IntegerOverflow)?;
                }
            }
        }
        more_significant = Some(plane);
    }
    Ok(values)
}

/// Computes one signed pixel position from the 24.8 halftone grid.
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures
        out.0 as int == halftone_spec::grid_x_fixed(header, grid_x, grid_y),
        out.1 as int == halftone_spec::grid_y_fixed(header, grid_x, grid_y),
))]
#[allow(clippy::cast_lossless)]
fn grid_fixed_position(header: &HalftoneHeader, grid_x: u32, grid_y: u32) -> (i64, i64) {
    let ng = grid_x as i64;
    let mg = grid_y as i64;
    let vector_x = header.grid_vector_x as i64;
    let vector_y = header.grid_vector_y as i64;
    let origin_x = header.grid_x as i64;
    let origin_y = header.grid_y as i64;
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(0 <= ng <= u32::MAX as int);
        assert(0 <= mg <= u32::MAX as int);
        assert(0 <= vector_x <= u16::MAX as int);
        assert(0 <= vector_y <= u16::MAX as int);
        halftone_properties::product_upper_bound(
            mg as nat,
            vector_y as nat,
            u32::MAX as nat,
            u16::MAX as nat,
        );
        halftone_properties::product_upper_bound(
            ng as nat,
            vector_x as nat,
            u32::MAX as nat,
            u16::MAX as nat,
        );
        halftone_properties::product_upper_bound(
            mg as nat,
            vector_x as nat,
            u32::MAX as nat,
            u16::MAX as nat,
        );
        halftone_properties::product_upper_bound(
            ng as nat,
            vector_y as nat,
            u32::MAX as nat,
            u16::MAX as nat,
        );
        assert(
            (u32::MAX as nat) * (u16::MAX as nat)
                == 281_470_681_677_825
        ) by (compute);
        assert(
            2 * 281_470_681_677_825 + (i32::MAX as int)
                < i64::MAX as int
        ) by (compute);
        assert(
            (i32::MIN as int) - 281_470_681_677_825
                > i64::MIN as int
        ) by (compute);
    }
    (
        origin_x + mg * vector_y + ng * vector_x,
        origin_y + mg * vector_x - ng * vector_y,
    )
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out == halftone_spec::fixed_pixel(value),
))]
fn fixed_to_pixel(value: i64) -> i64 {
    value >> 8
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok((x, y)) =>
            x == halftone_spec::fixed_pixel(
                halftone_spec::grid_x_fixed(header, grid_x, grid_y) as i64,
            )
            && y == halftone_spec::fixed_pixel(
                halftone_spec::grid_y_fixed(header, grid_x, grid_y) as i64,
            ),
        Err(_) => false,
    },
))]
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn grid_position(
    header: &HalftoneHeader,
    grid_x: u32,
    grid_y: u32,
) -> Result<(i64, i64), HalftoneError> {
    let (x_fixed, y_fixed) = grid_fixed_position(header, grid_x, grid_y);
    Ok((fixed_to_pixel(x_fixed), fixed_to_pixel(y_fixed)))
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        right as int == x as int + pattern_width as int,
        bottom as int == y as int + pattern_height as int,
    ensures out == halftone_spec::wholly_outside(
        x,
        y,
        pattern_width,
        pattern_height,
        region_width,
        region_height,
    ),
))]
#[allow(clippy::cast_lossless, clippy::too_many_arguments)]
fn half_open_outside(
    x: i64,
    y: i64,
    right: i64,
    bottom: i64,
    pattern_width: u32,
    pattern_height: u32,
    region_width: u32,
    region_height: u32,
) -> bool {
    let _ = (pattern_width, pattern_height);
    right <= 0 || x >= region_width as i64 || bottom <= 0 || y >= region_height as i64
}

/// Builds the normative skip mask for wholly out-of-bounds patterns.
#[allow(clippy::too_many_arguments)]
pub(crate) fn generate_skip_mask(
    header: &HalftoneHeader,
    region_width: u32,
    region_height: u32,
    pattern_width: u32,
    pattern_height: u32,
    limits: &Limits,
    work: &mut WorkBudget,
) -> Result<Bitmap, HalftoneError> {
    let cells = u64::from(header.grid_width)
        .checked_mul(u64::from(header.grid_height))
        .ok_or(MalformedError::IntegerOverflow)?;
    work.charge(cells)?;
    let mut skip = Bitmap::blank(header.grid_width, header.grid_height, limits)?;
    for mg in 0..header.grid_height {
        for ng in 0..header.grid_width {
            let (x, y) = grid_position(header, ng, mg)?;
            let right = x
                .checked_add(i64::from(pattern_width))
                .ok_or(MalformedError::IntegerOverflow)?;
            let bottom = y
                .checked_add(i64::from(pattern_height))
                .ok_or(MalformedError::IntegerOverflow)?;
            #[cfg(verus_keep_ghost)]
            production_skip_boundary!(
                x,
                y,
                pattern_width,
                pattern_height,
                region_width,
                region_height
            );
            let value = half_open_outside(
                x,
                y,
                right,
                bottom,
                pattern_width,
                pattern_height,
                region_width,
                region_height,
            );
            skip.set_pixel(ng, mg, value)?;
        }
    }
    Ok(skip)
}

/// Validates every decoded gray value before it can index the dictionary.
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) => halftone_spec::indexes_valid(indexes@, pattern_count),
        Err(_) => !halftone_spec::indexes_valid(indexes@, pattern_count),
    },
))]
pub(crate) fn validate_pattern_indices(
    indexes: &[u32],
    pattern_count: u64,
) -> Result<(), HalftoneError> {
    let mut position = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            position <= indexes@.len(),
            forall|i: int| 0 <= i < position
                ==> (indexes@[i] as u64) < pattern_count,
        decreases indexes.len() - position,
    ))]
    while position < indexes.len() {
        let index = indexes[position];
        if u64::from(index) >= pattern_count {
            return Err(HalftoneError::InvalidPatternIndex {
                index,
                pattern_count,
            });
        }
        position += 1;
    }
    Ok(())
}

/// Decodes and renders a complete halftone region.
#[allow(clippy::too_many_arguments)]
pub(crate) fn decode_halftone_region<D: GenericRegionDecoder>(
    header: &HalftoneHeader,
    region_width: u32,
    region_height: u32,
    patterns: &[Bitmap],
    decoder: &mut D,
    limits: &Limits,
    work: &mut WorkBudget,
) -> Result<Bitmap, HalftoneError> {
    if header.template > 3 || (header.mmr && (header.template != 0 || header.enable_skip)) {
        return Err(HalftoneError::InvalidHalftoneConfiguration);
    }
    enforce_bitmap_dimensions(region_width, region_height, limits)?;
    let first = patterns
        .first()
        .ok_or(HalftoneError::EmptyPatternDictionary)?;
    let pattern_width = first.width();
    let pattern_height = first.height();
    if pattern_width == 0
        || pattern_height == 0
        || patterns
            .iter()
            .any(|pattern| pattern.width() != pattern_width || pattern.height() != pattern_height)
    {
        return Err(HalftoneError::InconsistentPatternDimensions);
    }
    let pattern_count =
        u64::try_from(patterns.len()).map_err(|_| MalformedError::IntegerOverflow)?;
    enforce_limit(
        LimitKind::Symbols,
        u64::from(limits.max_symbols),
        pattern_count,
    )?;
    let bit_depth = gray_bit_depth(pattern_count)?;
    enforce_halftone_memory(
        region_width,
        region_height,
        header,
        patterns,
        bit_depth,
        limits,
    )?;

    let grid_cells = u64::from(header.grid_width)
        .checked_mul(u64::from(header.grid_height))
        .ok_or(MalformedError::IntegerOverflow)?;
    let pattern_pixels = u64::from(pattern_width)
        .checked_mul(u64::from(pattern_height))
        .ok_or(MalformedError::IntegerOverflow)?;
    let render_work = grid_cells
        .checked_mul(
            pattern_pixels
                .checked_add(1)
                .ok_or(MalformedError::IntegerOverflow)?,
        )
        .ok_or(MalformedError::IntegerOverflow)?;
    work.charge(render_work)?;

    let skip = if header.enable_skip {
        Some(generate_skip_mask(
            header,
            region_width,
            region_height,
            pattern_width,
            pattern_height,
            limits,
            work,
        )?)
    } else {
        None
    };
    let indexes = decode_gray_planes(
        header.grid_width,
        header.grid_height,
        bit_depth,
        header.mmr,
        header.template,
        skip.as_ref(),
        decoder,
        limits,
        work,
    )?;
    validate_pattern_indices(&indexes, pattern_count)?;

    let mut output = Bitmap::filled(region_width, region_height, header.default_pixel, limits)?;
    for mg in 0..header.grid_height {
        for ng in 0..header.grid_width {
            let offset = pixel_offset(header.grid_width, ng, mg)?;
            let pattern_index = *indexes.get(offset).ok_or(MalformedError::IntegerOverflow)?;
            let pattern = patterns
                .get(usize::try_from(pattern_index).map_err(|_| MalformedError::IntegerOverflow)?)
                .ok_or(HalftoneError::InvalidPatternIndex {
                    index: pattern_index,
                    pattern_count,
                })?;
            let (x, y) = grid_position(header, ng, mg)?;
            render_clipped_pattern(&mut output, pattern, x, y, header.operator)?;
        }
    }
    Ok(output)
}

#[allow(clippy::similar_names)]
fn render_clipped_pattern(
    destination: &mut Bitmap,
    pattern: &Bitmap,
    x: i64,
    y: i64,
    operator: CombinationOperator,
) -> Result<(), HalftoneError> {
    let right = x
        .checked_add(i64::from(pattern.width()))
        .ok_or(MalformedError::IntegerOverflow)?;
    let bottom = y
        .checked_add(i64::from(pattern.height()))
        .ok_or(MalformedError::IntegerOverflow)?;
    let (destination_x_start, destination_x_end) = clipped_axis(x, right, destination.width());
    let (destination_y_start, destination_y_end) = clipped_axis(y, bottom, destination.height());
    if destination_x_start >= destination_x_end || destination_y_start >= destination_y_end {
        return Ok(());
    }

    for destination_y in destination_y_start..destination_y_end {
        for destination_x in destination_x_start..destination_x_end {
            let source_x = u32::try_from(
                destination_x
                    .checked_sub(x)
                    .ok_or(MalformedError::IntegerOverflow)?,
            )
            .map_err(|_| MalformedError::IntegerOverflow)?;
            let source_y = u32::try_from(
                destination_y
                    .checked_sub(y)
                    .ok_or(MalformedError::IntegerOverflow)?,
            )
            .map_err(|_| MalformedError::IntegerOverflow)?;
            let destination_x =
                u32::try_from(destination_x).map_err(|_| MalformedError::IntegerOverflow)?;
            let destination_y =
                u32::try_from(destination_y).map_err(|_| MalformedError::IntegerOverflow)?;
            let source = pattern
                .pixel(source_x, source_y)
                .ok_or(MalformedError::IntegerOverflow)?;
            let current = destination
                .pixel(destination_x, destination_y)
                .ok_or(MalformedError::IntegerOverflow)?;
            destination.set_pixel(
                destination_x,
                destination_y,
                operator.apply(current, source),
            )?;
        }
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures halftone_spec::clipped_axis_valid(
        origin,
        far_edge,
        bound,
        out.0,
        out.1,
    ),
))]
#[allow(clippy::cast_lossless)]
fn clipped_axis(origin: i64, far_edge: i64, bound: u32) -> (i64, i64) {
    (origin.max(0), far_edge.min(bound as i64))
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out == [
        (-pattern_width as i32, 0i32),
        (-3i32, -1i32),
        (2i32, -2i32),
        (-2i32, -2i32),
    ],
))]
#[allow(clippy::cast_lossless)]
fn pattern_adaptive_pixels(pattern_width: u8) -> [(i32, i32); 4] {
    [(-(pattern_width as i32), 0), (-3, -1), (2, -2), (-2, -2)]
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out == [
        (if template <= 1 { 3i32 } else { 2i32 }, -1i32),
        (-3i32, -1i32),
        (2i32, -2i32),
        (-2i32, -2i32),
    ],
))]
const fn gray_adaptive_pixels(template: u8) -> [(i32, i32); 4] {
    [
        (if template <= 1 { 3 } else { 2 }, -1),
        (-3, -1),
        (2, -2),
        (-2, -2),
    ]
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) =>
            header.pattern_width > 0
            && header.pattern_height > 0
            && header.template <= 3
            && (!header.mmr || header.template == 0),
        Err(_) => true,
    },
))]
#[allow(clippy::bool_to_int_with_if)]
fn validate_pattern_header(header: PatternDictionaryHeader) -> Result<(), HalftoneError> {
    if header.pattern_width == 0 || header.pattern_height == 0 {
        return Err(HalftoneError::InvalidPatternDimensions {
            width: header.pattern_width,
            height: header.pattern_height,
        });
    }
    if header.template > 3 || (header.mmr && header.template != 0) {
        let encoded = (if header.mmr { 1 } else { 0 }) | (header.template << 1);
        return Err(HalftoneError::InvalidPatternDictionaryFlags(encoded));
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) =>
            crate::views::bitmap_width(bitmap) == expected_width
            && crate::views::bitmap_height(bitmap) == expected_height,
        Err(_) => true,
    },
))]
fn require_bitmap_dimensions(
    bitmap: &Bitmap,
    expected_width: u32,
    expected_height: u32,
) -> Result<(), HalftoneError> {
    if bitmap.width() != expected_width || bitmap.height() != expected_height {
        return Err(HalftoneError::InvalidBitmapDimensions {
            expected_width,
            expected_height,
            actual_width: bitmap.width(),
            actual_height: bitmap.height(),
        });
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Some(total) =>
            total as nat
                == first as nat + second as nat + third as nat + fourth as nat
            && first <= total
            && second <= total
            && third <= total
            && fourth <= total,
        None =>
            first as nat + second as nat + third as nat + fourth as nat
                > u64::MAX as nat,
    },
))]
fn checked_memory_sum(first: u64, second: u64, third: u64, fourth: u64) -> Option<u64> {
    first
        .checked_add(second)?
        .checked_add(third)?
        .checked_add(fourth)
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn enforce_halftone_memory(
    region_width: u32,
    region_height: u32,
    header: &HalftoneHeader,
    patterns: &[Bitmap],
    bit_depth: u8,
    limits: &Limits,
) -> Result<(), HalftoneError> {
    let mut pattern_bytes = 0_u64;
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= patterns@.len(),
        decreases patterns@.len() - index,
    ))]
    while index < patterns.len() {
        let Ok(bytes) = u64::try_from(patterns[index].bytes().len()) else {
            return Err(MalformedError::IntegerOverflow.into());
        };
        let Some(next) = pattern_bytes.checked_add(bytes) else {
            return Err(MalformedError::IntegerOverflow.into());
        };
        pattern_bytes = next;
        index += 1;
    }
    let Some(cells) = u64::from(header.grid_width).checked_mul(u64::from(header.grid_height))
    else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    let Some(values_bytes) = cells.checked_mul(4) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    let plane_bytes = bitmap_bytes(header.grid_width, header.grid_height)?;
    let skip_bytes = if header.enable_skip { plane_bytes } else { 0 };
    let active_planes = if bit_depth == 0 {
        0
    } else if bit_depth == 1 {
        1
    } else {
        2
    };
    let Some(active_plane_bytes) = plane_bytes.checked_mul(active_planes) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    let Some(gray_peak) =
        checked_memory_sum(pattern_bytes, values_bytes, skip_bytes, active_plane_bytes)
    else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    let output_bytes = bitmap_bytes(region_width, region_height)?;
    let Some(render_peak) =
        checked_memory_sum(pattern_bytes, values_bytes, skip_bytes, output_bytes)
    else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    let peak = if gray_peak >= render_peak {
        gray_peak
    } else {
        render_peak
    };
    enforce_limit(
        LimitKind::Memory,
        limits.max_decoder_managed_memory_bytes,
        peak,
    )
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
#[allow(clippy::cast_lossless)]
fn enforce_bitmap_dimensions(
    width: u32,
    height: u32,
    limits: &Limits,
) -> Result<(), HalftoneError> {
    enforce_limit(LimitKind::Width, limits.max_width as u64, width as u64)?;
    enforce_limit(LimitKind::Height, limits.max_height as u64, height as u64)?;
    let Some(pixels) = (width as u64).checked_mul(height as u64) else {
        return Err(HalftoneError::Decode(DecodeError::Malformed(
            MalformedError::IntegerOverflow,
        )));
    };
    enforce_limit(LimitKind::PagePixels, limits.max_pixels_per_page, pixels)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) => actual <= limit,
        Err(_) => actual > limit,
    },
))]
fn enforce_limit(kind: LimitKind, limit: u64, actual: u64) -> Result<(), HalftoneError> {
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

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(bytes) =>
            bytes as nat
                == ((width as nat + 7) / 8) * height as nat,
        Err(_) => true,
    },
))]
#[allow(clippy::cast_lossless)]
fn bitmap_bytes(width: u32, height: u32) -> Result<u64, HalftoneError> {
    let Some(stride) = (width as u64).checked_add(7) else {
        return Err(HalftoneError::Decode(DecodeError::Malformed(
            MalformedError::IntegerOverflow,
        )));
    };
    let Some(bytes) = (stride / 8).checked_mul(height as u64) else {
        return Err(HalftoneError::Decode(DecodeError::Malformed(
            MalformedError::IntegerOverflow,
        )));
    };
    Ok(bytes)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out as nat == halftone_spec::row_major_offset(width, x, y),
))]
fn row_major_offset(width: u32, x: u32, y: u32) -> u64 {
    #[cfg(verus_keep_ghost)]
    proof! {
        halftone_properties::product_upper_bound(
            y as nat,
            width as nat,
            u32::MAX as nat,
            u32::MAX as nat,
        );
        assert(
            (u32::MAX as nat) * (u32::MAX as nat)
                + (u32::MAX as nat)
                < u64::MAX as nat
        ) by (compute);
    }
    u64::from(y) * u64::from(width) + u64::from(x)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(offset) =>
            offset as nat == halftone_spec::row_major_offset(width, x, y),
        Err(_) =>
            halftone_spec::row_major_offset(width, x, y) > usize::MAX,
    },
))]
#[allow(clippy::cast_possible_truncation)]
fn pixel_offset(width: u32, x: u32, y: u32) -> Result<usize, HalftoneError> {
    let offset = row_major_offset(width, x, y);
    if offset > usize::MAX as u64 {
        return Err(HalftoneError::Decode(DecodeError::Malformed(
            MalformedError::IntegerOverflow,
        )));
    }
    Ok(offset as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    fn generous_limits() -> Limits {
        Limits {
            max_width: u32::MAX,
            max_height: u32::MAX,
            max_pixels_per_page: u64::MAX,
            max_decoder_managed_memory_bytes: u64::MAX,
            max_symbols: u32::MAX,
            max_work_units: u64::MAX,
            ..Limits::default()
        }
    }

    fn bitmap(rows: &[&str]) -> Bitmap {
        let height = u32::try_from(rows.len()).expect("test height fits");
        let width =
            u32::try_from(rows.first().map_or(0, |row| row.len())).expect("test width fits");
        let mut output =
            Bitmap::blank(width, height, &generous_limits()).expect("test bitmap allocates");
        for (y, row) in rows.iter().enumerate() {
            assert_eq!(row.len(), width as usize);
            for (x, value) in row.bytes().enumerate() {
                output
                    .set_pixel(
                        u32::try_from(x).expect("test x fits"),
                        u32::try_from(y).expect("test y fits"),
                        value == b'1',
                    )
                    .expect("test pixel is in bounds");
            }
        }
        output
    }

    #[test]
    fn parses_pattern_header_and_rejects_invalid_flags_and_sizes() {
        let mut reader = ByteReader::new(&[0, 3, 2, 0, 0, 0, 1]);
        let header =
            parse_pattern_dictionary_header(&mut reader).expect("valid header should parse");
        assert_eq!(header.pattern_width, 3);
        assert_eq!(header.pattern_height, 2);
        assert_eq!(header.pattern_count(), 2);
        assert!(matches!(
            parse_pattern_dictionary_header(&mut ByteReader::new(&[3, 1, 1, 0, 0, 0, 0])),
            Err(HalftoneError::InvalidPatternDictionaryFlags(3))
        ));
        assert!(matches!(
            parse_pattern_dictionary_header(&mut ByteReader::new(&[0, 0, 1, 0, 0, 0, 0])),
            Err(HalftoneError::InvalidPatternDimensions {
                width: 0,
                height: 1
            })
        ));
    }

    #[test]
    fn parses_halftone_header_and_signed_offsets() {
        let bytes = [
            0xca, 0, 0, 0, 2, 0, 0, 0, 3, 0xff, 0xff, 0xff, 0xff, 0, 0, 1, 0, 3, 0, 1, 0,
        ];
        let header = parse_halftone_header(&mut ByteReader::new(&bytes))
            .expect("valid halftone header should parse");
        assert!(!header.mmr);
        assert_eq!(header.template, 1);
        assert!(header.enable_skip);
        assert_eq!(header.operator, CombinationOperator::Replace);
        assert!(header.default_pixel);
        assert_eq!(header.grid_x, -1);
        assert!(matches!(
            parse_halftone_header(&mut ByteReader::new(&[
                0x39, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ])),
            Err(HalftoneError::InvalidHalftoneFlags(0x39))
        ));
        assert!(matches!(
            parse_halftone_header(&mut ByteReader::new(&[
                0x50, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ])),
            Err(HalftoneError::Decode(DecodeError::Malformed(
                MalformedError::InvalidCombinationOperator(5)
            )))
        ));
    }

    #[test]
    fn slices_collective_bitmap_without_row_aliasing() {
        let collective = bitmap(&["101011", "010110"]);
        let header = PatternDictionaryHeader {
            mmr: false,
            template: 0,
            pattern_width: 3,
            pattern_height: 2,
            gray_max: 1,
        };
        let mut work = WorkBudget::new(&generous_limits());
        let patterns = slice_collective_bitmap(&collective, header, &generous_limits(), &mut work)
            .expect("collective bitmap should slice");
        assert_eq!(patterns, [bitmap(&["101", "010"]), bitmap(&["011", "110"])]);
    }

    #[test]
    fn pattern_decode_supplies_normative_generic_parameters() {
        let header = PatternDictionaryHeader {
            mmr: false,
            template: 0,
            pattern_width: 3,
            pattern_height: 2,
            gray_max: 1,
        };
        let mut calls = 0;
        let mut decoder = |request: GenericRegionRequest<'_>| {
            calls += 1;
            assert_eq!((request.width, request.height), (6, 2));
            assert!(!request.typical_prediction);
            assert!(!request.extended_template);
            assert!(request.skip.is_none());
            assert_eq!(
                request.coding,
                GenericRegionCoding::Arithmetic {
                    template: 0,
                    adaptive_pixels: [(-3, 0), (-3, -1), (2, -2), (-2, -2)]
                }
            );
            Ok(bitmap(&["101011", "010110"]))
        };
        let limits = generous_limits();
        let mut work = WorkBudget::new(&limits);
        let patterns = decode_pattern_dictionary(header, &mut decoder, &limits, &mut work)
            .expect("pattern dictionary should decode");
        assert_eq!(calls, 1);
        assert_eq!(patterns.len(), 2);
    }

    #[test]
    fn reconstructs_gray_planes_most_significant_first() {
        let mut planes = VecDeque::from([bitmap(&["1110"]), bitmap(&["0111"]), bitmap(&["0010"])]);
        let mut decoder = |request: GenericRegionRequest<'_>| {
            assert!(matches!(
                request.coding,
                GenericRegionCoding::Arithmetic { template: 0, .. }
            ));
            assert!(!request.typical_prediction);
            assert!(!request.extended_template);
            Ok(planes.pop_front().expect("three planes are supplied"))
        };
        let mut work = WorkBudget::new(&generous_limits());
        let values = decode_gray_planes(
            4,
            1,
            3,
            false,
            0,
            None,
            &mut decoder,
            &generous_limits(),
            &mut work,
        )
        .expect("planes should reconstruct");
        assert_eq!(values, [7, 4, 5, 3]);
        assert!(planes.is_empty());
    }

    #[test]
    fn mmr_gray_plane_requires_eofb() {
        let mut calls = 0;
        let mut decoder = |request: GenericRegionRequest<'_>| {
            calls += 1;
            assert_eq!(
                request.coding,
                GenericRegionCoding::Mmr { require_eofb: true }
            );
            Ok(bitmap(&["0"]))
        };
        let limits = generous_limits();
        let mut work = WorkBudget::new(&limits);
        let values = decode_gray_planes(1, 1, 1, true, 0, None, &mut decoder, &limits, &mut work)
            .expect("MMR gray plane should decode");
        assert_eq!(calls, 1);
        assert_eq!(values, [0]);
    }

    #[test]
    fn one_pattern_uses_no_gray_plane() {
        let mut calls = 0;
        let mut decoder = |_request: GenericRegionRequest<'_>| {
            calls += 1;
            Bitmap::blank(1, 1, &generous_limits())
        };
        let mut work = WorkBudget::new(&generous_limits());
        let values = decode_gray_planes(
            2,
            2,
            gray_bit_depth(1).expect("one pattern has a depth"),
            false,
            0,
            None,
            &mut decoder,
            &generous_limits(),
            &mut work,
        )
        .expect("zero-plane image should decode");
        assert_eq!(values, [0, 0, 0, 0]);
        assert_eq!(calls, 0);
    }

    #[test]
    fn gray_depth_is_exact_at_every_power_of_two() {
        assert!(gray_bit_depth(0).is_err());
        assert_eq!(gray_bit_depth(1).expect("one pattern is valid"), 0);
        for depth in 1_u8..=32 {
            let power = 1_u64 << depth;
            assert_eq!(
                gray_bit_depth(power).expect("power-of-two count is valid"),
                depth
            );
            if depth < 32 {
                assert_eq!(
                    gray_bit_depth(power + 1).expect("next count is valid"),
                    depth + 1
                );
            }
        }
        assert!(gray_bit_depth((1_u64 << 32) + 1).is_err());
    }

    #[test]
    fn grid_math_uses_signed_arithmetic_shift() {
        let header = HalftoneHeader {
            mmr: false,
            template: 0,
            enable_skip: false,
            operator: CombinationOperator::Or,
            default_pixel: false,
            grid_width: 1,
            grid_height: 1,
            grid_x: -1,
            grid_y: 512,
            grid_vector_x: 768,
            grid_vector_y: 256,
        };
        assert_eq!(
            grid_position(&header, 0, 0).expect("position fits"),
            (-1, 2)
        );
        assert_eq!(grid_position(&header, 2, 3).expect("position fits"), (8, 9));
    }

    #[test]
    fn grid_math_extremes_fit_signed_intermediates() {
        let header = HalftoneHeader {
            mmr: false,
            template: 0,
            enable_skip: false,
            operator: CombinationOperator::Or,
            default_pixel: false,
            grid_width: 1,
            grid_height: 1,
            grid_x: i32::MIN,
            grid_y: i32::MAX,
            grid_vector_x: u16::MAX,
            grid_vector_y: u16::MAX,
        };
        let grid_x = u32::MAX;
        let grid_y = u32::MAX;
        let x_fixed = i128::from(header.grid_x)
            + i128::from(grid_y) * i128::from(header.grid_vector_y)
            + i128::from(grid_x) * i128::from(header.grid_vector_x);
        let y_fixed = i128::from(header.grid_y)
            + i128::from(grid_y) * i128::from(header.grid_vector_x)
            - i128::from(grid_x) * i128::from(header.grid_vector_y);
        assert_eq!(
            grid_position(&header, grid_x, grid_y).expect("all grid products fit"),
            (
                i64::try_from(x_fixed >> 8).expect("x position fits"),
                i64::try_from(y_fixed >> 8).expect("y position fits")
            )
        );
    }

    #[test]
    fn skip_mask_obeys_half_open_boundary_rules() {
        let header = HalftoneHeader {
            mmr: false,
            template: 0,
            enable_skip: true,
            operator: CombinationOperator::Or,
            default_pixel: false,
            grid_width: 4,
            grid_height: 1,
            grid_x: -3 * 256,
            grid_y: 0,
            grid_vector_x: 256,
            grid_vector_y: 0,
        };
        let mut work = WorkBudget::new(&generous_limits());
        let skip = generate_skip_mask(&header, 10, 8, 3, 2, &generous_limits(), &mut work)
            .expect("skip mask should fit");
        assert_eq!(
            (0..4).map(|x| skip.pixel(x, 0)).collect::<Vec<_>>(),
            [Some(true), Some(false), Some(false), Some(false)]
        );
    }

    #[test]
    fn clipped_replace_rendering_and_order_are_deterministic() {
        let limits = generous_limits();
        let patterns = [bitmap(&["101", "011"]), bitmap(&["010", "110"])];
        let header = HalftoneHeader {
            mmr: false,
            template: 0,
            enable_skip: false,
            operator: CombinationOperator::Replace,
            default_pixel: false,
            grid_width: 2,
            grid_height: 1,
            grid_x: -256,
            grid_y: 0,
            grid_vector_x: 512,
            grid_vector_y: 0,
        };
        let mut planes = VecDeque::from([bitmap(&["01"])]);
        let mut decoder = |_request: GenericRegionRequest<'_>| {
            Ok(planes.pop_front().expect("one plane is supplied"))
        };
        let mut work = WorkBudget::new(&limits);
        let output =
            decode_halftone_region(&header, 4, 2, &patterns, &mut decoder, &limits, &mut work)
                .expect("halftone should render");
        assert_eq!(output, bitmap(&["0010", "1110"]));
    }

    #[test]
    fn rejects_unused_gray_code_and_work_overrun() {
        assert!(matches!(
            validate_pattern_indices(&[0, 1, 3], 3),
            Err(HalftoneError::InvalidPatternIndex {
                index: 3,
                pattern_count: 3
            })
        ));
        let limits = Limits {
            max_work_units: 3,
            ..generous_limits()
        };
        let mut work = WorkBudget::new(&limits);
        assert!(work.charge(3).is_ok());
        assert!(matches!(
            work.charge(1),
            Err(HalftoneError::Decode(DecodeError::LimitExceeded(
                LimitExceeded {
                    kind: LimitKind::Work,
                    limit: 3,
                    actual: 4
                }
            )))
        ));
    }
}

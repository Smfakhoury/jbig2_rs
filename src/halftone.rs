use crate::bitmap::CombinationOperator;
use crate::generic_region::{
    decode_generic_on, GenericArithmeticCall, GenericArithmeticSession, GenericRegionError,
    GenericTemplate, TemplateOffset,
};
use crate::mmr::{decode_mmr, decode_mmr_prefix, EofbPolicy, MmrError};
use crate::reader::ByteReader;
use crate::{
    Bitmap, DecodeError, GenericRegionErrorKind, HalftoneErrorKind, LimitExceeded, LimitKind,
    Limits, MalformedError, MmrErrorKind,
};
#[cfg(verus_keep_ghost)]
use vstd::layout;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg(verus_keep_ghost)]
#[rustfmt::skip]
#[path = "../specs/halftone.rs"]
pub(crate) mod halftone_spec;

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
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct GenericRegionRequest<'a> {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) coding: GenericRegionCoding,
    pub(crate) typical_prediction: bool,
    pub(crate) extended_template: bool,
    pub(crate) skip: Option<&'a Bitmap>,
}

#[cfg(test)]
pub(crate) trait GenericRegionTestDecoder {
    fn decode_generic_region(
        &mut self,
        request: GenericRegionRequest<'_>,
    ) -> Result<Bitmap, DecodeError>;
}

#[cfg(test)]
impl<F> GenericRegionTestDecoder for F
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

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn generic_error_into_decode(error: GenericRegionError) -> DecodeError {
    let kind = match error {
        GenericRegionError::Decode(error) => return error,
        GenericRegionError::ReservedFlags(flags) => GenericRegionErrorKind::ReservedFlags(flags),
        GenericRegionError::InvalidFlags(flags) => GenericRegionErrorKind::InvalidFlags(flags),
        GenericRegionError::InvalidAdaptivePixel { number, x, y } => {
            GenericRegionErrorKind::InvalidAdaptivePixel { number, x, y }
        }
        GenericRegionError::InvalidSkipDimensions => GenericRegionErrorKind::InvalidSkipDimensions,
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
    DecodeError::Malformed(MalformedError::InvalidGenericRegion(kind))
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn mmr_error_into_decode(error: MmrError) -> DecodeError {
    let kind = match error {
        MmrError::Reader(error) => return DecodeError::Malformed(error),
        MmrError::Bitmap(error) => return error,
        MmrError::Allocation { requested_bytes } => {
            return DecodeError::Allocation { requested_bytes };
        }
        MmrError::IntegerOverflow => {
            return DecodeError::Malformed(MalformedError::IntegerOverflow);
        }
        MmrError::InvalidMode { bit_offset } => MmrErrorKind::InvalidMode { bit_offset },
        MmrError::ExtensionMode { bit_offset } => MmrErrorKind::ExtensionMode { bit_offset },
        MmrError::InvalidRunCode { bit_offset, black } => {
            MmrErrorKind::InvalidRunCode { bit_offset, black }
        }
        MmrError::RunExceedsRow => MmrErrorKind::RunExceedsRow,
        MmrError::InvalidTransition => MmrErrorKind::InvalidTransition,
        MmrError::NonProgress => MmrErrorKind::NonProgress,
        MmrError::TooManyModes => MmrErrorKind::TooManyModes,
        MmrError::MissingEofb => MmrErrorKind::MissingEofb,
        MmrError::InvalidEofb => MmrErrorKind::InvalidEofb,
        MmrError::TrailingData => MmrErrorKind::TrailingData,
        MmrError::NonZeroEofbPadding => MmrErrorKind::NonZeroPadding,
    };
    DecodeError::Malformed(MalformedError::InvalidMmr(kind))
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug, Clone, Copy)]
pub(crate) enum ArithmeticPurpose {
    Pattern,
    Halftone,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) enum GenericRegionState<'a> {
    Arithmetic {
        session: GenericArithmeticSession<'a>,
        template: u8,
        purpose: ArithmeticPurpose,
    },
    PatternMmr {
        coded: &'a [u8],
        limits: Limits,
        used: bool,
    },
    GrayMmr {
        coded: &'a [u8],
        limits: Limits,
        offset: usize,
    },
    #[cfg(test)]
    Scripted(&'a mut dyn GenericRegionTestDecoder),
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(state) => halftone_spec::decoder_state_valid(&state),
        Err(_) => true,
    },
))]
fn new_arithmetic_state<'a>(
    coded: &'a [u8],
    template: GenericTemplate,
    template_code: u8,
    purpose: ArithmeticPurpose,
    limits: &Limits,
) -> Result<GenericRegionState<'a>, DecodeError> {
    let session = match GenericArithmeticSession::new(coded, template, false, limits) {
        Ok(session) => session,
        Err(error) => return Err(generic_error_into_decode(error)),
    };
    Ok(GenericRegionState::Arithmetic {
        session,
        template: template_code,
        purpose,
    })
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(state) => halftone_spec::decoder_state_valid(&state),
        Err(_) => true,
    },
))]
pub(crate) fn new_pattern_arithmetic_state<'a>(
    coded: &'a [u8],
    template: GenericTemplate,
    template_code: u8,
    limits: &Limits,
) -> Result<GenericRegionState<'a>, DecodeError> {
    new_arithmetic_state(
        coded,
        template,
        template_code,
        ArithmeticPurpose::Pattern,
        limits,
    )
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(state) => halftone_spec::decoder_state_valid(&state),
        Err(_) => true,
    },
))]
pub(crate) fn new_halftone_arithmetic_state<'a>(
    coded: &'a [u8],
    template: GenericTemplate,
    template_code: u8,
    limits: &Limits,
) -> Result<GenericRegionState<'a>, DecodeError> {
    new_arithmetic_state(
        coded,
        template,
        template_code,
        ArithmeticPurpose::Halftone,
        limits,
    )
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures halftone_spec::decoder_state_valid(&out),
))]
pub(crate) fn new_pattern_mmr_state<'a>(
    coded: &'a [u8],
    limits: &Limits,
) -> GenericRegionState<'a> {
    GenericRegionState::PatternMmr {
        coded,
        limits: *limits,
        used: false,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures halftone_spec::decoder_state_valid(&out),
))]
pub(crate) fn new_gray_mmr_state<'a>(coded: &'a [u8], limits: &Limits) -> GenericRegionState<'a> {
    GenericRegionState::GrayMmr {
        coded,
        limits: *limits,
        offset: 0,
    }
}

#[allow(clippy::elidable_lifetime_names)]
impl<'a> GenericRegionState<'a> {
    #[cfg(test)]
    pub(crate) fn scripted(decoder: &'a mut dyn GenericRegionTestDecoder) -> Self {
        Self::Scripted(decoder)
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            halftone_spec::decoder_state_valid(old(self)),
            match request.skip {
                Some(skip) => crate::spec::bitmap::storage_valid(skip),
                None => true,
            },
        ensures match out {
            Ok(bitmap) =>
                halftone_spec::decoder_state_valid(final(self))
                    && crate::spec::bitmap::storage_valid(&bitmap),
            Err(_) => true,
        },
    ))]
    #[allow(clippy::too_many_lines)]
    fn decode_generic_region(
        &mut self,
        request: GenericRegionRequest<'_>,
    ) -> Result<Bitmap, DecodeError> {
        match self {
            Self::Arithmetic {
                session,
                template,
                purpose,
            } => {
                let GenericRegionCoding::Arithmetic {
                    template: requested_template,
                    adaptive_pixels,
                } = request.coding
                else {
                    let reason = match purpose {
                        ArithmeticPurpose::Pattern => "pattern dictionary coding mode changed",
                        ArithmeticPurpose::Halftone => "halftone coding mode changed",
                    };
                    return Err(MalformedError::InvalidPageState(reason).into());
                };
                if requested_template != *template {
                    let reason = match purpose {
                        ArithmeticPurpose::Pattern => "pattern dictionary template changed",
                        ArithmeticPurpose::Halftone => "halftone template changed",
                    };
                    return Err(MalformedError::InvalidPageState(reason).into());
                }
                let mut adaptive = [TemplateOffset::new(0, 0); 12];
                adaptive[0] = TemplateOffset::new(adaptive_pixels[0].0, adaptive_pixels[0].1);
                adaptive[1] = TemplateOffset::new(adaptive_pixels[1].0, adaptive_pixels[1].1);
                adaptive[2] = TemplateOffset::new(adaptive_pixels[2].0, adaptive_pixels[2].1);
                adaptive[3] = TemplateOffset::new(adaptive_pixels[3].0, adaptive_pixels[3].1);
                match decode_generic_on(
                    &mut session.decoder,
                    &mut session.contexts,
                    GenericArithmeticCall {
                        width: request.width,
                        height: request.height,
                        adaptive,
                        typical_prediction: request.typical_prediction,
                        skip: request.skip,
                    },
                    request.height,
                    &session.limits,
                    &mut session.work_used,
                ) {
                    Ok(bitmap) => Ok(bitmap),
                    Err(error) => Err(generic_error_into_decode(error)),
                }
            }
            Self::PatternMmr {
                coded,
                limits,
                used,
            } => {
                if *used {
                    return Err(MalformedError::InvalidPageState(
                        "pattern dictionary requested multiple MMR bitmaps",
                    )
                    .into());
                }
                if request.coding
                    != (GenericRegionCoding::Mmr {
                        require_eofb: false,
                    })
                {
                    return Err(MalformedError::InvalidPageState(
                        "pattern dictionary coding mode changed",
                    )
                    .into());
                }
                *used = true;
                match decode_mmr(
                    coded,
                    request.width,
                    request.height,
                    limits,
                    EofbPolicy::Optional,
                ) {
                    Ok(bitmap) => Ok(bitmap),
                    Err(error) => Err(mmr_error_into_decode(error)),
                }
            }
            Self::GrayMmr {
                coded,
                limits,
                offset,
            } => {
                if request.coding != (GenericRegionCoding::Mmr { require_eofb: true }) {
                    return Err(
                        MalformedError::InvalidPageState("halftone coding mode changed").into(),
                    );
                }
                let remaining = coded
                    .get(*offset..)
                    .ok_or(MalformedError::IntegerOverflow)?;
                let (bitmap, consumed) =
                    match decode_mmr_prefix(remaining, request.width, request.height, limits) {
                        Ok(decoded) => decoded,
                        Err(error) => return Err(mmr_error_into_decode(error)),
                    };
                *offset = offset
                    .checked_add(consumed)
                    .ok_or(MalformedError::IntegerOverflow)?;
                Ok(bitmap)
            }
            #[cfg(test)]
            Self::Scripted(decoder) => decoder.decode_generic_region(request),
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub(crate) fn finish(&self) -> Result<(), DecodeError> {
        match self {
            Self::Arithmetic { session, .. } => {
                if let Err(error) = session.validate_final_marker() {
                    return Err(generic_error_into_decode(error));
                }
            }
            Self::PatternMmr { .. } => {}
            Self::GrayMmr { coded, offset, .. } => {
                if *offset != coded.len() {
                    let trailing = coded
                        .len()
                        .checked_sub(*offset)
                        .and_then(|bytes| u64::try_from(bytes).ok())
                        .ok_or(MalformedError::IntegerOverflow)?;
                    return Err(MalformedError::TrailingData(trailing).into());
                }
            }
            #[cfg(test)]
            Self::Scripted(_) => {}
        }
        Ok(())
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
            halftone_spec::work_valid(&out),
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

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) fn halftone_error_into_decode(error: HalftoneError) -> DecodeError {
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
        HalftoneError::InvalidGrayBitDepth(depth) => HalftoneErrorKind::InvalidGrayBitDepth(depth),
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
    DecodeError::Malformed(MalformedError::InvalidHalftone(kind))
}

impl From<HalftoneError> for DecodeError {
    fn from(error: HalftoneError) -> Self {
        halftone_error_into_decode(error)
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
    ensures
        final(reader).bytes@ == old(reader).bytes@,
        final(reader).position <= final(reader).bytes@.len(),
        match out {
        Ok(header) =>
            header.pattern_width > 0
            && header.pattern_height > 0
            && header.template <= 3
            && (!header.mmr || header.template == 0),
        Err(_) => true,
        },
        match out {
            Ok(header) =>
                halftone_spec::pattern_header_fields(
                    header, old(reader).bytes@, old(reader).position as int,
                )
                && final(reader).position == old(reader).position + 7,
            Err(_) => true,
        },
        out.is_ok() == halftone_spec::pattern_header_accepted(
            old(reader).bytes@, old(reader).position as int,
        ),
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
    ensures
        final(reader).bytes@ == old(reader).bytes@,
        final(reader).position <= final(reader).bytes@.len(),
        match out {
        Ok(header) =>
            header.template <= 3
            && (!header.mmr
                || (header.template == 0 && !header.enable_skip)),
        Err(_) => true,
        },
        match out {
            Ok(header) =>
                halftone_spec::halftone_header_fields(
                    header, old(reader).bytes@, old(reader).position as int,
                )
                && final(reader).position == old(reader).position + 21,
            Err(_) => true,
        },
        out.is_ok() == halftone_spec::halftone_header_accepted(
            old(reader).bytes@, old(reader).position as int,
        ),
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
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        halftone_spec::decoder_state_valid(old(decoder)),
        halftone_spec::work_valid(old(work)),
    ensures match out {
        Ok(patterns) =>
            halftone_spec::decoder_state_valid(final(decoder))
                && halftone_spec::work_valid(final(work))
                && halftone_spec::bitmaps_valid(patterns@),
        Err(_) => true,
    },
))]
pub(crate) fn decode_pattern_dictionary(
    header: PatternDictionaryHeader,
    decoder: &mut GenericRegionState<'_>,
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
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        crate::spec::bitmap::storage_valid(collective),
        halftone_spec::work_valid(old(work)),
    ensures match out {
        Ok(patterns) =>
            halftone_spec::bitmaps_valid(patterns@)
                && halftone_spec::work_valid(final(work)),
        Err(_) => true,
    },
))]
#[allow(clippy::cast_lossless, clippy::too_many_lines)]
pub(crate) fn slice_collective_bitmap(
    collective: &Bitmap,
    header: PatternDictionaryHeader,
    limits: &Limits,
    work: &mut WorkBudget,
) -> Result<Vec<Bitmap>, HalftoneError> {
    validate_pattern_header(header)?;
    let raw_pattern_width = header.pattern_width;
    let raw_pattern_height = header.pattern_height;
    let pattern_width = raw_pattern_width as u32;
    let pattern_height = raw_pattern_height as u32;
    let pattern_count = header.pattern_count();
    enforce_limit(
        LimitKind::Symbols,
        u64::from(limits.max_symbols),
        pattern_count,
    )?;
    let expected_width = checked_collective_width(pattern_count, header.pattern_width)
        .ok_or(MalformedError::IntegerOverflow)?;
    require_bitmap_dimensions(collective, expected_width, u32::from(header.pattern_height))?;

    let pattern_bytes = bitmap_bytes(pattern_width, pattern_height)?;
    let retained_pattern_bytes = pattern_bytes
        .checked_mul(pattern_count)
        .ok_or(MalformedError::IntegerOverflow)?;
    let vector_bytes = pattern_count
        .checked_mul(match u64::try_from(core::mem::size_of::<Bitmap>()) {
            Ok(bytes) => bytes,
            Err(_) => return Err(MalformedError::IntegerOverflow.into()),
        })
        .ok_or(MalformedError::IntegerOverflow)?;
    let Ok(collective_bytes) = u64::try_from(collective.bytes().len()) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    let total_memory =
        checked_memory_sum(collective_bytes, retained_pattern_bytes, vector_bytes, 0)
            .ok_or(MalformedError::IntegerOverflow)?;
    enforce_limit(
        LimitKind::Memory,
        limits.max_decoder_managed_memory_bytes,
        total_memory,
    )?;

    let total_pixels = pattern_count
        .checked_mul(u64::from(pattern_width))
        .ok_or(MalformedError::IntegerOverflow)?
        .checked_mul(u64::from(pattern_height))
        .ok_or(MalformedError::IntegerOverflow)?;
    work.charge(total_pixels)?;

    let Ok(count) = usize::try_from(pattern_count) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    let mut patterns = Vec::new();
    if patterns.try_reserve(count).is_err() {
        return Err(DecodeError::Allocation {
            requested_bytes: vector_bytes,
        }
        .into());
    }
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            crate::spec::bitmap::storage_valid(collective),
            pattern_index <= pattern_count,
            patterns@.len() == pattern_index,
            halftone_spec::bitmaps_valid(patterns@),
            raw_pattern_width == header.pattern_width,
            raw_pattern_height == header.pattern_height,
            raw_pattern_width > 0,
            raw_pattern_height > 0,
            pattern_width > 0,
            pattern_height > 0,
            pattern_width as nat == raw_pattern_width as nat,
            pattern_height as nat == raw_pattern_height as nat,
            expected_width as nat
                == pattern_count as nat * header.pattern_width as nat,
            pattern_count as nat * header.pattern_width as nat
                <= u32::MAX as nat,
        decreases pattern_count - pattern_index,
    ))]
    for pattern_index in 0..pattern_count {
        let mut pattern = Bitmap::blank(pattern_width, pattern_height, limits)?;
        let source_x = pattern_index
            .checked_mul(u64::from(pattern_width))
            .ok_or(MalformedError::IntegerOverflow)?;
        let Ok(source_x) = u32::try_from(source_x) else {
            return Err(MalformedError::IntegerOverflow.into());
        };
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(source_x as nat
                == pattern_index as nat * header.pattern_width as nat);
        }
        #[cfg(verus_keep_ghost)]
        {
            let last_pattern_x = pattern_width - 1;
            production_slice_coordinate!(
                pattern_index,
                pattern_count,
                last_pattern_x,
                header,
                source_x,
                expected_width
            );
        }
        pattern.compose_region(
            collective,
            source_x,
            0,
            0,
            0,
            pattern_width,
            pattern_height,
            CombinationOperator::Replace,
        )?;
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

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        crate::spec::bitmap::storage_valid(old(current)),
        crate::spec::bitmap::storage_valid(previous),
    ensures match out {
        Ok(()) => crate::spec::bitmap::storage_valid(final(current)),
        Err(_) => true,
    },
))]
fn xor_gray_plane_bytes(current: &mut Bitmap, previous: &Bitmap) -> Result<(), HalftoneError> {
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::views::bitmap_fields(current);
    }
    if current.width != previous.width
        || current.height != previous.height
        || current.stride != previous.stride
        || current.data.len() != previous.data.len()
    {
        return Err(MalformedError::IntegerOverflow.into());
    }
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index <= current.data@.len(),
            current.width == old(current).width,
            current.height == old(current).height,
            current.stride == old(current).stride,
            current.data@.len() == old(current).data@.len(),
            current.data@.len() == previous.data@.len(),
        decreases current.data@.len() - index,
    ))]
    while index < current.data.len() {
        current.data[index] ^= previous.data[index];
        index += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::views::bitmap_fields(current);
        reveal(crate::spec::bitmap::storage_valid);
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires plane_index < 32,
    ensures
        final(values)@.len() == old(values)@.len(),
        match out {
            Ok(()) => offset as nat + 8 <= old(values)@.len(),
            Err(_) => true,
        },
))]
fn accumulate_gray_byte(
    values: &mut [u32],
    offset: usize,
    byte: u8,
    plane_index: u8,
) -> Result<(), HalftoneError> {
    let end = offset
        .checked_add(8)
        .ok_or(MalformedError::IntegerOverflow)?;
    let chunk = values
        .get_mut(offset..end)
        .ok_or(MalformedError::IntegerOverflow)?;
    chunk[0] |= u32::from((byte >> 7) & 1) << plane_index;
    chunk[1] |= u32::from((byte >> 6) & 1) << plane_index;
    chunk[2] |= u32::from((byte >> 5) & 1) << plane_index;
    chunk[3] |= u32::from((byte >> 4) & 1) << plane_index;
    chunk[4] |= u32::from((byte >> 3) & 1) << plane_index;
    chunk[5] |= u32::from((byte >> 2) & 1) << plane_index;
    chunk[6] |= u32::from((byte >> 1) & 1) << plane_index;
    chunk[7] |= u32::from(byte & 1) << plane_index;
    Ok(())
}

/// Decodes Gray-coded planes and reconstructs unsigned pattern indexes.
#[allow(
    clippy::cast_lossless,
    clippy::too_many_arguments,
    clippy::too_many_lines
)]
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        halftone_spec::decoder_state_valid(old(decoder)),
        halftone_spec::work_valid(old(work)),
        match skip {
            Some(bitmap) => crate::spec::bitmap::storage_valid(bitmap),
            None => true,
        },
    ensures match out {
        Ok(indexes) =>
            halftone_spec::decoder_state_valid(final(decoder))
                && halftone_spec::work_valid(final(work)),
        Err(_) => true,
    },
))]
pub(crate) fn decode_gray_planes(
    width: u32,
    height: u32,
    bit_depth: u8,
    mmr: bool,
    template: u8,
    skip: Option<&Bitmap>,
    decoder: &mut GenericRegionState<'_>,
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
    let concurrent_plane_count = if bit_depth == 0 {
        0
    } else if bit_depth == 1 {
        1
    } else {
        2
    };
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
        .try_reserve(pixel_count)
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
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            halftone_spec::decoder_state_valid(decoder),
            halftone_spec::work_valid(work),
            values@.len() == pixel_count,
            match skip {
                Some(bitmap) => crate::spec::bitmap::storage_valid(bitmap),
                None => true,
            },
            match more_significant {
                Some(ref bitmap) => crate::spec::bitmap::storage_valid(bitmap),
                None => true,
            },
    ))]
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
        if let Some(previous) = more_significant.as_ref() {
            xor_gray_plane_bytes(&mut plane, previous)?;
        }
        if plane_index >= 32 {
            return Err(HalftoneError::InvalidGrayBitDepth(bit_depth));
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(plane_index < 32);
        }
        let width_usize =
            usize::try_from(width).map_err(|_error| MalformedError::IntegerOverflow)?;
        let full_bytes = width_usize / 8;
        let tail_start = width
            .checked_sub(width % 8)
            .ok_or(MalformedError::IntegerOverflow)?;
        #[cfg(verus_keep_ghost)]
        proof! {
            vstd::arithmetic::div_mod::lemma_mod_bound(width as int, 8);
            assert(tail_start <= width);
        }
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                y <= height,
                plane_index < 32,
                tail_start <= width,
                crate::spec::bitmap::storage_valid(&plane),
                values@.len() == pixel_count,
                match more_significant {
                    Some(ref bitmap) => crate::spec::bitmap::storage_valid(bitmap),
                    None => true,
                },
            decreases height - y,
        ))]
        for y in 0..height {
            let y_usize = usize::try_from(y).map_err(|_error| MalformedError::IntegerOverflow)?;
            let row_start = y_usize
                .checked_mul(plane.stride)
                .ok_or(MalformedError::IntegerOverflow)?;
            let value_row = y_usize
                .checked_mul(width_usize)
                .ok_or(MalformedError::IntegerOverflow)?;
            let mut byte_index = 0_usize;
            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant
                    byte_index <= full_bytes,
                    y < height,
                    plane_index < 32,
                    crate::spec::bitmap::storage_valid(&plane),
                    values@.len() == pixel_count,
                    match more_significant {
                        Some(ref bitmap) => crate::spec::bitmap::storage_valid(bitmap),
                        None => true,
                    },
                decreases full_bytes - byte_index,
            ))]
            while byte_index < full_bytes {
                let source_index = row_start
                    .checked_add(byte_index)
                    .ok_or(MalformedError::IntegerOverflow)?;
                let byte = *plane
                    .data
                    .get(source_index)
                    .ok_or(MalformedError::IntegerOverflow)?;
                let value_offset = value_row
                    .checked_add(
                        byte_index
                            .checked_mul(8)
                            .ok_or(MalformedError::IntegerOverflow)?,
                    )
                    .ok_or(MalformedError::IntegerOverflow)?;
                accumulate_gray_byte(&mut values, value_offset, byte, plane_index)?;
                byte_index += 1;
            }
            let mut x = tail_start;
            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant
                    x <= width,
                    y < height,
                    plane_index < 32,
                    crate::spec::bitmap::storage_valid(&plane),
                    values@.len() == pixel_count,
                decreases width - x,
            ))]
            while x < width {
                let true_value = plane.pixel(x, y).ok_or(MalformedError::IntegerOverflow)?;
                if true_value {
                    let offset = pixel_offset(width, x, y)?;
                    let value = values
                        .get_mut(offset)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    *value |= 1_u32 << plane_index;
                }
                x += 1;
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
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires halftone_spec::work_valid(old(work)),
    ensures match out {
        Ok(bitmap) =>
            crate::spec::bitmap::storage_valid(&bitmap)
            && crate::views::bitmap_width(&bitmap) == header.grid_width
            && crate::views::bitmap_height(&bitmap) == header.grid_height
            && halftone_spec::work_valid(final(work)),
        Err(_) => true,
    },
))]
#[allow(clippy::cast_lossless, clippy::too_many_arguments)]
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
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            crate::spec::bitmap::storage_valid(&skip),
            crate::views::bitmap_width(&skip) == header.grid_width,
            crate::views::bitmap_height(&skip) == header.grid_height,
            mg <= header.grid_height,
        decreases header.grid_height - mg,
    ))]
    for mg in 0..header.grid_height {
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                crate::spec::bitmap::storage_valid(&skip),
                crate::views::bitmap_width(&skip) == header.grid_width,
                crate::views::bitmap_height(&skip) == header.grid_height,
                ng <= header.grid_width,
                mg < header.grid_height,
            decreases header.grid_width - ng,
        ))]
        for ng in 0..header.grid_width {
            let (x, y) = grid_position(header, ng, mg)?;
            let right = x
                .checked_add(pattern_width as i64)
                .ok_or(MalformedError::IntegerOverflow)?;
            let bottom = y
                .checked_add(pattern_height as i64)
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
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        halftone_spec::decoder_state_valid(old(decoder)),
        halftone_spec::work_valid(old(work)),
        halftone_spec::bitmaps_valid(patterns@),
    ensures match out {
        Ok(bitmap) =>
            halftone_spec::decoder_state_valid(final(decoder))
                && halftone_spec::work_valid(final(work))
                && crate::spec::bitmap::storage_valid(&bitmap),
        Err(_) => true,
    },
))]
pub(crate) fn decode_halftone_region(
    header: &HalftoneHeader,
    region_width: u32,
    region_height: u32,
    patterns: &[Bitmap],
    decoder: &mut GenericRegionState<'_>,
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
    if pattern_width == 0 || pattern_height == 0 {
        return Err(HalftoneError::InconsistentPatternDimensions);
    }
    let mut pattern_position = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            pattern_position <= patterns@.len(),
            halftone_spec::bitmaps_valid(patterns@),
        decreases patterns@.len() - pattern_position,
    ))]
    while pattern_position < patterns.len() {
        let pattern = &patterns[pattern_position];
        if pattern.width() != pattern_width || pattern.height() != pattern_height {
            return Err(HalftoneError::InconsistentPatternDimensions);
        }
        pattern_position += 1;
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
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            mg <= header.grid_height,
            crate::spec::bitmap::storage_valid(&output),
            halftone_spec::bitmaps_valid(patterns@),
            halftone_spec::indexes_valid(indexes@, pattern_count),
        decreases header.grid_height - mg,
    ))]
    for mg in 0..header.grid_height {
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                ng <= header.grid_width,
                mg < header.grid_height,
                crate::spec::bitmap::storage_valid(&output),
                halftone_spec::bitmaps_valid(patterns@),
                halftone_spec::indexes_valid(indexes@, pattern_count),
            decreases header.grid_width - ng,
        ))]
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

#[allow(clippy::cast_lossless, clippy::similar_names, clippy::too_many_lines)]
#[cfg_attr(verus_keep_ghost, verus_spec(
    requires
        crate::spec::bitmap::storage_valid(old(destination)),
        crate::spec::bitmap::storage_valid(pattern),
    ensures crate::spec::bitmap::storage_valid(final(destination)),
))]
fn render_clipped_pattern(
    destination: &mut Bitmap,
    pattern: &Bitmap,
    x: i64,
    y: i64,
    operator: CombinationOperator,
) -> Result<(), HalftoneError> {
    let pattern_width = pattern.width();
    let pattern_height = pattern.height();
    let destination_width = destination.width();
    let destination_height = destination.height();
    let pattern_width_i64 = pattern_width as i64;
    let pattern_height_i64 = pattern_height as i64;
    let right = x
        .checked_add(pattern_width_i64)
        .ok_or(MalformedError::IntegerOverflow)?;
    let bottom = y
        .checked_add(pattern_height_i64)
        .ok_or(MalformedError::IntegerOverflow)?;
    let (destination_x_start, destination_x_end) = clipped_axis(x, right, destination_width);
    let (destination_y_start, destination_y_end) = clipped_axis(y, bottom, destination_height);
    if destination_x_start >= destination_x_end || destination_y_start >= destination_y_end {
        return Ok(());
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        reveal(halftone_spec::clipped_axis_valid);
        assert(0 <= destination_x_start < destination_x_end);
        assert(destination_x_end <= destination_width);
        assert(x <= destination_x_start);
        assert(destination_x_end <= right);
        assert(0 <= destination_y_start < destination_y_end);
        assert(destination_y_end <= destination_height);
        assert(y <= destination_y_start);
        assert(destination_y_end <= bottom);
        assert(right == x + pattern_width_i64);
        assert(bottom == y + pattern_height_i64);
    }

    let source_x = u32::try_from(
        destination_x_start
            .checked_sub(x)
            .ok_or(MalformedError::IntegerOverflow)?,
    )
    .map_err(|_| MalformedError::IntegerOverflow)?;
    let source_y = u32::try_from(
        destination_y_start
            .checked_sub(y)
            .ok_or(MalformedError::IntegerOverflow)?,
    )
    .map_err(|_| MalformedError::IntegerOverflow)?;
    let destination_x =
        u32::try_from(destination_x_start).map_err(|_| MalformedError::IntegerOverflow)?;
    let destination_y =
        u32::try_from(destination_y_start).map_err(|_| MalformedError::IntegerOverflow)?;
    let width = u32::try_from(destination_x_end - destination_x_start)
        .map_err(|_| MalformedError::IntegerOverflow)?;
    let height = u32::try_from(destination_y_end - destination_y_start)
        .map_err(|_| MalformedError::IntegerOverflow)?;
    destination.compose_region(
        pattern,
        source_x,
        source_y,
        destination_x,
        destination_y,
        width,
        height,
        operator,
    )?;
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::views::bitmap_fields(destination);
        reveal(crate::spec::bitmap::storage_valid);
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
    use std::hint::black_box;
    use std::time::{Duration, Instant};

    fn assert_decode_error_exact(actual: DecodeError, expected: DecodeError) {
        match (actual, expected) {
            (DecodeError::Malformed(actual), DecodeError::Malformed(expected)) => {
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
            (DecodeError::Unsupported(actual), DecodeError::Unsupported(expected)) => {
                assert_eq!(actual, expected);
            }
            (actual, expected) => panic!("wrong classification: {actual:?}, expected {expected:?}"),
        }
    }

    fn empty_request(coding: GenericRegionCoding) -> GenericRegionRequest<'static> {
        GenericRegionRequest {
            width: 0,
            height: 0,
            coding,
            typical_prediction: false,
            extended_template: false,
            skip: None,
        }
    }

    #[test]
    fn raw_gray_shape_guards_and_skip_rectangles_use_exact_half_open_geometry() {
        for (width, height) in [(8, 2), (9, 1)] {
            let mut current = bitmap(&["101010101", "010101010"]);
            let previous = Bitmap::blank(width, height, &Limits::default()).unwrap();
            let before = current.bytes().to_vec();
            assert!(matches!(
                xor_gray_plane_bytes(&mut current, &previous),
                Err(HalftoneError::Decode(DecodeError::Malformed(
                    MalformedError::IntegerOverflow
                )))
            ));
            assert_eq!(current.bytes(), before);
        }
        for x in [-10_i64, -2, -1, 0, 8, 9, 10] {
            for y in [-10_i64, -2, -1, 0, 2, 3, 4] {
                for width in [1_u32, 2, 9] {
                    for height in [1_u32, 2, 3] {
                        let right = x + i64::from(width);
                        let bottom = y + i64::from(height);
                        let overlaps = x < 9 && right > 0 && y < 3 && bottom > 0;
                        assert_eq!(
                            half_open_outside(x, y, right, bottom, width, height, 9, 3),
                            !overlaps
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn raw_halftone_memory_peaks_match_wide_stage_arithmetic() {
        let patterns = [bitmap(&["101010101", "010101010"])];
        for (grid_width, grid_height) in [
            (0, 0),
            (1, 1),
            (9, 3),
            (u32::MAX, 1_000_000_000),
            (u32::MAX, u32::MAX),
        ] {
            for (width, height) in [(0, 0), (9, 3), (u32::MAX, u32::MAX)] {
                for enable_skip in [false, true] {
                    for depth in [0, 1, 2, u8::MAX] {
                        let header = HalftoneHeader {
                            mmr: false,
                            template: 0,
                            enable_skip,
                            operator: CombinationOperator::Or,
                            default_pixel: false,
                            grid_width,
                            grid_height,
                            grid_x: 0,
                            grid_y: 0,
                            grid_vector_x: 256,
                            grid_vector_y: 0,
                        };
                        let cells = u128::from(grid_width) * u128::from(grid_height);
                        let plane = u128::from(grid_width).div_ceil(8) * u128::from(grid_height);
                        let skip = if enable_skip { plane } else { 0 };
                        let active = u128::from(depth.min(2));
                        let output = u128::from(width).div_ceil(8) * u128::from(height);
                        let fixed = 4 + 4 * cells + skip;
                        let peak = (fixed + active * plane).max(fixed + output);
                        let representable = u64::try_from(peak).unwrap_or(u64::MAX);
                        for limit in [0, representable.saturating_sub(1), representable, u64::MAX] {
                            let limits = Limits {
                                max_decoder_managed_memory_bytes: limit,
                                ..Limits::default()
                            };
                            let result = enforce_halftone_memory(
                                width, height, &header, &patterns, depth, &limits,
                            );
                            if peak > u128::from(u64::MAX) {
                                assert!(matches!(
                                    result,
                                    Err(HalftoneError::Decode(DecodeError::Malformed(
                                        MalformedError::IntegerOverflow
                                    )))
                                ));
                            } else if peak > u128::from(limit) {
                                assert!(matches!(result, Err(HalftoneError::Decode(
                                    DecodeError::LimitExceeded(LimitExceeded {
                                        kind: LimitKind::Memory, limit: actual_limit, actual,
                                    })
                                )) if actual_limit == limit && u128::from(actual) == peak));
                            } else {
                                result.unwrap();
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn raw_pattern_and_bitmap_preflights_preserve_exact_fields_and_priority() {
        for width in [0, 1, u8::MAX] {
            for height in [0, 1, u8::MAX] {
                for template in 0..=u8::MAX {
                    for mmr in [false, true] {
                        let header = PatternDictionaryHeader {
                            mmr,
                            template,
                            pattern_width: width,
                            pattern_height: height,
                            gray_max: u32::MAX,
                        };
                        let result = validate_pattern_header(header);
                        if width == 0 || height == 0 {
                            assert!(matches!(result,
                                Err(HalftoneError::InvalidPatternDimensions { width: w, height: h })
                                    if w == width && h == height));
                        } else if template > 3 || (mmr && template != 0) {
                            let encoded = u8::try_from((u16::from(template) * 2) & 255).unwrap()
                                | u8::from(mmr);
                            assert!(matches!(result,
                                Err(HalftoneError::InvalidPatternDictionaryFlags(actual))
                                    if actual == encoded));
                        } else {
                            result.unwrap();
                        }
                    }
                }
            }
        }
        let limits = Limits::default();
        for (width, height) in [(0, 0), (0, 1), (1, 0), (1, 1), (9, 2)] {
            let bitmap = Bitmap::blank(width, height, &limits).unwrap();
            for expected_width in [0, 1, 9, u32::MAX] {
                for expected_height in [0, 1, 2, u32::MAX] {
                    let result =
                        require_bitmap_dimensions(&bitmap, expected_width, expected_height);
                    if (width, height) == (expected_width, expected_height) {
                        result.unwrap();
                    } else {
                        assert!(
                            matches!(result, Err(HalftoneError::InvalidBitmapDimensions {
                            expected_width: ew, expected_height: eh,
                            actual_width: aw, actual_height: ah,
                        }) if (ew, eh, aw, ah) == (expected_width, expected_height, width, height))
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn raw_collective_slicing_matches_literal_pixels_and_exact_budget_boundaries() {
        let generous = Limits::default();
        for count in [1_u32, 2, 3, 8] {
            for width in [1_u8, 7, 8, 9] {
                let total_width = count * u32::from(width);
                let mut collective = Bitmap::blank(total_width, 2, &generous).unwrap();
                for y in 0..2 {
                    for x in 0..total_width {
                        collective.set_pixel(x, y, (x + y * 3) % 5 < 2).unwrap();
                    }
                }
                let before = collective.bytes().to_vec();
                let header = PatternDictionaryHeader {
                    mmr: false,
                    template: 0,
                    pattern_width: width,
                    pattern_height: 2,
                    gray_max: count - 1,
                };
                let memory = u64::from(total_width).div_ceil(8) * 2
                    + u64::from(count)
                        * (u64::from(width).div_ceil(8) * 2
                            + u64::try_from(core::mem::size_of::<Bitmap>()).unwrap());
                let pixels = u64::from(total_width) * 2;
                for memory_limit in [memory - 1, memory] {
                    for work_limit in [pixels - 1, pixels] {
                        let limits = Limits {
                            max_symbols: count,
                            max_decoder_managed_memory_bytes: memory_limit,
                            max_work_units: work_limit,
                            ..generous
                        };
                        let mut work = WorkBudget::new(&limits);
                        let result =
                            slice_collective_bitmap(&collective, header, &limits, &mut work);
                        if memory_limit < memory {
                            assert!(matches!(result, Err(HalftoneError::Decode(
                                DecodeError::LimitExceeded(LimitExceeded {
                                    kind: LimitKind::Memory, limit, actual,
                                })
                            )) if limit == memory_limit && actual == memory));
                            assert_eq!(work.used(), 0);
                        } else if work_limit < pixels {
                            assert!(matches!(result, Err(HalftoneError::Decode(
                                DecodeError::LimitExceeded(LimitExceeded {
                                    kind: LimitKind::Work, limit, actual,
                                })
                            )) if limit == work_limit && actual == pixels));
                            assert_eq!(work.used(), 0);
                        } else {
                            let patterns = result.unwrap();
                            assert_eq!(patterns.len(), usize::try_from(count).unwrap());
                            for (index, pattern) in patterns.iter().enumerate() {
                                let mut expected = vec![0_u8; usize::from(width).div_ceil(8) * 2];
                                for y in 0..2_usize {
                                    for x in 0..usize::from(width) {
                                        if (index * usize::from(width) + x + y * 3) % 5 < 2 {
                                            expected[y * usize::from(width).div_ceil(8) + x / 8] |=
                                                0x80 >> (x % 8);
                                        }
                                    }
                                }
                                assert_eq!(pattern.bytes(), expected);
                            }
                            assert_eq!(work.used(), pixels);
                        }
                        assert_eq!(collective.bytes(), before);
                    }
                }
            }
        }
    }

    #[test]
    fn raw_render_extreme_coordinates_preserve_storage_on_checked_overflow() {
        let pattern = bitmap(&["101", "010"]);
        for x in [i64::MIN, -3, -1, 0, 8, i64::MAX - 2, i64::MAX] {
            for y in [i64::MIN, -2, -1, 0, 2, i64::MAX - 1, i64::MAX] {
                for operator in [
                    CombinationOperator::Or,
                    CombinationOperator::And,
                    CombinationOperator::Xor,
                    CombinationOperator::Xnor,
                    CombinationOperator::Replace,
                ] {
                    let mut destination = bitmap(&["101010101", "010101010", "111000111"]);
                    let before = destination.bytes().to_vec();
                    let result = render_clipped_pattern(&mut destination, &pattern, x, y, operator);
                    let overflows = i128::from(x) + 3 > i128::from(i64::MAX)
                        || i128::from(y) + 2 > i128::from(i64::MAX);
                    if overflows {
                        assert!(matches!(
                            result,
                            Err(HalftoneError::Decode(DecodeError::Malformed(
                                MalformedError::IntegerOverflow
                            )))
                        ));
                        assert_eq!(destination.bytes(), before);
                    } else {
                        result.unwrap();
                        let mut expected = bitmap(&["101010101", "010101010", "111000111"]);
                        scalar_render_clipped_pattern_oracle(
                            &mut expected,
                            &pattern,
                            x,
                            y,
                            operator,
                        )
                        .unwrap();
                        assert_eq!(destination.bytes(), expected.bytes());
                    }
                }
            }
        }
    }

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

    #[test]
    fn arithmetic_state_constructors_enforce_exact_input_and_context_budgets() {
        let coded = [0, 0, 0xff, 0xac];
        for (code, template, bits) in [
            (0, GenericTemplate::Template0, 16),
            (1, GenericTemplate::Template1, 13),
            (2, GenericTemplate::Template2, 10),
            (3, GenericTemplate::Template3, 10),
        ] {
            let bytes = 2_u64 << bits;
            let limits = Limits {
                max_stream_bytes: 4,
                max_decoder_managed_memory_bytes: bytes,
                ..Limits::default()
            };
            for pattern in [false, true] {
                let create = |data, limits: &Limits| {
                    if pattern {
                        new_pattern_arithmetic_state(data, template, code, limits)
                    } else {
                        new_halftone_arithmetic_state(data, template, code, limits)
                    }
                };
                assert!(create(&coded, &limits).is_ok());
                for data in [&[][..], &[0][..], &[0, 0][..], &[0xff, 0x90][..]] {
                    let Err(error) = create(data, &limits) else {
                        panic!("missing terminator accepted")
                    };
                    assert_decode_error_exact(
                        error,
                        DecodeError::Malformed(MalformedError::InvalidGenericRegion(
                            GenericRegionErrorKind::InvalidArithmeticTerminator,
                        )),
                    );
                }
                let Err(error) = create(
                    &coded,
                    &Limits {
                        max_decoder_managed_memory_bytes: bytes - 1,
                        ..limits
                    },
                ) else {
                    panic!("context budget exceeded")
                };
                assert_decode_error_exact(
                    error,
                    DecodeError::LimitExceeded(LimitExceeded {
                        kind: LimitKind::Memory,
                        limit: bytes - 1,
                        actual: bytes,
                    }),
                );
                let Err(error) = create(
                    &coded,
                    &Limits {
                        max_stream_bytes: 3,
                        max_decoder_managed_memory_bytes: 0,
                        ..limits
                    },
                ) else {
                    panic!("input budget exceeded")
                };
                assert_decode_error_exact(
                    error,
                    DecodeError::LimitExceeded(LimitExceeded {
                        kind: LimitKind::StreamBytes,
                        limit: 3,
                        actual: 4,
                    }),
                );
            }
        }
    }

    #[test]
    fn arithmetic_state_rejects_mode_and_template_changes_without_consumption() {
        let coded = [0, 0, 0xff, 0xac];
        for (code, template) in [
            (0, GenericTemplate::Template0),
            (1, GenericTemplate::Template1),
            (2, GenericTemplate::Template2),
            (3, GenericTemplate::Template3),
        ] {
            for pattern in [false, true] {
                let mut state = if pattern {
                    new_pattern_arithmetic_state(&coded, template, code, &Limits::default())
                        .unwrap()
                } else {
                    new_halftone_arithmetic_state(&coded, template, code, &Limits::default())
                        .unwrap()
                };
                for (coding, reason) in [
                    (
                        GenericRegionCoding::Mmr {
                            require_eofb: false,
                        },
                        if pattern {
                            "pattern dictionary coding mode changed"
                        } else {
                            "halftone coding mode changed"
                        },
                    ),
                    (
                        GenericRegionCoding::Arithmetic {
                            template: (code + 1) % 4,
                            adaptive_pixels: [(0, 0); 4],
                        },
                        if pattern {
                            "pattern dictionary template changed"
                        } else {
                            "halftone template changed"
                        },
                    ),
                ] {
                    let Err(error) = state.decode_generic_region(empty_request(coding)) else {
                        panic!("changed coding accepted")
                    };
                    assert_decode_error_exact(
                        error,
                        DecodeError::Malformed(MalformedError::InvalidPageState(reason)),
                    );
                    let GenericRegionState::Arithmetic { session, .. } = &state else {
                        panic!("state variant changed")
                    };
                    assert_eq!(session.decoder.byte_position(), 1);
                    assert_eq!(session.decoder.marker(), None);
                    assert_eq!(session.work_used, 0);
                }
                let image = state
                    .decode_generic_region(empty_request(GenericRegionCoding::Arithmetic {
                        template: code,
                        adaptive_pixels: gray_adaptive_pixels(code),
                    }))
                    .expect("zero-area helper bitmap");
                assert_eq!((image.width(), image.height(), image.data.len()), (0, 0, 0));
                let Err(error) = state.decode_generic_region(GenericRegionRequest {
                    width: 1,
                    height: 1,
                    coding: GenericRegionCoding::Arithmetic {
                        template: code,
                        adaptive_pixels: [(0, 0); 4],
                    },
                    ..empty_request(GenericRegionCoding::Arithmetic {
                        template: code,
                        adaptive_pixels: [(0, 0); 4],
                    })
                }) else {
                    panic!("noncausal adaptive pixel accepted")
                };
                assert_decode_error_exact(
                    error,
                    DecodeError::Malformed(MalformedError::InvalidGenericRegion(
                        GenericRegionErrorKind::InvalidAdaptivePixel {
                            number: 1,
                            x: 0,
                            y: 0,
                        },
                    )),
                );
                state.finish().expect("no wrong marker was observed");
            }
        }
    }

    #[test]
    fn pattern_mmr_is_single_use_and_mode_errors_do_not_consume_it() {
        let limits = Limits::default();
        let correct = empty_request(GenericRegionCoding::Mmr {
            require_eofb: false,
        });
        let mut state = new_pattern_mmr_state(&[], &limits);
        let Err(error) = state.decode_generic_region(empty_request(GenericRegionCoding::Mmr {
            require_eofb: true,
        })) else {
            panic!("changed mode accepted")
        };
        assert_decode_error_exact(
            error,
            DecodeError::Malformed(MalformedError::InvalidPageState(
                "pattern dictionary coding mode changed",
            )),
        );
        let image = state
            .decode_generic_region(correct)
            .expect("empty helper bitmap");
        assert_eq!((image.width(), image.height(), image.data.len()), (0, 0, 0));
        let Err(error) = state.decode_generic_region(correct) else {
            panic!("reused MMR bitmap accepted")
        };
        assert_decode_error_exact(
            error,
            DecodeError::Malformed(MalformedError::InvalidPageState(
                "pattern dictionary requested multiple MMR bitmaps",
            )),
        );
        state.finish().unwrap();
        let mut state = new_pattern_mmr_state(&[], &limits);
        let Err(error) = state.decode_generic_region(GenericRegionRequest {
            width: 1,
            height: 1,
            ..correct
        }) else {
            panic!("missing mode accepted")
        };
        assert_decode_error_exact(
            error,
            DecodeError::Malformed(MalformedError::UnexpectedEof {
                offset: 0,
                needed: 1,
                remaining: 0,
            }),
        );
        let Err(error) = state.decode_generic_region(correct) else {
            panic!("failed state reused")
        };
        assert_decode_error_exact(
            error,
            DecodeError::Malformed(MalformedError::InvalidPageState(
                "pattern dictionary requested multiple MMR bitmaps",
            )),
        );
    }

    #[test]
    fn gray_mmr_consumes_exact_prefixes_and_requires_all_declared_bytes() {
        let coded = [0, 0x10, 1, 0, 0x10, 1];
        let mut state = new_gray_mmr_state(&coded, &Limits::default());
        assert_decode_error_exact(
            state.finish().unwrap_err(),
            DecodeError::Malformed(MalformedError::TrailingData(6)),
        );
        let Err(error) = state.decode_generic_region(empty_request(GenericRegionCoding::Mmr {
            require_eofb: false,
        })) else {
            panic!("optional EOFB accepted for gray plane")
        };
        assert_decode_error_exact(
            error,
            DecodeError::Malformed(MalformedError::InvalidPageState(
                "halftone coding mode changed",
            )),
        );
        let request = empty_request(GenericRegionCoding::Mmr { require_eofb: true });
        for expected_offset in [3, 6] {
            let bitmap = state
                .decode_generic_region(request)
                .expect("exact EOFB prefix");
            assert_eq!((bitmap.width(), bitmap.height()), (0, 0));
            let GenericRegionState::GrayMmr { offset, .. } = &state else {
                panic!("wrong state")
            };
            assert_eq!(*offset, expected_offset);
            if expected_offset == 3 {
                assert_decode_error_exact(
                    state.finish().unwrap_err(),
                    DecodeError::Malformed(MalformedError::TrailingData(3)),
                );
            }
        }
        state.finish().unwrap();
        let Err(error) = state.decode_generic_region(request) else {
            panic!("missing EOFB accepted")
        };
        assert_decode_error_exact(
            error,
            DecodeError::Malformed(MalformedError::InvalidMmr(MmrErrorKind::MissingEofb)),
        );
        let GenericRegionState::GrayMmr { offset, .. } = state else {
            panic!("wrong state")
        };
        assert_eq!(offset, 6);
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn header_flags_fields_and_every_truncation_keep_exact_cursor_and_errors() {
        for flags in 0..=u8::MAX {
            let pattern = [flags, 1, 2, 0xff, 0xff, 0xff, 0xff];
            let mut reader = ByteReader::new(&pattern);
            if flags >= 8 || flags & 1 != 0 && flags & 6 != 0 {
                assert!(matches!(parse_pattern_dictionary_header(&mut reader),
                    Err(HalftoneError::InvalidPatternDictionaryFlags(actual)) if actual == flags));
                assert_eq!(reader.position(), 1);
            } else {
                let result = parse_pattern_dictionary_header(&mut reader).unwrap();
                assert_eq!(
                    (
                        result.mmr,
                        result.template,
                        result.pattern_width,
                        result.pattern_height
                    ),
                    (flags & 1 != 0, flags >> 1, 1, 2)
                );
                assert_eq!(result.pattern_count(), u64::from(u32::MAX) + 1);
                assert_eq!(reader.position(), 7);
            }
            let mut bytes = vec![flags];
            for value in [u32::MAX, 0, 0x8000_0000, 0x7fff_ffff] {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
            bytes.extend_from_slice(&[0xff, 0xff, 0, 0]);
            let mut reader = ByteReader::new(&bytes);
            let result = parse_halftone_header(&mut reader);
            let operator = (flags >> 4) & 7;
            if flags & 1 != 0 && flags & 14 != 0 {
                assert!(
                    matches!(result, Err(HalftoneError::InvalidHalftoneFlags(actual)) if actual == flags)
                );
                assert_eq!(reader.position(), 1);
            } else if operator > 4 {
                assert!(
                    matches!(result, Err(HalftoneError::Decode(DecodeError::Malformed(
                    MalformedError::InvalidCombinationOperator(actual)))) if actual == operator)
                );
                assert_eq!(reader.position(), 1);
            } else {
                let result = result.unwrap();
                assert_eq!(
                    (
                        result.grid_width,
                        result.grid_height,
                        result.grid_x,
                        result.grid_y
                    ),
                    (u32::MAX, 0, i32::MIN, i32::MAX)
                );
                assert_eq!((result.grid_vector_x, result.grid_vector_y), (u16::MAX, 0));
                assert_eq!(
                    result.operator,
                    [
                        CombinationOperator::Or,
                        CombinationOperator::And,
                        CombinationOperator::Xor,
                        CombinationOperator::Xnor,
                        CombinationOperator::Replace
                    ][usize::from(operator)]
                );
                assert_eq!(
                    (
                        result.mmr,
                        result.template,
                        result.enable_skip,
                        result.default_pixel
                    ),
                    (
                        flags & 1 != 0,
                        (flags >> 1) & 3,
                        flags & 8 != 0,
                        flags & 0x80 != 0
                    )
                );
                assert_eq!(reader.position(), 21);
            }
        }
        for (bytes, lengths) in [
            (vec![0, 1, 2, 0, 0, 0, 0], vec![1, 1, 1, 4]),
            (vec![0; 21], vec![1, 4, 4, 4, 4, 2, 2]),
        ] {
            for cut in 0..bytes.len() {
                let mut offset = 0;
                let needed = *lengths
                    .iter()
                    .find(|&&length| {
                        if offset + length > cut {
                            true
                        } else {
                            offset += length;
                            false
                        }
                    })
                    .unwrap();
                let mut reader = ByteReader::new(&bytes[..cut]);
                let error = if bytes.len() == 7 {
                    parse_pattern_dictionary_header(&mut reader).unwrap_err()
                } else {
                    parse_halftone_header(&mut reader).unwrap_err()
                };
                assert!(
                    matches!(error, HalftoneError::Decode(DecodeError::Malformed(
                    MalformedError::UnexpectedEof { offset: actual, needed: actual_needed, remaining }
                )) if actual == offset as u64 && actual_needed == needed as u64 && remaining == (cut - offset) as u64)
                );
                assert_eq!(reader.position(), offset);
            }
        }
    }

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

    fn next_random(state: &mut u64) -> u32 {
        *state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (*state >> 32) as u32
    }

    fn random_bitmap(width: u32, height: u32, state: &mut u64) -> Bitmap {
        let mut output =
            Bitmap::blank(width, height, &generous_limits()).expect("random bitmap allocates");
        for byte in &mut output.data {
            *byte = u8::try_from(next_random(state) & 0xff).expect("bounded byte");
        }
        if width % 8 != 0 {
            let mask = 0xff_u8 << (8 - width % 8);
            for row in 0..height {
                let row_start = usize::try_from(row).expect("row fits") * output.stride;
                output.data[row_start + output.stride - 1] &= mask;
            }
        }
        output
    }

    fn scalar_xor_gray_plane_oracle(
        current: &mut Bitmap,
        previous: &Bitmap,
    ) -> Result<(), HalftoneError> {
        for y in 0..current.height {
            for x in 0..current.width {
                let coded = current.pixel(x, y).ok_or(MalformedError::IntegerOverflow)?;
                let previous = previous
                    .pixel(x, y)
                    .ok_or(MalformedError::IntegerOverflow)?;
                current.set_pixel(x, y, gray_to_binary_bit(coded, Some(previous)))?;
            }
        }
        Ok(())
    }

    fn scalar_accumulate_gray_byte_oracle(
        values: &mut [u32],
        offset: usize,
        byte: u8,
        plane_index: u8,
    ) {
        for bit in 0..8 {
            if byte & (0x80 >> bit) != 0 {
                values[offset + bit] |= 1_u32 << plane_index;
            }
        }
    }

    fn scalar_render_clipped_pattern_oracle(
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
        let (x_start, x_end) = clipped_axis(x, right, destination.width());
        let (y_start, y_end) = clipped_axis(y, bottom, destination.height());
        for destination_y in y_start..y_end {
            for destination_x in x_start..x_end {
                let source_x = u32::try_from(destination_x - x)
                    .map_err(|_| MalformedError::IntegerOverflow)?;
                let source_y = u32::try_from(destination_y - y)
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

    fn median_duration(mut values: Vec<Duration>) -> Duration {
        values.sort_unstable();
        values[values.len() / 2]
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
        let mut adapter = |request: GenericRegionRequest<'_>| {
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
        let mut decoder = GenericRegionState::scripted(&mut adapter);
        let limits = generous_limits();
        let mut work = WorkBudget::new(&limits);
        let patterns = decode_pattern_dictionary(header, &mut decoder, &limits, &mut work)
            .expect("pattern dictionary should decode");
        assert_eq!(calls, 1);
        assert_eq!(patterns.len(), 2);
    }

    #[test]
    fn bytewise_gray_xor_matches_scalar_pixels_randomized() {
        let mut state = 0x6a18_ef03_91c5_d247;
        for width in [1, 2, 7, 8, 9, 15, 31, 32, 33, 63, 127] {
            for height in [1, 2, 5, 17] {
                let current = random_bitmap(width, height, &mut state);
                let previous = random_bitmap(width, height, &mut state);
                let mut expected = current.clone();
                let mut actual = current;
                scalar_xor_gray_plane_oracle(&mut expected, &previous)
                    .expect("scalar gray conversion");
                xor_gray_plane_bytes(&mut actual, &previous).expect("bytewise gray conversion");
                assert_eq!(actual, expected, "dimensions {width}x{height}");
            }
        }
    }

    #[test]
    fn fixed_gray_extraction_matches_scalar_oracle() {
        let mut state = 0x39d2_a175_64fe_08bc;
        for plane_index in [0, 1, 7, 15, 23, 31] {
            for byte in 0..=u8::MAX {
                let mut expected = vec![0_u32; 24];
                for value in &mut expected {
                    *value = next_random(&mut state);
                }
                let mut actual = expected.clone();
                scalar_accumulate_gray_byte_oracle(&mut expected, 8, byte, plane_index);
                accumulate_gray_byte(&mut actual, 8, byte, plane_index).expect("fixed extraction");
                assert_eq!(actual, expected);
            }
        }
    }

    #[test]
    #[ignore = "microbenchmark; run explicitly with --release --ignored --nocapture"]
    fn benchmark_bytewise_gray_xor_against_scalar_oracle() {
        let mut state = 0x842c_017d_a639_5bef;
        let current = random_bitmap(2048, 2048, &mut state);
        let previous = random_bitmap(2048, 2048, &mut state);
        let mut bytewise_times = Vec::new();
        let mut scalar_times = Vec::new();
        for sample in 0..11 {
            let mut bytewise = current.clone();
            let mut scalar = current.clone();
            if sample & 1 == 0 {
                let start = Instant::now();
                xor_gray_plane_bytes(&mut bytewise, &previous).expect("bytewise xor");
                bytewise_times.push(start.elapsed());
                black_box(bytewise.bytes());

                let start = Instant::now();
                scalar_xor_gray_plane_oracle(&mut scalar, &previous).expect("scalar xor");
                scalar_times.push(start.elapsed());
                black_box(scalar.bytes());
            } else {
                let start = Instant::now();
                scalar_xor_gray_plane_oracle(&mut scalar, &previous).expect("scalar xor");
                scalar_times.push(start.elapsed());
                black_box(scalar.bytes());

                let start = Instant::now();
                xor_gray_plane_bytes(&mut bytewise, &previous).expect("bytewise xor");
                bytewise_times.push(start.elapsed());
                black_box(bytewise.bytes());
            }
            assert_eq!(bytewise, scalar);
        }
        let bytewise = median_duration(bytewise_times);
        let scalar = median_duration(scalar_times);
        println!(
            "gray-xor scalar_median_ns={} bytewise_median_ns={} speedup={:.3}",
            scalar.as_nanos(),
            bytewise.as_nanos(),
            scalar.as_secs_f64() / bytewise.as_secs_f64(),
        );
    }

    #[test]
    #[ignore = "microbenchmark; run explicitly with --release --ignored --nocapture"]
    fn benchmark_fixed_gray_extraction_against_scalar_oracle() {
        let mut state = 0x19b4_7c2e_d560_83af;
        let mut bytes = vec![0_u8; 262_144];
        for byte in &mut bytes {
            *byte = u8::try_from(next_random(&mut state) & 0xff).expect("bounded byte");
        }
        let original = vec![0_u32; bytes.len() * 8];
        let mut fixed_times = Vec::new();
        let mut scalar_times = Vec::new();
        for sample in 0..11 {
            let mut fixed = original.clone();
            let mut scalar = original.clone();
            let run_fixed = |values: &mut [u32]| {
                let start = Instant::now();
                for (index, byte) in bytes.iter().copied().enumerate() {
                    accumulate_gray_byte(values, index * 8, byte, 13).expect("fixed extraction");
                }
                let elapsed = start.elapsed();
                black_box(values);
                elapsed
            };
            let run_scalar = |values: &mut [u32]| {
                let start = Instant::now();
                for (index, byte) in bytes.iter().copied().enumerate() {
                    scalar_accumulate_gray_byte_oracle(values, index * 8, byte, 13);
                }
                let elapsed = start.elapsed();
                black_box(values);
                elapsed
            };
            if sample & 1 == 0 {
                fixed_times.push(run_fixed(&mut fixed));
                scalar_times.push(run_scalar(&mut scalar));
            } else {
                scalar_times.push(run_scalar(&mut scalar));
                fixed_times.push(run_fixed(&mut fixed));
            }
            assert_eq!(fixed, scalar);
        }
        let fixed = median_duration(fixed_times);
        let scalar = median_duration(scalar_times);
        println!(
            "gray-extraction scalar_median_ns={} fixed_median_ns={} speedup={:.3}",
            scalar.as_nanos(),
            fixed.as_nanos(),
            scalar.as_secs_f64() / fixed.as_secs_f64(),
        );
    }

    #[test]
    fn reconstructs_gray_planes_most_significant_first() {
        let mut planes = VecDeque::from([bitmap(&["1110"]), bitmap(&["0111"]), bitmap(&["0010"])]);
        let mut adapter = |request: GenericRegionRequest<'_>| {
            assert!(matches!(
                request.coding,
                GenericRegionCoding::Arithmetic { template: 0, .. }
            ));
            assert!(!request.typical_prediction);
            assert!(!request.extended_template);
            Ok(planes.pop_front().expect("three planes are supplied"))
        };
        let mut decoder = GenericRegionState::scripted(&mut adapter);
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
        let mut adapter = |request: GenericRegionRequest<'_>| {
            calls += 1;
            assert_eq!(
                request.coding,
                GenericRegionCoding::Mmr { require_eofb: true }
            );
            Ok(bitmap(&["0"]))
        };
        let mut decoder = GenericRegionState::scripted(&mut adapter);
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
        let mut adapter = |_request: GenericRegionRequest<'_>| {
            calls += 1;
            Bitmap::blank(1, 1, &generous_limits())
        };
        let mut decoder = GenericRegionState::scripted(&mut adapter);
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
        let mut adapter = |_request: GenericRegionRequest<'_>| {
            Ok(planes.pop_front().expect("one plane is supplied"))
        };
        let mut decoder = GenericRegionState::scripted(&mut adapter);
        let mut work = WorkBudget::new(&limits);
        let output =
            decode_halftone_region(&header, 4, 2, &patterns, &mut decoder, &limits, &mut work)
                .expect("halftone should render");
        assert_eq!(output, bitmap(&["0010", "1110"]));
    }

    #[test]
    fn clipped_pattern_rendering_exhaustively_matches_scalar_oracle() {
        let limits = generous_limits();
        let operators = [
            CombinationOperator::Or,
            CombinationOperator::And,
            CombinationOperator::Xor,
            CombinationOperator::Xnor,
            CombinationOperator::Replace,
        ];
        for pattern_width in 1..=10 {
            for destination_width in 1..=10 {
                let pattern =
                    Bitmap::filled(pattern_width, 3, true, &limits).expect("pattern bitmap");
                let original =
                    Bitmap::filled(destination_width, 4, false, &limits).expect("destination");
                for x in -12..=12 {
                    for y in -4..=4 {
                        for operator in operators {
                            let mut expected = original.clone();
                            let mut actual = original.clone();
                            scalar_render_clipped_pattern_oracle(
                                &mut expected,
                                &pattern,
                                x,
                                y,
                                operator,
                            )
                            .expect("scalar render");
                            render_clipped_pattern(&mut actual, &pattern, x, y, operator)
                                .expect("chunk render");
                            assert_eq!(actual, expected);
                        }
                    }
                }
            }
        }
    }

    #[test]
    #[ignore = "microbenchmark; run explicitly with --release --ignored --nocapture"]
    fn benchmark_halftone_render_route_against_6333205_scalar_oracle() {
        let limits = generous_limits();
        let pattern = Bitmap::filled(1024, 512, true, &limits).expect("pattern");
        let original = Bitmap::blank(1024, 512, &limits).expect("destination");
        let mut chunk_times = Vec::new();
        let mut scalar_times = Vec::new();
        for sample in 0..11 {
            let mut chunk = original.clone();
            let mut scalar = original.clone();
            if sample % 2 == 0 {
                let start = Instant::now();
                render_clipped_pattern(&mut chunk, &pattern, -3, -2, CombinationOperator::Xor)
                    .expect("chunk render");
                chunk_times.push(start.elapsed());
                black_box(chunk.bytes());

                let start = Instant::now();
                scalar_render_clipped_pattern_oracle(
                    &mut scalar,
                    &pattern,
                    -3,
                    -2,
                    CombinationOperator::Xor,
                )
                .expect("scalar render");
                scalar_times.push(start.elapsed());
                black_box(scalar.bytes());
            } else {
                let start = Instant::now();
                scalar_render_clipped_pattern_oracle(
                    &mut scalar,
                    &pattern,
                    -3,
                    -2,
                    CombinationOperator::Xor,
                )
                .expect("scalar render");
                scalar_times.push(start.elapsed());
                black_box(scalar.bytes());

                let start = Instant::now();
                render_clipped_pattern(&mut chunk, &pattern, -3, -2, CombinationOperator::Xor)
                    .expect("chunk render");
                chunk_times.push(start.elapsed());
                black_box(chunk.bytes());
            }
            assert_eq!(chunk, scalar);
        }
        let chunk = median_duration(chunk_times);
        let scalar = median_duration(scalar_times);
        println!(
            "halftone scalar_6333205_median_ns={} chunk_median_ns={} speedup={:.3}",
            scalar.as_nanos(),
            chunk.as_nanos(),
            scalar.as_secs_f64() / chunk.as_secs_f64(),
        );
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

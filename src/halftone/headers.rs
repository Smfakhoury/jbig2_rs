#[cfg(verus_keep_ghost)]
use super::halftone_spec;
use super::{HalftoneError, HalftoneHeader, PatternDictionaryHeader};
use crate::bitmap::CombinationOperator;
use crate::reader::ByteReader;
use crate::MalformedError;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures
        out == halftone_spec::pattern_flags_valid(flags),
        out ==> (flags & 1 == 0 || (flags / 2) % 4 == 0),
))]
#[allow(clippy::verbose_bit_mask)]
pub(super) const fn pattern_flags_valid(flags: u8) -> bool {
    flags & 0xf8 == 0 && (flags & 1 == 0 || (flags / 2) % 4 == 0)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out == (flags / 2) % 4, out <= 3,
))]
pub(super) fn template_from_flags(flags: u8) -> u8 {
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

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures
        out == halftone_spec::halftone_flags_valid(flags),
        out ==> (
            flags & 1 == 0
            || ((flags / 2) % 4 == 0 && flags & 8 == 0)
        ),
))]
#[allow(clippy::verbose_bit_mask)]
pub(super) const fn halftone_flags_valid(flags: u8) -> bool {
    flags & 1 == 0 || ((flags / 2) % 4 == 0 && flags & 8 == 0)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out == halftone_spec::operator_for_code(code),
))]
pub(super) const fn combination_operator(code: u8) -> Option<CombinationOperator> {
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
pub(super) fn signed_grid_origin(value: u32) -> i32 {
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

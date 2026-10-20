use crate::reader::ByteReader;
use crate::{DecodeError, LimitExceeded, LimitKind, Limits, MalformedError};
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg(verus_keep_ghost)]
#[rustfmt::skip]
#[path = "../proofs/spec_properties/segment_header.rs"]
mod segment_header_properties;

#[cfg(verus_keep_ghost)]
#[rustfmt::skip]
#[path = "../proofs/shared/segment_header.rs"]
mod segment_header_shared;

#[cfg(verus_keep_ghost)]
#[macro_use]
#[rustfmt::skip]
#[path = "../proofs/production_impl/segment_header.rs"]
mod segment_header_proofs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) enum SegmentType {
    SymbolDictionary = 0,
    IntermediateTextRegion = 4,
    ImmediateTextRegion = 6,
    ImmediateLosslessTextRegion = 7,
    PatternDictionary = 16,
    IntermediateHalftoneRegion = 20,
    ImmediateHalftoneRegion = 22,
    ImmediateLosslessHalftoneRegion = 23,
    IntermediateGenericRegion = 36,
    ImmediateGenericRegion = 38,
    ImmediateLosslessGenericRegion = 39,
    IntermediateGenericRefinementRegion = 40,
    ImmediateGenericRefinementRegion = 42,
    ImmediateLosslessGenericRefinementRegion = 43,
    PageInformation = 48,
    EndOfPage = 49,
    EndOfStripe = 50,
    EndOfFile = 51,
    Profiles = 52,
    Tables = 53,
    ColourPalette = 54,
    Extension = 62,
}

impl SegmentType {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == self as u8,
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    const fn number(self) -> u8 {
        self as u8
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures match out {
            Ok(segment_type) =>
                crate::spec::segments::segment_type(number) == Some(segment_type),
            Err(_) => crate::spec::segments::segment_type(number).is_none(),
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    fn from_number(number: u8) -> Result<Self, MalformedError> {
        let segment_type = match number {
            0 => Self::SymbolDictionary,
            4 => Self::IntermediateTextRegion,
            6 => Self::ImmediateTextRegion,
            7 => Self::ImmediateLosslessTextRegion,
            16 => Self::PatternDictionary,
            20 => Self::IntermediateHalftoneRegion,
            22 => Self::ImmediateHalftoneRegion,
            23 => Self::ImmediateLosslessHalftoneRegion,
            36 => Self::IntermediateGenericRegion,
            38 => Self::ImmediateGenericRegion,
            39 => Self::ImmediateLosslessGenericRegion,
            40 => Self::IntermediateGenericRefinementRegion,
            42 => Self::ImmediateGenericRefinementRegion,
            43 => Self::ImmediateLosslessGenericRefinementRegion,
            48 => Self::PageInformation,
            49 => Self::EndOfPage,
            50 => Self::EndOfStripe,
            51 => Self::EndOfFile,
            52 => Self::Profiles,
            53 => Self::Tables,
            54 => Self::ColourPalette,
            62 => Self::Extension,
            _ => return Err(MalformedError::ReservedSegmentType(number)),
        };
        Ok(segment_type)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct RetentionFlags {
    pub(crate) values: Vec<u8>,
}

impl RetentionFlags {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures match out {
            Ok(flags) =>
                flags.values@.len() == count as nat + 1
                && crate::spec::segments::retention_values_match(flags.values@, bytes@),
            Err(_) => true,
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    fn from_bytes(count: u32, bytes: &[u8]) -> Result<Self, DecodeError> {
        let value_count = count
            .checked_add(1)
            .ok_or(MalformedError::IntegerOverflow)?;
        let Ok(capacity) = usize::try_from(value_count) else {
            return Err(MalformedError::IntegerOverflow.into());
        };
        let mut values = Vec::new();
        if values.try_reserve(capacity).is_err() {
            return Err(DecodeError::Allocation {
                requested_bytes: u64::from(value_count),
            });
        }

        fill_retention_values(&mut values, value_count, bytes)?;
        Ok(Self { values })
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires self.values@.len() > 0,
        ensures out == (self.values@[0] != 0),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn retain_this_segment(&self) -> bool {
        self.values[0] != 0
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures match out {
            Some(value) =>
                index < usize::MAX
                    && index + 1 < self.values@.len()
                    && value == (self.values@[(index + 1) as int] != 0),
            None => index == usize::MAX || index + 1 >= self.values@.len(),
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn retain_referred_segment(&self, index: usize) -> Option<bool> {
        let offset = index.checked_add(1)?;
        if offset >= self.values.len() {
            return None;
        }
        Some(self.values[offset] != 0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct SegmentHeader {
    pub(crate) number: u32,
    pub(crate) segment_type: SegmentType,
    pub(crate) deferred_non_retain: bool,
    pub(crate) referred_segments: Vec<u32>,
    pub(crate) retention_flags: RetentionFlags,
    pub(crate) page_association: u32,
    pub(crate) data_length: Option<u32>,
    pub(crate) byte_length: usize,
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires old(reader).position <= old(reader).bytes@.len(),
    ensures
        final(reader).bytes@ == old(reader).bytes@,
        final(reader).position <= final(reader).bytes@.len(),
        out.is_ok() ==> final(reader).position > old(reader).position,
))]
pub(crate) fn parse_segment_header(
    reader: &mut ByteReader<'_>,
    limits: &Limits,
) -> Result<SegmentHeader, DecodeError> {
    let start = reader.position();
    let number = reader.read_u32()?;
    let flags = reader.read_u8()?;
    let segment_type = SegmentType::from_number(flags & 0x3F)?;
    let page_association_is_long = flags & 0x40 != 0;
    let deferred_non_retain = flags & 0x80 != 0;
    let (referred_segments, retention_flags) = parse_referred_segments(reader, number, limits)?;

    let page_association = read_page_association(reader, page_association_is_long)?;
    let encoded_data_length = reader.read_u32()?;
    let data_length = decode_data_length(segment_type, encoded_data_length)?;
    let byte_length = header_byte_length(start, reader.position())?;

    Ok(SegmentHeader {
        number,
        segment_type,
        deferred_non_retain,
        referred_segments,
        retention_flags,
        page_association,
        data_length,
        byte_length,
    })
}

#[cfg_attr(verus_keep_ghost, verus_spec(
    requires old(reader).position <= old(reader).bytes@.len(),
    ensures
        final(reader).bytes@ == old(reader).bytes@,
        final(reader).position <= final(reader).bytes@.len(),
        final(reader).position >= old(reader).position,
))]
fn parse_referred_segments(
    reader: &mut ByteReader<'_>,
    segment_number: u32,
    limits: &Limits,
) -> Result<(Vec<u32>, RetentionFlags), DecodeError> {
    let first_count_byte = reader.read_u8()?;
    let count_code = first_count_byte >> 5;
    validate_count_code(count_code)?;
    let (count, retention_flags) = match count_code {
        0..=4 => {
            let count = validate_short_count(first_count_byte)?;
            let bytes = [first_count_byte];
            (count, RetentionFlags::from_bytes(count, &bytes)?)
        }
        7 => {
            let remaining_count_bytes = reader.read_bytes(3)?;
            let count_word = (u32::from(first_count_byte) << 24)
                | (u32::from(remaining_count_bytes[0]) << 16)
                | (u32::from(remaining_count_bytes[1]) << 8)
                | u32::from(remaining_count_bytes[2]);
            let count = validate_long_count(count_word)?;
            enforce_reference_limit(count, limits)?;
            let retention_bit_count = count + 1;
            let retention_byte_count = retention_byte_count(count)?;
            let bytes = reader.read_bytes(retention_byte_count)?;
            validate_unused_retention_bits(retention_bit_count, bytes)?;
            (count, RetentionFlags::from_bytes(count, bytes)?)
        }
        _ => return Err(MalformedError::IntegerOverflow.into()),
    };
    enforce_reference_limit(count, limits)?;

    let Ok(count_usize) = usize::try_from(count) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    let reference_bytes = reference_width(segment_number);
    let allocation_bytes = u64::from(count)
        .checked_mul(4)
        .ok_or(MalformedError::IntegerOverflow)?;
    let mut referred_segments = Vec::new();
    if referred_segments.try_reserve(count_usize).is_err() {
        return Err(DecodeError::Allocation {
            requested_bytes: allocation_bytes,
        });
    }
    read_referred_segments_into(
        reader,
        segment_number,
        count,
        reference_bytes,
        &mut referred_segments,
    )?;
    Ok((referred_segments, retention_flags))
}

#[cfg_attr(verus_keep_ghost, verus_spec(
    requires old(reader).position <= old(reader).bytes@.len(),
    ensures
        final(reader).bytes@ == old(reader).bytes@,
        final(reader).position <= final(reader).bytes@.len(),
        final(reader).position >= old(reader).position,
))]
fn read_referred_segments_into(
    reader: &mut ByteReader<'_>,
    segment_number: u32,
    count: u32,
    reference_bytes: u8,
    referred_segments: &mut Vec<u32>,
) -> Result<(), DecodeError> {
    let mut index = 0_u32;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index <= count,
            reader.position <= reader.bytes@.len(),
            reader.bytes@ == old(reader).bytes@,
            reader.position >= old(reader).position,
        decreases count - index,
    ))]
    while index < count {
        let referred = match reference_bytes {
            1 => u32::from(reader.read_u8()?),
            2 => u32::from(reader.read_u16()?),
            4 => reader.read_u32()?,
            _ => return Err(MalformedError::IntegerOverflow.into()),
        };
        require_lower_reference(segment_number, referred)?;
        referred_segments.push(referred);
        index += 1;
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires old(values)@.len() == 0,
    ensures
        out.is_ok() ==> final(values)@.len() == value_count as nat,
        out.is_ok() ==> crate::spec::segments::retention_values_match(final(values)@, bytes@),
))]
fn fill_retention_values(
    values: &mut Vec<u8>,
    value_count: u32,
    bytes: &[u8],
) -> Result<(), MalformedError> {
    let mut index = 0_u32;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index <= value_count,
            values@.len() == index as nat,
            crate::spec::segments::retention_values_match(values@, bytes@),
        decreases value_count - index,
    ))]
    while index < value_count {
        values.push(read_retention_flag(bytes, index)?);
        index += 1;
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) => count_code <= 4 || count_code == 7,
        Err(_) => count_code == 5 || count_code == 6 || count_code > 7,
    },
))]
fn validate_count_code(count_code: u8) -> Result<(), MalformedError> {
    if count_code > 7 {
        return Err(MalformedError::IntegerOverflow);
    }
    #[cfg(verus_keep_ghost)]
    production_count_code_partition!(count_code);
    if count_code == 5 || count_code == 6 {
        return Err(MalformedError::InvalidReferredToCountCode(count_code));
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(count) =>
            count == (first >> 5) as u32
                && crate::spec::segments::short_count_valid(first),
        Err(_) => !crate::spec::segments::short_count_valid(first),
    },
))]
fn validate_short_count(first: u8) -> Result<u32, MalformedError> {
    let count = first >> 5;
    match count {
        0 if first & 0x1E != 0 => {
            return Err(MalformedError::NonZeroUnusedRetentionFlags);
        }
        1 if first & 0x1C != 0 => {
            return Err(MalformedError::NonZeroUnusedRetentionFlags);
        }
        2 if first & 0x18 != 0 => {
            return Err(MalformedError::NonZeroUnusedRetentionFlags);
        }
        3 if first & 0x10 != 0 => {
            return Err(MalformedError::NonZeroUnusedRetentionFlags);
        }
        0..=4 => {}
        _ => return Err(MalformedError::InvalidReferredToCountCode(count)),
    }
    Ok(u32::from(count))
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(count) =>
            count == count_word & 0x1fff_ffff
            && count > 4
            && count <= 0x1fff_ffff,
        Err(_) => count_word & 0x1fff_ffff <= 4,
    },
))]
fn validate_long_count(count_word: u32) -> Result<u32, MalformedError> {
    let count = count_word & 0x1FFF_FFFF;
    if count <= 4 {
        return Err(MalformedError::NonCanonicalReferredToCount(count));
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        assert((count_word & 0x1fff_ffff) <= 0x1fff_ffff) by (bit_vector);
        assert(count <= 0x1fff_ffff);
    }
    Ok(count)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires count <= 0x1fff_ffff,
        ensures match out {
            Ok(bytes) => bytes as nat == (count as nat + 8) / 8,
            Err(_) => false,
        },
))]
fn retention_byte_count(count: u32) -> Result<usize, MalformedError> {
    let bytes = (count + 8) / 8;
    match usize::try_from(bytes) {
        Ok(value) => Ok(value),
        Err(_) => Err(MalformedError::IntegerOverflow),
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) => count <= limits.max_references_per_segment,
        Err(_) => count > limits.max_references_per_segment,
    },
))]
fn enforce_reference_limit(count: u32, limits: &Limits) -> Result<(), LimitExceeded> {
    if count > limits.max_references_per_segment {
        return Err(LimitExceeded {
            kind: LimitKind::ReferencesPerSegment,
            limit: u64::from(limits.max_references_per_segment),
            actual: u64::from(count),
        });
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out == crate::spec::segments::reference_width(segment_number),
))]
fn reference_width(segment_number: u32) -> u8 {
    #[cfg(verus_keep_ghost)]
    production_reference_width!(segment_number);
    match segment_number {
        0..=256 => 1,
        257..=65_536 => 2,
        _ => 4,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) => referred < segment,
        Err(_) => referred >= segment,
    },
))]
fn require_lower_reference(segment: u32, referred: u32) -> Result<(), MalformedError> {
    if referred >= segment {
        return Err(MalformedError::InvalidReferredSegmentNumber { segment, referred });
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures match out {
            Ok(value) =>
                ((index / 8) as nat) < bytes@.len()
                && value == crate::spec::segments::retention_bit(bytes@, index as nat),
            Err(_) => true,
        },
))]
fn read_retention_flag(bytes: &[u8], index: u32) -> Result<u8, MalformedError> {
    let Ok(byte_index) = usize::try_from(index / 8) else {
        return Err(MalformedError::IntegerOverflow);
    };
    let byte = *bytes
        .get(byte_index)
        .ok_or(MalformedError::IntegerOverflow)?;
    let result = match index % 8 {
        0 => byte & 1,
        1 => (byte >> 1) & 1,
        2 => (byte >> 2) & 1,
        3 => (byte >> 3) & 1,
        4 => (byte >> 4) & 1,
        5 => (byte >> 5) & 1,
        6 => (byte >> 6) & 1,
        _ => (byte >> 7) & 1,
    };
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(byte_index as nat == (index / 8) as nat);
    }
    Ok(result)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures match out {
            Ok(length) => match length {
                Some(value) => encoded != 0xffff_ffffu32 && value == encoded,
                None =>
                    encoded == 0xffff_ffffu32
                        && segment_type == SegmentType::ImmediateGenericRegion,
            },
            Err(_) =>
                encoded == 0xffff_ffffu32
                    && segment_type != SegmentType::ImmediateGenericRegion,
        },
))]
fn decode_data_length(
    segment_type: SegmentType,
    encoded: u32,
) -> Result<Option<u32>, MalformedError> {
    if encoded == 0xFFFF_FFFFu32 {
        match segment_type {
            SegmentType::ImmediateGenericRegion => Ok(None),
            _ => Err(MalformedError::InvalidUnknownSegmentLength(
                segment_type.number(),
            )),
        }
    } else {
        Ok(Some(encoded))
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures match out {
            Ok(length) => end >= start && length == end - start,
            Err(_) => end < start,
        },
))]
fn header_byte_length(start: usize, end: usize) -> Result<usize, MalformedError> {
    end.checked_sub(start)
        .ok_or(MalformedError::IntegerOverflow)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires old(reader).position <= old(reader).bytes@.len(),
    ensures
        final(reader).bytes@ == old(reader).bytes@,
        final(reader).position <= final(reader).bytes@.len(),
        match out {
            Ok(value) => if long {
                final(reader).position == old(reader).position + 4
                    && value == crate::spec::segments::be_u32_at(
                        old(reader).bytes@,
                        old(reader).position as int,
                    )
            } else {
                final(reader).position == old(reader).position + 1
                    && value == old(reader).bytes@[old(reader).position as int] as u32
            },
            Err(_) => final(reader).position == old(reader).position,
        },
))]
fn read_page_association(reader: &mut ByteReader<'_>, long: bool) -> Result<u32, MalformedError> {
    if long {
        reader.read_u32()
    } else {
        Ok(u32::from(reader.read_u8()?))
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) => crate::spec::segments::unused_retention_bits_zero(bit_count, bytes@),
        Err(_) => !crate::spec::segments::unused_retention_bits_zero(bit_count, bytes@),
    },
))]
fn validate_unused_retention_bits(bit_count: u32, bytes: &[u8]) -> Result<(), MalformedError> {
    let used_in_last_byte = bit_count % 8;
    if used_in_last_byte == 0 {
        return Ok(());
    }
    let used_mask = match used_in_last_byte {
        1 => 0x0001_u16,
        2 => 0x0003,
        3 => 0x0007,
        4 => 0x000F,
        5 => 0x001F,
        6 => 0x003F,
        7 => 0x007F,
        _ => return Err(MalformedError::IntegerOverflow),
    };
    let last = bytes.last().ok_or(MalformedError::IntegerOverflow)?;
    if u16::from(*last) & !used_mask != 0 {
        return Err(MalformedError::NonZeroUnusedRetentionFlags);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &[u8]) -> Result<SegmentHeader, DecodeError> {
        parse_segment_header(&mut ByteReader::new(input), &Limits::default())
    }

    const DEFINED_TYPES: [(u8, SegmentType); 22] = [
        (0, SegmentType::SymbolDictionary),
        (4, SegmentType::IntermediateTextRegion),
        (6, SegmentType::ImmediateTextRegion),
        (7, SegmentType::ImmediateLosslessTextRegion),
        (16, SegmentType::PatternDictionary),
        (20, SegmentType::IntermediateHalftoneRegion),
        (22, SegmentType::ImmediateHalftoneRegion),
        (23, SegmentType::ImmediateLosslessHalftoneRegion),
        (36, SegmentType::IntermediateGenericRegion),
        (38, SegmentType::ImmediateGenericRegion),
        (39, SegmentType::ImmediateLosslessGenericRegion),
        (40, SegmentType::IntermediateGenericRefinementRegion),
        (42, SegmentType::ImmediateGenericRefinementRegion),
        (43, SegmentType::ImmediateLosslessGenericRefinementRegion),
        (48, SegmentType::PageInformation),
        (49, SegmentType::EndOfPage),
        (50, SegmentType::EndOfStripe),
        (51, SegmentType::EndOfFile),
        (52, SegmentType::Profiles),
        (53, SegmentType::Tables),
        (54, SegmentType::ColourPalette),
        (62, SegmentType::Extension),
    ];

    fn generated_header(
        number: u32,
        flags: u8,
        page: u32,
        references: &[u32],
        retain: &[bool],
        length: u32,
    ) -> Vec<u8> {
        assert_eq!(retain.len(), references.len() + 1);
        let count = u32::try_from(references.len()).unwrap();
        let mut input = number.to_be_bytes().to_vec();
        input.push(flags);
        if count <= 4 {
            let bits = retain
                .iter()
                .enumerate()
                .fold(0_u8, |bits, (index, value)| {
                    bits | (u8::from(*value) << index)
                });
            input.push((u8::try_from(count).unwrap() << 5) | bits);
        } else {
            input.extend_from_slice(&(0xe000_0000 | count).to_be_bytes());
            let mut bytes = vec![0_u8; retain.len().div_ceil(8)];
            for (index, value) in retain.iter().enumerate() {
                bytes[index / 8] |= u8::from(*value) << (index % 8);
            }
            input.extend_from_slice(&bytes);
        }
        for referred in references {
            if number <= 256 {
                input.push(u8::try_from(*referred).unwrap());
            } else if number <= 65_536 {
                input.extend_from_slice(&u16::try_from(*referred).unwrap().to_be_bytes());
            } else {
                input.extend_from_slice(&referred.to_be_bytes());
            }
        }
        if flags & 0x40 == 0 {
            input.push(u8::try_from(page).unwrap());
        } else {
            input.extend_from_slice(&page.to_be_bytes());
        }
        input.extend_from_slice(&length.to_be_bytes());
        input
    }

    #[test]
    fn every_flag_byte_has_exact_type_page_and_deferred_fields() {
        for flags in 0..=u8::MAX {
            let page = if flags & 0x40 == 0 { 255 } else { 0x0102_0304 };
            let input = generated_header(u32::MAX, flags, page, &[], &[true], 0x1020_3040);
            let expected = DEFINED_TYPES.iter().find(|(code, _)| *code == flags & 0x3f);
            let Some((_, kind)) = expected else {
                assert!(matches!(parse(&input),
                    Err(DecodeError::Malformed(MalformedError::ReservedSegmentType(code)))
                        if code == flags & 0x3f));
                continue;
            };
            let header = parse(&input).unwrap();
            assert_eq!(header.number, u32::MAX);
            assert_eq!(header.segment_type, *kind);
            assert_eq!(header.page_association, page);
            assert_eq!(header.deferred_non_retain, flags & 0x80 != 0);
            assert_eq!(header.byte_length, input.len());
            assert_eq!(header.data_length, Some(0x1020_3040));
            assert!(header.retention_flags.retain_this_segment());
        }
        for code in 64..=u8::MAX {
            assert!(matches!(SegmentType::from_number(code),
                Err(MalformedError::ReservedSegmentType(value)) if value == code));
        }
    }

    #[test]
    fn every_short_count_byte_obeys_count_and_unused_bit_rules() {
        for first in 0..=u8::MAX {
            let count = first >> 5;
            let result = validate_short_count(first);
            if count > 4 {
                assert!(matches!(result,
                    Err(MalformedError::InvalidReferredToCountCode(value)) if value == count));
            } else {
                let allowed = (1_u16 << (count + 1)) - 1;
                if u16::from(first & 0x1f) & !allowed != 0 {
                    assert!(matches!(
                        result,
                        Err(MalformedError::NonZeroUnusedRetentionFlags)
                    ));
                } else {
                    assert_eq!(result.unwrap(), u32::from(count));
                }
            }
        }
        for code in 0..=u8::MAX {
            let result = validate_count_code(code);
            match code {
                0..=4 | 7 => assert!(result.is_ok()),
                5 | 6 => assert!(matches!(result,
                    Err(MalformedError::InvalidReferredToCountCode(value)) if value == code)),
                _ => assert!(matches!(result, Err(MalformedError::IntegerOverflow))),
            }
        }
    }

    #[test]
    fn reference_width_boundaries_and_retention_groups_round_trip() {
        for number in [256_u32, 257, 65_536, 65_537, u32::MAX] {
            for count in (0..=17).chain([31, 32, 33, 255, 256, 257]) {
                if count > number {
                    continue;
                }
                let references: Vec<u32> = (0..count).collect();
                let retain: Vec<bool> = (0..=count).map(|index| index % 3 == 1).collect();
                for flags in [0_u8, 0x40] {
                    let page = if flags == 0 { 17 } else { 0xf102_0304 };
                    let input = generated_header(number, flags, page, &references, &retain, 0);
                    let mut framed = vec![0xaa; 3];
                    framed.extend_from_slice(&input);
                    framed.extend_from_slice(&[0x55; 2]);
                    let mut reader = ByteReader::new(&framed);
                    reader.read_bytes(3).unwrap();
                    let header = parse_segment_header(&mut reader, &Limits::default()).unwrap();
                    assert_eq!(header.referred_segments, references);
                    assert_eq!(header.retention_flags.retain_this_segment(), retain[0]);
                    for index in 0..references.len() {
                        assert_eq!(
                            header.retention_flags.retain_referred_segment(index),
                            Some(retain[index + 1])
                        );
                    }
                    assert_eq!(
                        header
                            .retention_flags
                            .retain_referred_segment(references.len()),
                        None
                    );
                    assert_eq!(
                        header.retention_flags.retain_referred_segment(usize::MAX),
                        None
                    );
                    assert_eq!(header.page_association, page);
                    assert_eq!(header.byte_length, input.len());
                    assert_eq!(reader.position(), input.len() + 3);
                    assert_eq!(reader.remaining(), 2);
                }
            }
        }
    }

    #[test]
    fn every_two_byte_retention_mask_matches_forward_lsb_indexing() {
        for first in 0..=u8::MAX {
            for second in 0..=u8::MAX {
                let bytes = [first, second];
                let flags = RetentionFlags::from_bytes(15, &bytes).unwrap();
                assert_eq!(flags.values.len(), 16);
                for index in 0..16 {
                    assert_eq!(flags.values[index], (bytes[index / 8] >> (index % 8)) & 1);
                }
            }
        }
    }

    #[test]
    fn unused_long_retention_bits_validate_only_the_last_byte() {
        for bit_count in 1_u32..=40 {
            let used = bit_count % 8;
            let allowed = if used == 0 { 255 } else { (1_u16 << used) - 1 };
            for value in 0..=u8::MAX {
                let mut bytes = vec![0xff; usize::try_from(bit_count.div_ceil(8)).unwrap()];
                *bytes.last_mut().unwrap() = value;
                let result = validate_unused_retention_bits(bit_count, &bytes);
                if u16::from(value) & !allowed == 0 {
                    assert!(result.is_ok());
                } else {
                    assert!(matches!(
                        result,
                        Err(MalformedError::NonZeroUnusedRetentionFlags)
                    ));
                }
            }
        }
    }

    #[test]
    fn every_generated_header_prefix_is_a_typed_truncation() {
        for number in [256_u32, 257, 65_537] {
            for count in [0, 4, 5, 8, 16] {
                let references: Vec<u32> = (0..count).collect();
                let retain = vec![true; references.len() + 1];
                for flags in [0_u8, 0x40] {
                    let input = generated_header(number, flags, 1, &references, &retain, 0);
                    for length in 0..input.len() {
                        let mut reader = ByteReader::new(&input[..length]);
                        let result = parse_segment_header(&mut reader, &Limits::default());
                        assert!(matches!(result,
                            Err(DecodeError::Malformed(MalformedError::UnexpectedEof {
                                offset, needed, remaining,
                            })) if offset == u64::try_from(reader.position()).unwrap()
                                && remaining == u64::try_from(length - reader.position()).unwrap()
                                && needed > remaining),
                            "segment {number}, count {count}, flags {flags}, prefix {length}: {result:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn count_and_length_boundaries_preserve_exact_error_values() {
        for count in 0..=4 {
            assert!(matches!(validate_long_count(0xe000_0000 | count),
                Err(MalformedError::NonCanonicalReferredToCount(value)) if value == count));
        }
        for count in [5, 7, 8, 15, 16, 0x1fff_ffff] {
            assert_eq!(validate_long_count(0xe000_0000 | count).unwrap(), count);
            assert_eq!(
                retention_byte_count(count).unwrap(),
                usize::try_from((count + 8) / 8).unwrap()
            );
        }
        assert!(matches!(
            RetentionFlags::from_bytes(u32::MAX, &[]),
            Err(DecodeError::Malformed(MalformedError::IntegerOverflow))
        ));
        assert!(matches!(
            read_retention_flag(&[], 0),
            Err(MalformedError::IntegerOverflow)
        ));
        assert!(matches!(
            read_retention_flag(&[1], 8),
            Err(MalformedError::IntegerOverflow)
        ));
        assert!(matches!(
            validate_unused_retention_bits(1, &[]),
            Err(MalformedError::IntegerOverflow)
        ));
        assert!(matches!(
            header_byte_length(2, 1),
            Err(MalformedError::IntegerOverflow)
        ));
        assert_eq!(header_byte_length(7, 7).unwrap(), 0);
        for (code, kind) in DEFINED_TYPES {
            for length in [0, 1, u32::MAX - 1] {
                assert_eq!(decode_data_length(kind, length).unwrap(), Some(length));
            }
            let result = decode_data_length(kind, u32::MAX);
            if code == 38 {
                assert_eq!(result.unwrap(), None);
            } else {
                assert!(
                    matches!(result, Err(MalformedError::InvalidUnknownSegmentLength(value)) if value == code)
                );
            }
        }
    }

    #[test]
    fn reference_numbers_are_strictly_lower_at_every_width_boundary() {
        for segment in [1_u32, 256, 257, 65_536, 65_537, u32::MAX] {
            assert!(require_lower_reference(segment, 0).is_ok());
            assert!(require_lower_reference(segment, segment - 1).is_ok());
        }
        for (segment, referred) in [
            (0, 0),
            (0, 1),
            (1, 1),
            (1, 2),
            (256, 256),
            (257, 257),
            (65_536, 65_536),
            (65_537, u32::MAX),
            (u32::MAX, u32::MAX),
        ] {
            assert!(matches!(require_lower_reference(segment, referred),
                Err(MalformedError::InvalidReferredSegmentNumber {
                    segment: actual_segment, referred: actual_referred,
                }) if actual_segment == segment && actual_referred == referred));
        }
    }

    #[test]
    fn generated_reference_limits_fail_before_reading_references() {
        let limits = Limits {
            max_references_per_segment: 4,
            ..Limits::default()
        };
        for count in [5_u32, 8, 0x1fff_ffff] {
            let mut input = 65_537_u32.to_be_bytes().to_vec();
            input.push(0);
            input.extend_from_slice(&(0xe000_0000 | count).to_be_bytes());
            let mut reader = ByteReader::new(&input);
            assert!(matches!(parse_segment_header(&mut reader, &limits),
                Err(DecodeError::LimitExceeded(LimitExceeded {
                    kind: LimitKind::ReferencesPerSegment, limit: 4, actual,
                })) if actual == u64::from(count)));
            assert_eq!(reader.position(), input.len());
        }
    }

    #[test]
    fn parses_short_form_header() {
        let input = [
            0x00, 0x00, 0x00, 0x20, 0x86, 0x6B, 0x02, 0x1E, 0x05, 0x04, 0x00, 0x00, 0x00, 0x0A,
        ];
        let header = parse(&input).expect("header is valid");
        assert_eq!(header.number, 32);
        assert_eq!(header.segment_type, SegmentType::ImmediateTextRegion);
        assert!(header.deferred_non_retain);
        assert_eq!(&header.referred_segments, &[2, 30, 5]);
        assert!(header.retention_flags.retain_this_segment());
        assert_eq!(
            header.retention_flags.retain_referred_segment(0),
            Some(true)
        );
        assert_eq!(
            header.retention_flags.retain_referred_segment(1),
            Some(false)
        );
        assert_eq!(header.page_association, 4);
        assert_eq!(header.data_length, Some(10));
        assert_eq!(header.byte_length, input.len());
    }

    #[test]
    fn long_retention_flags_follow_wire_byte_order() {
        let flags = RetentionFlags::from_bytes(8, &[0x05, 0x01]).unwrap();
        assert_eq!(flags.values, [1, 0, 1, 0, 0, 0, 0, 0, 1]);
        assert!(flags.retain_this_segment());
        assert_eq!(flags.retain_referred_segment(1), Some(true));
        assert_eq!(flags.retain_referred_segment(7), Some(true));
        assert_eq!(flags.retain_referred_segment(8), None);
    }

    #[test]
    fn long_retention_unused_bits_are_in_the_final_wire_byte() {
        assert!(validate_unused_retention_bits(9, &[0xff, 0x01]).is_ok());
        assert!(matches!(
            validate_unused_retention_bits(9, &[0x01, 0xff]),
            Err(MalformedError::NonZeroUnusedRetentionFlags)
        ));
    }

    #[test]
    fn parses_long_form_header() {
        let mut input = [
            0x00, 0x00, 0x02, 0x34, 0x40, 0xE0, 0x00, 0x00, 0x09, 0x02, 0xFD, 0x01, 0x00, 0x00,
            0x02, 0x00, 0x1E, 0x00, 0x05, 0x02, 0x00, 0x02, 0x01, 0x02, 0x02, 0x02, 0x03, 0x02,
            0x04, 0x00, 0x00, 0x04, 0x01, 0x00, 0x00, 0x00, 0x00,
        ];
        // The worked example conflicts with 7.2.4; follow the explicit field rules.
        assert!(matches!(
            parse(&input),
            Err(DecodeError::Malformed(
                MalformedError::NonZeroUnusedRetentionFlags
            ))
        ));
        input.swap(9, 10);
        let header = parse(&input).expect("field-order correction is valid");
        assert_eq!(header.number, 564);
        assert_eq!(header.segment_type, SegmentType::SymbolDictionary);
        assert_eq!(
            &header.referred_segments,
            &[256, 2, 30, 5, 512, 513, 514, 515, 516]
        );
        assert!(header.retention_flags.retain_this_segment());
        assert_eq!(
            header.retention_flags.retain_referred_segment(0),
            Some(false)
        );
        assert_eq!(
            header.retention_flags.retain_referred_segment(7),
            Some(false)
        );
        assert_eq!(header.page_association, 1025);
        assert_eq!(header.data_length, Some(0));
    }

    #[test]
    fn rejects_reserved_count_codes() {
        let input = [
            0x00, 0x00, 0x00, 0x01, 0x30, 0xA0, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        assert!(matches!(
            parse(&input),
            Err(DecodeError::Malformed(
                MalformedError::InvalidReferredToCountCode(5)
            ))
        ));
    }

    #[test]
    fn rejects_nonzero_unused_short_retention_bits() {
        let input = [
            0x00, 0x00, 0x00, 0x01, 0x30, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        assert!(matches!(
            parse(&input),
            Err(DecodeError::Malformed(
                MalformedError::NonZeroUnusedRetentionFlags
            ))
        ));
    }

    #[test]
    fn enforces_reference_limit_before_allocating() {
        let input = [0x00, 0x00, 0x01, 0x00, 0x30, 0xE0, 0x00, 0x00, 0x05];
        let limits = Limits {
            max_references_per_segment: 4,
            ..Limits::default()
        };
        let error = parse_segment_header(&mut ByteReader::new(&input), &limits)
            .expect_err("five references exceed the configured limit");
        assert!(matches!(
            error,
            DecodeError::LimitExceeded(LimitExceeded {
                kind: LimitKind::ReferencesPerSegment,
                limit: 4,
                actual: 5
            })
        ));
    }

    #[test]
    fn rejects_nonpreceding_references() {
        let input = [
            0x00, 0x00, 0x00, 0x01, 0x30, 0x21, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        assert!(matches!(
            parse(&input),
            Err(DecodeError::Malformed(
                MalformedError::InvalidReferredSegmentNumber {
                    segment: 1,
                    referred: 1
                }
            ))
        ));
    }

    #[test]
    fn accepts_unknown_length_only_for_immediate_generic_regions() {
        let valid = [
            0x00, 0x00, 0x00, 0x01, 0x26, 0x00, 0x01, 0xFF, 0xFF, 0xFF, 0xFF,
        ];
        assert_eq!(parse(&valid).expect("length is valid").data_length, None);

        let invalid = [
            0x00, 0x00, 0x00, 0x01, 0x30, 0x00, 0x01, 0xFF, 0xFF, 0xFF, 0xFF,
        ];
        assert!(matches!(
            parse(&invalid),
            Err(DecodeError::Malformed(
                MalformedError::InvalidUnknownSegmentLength(48)
            ))
        ));
    }
}

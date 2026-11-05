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

#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct RetentionFlags {
    pub(crate) values: Vec<u8>,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl Clone for RetentionFlags {
    fn clone(&self) -> Self {
        #[cfg(verus_keep_ghost)]
        ensures(|out: Self| equal(out.view(), self.view()));
        let out = Self {
            values: self.values.clone(),
        };
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal(<RetentionFlags as View>::view);
        }
        out
    }
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

#[derive(Debug, PartialEq, Eq)]
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

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl Clone for SegmentHeader {
    fn clone(&self) -> Self {
        #[cfg(verus_keep_ghost)]
        ensures(|out: Self| equal(out.view(), self.view()));
        let out = Self {
            number: self.number,
            segment_type: self.segment_type,
            deferred_non_retain: self.deferred_non_retain,
            referred_segments: self.referred_segments.clone(),
            retention_flags: self.retention_flags.clone(),
            page_association: self.page_association,
            data_length: self.data_length,
            byte_length: self.byte_length,
        };
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal(<RetentionFlags as View>::view);
            reveal(<SegmentHeader as View>::view);
        }
        out
    }
}

#[cfg(verus_keep_ghost)]
verus! {
    impl View for RetentionFlags {
        type V = Seq<u8>;

        closed spec fn view(&self) -> Self::V {
            self.values@
        }
    }

    impl View for SegmentHeader {
        type V = (u32, SegmentType, bool, Seq<u32>, Seq<u8>, u32, Option<u32>, usize);

        closed spec fn view(&self) -> Self::V {
            (
                self.number, self.segment_type, self.deferred_non_retain,
                self.referred_segments@, self.retention_flags.values@,
                self.page_association, self.data_length, self.byte_length,
            )
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires old(reader).position <= old(reader).bytes@.len(),
    ensures
        final(reader).bytes@ == old(reader).bytes@,
        final(reader).position <= final(reader).bytes@.len(),
        out.is_ok() ==> final(reader).position > old(reader).position,
        match out {
            Ok(header) => crate::spec::segments::segment_header_fields(
                &header, old(reader).bytes@, old(reader).position as int,
                final(reader).position as int,
            ),
            Err(_) => true,
        },
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

    #[cfg(verus_keep_ghost)]
    production_segment_header_fields!();
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

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires old(reader).position <= old(reader).bytes@.len(),
    ensures
        final(reader).bytes@ == old(reader).bytes@,
        final(reader).position <= final(reader).bytes@.len(),
        final(reader).position >= old(reader).position,
        match out {
            Ok((references, retention)) =>
                crate::spec::segments::referred_header_fields(
                    old(reader).bytes@, old(reader).position as int, segment_number,
                    references@, retention.values@, final(reader).position as int,
                )
                && references@.len() <= limits.max_references_per_segment,
            Err(_) => true,
        },
))]
fn parse_referred_segments(
    reader: &mut ByteReader<'_>,
    segment_number: u32,
    limits: &Limits,
) -> Result<(Vec<u32>, RetentionFlags), DecodeError> {
    #[cfg(verus_keep_ghost)]
    proof_decl! { let ghost start = reader.position; }
    let first_count_byte = reader.read_u8()?;
    let count_code = first_count_byte >> 5;
    validate_count_code(count_code)?;
    let (count, retention_flags) = match count_code {
        0..=4 => {
            let count = validate_short_count(first_count_byte)?;
            let bytes = [first_count_byte];
            #[cfg(verus_keep_ghost)]
            production_segment_short_flags!(reader, start, bytes);
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
    #[cfg(verus_keep_ghost)]
    production_segment_count_fields!(reader, start, count, retention_flags);
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
    #[cfg(verus_keep_ghost)]
    proof_decl! { let ghost references_begin = reader.position; }
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(referred_segments@.len() == 0);
    }
    read_referred_segments_into(
        reader,
        segment_number,
        count,
        reference_bytes,
        &mut referred_segments,
    )?;
    #[cfg(verus_keep_ghost)]
    production_segment_reference_fields!(
        reader,
        start,
        count,
        segment_number,
        reference_bytes,
        references_begin,
        referred_segments,
        retention_flags,
    );
    Ok((referred_segments, retention_flags))
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires old(reader).position <= old(reader).bytes@.len(),
    ensures
        final(reader).bytes@ == old(reader).bytes@,
        final(reader).position <= final(reader).bytes@.len(),
        final(reader).position >= old(reader).position,
        out.is_ok() ==> {
            &&& final(reader).position == old(reader).position
                + count as nat * reference_bytes as nat
            &&& final(referred_segments)@.len() == old(referred_segments)@.len() + count
            &&& final(referred_segments)@.subrange(0, old(referred_segments)@.len() as int)
                == old(referred_segments)@
            &&& forall|index: int| 0 <= index < count ==>
                #[trigger] final(referred_segments)@[old(referred_segments)@.len() + index]
                    < segment_number
                && final(referred_segments)@[old(referred_segments)@.len() + index]
                    == crate::spec::segments::reference_value_at(
                        old(reader).bytes@,
                        old(reader).position + index * reference_bytes,
                        reference_bytes,
                    )
        },
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
            reader.position == old(reader).position + index as nat * reference_bytes as nat,
            referred_segments@.len() == old(referred_segments)@.len() + index,
            referred_segments@.subrange(0, old(referred_segments)@.len() as int)
                == old(referred_segments)@,
            forall|completed: int| 0 <= completed < index ==>
                #[trigger] referred_segments@[old(referred_segments)@.len() + completed]
                    < segment_number
                && referred_segments@[old(referred_segments)@.len() + completed]
                    == crate::spec::segments::reference_value_at(
                        old(reader).bytes@,
                        old(reader).position + completed * reference_bytes,
                        reference_bytes,
                    ),
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
mod tests;

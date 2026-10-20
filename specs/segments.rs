use vstd::prelude::*;

verus! {

pub(crate) open spec fn segment_type(
    number: u8,
) -> Option<crate::segment_header::SegmentType> {
    match number {
        0 => Some(crate::segment_header::SegmentType::SymbolDictionary),
        4 => Some(crate::segment_header::SegmentType::IntermediateTextRegion),
        6 => Some(crate::segment_header::SegmentType::ImmediateTextRegion),
        7 => Some(crate::segment_header::SegmentType::ImmediateLosslessTextRegion),
        16 => Some(crate::segment_header::SegmentType::PatternDictionary),
        20 => Some(crate::segment_header::SegmentType::IntermediateHalftoneRegion),
        22 => Some(crate::segment_header::SegmentType::ImmediateHalftoneRegion),
        23 => Some(crate::segment_header::SegmentType::ImmediateLosslessHalftoneRegion),
        36 => Some(crate::segment_header::SegmentType::IntermediateGenericRegion),
        38 => Some(crate::segment_header::SegmentType::ImmediateGenericRegion),
        39 => Some(crate::segment_header::SegmentType::ImmediateLosslessGenericRegion),
        40 => Some(crate::segment_header::SegmentType::IntermediateGenericRefinementRegion),
        42 => Some(crate::segment_header::SegmentType::ImmediateGenericRefinementRegion),
        43 => Some(crate::segment_header::SegmentType::ImmediateLosslessGenericRefinementRegion),
        48 => Some(crate::segment_header::SegmentType::PageInformation),
        49 => Some(crate::segment_header::SegmentType::EndOfPage),
        50 => Some(crate::segment_header::SegmentType::EndOfStripe),
        51 => Some(crate::segment_header::SegmentType::EndOfFile),
        52 => Some(crate::segment_header::SegmentType::Profiles),
        53 => Some(crate::segment_header::SegmentType::Tables),
        54 => Some(crate::segment_header::SegmentType::ColourPalette),
        62 => Some(crate::segment_header::SegmentType::Extension),
        _ => None,
    }
}

pub(crate) open spec fn reference_width(segment_number: u32) -> u8 {
    if segment_number <= 256 {
        1
    } else if segment_number <= 65_536 {
        2
    } else {
        4
    }
}

pub(crate) open spec fn short_count_valid(first: u8) -> bool {
    match first >> 5 {
        0 => first & 0x1e == 0,
        1 => first & 0x1c == 0,
        2 => first & 0x18 == 0,
        3 => first & 0x10 == 0,
        4 => true,
        _ => false,
    }
}

pub(crate) open spec fn retention_bit(bytes: Seq<u8>, index: nat) -> u8
    recommends index / 8 < bytes.len(),
{
    let byte = bytes[(index / 8) as int];
    if index % 8 == 0 { byte & 1 }
    else if index % 8 == 1 { (byte >> 1) & 1 }
    else if index % 8 == 2 { (byte >> 2) & 1 }
    else if index % 8 == 3 { (byte >> 3) & 1 }
    else if index % 8 == 4 { (byte >> 4) & 1 }
    else if index % 8 == 5 { (byte >> 5) & 1 }
    else if index % 8 == 6 { (byte >> 6) & 1 }
    else { (byte >> 7) & 1 }
}

pub(crate) open spec fn retention_values_match(values: Seq<u8>, bytes: Seq<u8>) -> bool {
    forall|index: int| 0 <= index < values.len() ==>
        index / 8 < bytes.len()
        && values[index] == retention_bit(bytes, index as nat)
}

pub(crate) open spec fn unused_retention_bits_zero(
    bit_count: u32,
    bytes: Seq<u8>,
) -> bool {
    let used = bit_count % 8;
    let mask: u16 = match used {
        1 => 0x0001u16,
        2 => 0x0003u16,
        3 => 0x0007u16,
        4 => 0x000fu16,
        5 => 0x001fu16,
        6 => 0x003fu16,
        7 => 0x007fu16,
        _ => 0xffffu16,
    };
    used == 0
        || (bytes.len() > 0
            && ((bytes[bytes.len() - 1] as u16) & !mask) == 0u16)
}

pub(crate) open spec fn combination_operator(
    encoded: u8,
    replace_allowed: bool,
) -> Option<crate::bitmap::CombinationOperator> {
    match encoded {
        0 => Some(crate::bitmap::CombinationOperator::Or),
        1 => Some(crate::bitmap::CombinationOperator::And),
        2 => Some(crate::bitmap::CombinationOperator::Xor),
        3 => Some(crate::bitmap::CombinationOperator::Xnor),
        4 if replace_allowed => Some(crate::bitmap::CombinationOperator::Replace),
        _ => None,
    }
}

pub(crate) open spec fn dimensions_within_limits(
    width: u32,
    height: u32,
    limits: crate::Limits,
) -> bool {
    width <= limits.max_width
        && height <= limits.max_height
        && width as nat * height as nat <= limits.max_pixels_per_page as nat
}

pub(crate) open spec fn extension_flags_valid(extension_type: u32) -> bool {
    extension_type & 0x8000_0000 == 0
        || extension_type & 0x2000_0000 != 0
}

pub(crate) open spec fn be_u32_at(bytes: Seq<u8>, start: int) -> u32
    recommends 0 <= start, start + 4 <= bytes.len(),
{
    ((bytes[start] as u32) << 24)
        | ((bytes[start + 1] as u32) << 16)
        | ((bytes[start + 2] as u32) << 8)
        | (bytes[start + 3] as u32)
}

pub(crate) open spec fn profiles_payload_valid(bytes: Seq<u8>) -> bool {
    bytes.len() >= 4
        && bytes.len() as nat
            == 4 * (be_u32_at(bytes, 0) as nat + 1)
}

pub(crate) open spec fn profile_length_matches(bytes: Seq<u8>, count: u32) -> bool {
    bytes.len() == 4 * (count as nat + 1)
}

pub(crate) open spec fn extension_payload_flags_valid(bytes: Seq<u8>) -> bool {
    bytes.len() >= 4
        && extension_flags_valid(be_u32_at(bytes, 0))
}

pub(crate) open spec fn extension_kind(extension_type: u32) -> Option<u8> {
    if !extension_flags_valid(extension_type)
        || extension_type & 0x8000_0000 != 0
    {
        None
    } else if extension_type == 0x2000_0000 {
        Some(1)
    } else if extension_type == 0x2000_0002 {
        Some(2)
    } else {
        Some(0)
    }
}

pub(crate) open spec fn final_code_unit_zero(
    bytes: Seq<u8>,
    code_unit_size: usize,
) -> bool {
    code_unit_size > 0
        && bytes.len() >= code_unit_size
        && forall|i: int|
            bytes.len() - code_unit_size <= i < bytes.len()
                ==> bytes[i] == 0
}

pub(crate) open spec fn region_flags_match(
    info: crate::segment_data::RegionInfo,
    flags: u8,
) -> bool {
    flags & 0xf0 == 0
        && flags & 0x08 == 0
        && combination_operator(flags & 0x07, true) == Some(info.operator)
}

pub(crate) open spec fn page_flags_match(
    info: crate::segment_data::PageInformation,
    flags: u8,
    striping: u16,
) -> bool {
    &&& flags & 0x80 == 0
    &&& info.eventually_lossless == (flags & 0x01 != 0)
    &&& info.might_contain_refinements == (flags & 0x02 != 0)
    &&& info.default_pixel == (flags & 0x04 != 0)
    &&& combination_operator((flags >> 3) & 0x03, false)
        == Some(info.default_operator)
    &&& info.requires_auxiliary_buffers == (flags & 0x20 != 0)
    &&& info.combination_operator_override == (flags & 0x40 != 0)
    &&& info.striped == (striping & 0x8000 != 0)
    &&& info.max_stripe_size == striping & 0x7fff
}

}

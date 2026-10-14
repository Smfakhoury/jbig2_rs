use vstd::prelude::*;

verus! {

pub open spec fn has_signature(input: Seq<u8>) -> bool {
    input.len() >= 8
        && input[0] == 0x97
        && input[1] == 0x4A
        && input[2] == 0x42
        && input[3] == 0x32
        && input[4] == 0x0D
        && input[5] == 0x0A
        && input[6] == 0x1A
        && input[7] == 0x0A
}

pub open spec fn valid(input: Seq<u8>) -> bool {
    input.len() >= 9
        && has_signature(input)
        && input[8] & 0xF0 == 0
        && (input[8] & 0x02 != 0 || input.len() >= 13)
}

pub open spec fn organization(input: Seq<u8>) -> crate::file_header::Organization
    recommends valid(input),
{
    if input[8] & 0x01 == 0 {
        crate::file_header::Organization::RandomAccess
    } else {
        crate::file_header::Organization::Sequential
    }
}

pub open spec fn page_count(input: Seq<u8>) -> Option<u32>
    recommends valid(input),
{
    if input[8] & 0x02 != 0 {
        None
    } else {
        Some(
            ((input[9] as u32) << 24)
                | ((input[10] as u32) << 16)
                | ((input[11] as u32) << 8)
                | input[12] as u32,
        )
    }
}

pub open spec fn extended_templates(input: Seq<u8>) -> bool
    recommends valid(input),
{
    input[8] & 0x04 != 0
}

pub open spec fn colour_regions(input: Seq<u8>) -> bool
    recommends valid(input),
{
    input[8] & 0x08 != 0
}

pub open spec fn byte_length(input: Seq<u8>) -> usize
    recommends valid(input),
{
    if input[8] & 0x02 != 0 {
        9
    } else {
        13
    }
}

}

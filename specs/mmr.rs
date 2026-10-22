use vstd::prelude::*;

verus! {

// Membership in implementation data is not an independent normative-table proof.
pub(super) open spec fn mode_code_in_implementation_table(code: super::ModeCode) -> bool {
    super::MODE_CODES@.contains(code)
}

pub(super) open spec fn mode_matches(
    code: super::ModeCode,
    bits: u16,
    length: u8,
) -> bool {
    code.bits == bits && code.length == length
}

pub(super) open spec fn run_code_in_implementation_tables(
    black: bool,
    code: super::RunCode,
) -> bool {
    if black {
        super::BLACK_TERMINATING@.contains(code)
            || super::BLACK_MAKEUP@.contains(code)
            || super::COMMON_MAKEUP@.contains(code)
    } else {
        super::WHITE_TERMINATING@.contains(code)
            || super::WHITE_MAKEUP@.contains(code)
            || super::COMMON_MAKEUP@.contains(code)
    }
}

pub(super) open spec fn run_key_matches(
    code: super::RunCode,
    bits: u16,
    length: u8,
) -> bool {
    code.bits == bits && code.length == length
}

pub(super) open spec fn reader_valid(reader: &crate::reader::BitReader) -> bool {
    reader.bytes@.len() <= usize::MAX / 8
        && reader.bit_position <= reader.bytes@.len() * 8
}

pub(super) open spec fn transitions_ordered(
    transitions: Seq<u32>,
    width: u32,
) -> bool {
    forall|index: int| 0 <= index < transitions.len() ==>
        transitions[index] <= width
    && forall|left: int, right: int|
        0 <= left < right < transitions.len() ==>
            transitions[left] <= transitions[right]
}

pub(super) open spec fn reference_line_valid(
    transitions: Seq<u32>,
    width: u32,
) -> bool {
    transitions.len() >= 2
        && transitions[transitions.len() - 2] == width
        && transitions[transitions.len() - 1] == width
        && transitions_ordered(transitions, width)
}

pub(super) open spec fn span_byte(
    bitmap: &crate::bitmap::Bitmap,
    row: u32,
    x: u32,
) -> nat {
    row as nat * bitmap.stride as nat + x as nat / 8
}

pub(super) open spec fn bitmap_storage_valid(bitmap: &crate::bitmap::Bitmap) -> bool {
    bitmap.width as nat <= bitmap.stride as nat * 8
        && bitmap.data@.len()
            == bitmap.stride as nat * bitmap.height as nat
}

pub(super) open spec fn exact_eofb(
    bytes: Seq<u8>,
    start: nat,
) -> bool {
    crate::spec::readers::bits_value(bytes, start, 24) == super::EOFB
}

}

// Private helper preconditions still use mmr_spec::transition_prefix_valid.
include!("../proofs/model_support/mmr.rs");

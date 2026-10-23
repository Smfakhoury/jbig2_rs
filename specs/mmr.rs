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

pub(super) open spec fn run_bit_length(codes: Seq<super::RunCode>) -> nat
    decreases codes.len(),
{
    if codes.len() == 0 { 0 } else {
        run_bit_length(codes.drop_last()) + codes.last().length as nat
    }
}

pub(super) open spec fn run_value_sum(codes: Seq<super::RunCode>) -> nat
    decreases codes.len(),
{
    if codes.len() == 0 { 0 } else {
        run_value_sum(codes.drop_last()) + codes.last().value as nat
    }
}

pub(super) open spec fn run_prefix_matches(
    black: bool, bytes: Seq<u8>, start: nat, codes: Seq<super::RunCode>,
) -> bool
    decreases codes.len(),
{
    if codes.len() == 0 {
        start <= bytes.len() * 8
    } else {
        let prefix = codes.drop_last();
        let code = codes.last();
        run_prefix_matches(black, bytes, start, prefix)
            && run_code_in_implementation_tables(black, code)
            && start + run_bit_length(codes) <= bytes.len() * 8
            && code.bits as nat == crate::spec::readers::bits_value(
                bytes, start + run_bit_length(prefix), code.length as nat,
            )
    }
}

pub(super) open spec fn run_all_makeups(codes: Seq<super::RunCode>) -> bool
    decreases codes.len(),
{
    codes.len() == 0
        || (run_all_makeups(codes.drop_last()) && !codes.last().terminal)
}

pub(super) open spec fn complete_run(
    black: bool, bytes: Seq<u8>, start: nat, end: nat, total: u32,
    codes: Seq<super::RunCode>,
) -> bool {
    codes.len() > 0
        && run_prefix_matches(black, bytes, start, codes)
        && run_all_makeups(codes.drop_last())
        && codes.last().terminal
        && end == start + run_bit_length(codes)
        && total as nat == run_value_sum(codes)
}

pub(super) open spec fn horizontal_pair(
    black: bool, bytes: Seq<u8>, start: nat, finish: nat,
    x: u32, width: u32, after_first: u32, end: u32,
) -> bool {
    x <= after_first <= end <= width
        && x < end
        && exists|first: Seq<super::RunCode>, second: Seq<super::RunCode>|
            #[trigger] complete_run(
                black, bytes, start, start + run_bit_length(first),
                (after_first - x) as u32, first,
            )
            && finish == start + run_bit_length(first) + #[trigger] run_bit_length(second)
            && complete_run(
                !black, bytes, start + run_bit_length(first), finish,
                (end - after_first) as u32, second,
            )
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

pub(super) open spec fn reference_position_eligible(
    position: u32, x: u32, initial: bool,
) -> bool {
    position > x || (initial && position == x)
}

pub(super) open spec fn first_reference_candidate(
    transitions: Seq<u32>, x: u32, black: bool, initial: bool,
    start: int, index: int,
) -> bool {
    0 <= start <= index < transitions.len() - 1
        && reference_position_eligible(transitions[index], x, initial)
        && (index % 2 == 0) != black
        && forall|previous: int| start <= previous < index ==>
            !reference_position_eligible(#[trigger] transitions[previous], x, initial)
                || (previous % 2 == 0) == black
}

pub(super) open spec fn reference_pair_matches(
    transitions: Seq<u32>, x: u32, black: bool, width: u32, initial: bool,
    start: int, b1: u32, b2: u32,
) -> bool {
    (exists|index: int|
        #[trigger] first_reference_candidate(transitions, x, black, initial, start, index)
            && b1 == transitions[index] && b2 == transitions[index + 1])
        || (!(exists|index: int|
            #[trigger] first_reference_candidate(transitions, x, black, initial, start, index))
            && b1 == width && b2 == width)
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

pub(super) open spec fn span_result(
    before: &crate::bitmap::Bitmap,
    after: &crate::bitmap::Bitmap,
    row: u32,
    start: u32,
    end: u32,
    black: bool,
) -> bool {
    if black {
        crate::spec::bitmap::black_span_result(before, after, row, start, end)
    } else {
        after.data@ == before.data@
    }
}

pub(super) open spec fn transition_count_at(transitions: Seq<u32>, x: u32) -> nat
    decreases transitions.len(),
{
    if transitions.len() == 0 {
        0
    } else {
        transition_count_at(transitions.subrange(0, transitions.len() - 1), x)
            + if transitions[transitions.len() - 1] <= x { 1nat } else { 0nat }
    }
}

pub(super) open spec fn transition_black_at(transitions: Seq<u32>, x: u32) -> bool {
    transition_count_at(transitions, x) % 2 == 1
}

pub(super) open spec fn row_cache_matches(
    bitmap: &crate::Bitmap, row: u32, transitions: Seq<u32>,
) -> bool {
    row < bitmap.height
        && forall|x: u32| x < bitmap.width ==>
            #[trigger] crate::spec::bitmap::stored_pixel(bitmap, x, row)
                == transition_black_at(transitions, x)
}

pub(super) open spec fn white_reference_cache(transitions: Seq<u32>, width: u32) -> bool {
    forall|x: u32| x < width ==> !#[trigger] transition_black_at(transitions, x)
}

pub(super) open spec fn span_pixels(
    before: &crate::Bitmap, after: &crate::Bitmap,
    row: u32, start: u32, end: u32, black: bool,
) -> bool {
    forall|x: u32, y: u32| x < before.width && y < before.height ==>
        #[trigger] crate::spec::bitmap::stored_pixel(after, x, y)
            == ((black && y == row && start <= x < end)
                || crate::spec::bitmap::stored_pixel(before, x, y))
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

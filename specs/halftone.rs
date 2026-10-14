use vstd::prelude::*;

verus! {

pub(crate) closed spec fn work_limit(work: &super::WorkBudget) -> u64 {
    work.limit
}

pub(crate) closed spec fn work_used(work: &super::WorkBudget) -> u64 {
    work.used
}

pub(super) proof fn work_fields(work: &super::WorkBudget)
    ensures
        work_limit(work) == work.limit,
        work_used(work) == work.used,
{}

pub(super) open spec fn pattern_flags_valid(flags: u8) -> bool {
    flags & 0xf8 == 0
        && (flags & 1 == 0 || (flags / 2) % 4 == 0)
}

pub(super) open spec fn halftone_flags_valid(flags: u8) -> bool {
    flags & 1 == 0 || ((flags / 2) % 4 == 0 && flags & 8 == 0)
}

pub(super) open spec fn operator_code_valid(flags: u8) -> bool {
    ((flags >> 4) & 7) <= 4
}

pub(super) open spec fn operator_for_code(
    code: u8,
) -> Option<crate::bitmap::CombinationOperator> {
    match code {
        0 => Some(crate::bitmap::CombinationOperator::Or),
        1 => Some(crate::bitmap::CombinationOperator::And),
        2 => Some(crate::bitmap::CombinationOperator::Xor),
        3 => Some(crate::bitmap::CombinationOperator::Xnor),
        4 => Some(crate::bitmap::CombinationOperator::Replace),
        _ => None,
    }
}

pub(crate) open spec fn signed_u32(value: u32) -> int {
    if value <= i32::MAX as u32 {
        value as int
    } else {
        value as int - 4_294_967_296
    }
}

pub(crate) open spec fn pattern_count(
    header: super::PatternDictionaryHeader,
) -> nat {
    header.gray_max as nat + 1
}

pub(crate) open spec fn pow2(exponent: nat) -> nat
    decreases exponent,
{
    if exponent == 0 {
        1
    } else {
        2 * pow2((exponent - 1) as nat)
    }
}

pub(crate) open spec fn gray_depth_valid(count: u64, depth: u8) -> bool {
    1 <= count
        && count as nat <= pow2(32)
        && if count == 1 {
            depth == 0
        } else {
            1 <= depth <= 32
                && pow2((depth - 1) as nat) < count as nat
                && count as nat <= pow2(depth as nat)
        }
}

pub(crate) open spec fn gray_depth_expected(count: u64) -> u8 {
    if count <= 1 { 0 }
    else if count <= 2 { 1 }
    else if count <= 4 { 2 }
    else if count <= 8 { 3 }
    else if count <= 16 { 4 }
    else if count <= 32 { 5 }
    else if count <= 64 { 6 }
    else if count <= 128 { 7 }
    else if count <= 256 { 8 }
    else if count <= 512 { 9 }
    else if count <= 1_024 { 10 }
    else if count <= 2_048 { 11 }
    else if count <= 4_096 { 12 }
    else if count <= 8_192 { 13 }
    else if count <= 16_384 { 14 }
    else if count <= 32_768 { 15 }
    else if count <= 65_536 { 16 }
    else if count <= 131_072 { 17 }
    else if count <= 262_144 { 18 }
    else if count <= 524_288 { 19 }
    else if count <= 1_048_576 { 20 }
    else if count <= 2_097_152 { 21 }
    else if count <= 4_194_304 { 22 }
    else if count <= 8_388_608 { 23 }
    else if count <= 16_777_216 { 24 }
    else if count <= 33_554_432 { 25 }
    else if count <= 67_108_864 { 26 }
    else if count <= 134_217_728 { 27 }
    else if count <= 268_435_456 { 28 }
    else if count <= 536_870_912 { 29 }
    else if count <= 1_073_741_824 { 30 }
    else if count <= 2_147_483_648 { 31 }
    else { 32 }
}

pub(super) open spec fn gray_binary_bit(coded: bool, previous: Option<bool>) -> bool {
    match previous {
        Some(bit) => coded != bit,
        None => coded,
    }
}

pub(super) open spec fn index_bit_inserted(
    old_value: u32,
    plane: u8,
    bit: bool,
    new_value: u32,
) -> bool {
    plane < 32
        && new_value
            == if bit {
                old_value | (1u32 << plane)
            } else {
                old_value
            }
}

pub(crate) open spec fn grid_x_fixed(
    header: &super::HalftoneHeader,
    grid_x: u32,
    grid_y: u32,
) -> int {
    header.grid_x as int
        + grid_y as int * header.grid_vector_y as int
        + grid_x as int * header.grid_vector_x as int
}

pub(crate) open spec fn grid_y_fixed(
    header: &super::HalftoneHeader,
    grid_x: u32,
    grid_y: u32,
) -> int {
    header.grid_y as int
        + grid_y as int * header.grid_vector_x as int
        - grid_x as int * header.grid_vector_y as int
}

pub(crate) open spec fn fixed_pixel(value: i64) -> i64 {
    value >> 8
}

pub(super) open spec fn wholly_outside(
    x: i64,
    y: i64,
    pattern_width: u32,
    pattern_height: u32,
    region_width: u32,
    region_height: u32,
) -> bool {
    x as int + pattern_width as int <= 0
        || x as int >= region_width as int
        || y as int + pattern_height as int <= 0
        || y as int >= region_height as int
}

pub(crate) open spec fn indexes_valid(indexes: Seq<u32>, count: u64) -> bool {
    forall|i: int| 0 <= i < indexes.len() ==> (indexes[i] as u64) < count
}

pub(super) open spec fn clipped_axis_valid(
    origin: i64,
    far_edge: i64,
    bound: u32,
    start: i64,
    end: i64,
) -> bool {
    start == if origin < 0 { 0 } else { origin }
        && end == if far_edge < bound as i64 { far_edge } else { bound as i64 }
        && 0 <= start
        && end <= bound
        && (start < end ==> (
            origin <= start
                && end <= far_edge
        ))
}

pub(super) open spec fn row_major_offset(
    width: u32,
    x: u32,
    y: u32,
) -> nat {
    y as nat * width as nat + x as nat
}

pub(super) open spec fn render_precedes(
    first_x: u32,
    first_y: u32,
    second_x: u32,
    second_y: u32,
) -> bool {
    first_y < second_y || (first_y == second_y && first_x < second_x)
}

}

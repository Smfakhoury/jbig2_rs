use vstd::prelude::*;

verus! {

pub(super) proof fn normative_run_value_is_positive_when_makeup(
    black: bool,
    code: super::RunCode,
)
    requires
        super::mmr_spec::run_code_in_implementation_tables(black, code),
        !code.terminal,
    ensures code.value >= 64,
{
    reveal(super::mmr_spec::run_code_in_implementation_tables);
    if black {
        assert(super::BLACK_MAKEUP@.len() == 27);
    } else {
        assert(super::WHITE_MAKEUP@.len() == 27);
    }
    assert(super::COMMON_MAKEUP@.len() == 13);
}

pub(super) proof fn normative_run_value_is_bounded(
    black: bool,
    code: super::RunCode,
)
    requires super::mmr_spec::run_code_in_implementation_tables(black, code),
    ensures code.value <= 2560,
{
    reveal(super::mmr_spec::run_code_in_implementation_tables);
    assert(super::WHITE_TERMINATING@.len() == 64);
    assert(super::BLACK_TERMINATING@.len() == 64);
    assert(super::WHITE_MAKEUP@.len() == 27);
    assert(super::BLACK_MAKEUP@.len() == 27);
    assert(super::COMMON_MAKEUP@.len() == 13);
}

pub(super) proof fn normative_run_length_is_bounded(
    black: bool,
    code: super::RunCode,
)
    requires super::mmr_spec::run_code_in_implementation_tables(black, code),
    ensures 1 <= code.length <= super::MAX_RUN_CODE_BITS,
{
    reveal(super::mmr_spec::run_code_in_implementation_tables);
    assert(super::WHITE_TERMINATING@.len() == 64);
    assert(super::BLACK_TERMINATING@.len() == 64);
    assert(super::WHITE_MAKEUP@.len() == 27);
    assert(super::BLACK_MAKEUP@.len() == 27);
    assert(super::COMMON_MAKEUP@.len() == 13);
}

pub(super) proof fn eofb_value_is_exact()
    ensures super::EOFB == 0x001001,
{}

pub(super) proof fn pass_transition_bounds(x: u32, b2: u32, width: u32)
    requires x < b2 <= width,
    ensures
        x < b2,
        width - b2 < width - x,
{}

pub(super) proof fn horizontal_transition_bounds(
    x: u32,
    width: u32,
    first: u32,
    second: u32,
)
    requires
        x <= width,
        first <= width - x,
        second <= width - (x + first),
        first + second > 0,
    ensures
        x < x + first + second <= width,
        width - (x + first + second) < width - x,
{}

pub(super) proof fn vertical_transition_progress(
    x: u32,
    target: u32,
    width: u32,
    black: bool,
)
    requires x <= target <= width,
    ensures
        target <= width,
        target > x || !black != black,
{}

pub(super) proof fn mode_budget_decreases(count: u64, maximum: u64)
    requires count <= maximum,
    ensures
        maximum + 1 - (count + 1)
            < maximum + 1 - count,
{}

pub(super) proof fn managed_memory_components_bounded(
    bitmap_bytes: u64,
    transition_bytes: u64,
    all_transition_bytes: u64,
    limit: u64,
)
    requires
        all_transition_bytes == transition_bytes * 2,
        bitmap_bytes + all_transition_bytes <= limit,
    ensures
        bitmap_bytes <= limit,
        transition_bytes <= limit,
        all_transition_bytes <= limit,
{}

pub(super) proof fn transition_result_is_deterministic(
    old_x: u32,
    new_x: u32,
    old_black: bool,
    new_black: bool,
    duplicate_new_x: u32,
    duplicate_new_black: bool,
)
    requires
        duplicate_new_x == new_x,
        duplicate_new_black == new_black,
    ensures
        (duplicate_new_x, duplicate_new_black) == (new_x, new_black),
        old_x == old_x,
        old_black == old_black,
{}

}

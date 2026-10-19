use vstd::prelude::*;

verus! {

pub(super) proof fn floor_half_bounds(value: int)
    ensures
        2 * super::symbol_text_spec::floor_half(value) <= value,
        value < 2 * super::symbol_text_spec::floor_half(value) + 2,
{
    reveal(super::symbol_text_spec::floor_half);
    assert(value == (value / 2) * 2 + value % 2) by (nonlinear_arith);
    assert(0 <= value % 2 < 2) by (nonlinear_arith);
}

pub(super) proof fn strip_log_is_unique(size: u8, left: u8, right: u8)
    requires
        super::symbol_text_spec::valid_strip_size(size),
        super::symbol_text_spec::strip_log_valid(size, left),
        super::symbol_text_spec::strip_log_valid(size, right),
    ensures left == right,
{
    reveal(super::symbol_text_spec::valid_strip_size);
    reveal(super::symbol_text_spec::strip_log_valid);
}

pub(super) proof fn code_length_is_bounded(count: u32)
    ensures super::symbol_text_spec::symbol_code_length_expected(count) <= 32,
{
    reveal(super::symbol_text_spec::symbol_code_length_expected);
}

pub(super) proof fn zero_run_still_consumes_work(
    index: nat,
    work: nat,
    total: nat,
    work_limit: nat,
)
    requires
        index < total,
        work < work_limit,
    ensures
        super::symbol_text_spec::export_progress_measure(
            index, work + 1, total, work_limit
        ) < super::symbol_text_spec::export_progress_measure(
            index, work, total, work_limit
        ),
{
    reveal(super::symbol_text_spec::export_progress_measure);
    assert(total + 1 > 0);
    assert((work_limit - (work + 1)) + 1 == work_limit - work);
    assert(
        (work_limit - (work + 1)) * (total + 1) + (total - index)
            < (work_limit - work) * (total + 1) + (total - index)
    ) by (nonlinear_arith);
}

pub(super) proof fn advancing_export_run_decreases_measure(
    index: nat,
    next_index: nat,
    work: nat,
    total: nat,
    work_limit: nat,
)
    requires
        index < next_index <= total,
        work <= work_limit,
    ensures
        super::symbol_text_spec::export_progress_measure(
            next_index, work, total, work_limit
        ) < super::symbol_text_spec::export_progress_measure(
            index, work, total, work_limit
        ),
{
    reveal(super::symbol_text_spec::export_progress_measure);
}

pub(super) proof fn custom_table_oob_is_exact(
    requires_oob: bool,
    supplied_oob: bool,
)
    ensures
        super::symbol_text_spec::custom_oob_compatible(
            requires_oob, supplied_oob
        ) == (requires_oob == supplied_oob),
{
    reveal(super::symbol_text_spec::custom_oob_compatible);
}

pub(super) proof fn clipped_interval_is_bounded(
    origin: int,
    extent: nat,
    bound: nat,
)
    requires extent > 0,
    ensures
        super::symbol_text_spec::clipped_start(origin) >= 0,
        super::symbol_text_spec::clipped_end(origin, extent, bound)
            <= bound as int,
{
    reveal(super::symbol_text_spec::clipped_start);
    reveal(super::symbol_text_spec::clipped_end);
}

pub(super) proof fn placement_is_deterministic(
    state: &super::TextRegionState,
    within_strip_t: u8,
    width: u32,
    height: u32,
    left: super::Placement,
    right: super::Placement,
)
    requires
        super::symbol_text_spec::placement_for_current(
            state, within_strip_t, width, height, left
        ),
        super::symbol_text_spec::placement_for_current(
            state, within_strip_t, width, height, right
        ),
    ensures left == right,
{
    reveal(super::symbol_text_spec::placement_for_current);
}

}

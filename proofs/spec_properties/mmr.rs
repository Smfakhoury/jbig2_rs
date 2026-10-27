// Source-only author navigation; named purposes are not normative approval.
// Guide, official acquisition and source identity: ../../specs/README.md
// All requirement links and inherited locators: ../../validation/traceability/requirements.json
// REQUIREMENT CONTEXT is not a direct normative equation, including for proof/policy helpers.

use vstd::prelude::*;

verus! {

pub(super) proof fn transition_count_append(before: Seq<u32>, position: u32, x: u32)
    ensures super::mmr_spec::transition_count_at(before.push(position), x)
        == super::mmr_spec::transition_count_at(before, x)
            + if position <= x { 1nat } else { 0nat },
{
    vstd::assert_seqs_equal!(
        before.push(position).subrange(0, before.len() as int), before,
    );
}

pub(super) proof fn raster_step_append(
    bytes: Seq<u8>, reference: Seq<u32>, width: u32, start: nat,
    steps: Seq<super::mmr_spec::RasterStep>, step: super::mmr_spec::RasterStep,
)
    requires
        super::mmr_spec::raster_steps(bytes, reference, width, start, steps, step.before),
        super::mmr_spec::raster_step(bytes, reference, width, steps.len() == 0, step),
    ensures super::mmr_spec::raster_steps(
        bytes, reference, width, start, steps.push(step), step.after,
    ),
{
    assert(steps.push(step).drop_last() == steps);
}

pub(super) proof fn raster_row_append(
    bytes: Seq<u8>, width: u32, rows: Seq<super::mmr_spec::RasterRow>,
    row: super::mmr_spec::RasterRow,
)
    requires
        super::mmr_spec::raster_rows(bytes, width, rows),
        row.start == super::mmr_spec::rows_position(rows),
        super::mmr_spec::raster_steps(
            bytes, super::mmr_spec::rows_reference(rows, width), width,
            row.start, row.steps, row.state,
        ),
        row.state.x == width,
    ensures
        super::mmr_spec::raster_rows(bytes, width, rows.push(row)),
        super::mmr_spec::rows_position(rows.push(row)) == row.state.position,
        super::mmr_spec::rows_reference(rows.push(row), width)
            == row.state.transitions.push(width).push(width),
{
    assert(rows.push(row).drop_last() == rows);
}

pub(super) proof fn rows_cache_append(
    bitmap: &crate::Bitmap, rows: Seq<super::mmr_spec::RasterRow>,
    row: super::mmr_spec::RasterRow,
)
    requires
        super::mmr_spec::rows_cache_pixels(bitmap, rows),
        rows.len() < bitmap.height,
        super::mmr_spec::row_cache_matches(
            bitmap, rows.len() as u32,
            row.state.transitions.push(bitmap.width).push(bitmap.width),
        ),
    ensures super::mmr_spec::rows_cache_pixels(bitmap, rows.push(row)),
{
    assert forall|index: int| 0 <= index < rows.push(row).len() implies
        super::mmr_spec::row_cache_matches(
            bitmap, index as u32,
            #[trigger] rows.push(row)[index].state.transitions
                .push(bitmap.width).push(bitmap.width),
        ) by {
        if index < rows.len() {
            assert(rows.push(row)[index] == rows[index]);
        } else {
            assert(index == rows.len());
        }
    }
}

pub(super) proof fn span_row_cache_frame(
    before: &crate::Bitmap, after: &crate::Bitmap,
    row: u32, start: u32, end: u32, black: bool,
)
    requires
        crate::spec::bitmap::storage_valid(before),
        crate::spec::bitmap::storage_valid(after),
        after.width == before.width, after.height == before.height,
        super::mmr_spec::span_pixels(before, after, row, start, end, black),
    ensures forall|other: u32, cache: Seq<u32>|
        other != row && super::mmr_spec::row_cache_matches(before, other, cache) ==>
            super::mmr_spec::row_cache_matches(after, other, cache),
{
    assert forall|other: u32, cache: Seq<u32>|
        other != row && super::mmr_spec::row_cache_matches(before, other, cache) implies
            super::mmr_spec::row_cache_matches(after, other, cache) by {
        assert forall|x: u32| x < before.width implies
            #[trigger] crate::spec::bitmap::stored_pixel(after, x, other)
                == super::mmr_spec::transition_black_at(cache, x) by {}
    }
}

pub(super) proof fn transition_count_all_before(
    transitions: Seq<u32>, bound: u32, x: u32,
)
    requires
        bound <= x,
        forall|index: int| 0 <= index < transitions.len() ==>
            transitions[index] < bound,
    ensures super::mmr_spec::transition_count_at(transitions, x) == transitions.len(),
    decreases transitions.len(),
{
    if transitions.len() > 0 {
        let prefix = transitions.subrange(0, transitions.len() - 1);
        assert forall|index: int| 0 <= index < prefix.len() implies
            #[trigger] prefix[index] < bound by {
            assert(prefix[index] == transitions[index]);
        }
        transition_count_all_before(prefix, bound, x);
        assert(transitions[transitions.len() - 1] < bound);
    }
}

pub(super) proof fn recorded_span_cache_pixels(
    before: &crate::Bitmap, after: &crate::Bitmap,
    row: u32, start: u32, end: u32, black: bool,
    old_cache: Seq<u32>, new_cache: Seq<u32>, old_emitted: bool, new_emitted: bool,
    width: u32,
)
    requires
        super::mmr_spec::row_cache_prefix(before, row, old_cache, old_emitted, start),
        crate::spec::bitmap::storage_valid(after),
        after.width == before.width, after.height == before.height,
        after.stride == before.stride, after.data@.len() == before.data@.len(),
        start <= end <= before.width,
        super::mmr_spec::transition_prefix_valid(old_cache, start, width),
        super::mmr_spec::span_result(before, after, row, start, end, black),
        super::mmr_spec::transition_span_result(
            old_cache, new_cache, old_emitted, new_emitted, start, end, black,
        ),
    ensures super::mmr_spec::row_cache_prefix(after, row, new_cache, new_emitted, end),
{
    crate::views::bitmap_fields(before);
    crate::views::bitmap_fields(after);
    reveal(super::mmr_spec::row_cache_prefix);
    reveal(super::mmr_spec::transition_span_result);
    reveal(super::mmr_spec::transition_prefix_valid);
    reveal(super::mmr_spec::span_result);
    reveal(crate::spec::bitmap::black_span_result);
    assert forall|x: u32| x < before.width implies
        #[trigger] crate::spec::bitmap::stored_pixel(after, x, row)
            == (x < end && super::mmr_spec::transition_black_at(new_cache, x)) by {
        crate::bitmap::bitmap_shared::in_bounds_byte(before, x, row);
        crate::bitmap::bitmap_shared::in_bounds_byte(after, x, row);
        assert(crate::spec::bitmap::stored_pixel(before, x, row)
            == (x < start && super::mmr_spec::transition_black_at(old_cache, x)));
        if black {
            assert(crate::spec::bitmap::stored_pixel(after, x, row)
                == (start <= x < end || crate::spec::bitmap::stored_pixel(before, x, row)));
        } else {
            assert(crate::spec::bitmap::stored_pixel(after, x, row)
                == crate::spec::bitmap::stored_pixel(before, x, row));
        }
        if x < start {
            if start < end && old_emitted != black {
                transition_count_append(old_cache, start, x);
            }
        } else if x < end {
            transition_count_all_before(old_cache, start, x);
            if old_emitted != black {
                transition_count_append(old_cache, start, x);
            }
            assert(super::mmr_spec::transition_black_at(new_cache, x) == black);
        }
    }
}

pub(super) proof fn span_pixel_frame(
    before: &crate::Bitmap, after: &crate::Bitmap,
    row: u32, start: u32, end: u32, black: bool,
)
    requires
        crate::spec::bitmap::storage_valid(before),
        crate::spec::bitmap::storage_valid(after),
        after.width == before.width, after.height == before.height,
        after.stride == before.stride, after.data@.len() == before.data@.len(),
        row < before.height, start <= end <= before.width,
        super::mmr_spec::span_result(before, after, row, start, end, black),
    ensures super::mmr_spec::span_pixels(before, after, row, start, end, black),
{
    crate::views::bitmap_fields(before);
    crate::views::bitmap_fields(after);
    reveal(super::mmr_spec::span_result);
    reveal(crate::spec::bitmap::black_span_result);
    assert forall|x: u32, y: u32| x < before.width && y < before.height implies
        #[trigger] crate::spec::bitmap::stored_pixel(after, x, y)
            == ((black && y == row && start <= x < end)
                || crate::spec::bitmap::stored_pixel(before, x, y)) by {
        crate::bitmap::bitmap_shared::in_bounds_byte(before, x, y);
        crate::bitmap::bitmap_shared::in_bounds_byte(after, x, y);
    }
}

pub(super) proof fn span_keeps_future_white(
    before: &crate::Bitmap, after: &crate::Bitmap,
    row: u32, start: u32, end: u32, black: bool, first: nat,
)
    requires
        super::mmr_spec::future_rows_white(before, first),
        crate::spec::bitmap::storage_valid(after),
        after.width == before.width, after.height == before.height,
        (row as nat) < first,
        super::mmr_spec::span_pixels(before, after, row, start, end, black),
    ensures super::mmr_spec::future_rows_white(after, first),
{
    crate::views::bitmap_fields(before);
    crate::views::bitmap_fields(after);
    assert forall|x: u32, y: u32| x < before.width && first <= y < before.height implies
        !#[trigger] crate::spec::bitmap::stored_pixel(after, x, y) by {
        assert(y != row);
    }
}

pub(super) proof fn zeroed_rows_are_white(bitmap: &crate::Bitmap)
    requires
        crate::spec::bitmap::storage_valid(bitmap),
        crate::spec::bitmap::zeroed_storage(bitmap),
    ensures super::mmr_spec::future_rows_white(bitmap, 0),
{
    crate::views::bitmap_fields(bitmap);
    reveal(crate::spec::bitmap::zeroed_storage);
    assert forall|x: u32, y: u32| x < bitmap.width && y < bitmap.height implies
        !#[trigger] crate::spec::bitmap::stored_pixel(bitmap, x, y) by {
        crate::bitmap::bitmap_shared::in_bounds_byte(bitmap, x, y);
        assert(crate::views::bitmap_data(bitmap)[
            crate::spec::bitmap::byte_index(bitmap.stride, x, y) as int
        ] == 0u8);
        let mask = crate::spec::bitmap::bit_mask(x);
        assert((0u8 & mask) == 0u8) by(bit_vector);
        reveal(crate::spec::bitmap::stored_pixel);
        reveal(crate::spec::bitmap::byte_bit);
    }
}

pub(super) proof fn white_future_advance(bitmap: &crate::Bitmap, first: nat, next: nat)
    requires super::mmr_spec::future_rows_white(bitmap, first), first <= next,
    ensures super::mmr_spec::future_rows_white(bitmap, next),
{
}

pub(super) proof fn empty_row_cache(bitmap: &crate::Bitmap, row: u32)
    requires super::mmr_spec::future_rows_white(bitmap, row as nat), row < bitmap.height,
    ensures super::mmr_spec::row_cache_prefix(bitmap, row, Seq::empty(), false, 0),
{
}

pub(super) proof fn complete_row_cache(
    bitmap: &crate::Bitmap, row: u32, cache: Seq<u32>, emitted: bool,
)
    requires super::mmr_spec::row_cache_prefix(bitmap, row, cache, emitted, bitmap.width),
    ensures super::mmr_spec::row_cache_matches(bitmap, row, cache),
{
}

pub(super) proof fn finished_row_cache(
    bitmap: &crate::Bitmap, row: u32, cache: Seq<u32>, width: u32,
)
    requires super::mmr_spec::row_cache_matches(bitmap, row, cache), width == bitmap.width,
    ensures super::mmr_spec::row_cache_matches(bitmap, row, cache.push(width).push(width)),
{
    assert forall|x: u32| x < width implies
        #[trigger] crate::spec::bitmap::stored_pixel(bitmap, x, row)
            == super::mmr_spec::transition_black_at(cache.push(width).push(width), x) by {
        transition_count_append(cache, width, x);
        transition_count_append(cache.push(width), width, x);
    }
}

pub(super) proof fn initial_reference_cache(width: u32)
    ensures super::mmr_spec::white_reference_cache(
        Seq::empty().push(width).push(width), width,
    ),
{
    assert forall|x: u32| x < width implies
        !#[trigger] super::mmr_spec::transition_black_at(
            Seq::empty().push(width).push(width), x,
        ) by {
        transition_count_append(Seq::empty(), width, x);
        transition_count_append(Seq::empty().push(width), width, x);
    }
}

// Model navigation: spec::reference_prefix_skipped [proof-support]
// Purpose: Bounded skipped prefix excluding eligible reference candidates.
// Audit: mmr-reference-prefix-skipped
// REQUIREMENT CONTEXT: T6-REFERENCE-CHANGING-ELEMENTS | ITU-T T.6 (1988) | 2.2.2; 2.2.5.2
// Fidelity: unresolved; no independent normative approval.
pub(super) open spec fn reference_prefix_skipped(
    transitions: Seq<u32>, x: u32, initial: bool, end: int,
) -> bool {
    0 <= end <= transitions.len()
        && forall|previous: int| 0 <= previous < end ==>
            !super::mmr_spec::reference_position_eligible(
                #[trigger] transitions[previous], x, initial,
            )
}

pub(super) proof fn reference_suffix_is_global(
    transitions: Seq<u32>, x: u32, black: bool, width: u32, initial: bool,
    start: int, b1: u32, b2: u32,
)
    requires
        0 <= start < transitions.len(),
        reference_prefix_skipped(transitions, x, initial, start),
        super::mmr_spec::reference_pair_matches(
            transitions, x, black, width, initial, start, b1, b2,
        ),
    ensures super::mmr_spec::reference_pair_matches(
        transitions, x, black, width, initial, 0, b1, b2,
    ),
{
    if exists|index: int| super::mmr_spec::first_reference_candidate(
        transitions, x, black, initial, start, index,
    ) && b1 == transitions[index] && b2 == transitions[index + 1] {
        let index = choose|index: int| super::mmr_spec::first_reference_candidate(
            transitions, x, black, initial, start, index,
        ) && b1 == transitions[index] && b2 == transitions[index + 1];
        assert forall|previous: int| 0 <= previous < index implies
            !super::mmr_spec::reference_position_eligible(
                #[trigger] transitions[previous], x, initial,
            ) || (previous % 2 == 0) == black by {
            if previous < start {
                assert(!super::mmr_spec::reference_position_eligible(
                    transitions[previous], x, initial,
                ));
            }
        }
        assert(super::mmr_spec::first_reference_candidate(
            transitions, x, black, initial, 0, index,
        ));
    } else {
        assert(b1 == width && b2 == width);
        assert forall|index: int| !#[trigger] super::mmr_spec::first_reference_candidate(
            transitions, x, black, initial, 0, index,
        ) by {
            if super::mmr_spec::first_reference_candidate(
                transitions, x, black, initial, 0, index,
            ) {
                if index < start {
                    assert(!super::mmr_spec::reference_position_eligible(
                        transitions[index], x, initial,
                    ));
                } else {
                    assert(super::mmr_spec::first_reference_candidate(
                        transitions, x, black, initial, start, index,
                    ));
                }
            }
        }
    }
}

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

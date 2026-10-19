use vstd::prelude::*;

verus! {

pub(super) proof fn pow2_positive(exponent: nat)
    ensures super::generic_region_spec::pow2(exponent) > 0,
    decreases exponent,
{
    if exponent > 0 {
        pow2_positive((exponent - 1) as nat);
    }
}

pub(super) proof fn pow2_step(exponent: nat)
    ensures
        super::generic_region_spec::pow2(exponent + 1)
            == 2 * super::generic_region_spec::pow2(exponent),
{}

pub(super) proof fn context_value_bound(bits: Seq<bool>)
    ensures
        super::generic_region_spec::context_value(bits)
            < super::generic_region_spec::pow2(bits.len()),
    decreases bits.len(),
{
    if bits.len() > 0 {
        context_value_bound(bits.drop_last());
        pow2_step((bits.len() - 1) as nat);
    }
}

pub(super) proof fn context_slot_identity(bits: Seq<bool>, value: bool)
    ensures
        super::generic_region_spec::context_value(bits.push(value))
            == 2 * super::generic_region_spec::context_value(bits)
                + if value { 1nat } else { 0nat },
{
    reveal(super::generic_region_spec::context_value);
    assert(bits.push(value).drop_last() == bits);
    assert(bits.push(value).last() == value);
}

pub(super) proof fn all_generic_templates_have_bounded_sltp(
    template: super::GenericTemplate,
)
    ensures
        super::generic_region_spec::generic_sltp_context(template)
            < super::generic_region_spec::pow2(
                super::generic_region_spec::generic_context_bits(template),
            ),
{
    match template {
        super::GenericTemplate::Template0 => {
            assert(super::generic_region_spec::pow2(16) == 65536) by (compute);
        },
        super::GenericTemplate::Template1 => {
            assert(super::generic_region_spec::pow2(13) == 8192) by (compute);
        },
        super::GenericTemplate::Template2 => {
            assert(super::generic_region_spec::pow2(10) == 1024) by (compute);
        },
        super::GenericTemplate::Template3 => {
            assert(super::generic_region_spec::pow2(10) == 1024) by (compute);
        },
    }
}

pub(super) proof fn all_refinement_templates_have_bounded_sltp(
    template: super::RefinementTemplate,
)
    ensures
        super::generic_region_spec::refinement_sltp_context(template)
            < super::generic_region_spec::pow2(
                super::generic_region_spec::refinement_context_bits(template),
            ),
{
    match template {
        super::RefinementTemplate::Template0 => {
            assert(super::generic_region_spec::pow2(13) == 8192) by (compute);
        },
        super::RefinementTemplate::Template1 => {
            assert(super::generic_region_spec::pow2(10) == 1024) by (compute);
        },
    }
}

pub(super) proof fn every_generic_adaptive_slot_has_legal_number(
    template: super::GenericTemplate,
    extended: bool,
    slot: nat,
)
    requires
        slot < super::generic_region_spec::generic_slot_count(template),
    ensures
        match super::generic_region_spec::generic_slot_adaptive(
            template,
            extended,
            slot,
        ) {
            Some(number) =>
                1 <= number
                && number <= super::generic_region_spec::generic_adaptive_count(
                    template,
                    extended,
                ),
            None => true,
        },
{
    match template {
        super::GenericTemplate::Template0 => {
            if extended {
                assert(slot < 16);
            } else {
                assert(slot < 16);
            }
        },
        super::GenericTemplate::Template1 => { assert(slot < 13); },
        super::GenericTemplate::Template2 => { assert(slot < 10); },
        super::GenericTemplate::Template3 => { assert(slot < 10); },
    }
}

pub(super) proof fn every_refinement_adaptive_slot_has_legal_number(
    template: super::RefinementTemplate,
    slot: nat,
)
    requires
        slot < super::generic_region_spec::refinement_slot_count(template),
    ensures
        match super::generic_region_spec::refinement_slot_adaptive(template, slot) {
            Some(number) => 1 <= number <= 2,
            None => true,
        },
{
    match template {
        super::RefinementTemplate::Template0 => { assert(slot < 13); },
        super::RefinementTemplate::Template1 => { assert(slot < 10); },
    }
}

pub(super) proof fn out_of_bounds_samples_are_zero(
    pixels: Seq<Seq<bool>>,
    x: int,
    y: int,
)
    requires
        y < 0 || y >= pixels.len()
            || x < 0
            || (0 <= y < pixels.len() && x >= pixels[y].len()),
    ensures !super::generic_region_spec::sample_or_zero(pixels, x, y),
{}

pub(super) proof fn tpgd_state_transition(ltp: bool, sltp: bool)
    ensures
        super::generic_region_spec::tpgd_next(ltp, sltp)
            == (if sltp { !ltp } else { ltp }),
{}

pub(super) proof fn tpgd_typical_row_consumes_no_pixels(
    width: nat,
    skipped: nat,
)
    ensures
        super::generic_region_spec::generic_pixel_decisions(
            width,
            true,
            skipped,
        ) == 0,
{}

pub(super) proof fn skip_never_increases_consumption(
    width: nat,
    skipped: nat,
)
    ensures
        super::generic_region_spec::generic_pixel_decisions(
            width,
            false,
            skipped,
        ) <= width,
{}

pub(super) proof fn tpgr_state_transition(
    ltp: bool,
    neighbourhood_uniform: bool,
)
    ensures
        super::generic_region_spec::tpgr_uses_prediction(
            ltp,
            neighbourhood_uniform,
        ) ==> ltp && neighbourhood_uniform,
{}

pub(super) proof fn tpgr_prediction_never_increases_consumption(
    pixels: nat,
    predictable: nat,
    ltp: bool,
)
    ensures
        super::generic_region_spec::refinement_pixel_decisions(
            pixels,
            predictable,
            ltp,
        ) <= pixels,
{}

pub(super) proof fn compatible_snapshot_is_reflexive(
    template: nat,
    extended: bool,
    count: nat,
)
    ensures
        super::generic_region_spec::contexts_compatible(
            template,
            extended,
            count,
            template,
            extended,
            count,
        ),
{}

pub(super) proof fn reset_contexts_are_valid(count: nat)
    ensures
        super::generic_region_spec::probability_states_valid(
            super::generic_region_spec::reset_context_states(count),
        ),
{
    assert forall|index: int| 0 <= index < count implies
        #[trigger] super::generic_region_spec::reset_context_states(count)[index]
            == (0nat, 0nat) by {
        reveal(super::generic_region_spec::reset_context_states);
    }
}

pub(super) proof fn restore_preserves_context_validity(
    current: Seq<(nat, nat)>,
    snapshot: Seq<(nat, nat)>,
    compatible: bool,
)
    requires
        super::generic_region_spec::probability_states_valid(current),
        super::generic_region_spec::probability_states_valid(snapshot),
    ensures
        super::generic_region_spec::probability_states_valid(
            super::generic_region_spec::restore_context_states(
                current,
                snapshot,
                compatible,
            ),
        ),
{
    reveal(super::generic_region_spec::restore_context_states);
}

proof fn multiply_monotonic(factor: nat, first: nat, second: nat)
    requires first <= second,
    ensures factor * first <= factor * second,
{
    let difference = (second - first) as nat;
    assert(second == first + difference);
    assert(0 <= factor * difference);
    assert(factor * second == factor * (first + difference));
    assert(
        factor * (first + difference)
            == factor * first + factor * difference
    ) by (nonlinear_arith);
}

pub(super) proof fn generic_work_monotonic_rows(
    width: nat,
    first: nat,
    second: nat,
    typical_prediction: bool,
)
    requires first <= second,
    ensures
        super::generic_region_spec::generic_work_spec(
            width,
            first,
            typical_prediction,
        ) <= super::generic_region_spec::generic_work_spec(
            width,
            second,
            typical_prediction,
        ),
{
    reveal(super::generic_region_spec::generic_work_spec);
    multiply_monotonic(width, first, second);
    multiply_monotonic(width * 17, first, second);
}

pub(super) proof fn refinement_work_monotonic_height(
    width: nat,
    first: nat,
    second: nat,
    typical_prediction: bool,
)
    requires first <= second,
    ensures
        super::generic_region_spec::refinement_work_spec(
            width,
            first,
            typical_prediction,
        ) <= super::generic_region_spec::refinement_work_spec(
            width,
            second,
            typical_prediction,
        ),
{
    reveal(super::generic_region_spec::refinement_work_spec);
    if typical_prediction {
        multiply_monotonic(width, first, second);
        multiply_monotonic(width * 23, first, second);
    } else {
        multiply_monotonic(width, first, second);
        multiply_monotonic(width * 14, first, second);
    }
}

pub(super) proof fn bitmap_storage_monotonic_height(
    width: nat,
    first: nat,
    second: nat,
)
    requires first <= second,
    ensures
        super::generic_region_spec::bitmap_storage_spec(width, first)
            <= super::generic_region_spec::bitmap_storage_spec(width, second),
{
    reveal(super::generic_region_spec::bitmap_storage_spec);
    let stride = (width + 7) / 8;
    multiply_monotonic(stride, first, second);
}

pub(super) proof fn row_loop_measure_decreases(row: nat, rows: nat)
    requires row < rows,
    ensures
        super::generic_region_spec::row_measure(row + 1, rows)
            < super::generic_region_spec::row_measure(row, rows),
{}

pub(super) proof fn pixel_loop_measure_decreases(x: nat, width: nat)
    requires x < width,
    ensures
        super::generic_region_spec::pixel_measure(x + 1, width)
            < super::generic_region_spec::pixel_measure(x, width),
{}

pub(super) proof fn unknown_length_marker_accounting(
    marker_offset: nat,
    mmr: bool,
)
    ensures
        super::generic_region_spec::unknown_length_accounting(
            marker_offset,
            mmr,
        ).1 == marker_offset + 6,
        mmr ==> super::generic_region_spec::unknown_length_accounting(
            marker_offset,
            mmr,
        ).0 == marker_offset,
        !mmr ==> super::generic_region_spec::unknown_length_accounting(
            marker_offset,
            mmr,
        ).0 == marker_offset + 2,
{}

/// Equal entropy decisions and context-index streams produce equal pixels.
pub(super) proof fn decoding_is_deterministic(
    first_decisions: Seq<bool>,
    second_decisions: Seq<bool>,
    first_contexts: Seq<nat>,
    second_contexts: Seq<nat>,
)
    requires
        first_decisions == second_decisions,
        first_contexts == second_contexts,
    ensures
        super::generic_region_spec::decoded_pixels(
            first_decisions,
            first_contexts,
        ) == super::generic_region_spec::decoded_pixels(
            second_decisions,
            second_contexts,
        ),
{}

}

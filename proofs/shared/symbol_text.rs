use vstd::prelude::*;

verus! {

pub(super) proof fn tracker_fields(tracker: &super::ResourceTracker)
    ensures
        super::symbol_text_spec::work_units(tracker) == tracker.work_units,
        super::symbol_text_spec::bitmap_bytes(tracker) == tracker.decoded_bitmap_bytes,
{    reveal(super::symbol_text_spec::work_units);
    reveal(super::symbol_text_spec::bitmap_bytes);
}

pub(super) proof fn pow2_positive(exponent: nat)
    ensures super::symbol_text_spec::pow2(exponent) > 0,
    decreases exponent,
{
    reveal(super::symbol_text_spec::pow2);
    if exponent != 0 {
        pow2_positive((exponent - 1) as nat);
    }
}

}

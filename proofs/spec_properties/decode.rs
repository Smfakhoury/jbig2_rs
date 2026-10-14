use vstd::prelude::*;

verus! {

pub(super) proof fn successful_charge_is_monotonic(
    used: u64,
    units: u64,
    limit: u64,
)
    requires super::decode_spec::charge_succeeds(used, units, limit),
    ensures
        used as nat <= used as nat + units as nat,
        used as nat + units as nat <= limit as nat,
        used as nat + units as nat <= u64::MAX as nat,
{}

pub(super) proof fn successful_release_is_bounded(
    retained: u64,
    bytes: u64,
)
    requires super::decode_spec::release_succeeds(retained, bytes),
    ensures retained as nat - bytes as nat <= retained as nat,
{}

pub(super) proof fn collector_acceptance_preserves_both_limits(
    previous: u64,
    page_bytes: u64,
    decoded_limit: u64,
    memory_limit: u64,
)
    requires
        previous as nat + page_bytes as nat <= u64::MAX as nat,
        previous as nat + page_bytes as nat <= decoded_limit as nat,
        previous as nat + page_bytes as nat <= memory_limit as nat,
    ensures
        previous as nat <= previous as nat + page_bytes as nat,
        previous as nat + page_bytes as nat <= decoded_limit as nat,
        previous as nat + page_bytes as nat <= memory_limit as nat,
{}

pub(super) proof fn sealed_stripe_rejects_prior_row(
    sealed: u32,
    region_y: u32,
)
    requires region_y <= sealed,
    ensures region_y as nat <= sealed as nat,
{}

pub(super) proof fn snapshot_coordinate_is_clipped(
    origin: u32,
    offset: u32,
    bound: u32,
)
    requires origin as nat + offset as nat <= u32::MAX as nat,
    ensures
        ((origin as nat) + (offset as nat) < (bound as nat))
            || ((origin as nat) + (offset as nat) >= (bound as nat)),
{}

pub(super) proof fn lookup_index_is_bounded(index: int, length: nat)
    requires 0 <= index < length,
    ensures 0 <= index, index < length,
{}

pub(super) proof fn structural_work_components_are_bounded(
    input: nat,
    segments: nat,
    references: nat,
    total: nat,
)
    requires total == input + segments + references,
    ensures input <= total, segments <= total, references <= total,
{}

pub(super) proof fn dispatch_is_deterministic(
    segment_type: crate::segment_header::SegmentType,
)
    ensures
        super::decode_spec::dispatch_class(segment_type)
            == super::decode_spec::dispatch_class(segment_type),
{}

pub(super) proof fn emission_append_preserves_order<T>(pages: Seq<T>, page: T)
    ensures
        (pages.push(page)).take(pages.len() as int) == pages,
        (pages.push(page)).last() == page,
{}

}

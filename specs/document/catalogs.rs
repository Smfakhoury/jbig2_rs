// Project catalog accounting and proof support; no normative completeness claim.
use super::*;

verus! {

// Model navigation: spec::reference_total [project-policy]
// Purpose: Unbounded referred-entry count for exact project work/reference accounting.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub(crate) open spec fn reference_total(
    segments: Seq<crate::document::ParsedSegment<'_>>,
    prefix: nat,
) -> nat
    recommends prefix <= segments.len(),
    decreases prefix,
{
    if prefix == 0 {
        0
    } else {
        reference_total(segments, (prefix - 1) as nat)
            + segments[(prefix - 1) as int].header.referred_segments@.len()
    }
}

// Lift prefix overflow to the complete concrete catalog count.
pub(crate) proof fn reference_total_monotone(
    segments: Seq<crate::document::ParsedSegment<'_>>,
    low: nat,
    high: nat,
)
    requires low <= high <= segments.len(),
    ensures reference_total(segments, low) <= reference_total(segments, high),
    decreases high - low,
{
    if low < high {
        reference_total_monotone(segments, low, (high - 1) as nat);
    }
}

}

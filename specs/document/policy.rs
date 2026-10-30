// Source-only author navigation, not normative approval.
// Guide: ../README.md; audit ledger: ../../validation/traceability/requirements.json

use super::*;

verus! {

// Model navigation: spec::reference_class_allowed [candidate-standard-facing-model]
// Purpose: Abstract document order, reference ownership, retention and profile candidates.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn reference_class_allowed(
    owner: SegmentClass,
    references: Seq<Reference>,
) -> bool {
    match owner {
        SegmentClass::SymbolDictionary =>
            references.filter(|r: Reference|
                r.class == SegmentClass::Tables
            ).len() <= 4
            && forall|i: int| 0 <= i < references.len() ==>
                references[i].class == SegmentClass::SymbolDictionary
                    || references[i].class == SegmentClass::Tables,
        SegmentClass::IntermediateTextRegion
        | SegmentClass::ImmediateTextRegion =>
            references.filter(|r: Reference|
                r.class == SegmentClass::Tables
            ).len() <= 8
            && forall|i: int| 0 <= i < references.len() ==>
                references[i].class == SegmentClass::SymbolDictionary
                    || references[i].class == SegmentClass::Tables,
        SegmentClass::IntermediateHalftoneRegion
        | SegmentClass::ImmediateHalftoneRegion =>
            references.len() == 1
                && references[0].class == SegmentClass::PatternDictionary,
        SegmentClass::IntermediateRefinementRegion =>
            references.len() == 1
                && (
                    references[0].class == SegmentClass::IntermediateTextRegion
                    || references[0].class
                        == SegmentClass::IntermediateHalftoneRegion
                    || references[0].class
                        == SegmentClass::IntermediateGenericRegion
                    || references[0].class
                        == SegmentClass::IntermediateRefinementRegion
                ),
        SegmentClass::ImmediateRefinementRegion =>
            references.len() <= 1
                && forall|i: int| 0 <= i < references.len() ==>
                    references[i].class == SegmentClass::IntermediateTextRegion
                    || references[i].class
                        == SegmentClass::IntermediateHalftoneRegion
                    || references[i].class
                        == SegmentClass::IntermediateGenericRegion
                    || references[i].class
                        == SegmentClass::IntermediateRefinementRegion,
        SegmentClass::PatternDictionary
        | SegmentClass::IntermediateGenericRegion
        | SegmentClass::ImmediateGenericRegion
        | SegmentClass::PageInformation
        | SegmentClass::EndOfPage
        | SegmentClass::EndOfStripe
        | SegmentClass::EndOfFile
        | SegmentClass::Profiles
        | SegmentClass::Tables
        | SegmentClass::ColourPalette => references.len() == 0,
        SegmentClass::Extension => true,
    }
}

// Model navigation: spec::segment_policy_allowed [candidate-standard-facing-model]
// Purpose: Abstract document order, reference ownership, retention and profile candidates.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn segment_policy_allowed(segment: Segment) -> bool {
    page_association_allowed(segment.class, segment.page)
        && reference_class_allowed(segment.class, segment.references)
}

// Model navigation: spec::depth_witness [proof-support]
// Purpose: Bounded rank witnesses and supplied lower-reference chain vocabulary.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn depth_witness(
    segments: Seq<Segment>,
    depths: Map<nat, nat>,
    max_depth: nat,
) -> bool {
    &&& forall|i: int| 0 <= i < segments.len() ==> {
        &&& depths.dom().contains(segments[i].number)
        &&& 1 <= depths[segments[i].number] <= max_depth
    }
    &&& forall|i: int, r: int|
        0 <= i < segments.len() && 0 <= r < segments[i].references.len()
            && contains_number(segments, segments[i].references[r].number)
            ==> {
                let target = #[trigger] segments[i].references[r].number;
                &&& depths.dom().contains(target)
                &&& depths[target] + 1 <= depths[segments[i].number]
            }
}

// Model navigation: spec::is_last_use [candidate-standard-facing-model]
// Purpose: Abstract document order, reference ownership, retention and profile candidates.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn is_last_use(
    uses: Seq<nat>,
    use_index: int,
    number: nat,
) -> bool {
    &&& 0 <= use_index < uses.len()
    &&& uses[use_index] == number
    &&& forall|later: int| use_index < later < uses.len() ==> uses[later] != number
}

// Model navigation: spec::release_allowed [candidate-standard-facing-model]
// Purpose: Abstract document order, reference ownership, retention and profile candidates.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn release_allowed(
    uses: Seq<nat>,
    use_index: int,
    number: nat,
) -> bool {
    is_last_use(uses, use_index, number)
}

// Model navigation: spec::producer_retained_for_later_use [candidate-standard-facing-model]
// Purpose: Abstract document order, reference ownership, retention and profile candidates.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn producer_retained_for_later_use(
    uses: Seq<nat>,
    producer_number: nat,
    retained: bool,
) -> bool {
    (exists|i: int| 0 <= i < uses.len() && uses[i] == producer_number)
        ==> retained
}

// Model navigation: spec::lower_chain [proof-support]
// Purpose: Bounded rank witnesses and supplied lower-reference chain vocabulary.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn lower_chain(chain: Seq<nat>) -> bool
    decreases chain.len(),
{
    if chain.len() <= 1 {
        true
    } else {
        lower_chain(chain.drop_last())
            && chain.last() < chain[chain.len() - 2]
    }
}

// Model navigation: spec::contiguous_pages [candidate-standard-facing-model]
// Purpose: Abstract document order, reference ownership, retention and profile candidates.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn contiguous_pages(pages: Seq<nat>) -> bool {
    forall|i: int| 0 <= i < pages.len() ==> pages[i] == i + 1
}

pub struct ProfileDeclaration {
    pub segment_index: nat,
    pub page: nat,
    pub values: Set<nat>,
}

// Model navigation: spec::valid_profiles [candidate-standard-facing-model]
// Purpose: Abstract document order, reference ownership, retention and profile candidates.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn valid_profiles(
    declarations: Seq<ProfileDeclaration>,
) -> bool {
    declarations.len() == 0
    || {
        &&& declarations[0].segment_index == 0
        &&& declarations[0].page == 0
        &&& forall|i: int| 1 <= i < declarations.len() ==> {
            &&& #[trigger] declarations[i].page != 0
            &&& declarations[0].values.subset_of(declarations[i].values)
            &&& forall|j: int| 1 <= j < i ==>
                declarations[j].page != declarations[i].page
        }
    }
}

pub struct PageState {
    pub information_seen: bool,
    pub ended: bool,
    pub striped: bool,
    pub fixed_height: Option<nat>,
    pub last_stripe: Option<nat>,
    pub max_stripe_size: nat,
}

pub enum PageEvent {
    Information {
        striped: bool,
        height: Option<nat>,
        max_stripe_size: nat,
    },
    Stripe { row: nat },
    End,
    Content,
}

// Model navigation: spec::initial_page_state [candidate-standard-facing-model]
// Purpose: Abstract page-event/final-height and marker-extent candidates.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn initial_page_state() -> PageState {
    PageState {
        information_seen: false,
        ended: false,
        striped: false,
        fixed_height: None,
        last_stripe: None,
        max_stripe_size: 0,
    }
}

// Model navigation: spec::page_step [candidate-standard-facing-model]
// Purpose: Abstract page-event/final-height and marker-extent candidates.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn page_step(state: PageState, event: PageEvent) -> Option<PageState> {
    match event {
        PageEvent::Information { striped, height, max_stripe_size } => {
            if state.information_seen {
                None
            } else if height.is_none() && !striped {
                None
            } else {
                Some(PageState {
                    information_seen: true,
                    ended: false,
                    striped,
                    fixed_height: height,
                    last_stripe: None,
                    max_stripe_size,
                })
            }
        },
        PageEvent::Stripe { row } => {
            let ordered = match state.last_stripe {
                Some(previous) =>
                    previous < row && row - previous <= state.max_stripe_size,
                None => row <= state.max_stripe_size,
            };
            let below_height = match state.fixed_height {
                Some(height) => row < height,
                None => true,
            };
            if state.information_seen && !state.ended && state.striped
                && ordered && below_height {
                Some(PageState { last_stripe: Some(row), ..state })
            } else {
                None
            }
        },
        PageEvent::End => {
            if state.information_seen && !state.ended
                && (state.fixed_height.is_some() || state.last_stripe.is_some()) {
                Some(PageState { ended: true, ..state })
            } else {
                None
            }
        },
        PageEvent::Content => {
            if state.information_seen && !state.ended {
                Some(state)
            } else {
                None
            }
        },
    }
}

// Model navigation: spec::final_height [candidate-standard-facing-model]
// Purpose: Abstract page-event/final-height and marker-extent candidates.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn final_height(state: PageState) -> Option<nat> {
    match state.fixed_height {
        Some(height) => Some(height),
        None => match state.last_stripe {
            Some(row) => Some(row + 1),
            None => None,
        },
    }
}

}

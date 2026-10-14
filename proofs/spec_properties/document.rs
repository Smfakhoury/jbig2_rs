use vstd::prelude::*;

verus! {

pub(super) proof fn reader_cap_is_inclusive(consumed: nat, cap: nat)
    requires consumed <= cap,
    ensures consumed <= cap,
{}

pub(super) proof fn bounded_reader_never_exceeds_cap(
    chunks: Seq<nat>,
    cap: nat,
)
    requires super::document_spec::bounded_reads(chunks, cap),
    ensures super::document_spec::sum(chunks) <= cap,
{
    if chunks.len() == 0 {
        assert(super::document_spec::sum(chunks) == 0);
    }
}

pub(super) proof fn envelope_accounts_for_every_byte(
    envelope: super::document_spec::Envelope,
    file_header_bytes: nat,
    header_bytes: Seq<nat>,
    payload_bytes: Seq<nat>,
)
    ensures
        super::document_spec::envelope_bytes(
            envelope,
            file_header_bytes,
            header_bytes,
            payload_bytes,
        ) >= super::document_spec::sum(payload_bytes),
{
    match envelope {
        super::document_spec::Envelope::Sequential => {},
        super::document_spec::Envelope::RandomAccess => {},
        super::document_spec::Envelope::Embedded => {},
    }
}

pub(super) proof fn standalone_envelopes_have_equal_accounting(
    file_header_bytes: nat,
    header_bytes: Seq<nat>,
    payload_bytes: Seq<nat>,
)
    ensures
        super::document_spec::envelope_bytes(
            super::document_spec::Envelope::Sequential,
            file_header_bytes,
            header_bytes,
            payload_bytes,
        ) == super::document_spec::envelope_bytes(
            super::document_spec::Envelope::RandomAccess,
            file_header_bytes,
            header_bytes,
            payload_bytes,
        ),
{}

pub(super) proof fn embedded_omits_only_file_header(
    file_header_bytes: nat,
    header_bytes: Seq<nat>,
    payload_bytes: Seq<nat>,
)
    ensures
        super::document_spec::envelope_bytes(
            super::document_spec::Envelope::Sequential,
            file_header_bytes,
            header_bytes,
            payload_bytes,
        ) == file_header_bytes
            + super::document_spec::envelope_bytes(
                super::document_spec::Envelope::Embedded,
                file_header_bytes,
                header_bytes,
                payload_bytes,
            ),
{}

pub(super) proof fn unknown_marker_consumption_is_bounded(
    input_len: nat,
    marker_offset: nat,
)
    requires super::document_spec::unknown_marker_fits(input_len, marker_offset),
    ensures
        marker_offset >= 18,
        marker_offset + 6 <= input_len,
{}

pub(super) proof fn unknown_marker_row_is_bounded(
    input_len: nat,
    marker_offset: nat,
    declared_height: nat,
    row_count: nat,
)
    requires
        super::document_spec::unknown_marker_valid(
            input_len,
            marker_offset,
            declared_height,
            row_count,
        ),
    ensures
        18 <= marker_offset,
        marker_offset + 6 <= input_len,
        row_count <= declared_height,
{}

pub(super) proof fn segment_and_page_counts_are_bounded(
    segment_count: nat,
    page_count: nat,
    max_segments: nat,
    max_pages: nat,
)
    requires
        super::document_spec::count_limits(
            segment_count,
            page_count,
            max_segments,
            max_pages,
        ),
    ensures
        segment_count <= max_segments,
        page_count <= max_pages,
{}

pub(super) proof fn work_limit_bounds_components(
    input_bytes: nat,
    segments: Seq<super::document_spec::Segment>,
    limit: nat,
)
    requires super::document_spec::work(input_bytes, segments) <= limit,
    ensures
        input_bytes <= limit,
        segments.len() <= limit,
        super::document_spec::reference_count(segments) <= limit,
{}

pub(super) proof fn distinct_indexes_cannot_duplicate(
    segments: Seq<super::document_spec::Segment>,
    i: int,
    j: int,
)
    requires
        super::document_spec::unique_numbers(segments),
        0 <= i < segments.len(),
        0 <= j < segments.len(),
        i != j,
    ensures segments[i].number != segments[j].number,
{}

pub(super) proof fn every_reference_is_present_and_lower(
    segments: Seq<super::document_spec::Segment>,
    globals: Set<nat>,
    segment_index: int,
    reference_index: int,
)
    requires
        super::document_spec::references_resolve_lower(segments, globals),
        0 <= segment_index < segments.len(),
        0 <= reference_index < segments[segment_index].references.len(),
    ensures
        segments[segment_index].references[reference_index].number
            < segments[segment_index].number,
        super::document_spec::contains_number(
            segments,
            segments[segment_index].references[reference_index].number,
        ) || globals.contains(
            segments[segment_index].references[reference_index].number,
        ),
{}

pub(super) proof fn accepted_segment_obeys_type_and_page_rules(
    segment: super::document_spec::Segment,
)
    requires super::document_spec::segment_policy_allowed(segment),
    ensures
        super::document_spec::page_association_allowed(
            segment.class,
            segment.page,
        ),
        super::document_spec::reference_class_allowed(
            segment.class,
            segment.references,
        ),
{}

pub(super) proof fn local_reference_strictly_reduces_depth(
    segments: Seq<super::document_spec::Segment>,
    depths: Map<nat, nat>,
    max_depth: nat,
    segment_index: int,
    reference_index: int,
)
    requires
        super::document_spec::depth_witness(segments, depths, max_depth),
        0 <= segment_index < segments.len(),
        0 <= reference_index < segments[segment_index].references.len(),
        super::document_spec::contains_number(
            segments,
            segments[segment_index].references[reference_index].number,
        ),
    ensures
        depths[
            segments[segment_index].references[reference_index].number
        ] < depths[segments[segment_index].number],
        depths[segments[segment_index].number] <= max_depth,
{}

pub(super) proof fn lower_number_chains_are_finite(
    chain: Seq<nat>,
)
    requires
        chain.len() > 0,
        super::document_spec::lower_chain(chain),
    ensures
        chain.last() + chain.len() <= chain[0] + 1,
        chain.len() <= chain[0] + 1,
    decreases chain.len(),
{
    if chain.len() > 1 {
        let prefix = chain.drop_last();
        lower_number_chains_are_finite(prefix);
        assert(prefix.len() > 0);
        assert(prefix[0] == chain[0]);
        assert(prefix.last() == chain[chain.len() - 2]);
        assert(chain.last() + 1 <= prefix.last());
    } else {
        assert(chain.last() == chain[0]);
    }
}

pub(super) proof fn nonretention_is_only_at_last_use(
    uses: Seq<nat>,
    use_index: int,
    number: nat,
)
    requires super::document_spec::release_allowed(uses, use_index, number),
    ensures
        forall|later: int|
            use_index < later < uses.len() ==> uses[later] != number,
{}

pub(super) proof fn later_use_requires_producer_retention(
    uses: Seq<nat>,
    producer_number: nat,
    retained: bool,
)
    requires
        super::document_spec::producer_retained_for_later_use(
            uses,
            producer_number,
            retained,
        ),
        exists|i: int|
            0 <= i < uses.len() && uses[i] == producer_number,
    ensures retained,
{}

pub(super) proof fn contiguous_pages_have_no_missing_number(
    pages: Seq<nat>,
    index: int,
)
    requires
        super::document_spec::contiguous_pages(pages),
        0 <= index < pages.len(),
    ensures pages[index] == index + 1,
{}

pub(super) proof fn page_profile_contains_global_profile(
    declarations: Seq<super::document_spec::ProfileDeclaration>,
    declaration_index: int,
    profile: nat,
)
    requires
        super::document_spec::valid_profiles(declarations),
        1 <= declaration_index < declarations.len(),
        declarations[0].values.contains(profile),
    ensures
        declarations[declaration_index].page != 0,
        declarations[declaration_index].values.contains(profile),
{
    assert(declarations.len() != 0);
    assert(forall|i: int| 1 <= i < declarations.len() ==> {
        &&& declarations[i].page != 0
        &&& declarations[0].values.subset_of(declarations[i].values)
        &&& forall|j: int| 1 <= j < i ==>
            declarations[j].page != declarations[i].page
    });
    assert(declarations[declaration_index].page != 0);
    assert(declarations[0].values.subset_of(
        declarations[declaration_index].values,
    ));
}

pub(super) proof fn accepted_stripe_is_ordered(
    state: super::document_spec::PageState,
    row: nat,
    next: super::document_spec::PageState,
)
    requires
        super::document_spec::page_step(
            state,
            super::document_spec::PageEvent::Stripe { row },
        ) == Some(next),
    ensures
        state.information_seen,
        !state.ended,
        state.striped,
        next.last_stripe == Some(row),
        match state.last_stripe {
            Some(previous) =>
                previous < row && row - previous <= state.max_stripe_size,
            None => row <= state.max_stripe_size,
        },
{
    reveal(super::document_spec::page_step);
}

pub(super) proof fn accepted_information_starts_page(
    state: super::document_spec::PageState,
    striped: bool,
    height: Option<nat>,
    max_stripe_size: nat,
    next: super::document_spec::PageState,
)
    requires
        super::document_spec::page_step(
            state,
            super::document_spec::PageEvent::Information {
                striped,
                height,
                max_stripe_size,
            },
        ) == Some(next),
    ensures
        !state.information_seen,
        next.information_seen,
        !next.ended,
        next.striped == striped,
        next.fixed_height == height,
{
    reveal(super::document_spec::page_step);
}

pub(super) proof fn accepted_end_closes_page(
    state: super::document_spec::PageState,
    next: super::document_spec::PageState,
)
    requires
        super::document_spec::page_step(
            state,
            super::document_spec::PageEvent::End,
        ) == Some(next),
    ensures
        state.information_seen,
        !state.ended,
        next.ended,
        super::document_spec::final_height(next).is_some(),
{
    reveal(super::document_spec::page_step);
    reveal(super::document_spec::final_height);
}

pub(super) proof fn ended_page_rejects_more_segments(
    state: super::document_spec::PageState,
    event: super::document_spec::PageEvent,
)
    requires state.information_seen, state.ended,
    ensures super::document_spec::page_step(state, event).is_none(),
{
    reveal(super::document_spec::page_step);
    match event {
        super::document_spec::PageEvent::Information {
            striped: _,
            height: _,
            max_stripe_size: _,
        } => {},
        super::document_spec::PageEvent::Stripe { row: _ } => {},
        super::document_spec::PageEvent::End => {},
        super::document_spec::PageEvent::Content => {},
    }
}

pub(super) proof fn fixed_height_is_final(
    state: super::document_spec::PageState,
    height: nat,
)
    requires state.fixed_height == Some(height),
    ensures super::document_spec::final_height(state) == Some(height),
{
    reveal(super::document_spec::final_height);
}

pub(super) proof fn unknown_height_finishes_at_last_stripe(
    state: super::document_spec::PageState,
)
    requires
        state.ended,
        state.fixed_height.is_none(),
        state.last_stripe.is_some(),
    ensures
        super::document_spec::final_height(state)
            == Some(state.last_stripe.unwrap() + 1),
{
    reveal(super::document_spec::final_height);
}

pub(super) proof fn document_model_is_deterministic(
    envelope: super::document_spec::Envelope,
    file_header_bytes: nat,
    header_bytes: Seq<nat>,
    payload_bytes: Seq<nat>,
    state: super::document_spec::PageState,
    event: super::document_spec::PageEvent,
)
    ensures
        super::document_spec::envelope_bytes(
            envelope,
            file_header_bytes,
            header_bytes,
            payload_bytes,
        ) == super::document_spec::envelope_bytes(
            envelope,
            file_header_bytes,
            header_bytes,
            payload_bytes,
        ),
        super::document_spec::page_step(state, event)
            == super::document_spec::page_step(state, event),
{}

}

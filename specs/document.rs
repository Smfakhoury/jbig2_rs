use vstd::prelude::*;
use crate::file_header::file_header_spec;

verus! {

// Project quota/error policy, not a normative JBIG2 decoding equation.
pub(crate) open spec fn quota_result(
    actual: nat,
    limit: u64,
    kind: crate::LimitKind,
) -> Result<(), crate::DecodeError> {
    if actual > u64::MAX {
        Err(crate::DecodeError::Malformed(crate::MalformedError::IntegerOverflow))
    } else if actual > limit {
        Err(crate::DecodeError::LimitExceeded(crate::LimitExceeded {
            kind, limit, actual: actual as u64,
        }))
    } else {
        Ok(())
    }
}

pub(crate) open spec fn next_count_quota_result(
    count: usize,
    limit: u32,
    kind: crate::LimitKind,
) -> Result<(), crate::DecodeError> {
    if count == usize::MAX {
        Err(crate::DecodeError::Malformed(crate::MalformedError::IntegerOverflow))
    } else {
        quota_result(count as nat + 1, limit as u64, kind)
    }
}

pub closed spec fn decoder_limits(
    decoder: &crate::document::Decoder,
) -> crate::Limits {
    decoder.limits
}

pub open spec fn default_limits(limits: crate::Limits) -> bool {
    &&& limits.max_width == 1_000_000
    &&& limits.max_height == 1_000_000
    &&& limits.max_pixels_per_page == 1_000_000_000
    &&& limits.max_decoder_managed_memory_bytes == 1_073_741_824
    &&& limits.max_stream_bytes == 1_073_741_824
    &&& limits.max_pages == 10_000
    &&& limits.max_segments == 100_000
    &&& limits.max_symbols == 1_000_000
    &&& limits.max_references_per_segment == 4_096
    &&& limits.max_total_references == 1_000_000
    &&& limits.max_reference_graph_depth == 128
    &&& limits.max_nesting_depth == 128
    &&& limits.max_work_units == 2_000_000_000
    &&& limits.max_cumulative_decoded_bitmap_bytes == 4_294_967_296
    &&& limits.max_retained_state_bytes == 536_870_912
}

pub closed spec fn document_organization(
    document: &crate::document::DocumentInfo,
) -> crate::file_header::Organization {
    document.organization
}

pub closed spec fn document_declared_page_count(
    document: &crate::document::DocumentInfo,
) -> Option<u32> {
    document.declared_page_count
}

pub closed spec fn document_segment_count(
    document: &crate::document::DocumentInfo,
) -> u32 {
    document.segment_count
}

pub closed spec fn document_pages(
    document: &crate::document::DocumentInfo,
) -> Seq<crate::document::PageInfo> {
    document.pages@
}

pub open spec fn declared_count_matches_pages(
    document: &crate::document::DocumentInfo,
) -> bool {
    match document_declared_page_count(document) {
        Some(count) => document_pages(document).len() == count as nat,
        None => true,
    }
}

pub open spec fn standalone_metadata(
    input: Seq<u8>,
    document: &crate::document::DocumentInfo,
) -> bool {
    &&& file_header_spec::valid(input)
    &&& !file_header_spec::colour_regions(input)
    &&& document_organization(document) == file_header_spec::organization(input)
    &&& document_declared_page_count(document) == file_header_spec::page_count(input)
    &&& declared_count_matches_pages(document)
}

pub open spec fn embedded_metadata(
    document: &crate::document::DocumentInfo,
) -> bool {
    &&& document_organization(document) == crate::file_header::Organization::Sequential
    &&& document_declared_page_count(document) == None
}

pub(crate) open spec fn payload_window(
    header: &crate::segment_header::SegmentHeader,
    bytes: Seq<u8>,
    start: int,
    end: int,
    payload: Seq<u8>,
) -> bool {
    &&& 0 <= start <= end <= bytes.len()
    &&& payload == bytes.subrange(start, end)
    &&& match header.data_length {
        Some(length) => end - start == length,
        None => true,
    }
}

pub(crate) open spec fn sequential_offset(
    segments: Seq<crate::document::ParsedSegment<'_>>,
    prefix: nat,
) -> nat
    recommends prefix <= segments.len(),
    decreases prefix,
{
    if prefix == 0 {
        0
    } else {
        sequential_offset(segments, (prefix - 1) as nat)
            + segments[(prefix - 1) as int].header.byte_length as nat
            + segments[(prefix - 1) as int].payload@.len()
    }
}

#[verifier::opaque]
pub(crate) open spec fn sequential_container_fields(
    bytes: Seq<u8>,
    segments: Seq<crate::document::ParsedSegment<'_>>,
    ordered: bool,
) -> bool {
    &&& sequential_offset(segments, segments.len()) == bytes.len()
    &&& forall|index: int| 0 <= index < segments.len() ==> {
        let segment = #[trigger] segments[index];
        let start = sequential_offset(segments, index as nat) as int;
        let payload_start = start + segment.header.byte_length;
        let end = sequential_offset(segments, (index + 1) as nat) as int;
        &&& 0 <= start < payload_start <= end <= bytes.len()
        &&& crate::spec::segments::segment_header_fields(
            &segment.header, bytes, start, payload_start,
        )
        &&& payload_window(&segment.header, bytes, payload_start, end, segment.payload@)
        &&& (ordered && index > 0 ==> segments[index - 1].header.number < segment.header.number)
    }
}

pub(crate) open spec fn segment_headers(
    segments: Seq<crate::document::ParsedSegment<'_>>,
) -> Seq<crate::segment_header::SegmentHeader> {
    Seq::new(segments.len(), |index: int| segments[index].header)
}

pub(crate) open spec fn header_byte_offset(
    headers: Seq<crate::segment_header::SegmentHeader>,
    prefix: nat,
) -> nat
    recommends prefix <= headers.len(),
    decreases prefix,
{
    if prefix == 0 {
        0
    } else {
        header_byte_offset(headers, (prefix - 1) as nat)
            + headers[(prefix - 1) as int].byte_length as nat
    }
}

pub(crate) open spec fn payload_byte_offset(
    segments: Seq<crate::document::ParsedSegment<'_>>,
    prefix: nat,
) -> nat
    recommends prefix <= segments.len(),
    decreases prefix,
{
    if prefix == 0 {
        0
    } else {
        payload_byte_offset(segments, (prefix - 1) as nat)
            + segments[(prefix - 1) as int].payload@.len()
    }
}

#[verifier::opaque]
pub(crate) open spec fn random_access_segment_fields(
    bytes: Seq<u8>,
    segments: Seq<crate::document::ParsedSegment<'_>>,
    index: int,
) -> bool
    recommends 0 <= index < segments.len(),
{
    let headers = segment_headers(segments);
    let header_end = header_byte_offset(headers, headers.len());
    let segment = segments[index];
    let start = header_byte_offset(headers, index as nat) as int;
    let end = header_byte_offset(headers, (index + 1) as nat) as int;
    let payload_start = header_end + payload_byte_offset(segments, index as nat);
    let payload_end = header_end + payload_byte_offset(segments, (index + 1) as nat);
    &&& 0 <= start < end <= header_end <= payload_start <= payload_end <= bytes.len()
    &&& crate::spec::segments::segment_header_fields(
        &segment.header, bytes, start, end,
    )
    &&& payload_window(
        &segment.header, bytes, payload_start as int, payload_end as int, segment.payload@,
    )
    &&& (index > 0 ==> headers[index - 1].number < segment.header.number)
    &&& (index + 1 < segments.len() ==>
        segment.header.segment_type != crate::segment_header::SegmentType::EndOfFile)
}

#[verifier::opaque]
pub(crate) open spec fn random_access_container_fields(
    bytes: Seq<u8>,
    segments: Seq<crate::document::ParsedSegment<'_>>,
) -> bool {
    let headers = segment_headers(segments);
    let header_end = header_byte_offset(headers, headers.len());
    &&& segments.len() > 0
    &&& headers.last().segment_type == crate::segment_header::SegmentType::EndOfFile
    &&& header_end + payload_byte_offset(segments, segments.len()) == bytes.len()
    &&& forall|index: int| 0 <= index < segments.len() ==>
        #[trigger] random_access_segment_fields(bytes, segments, index)
}

pub(crate) open spec fn page_information_count(
    segments: Seq<crate::document::ParsedSegment<'_>>,
    prefix: nat,
) -> nat
    recommends prefix <= segments.len(),
    decreases prefix,
{
    if prefix == 0 {
        0
    } else {
        let header = segments[(prefix - 1) as int].header;
        page_information_count(segments, (prefix - 1) as nat)
            + if header.page_association != 0
                && header.segment_type == crate::segment_header::SegmentType::PageInformation {
                1nat
            } else {
                0nat
            }
    }
}

pub closed spec fn global_segment_count(
    globals: &crate::document::GlobalSegments,
) -> nat {
    globals.types@.len()
}

pub closed spec fn global_bytes(
    globals: &crate::document::GlobalSegments,
) -> Seq<u8> {
    globals.bytes@
}

pub(crate) open spec fn production_is_intermediate_region(
    segment_type: crate::segment_header::SegmentType,
) -> bool {
    segment_type == crate::segment_header::SegmentType::IntermediateTextRegion
        || segment_type == crate::segment_header::SegmentType::IntermediateHalftoneRegion
        || segment_type == crate::segment_header::SegmentType::IntermediateGenericRegion
        || segment_type
            == crate::segment_header::SegmentType::IntermediateGenericRefinementRegion
}

pub enum Envelope {
    Sequential,
    RandomAccess,
    Embedded,
}

pub enum SegmentClass {
    SymbolDictionary,
    IntermediateTextRegion,
    ImmediateTextRegion,
    PatternDictionary,
    IntermediateHalftoneRegion,
    ImmediateHalftoneRegion,
    IntermediateGenericRegion,
    ImmediateGenericRegion,
    IntermediateRefinementRegion,
    ImmediateRefinementRegion,
    PageInformation,
    EndOfPage,
    EndOfStripe,
    EndOfFile,
    Profiles,
    Tables,
    ColourPalette,
    Extension,
}

pub struct Reference {
    pub number: nat,
    pub class: SegmentClass,
}

pub struct Segment {
    pub number: nat,
    pub class: SegmentClass,
    pub page: nat,
    pub references: Seq<Reference>,
}

pub open spec fn sum(values: Seq<nat>) -> nat
    decreases values.len(),
{
    if values.len() == 0 {
        0
    } else {
        sum(values.drop_last()) + values.last()
    }
}

pub open spec fn reference_count(segments: Seq<Segment>) -> nat
    decreases segments.len(),
{
    if segments.len() == 0 {
        0
    } else {
        reference_count(segments.drop_last()) + segments.last().references.len()
    }
}

pub open spec fn work(input_bytes: nat, segments: Seq<Segment>) -> nat {
    input_bytes + segments.len() + reference_count(segments)
}

pub open spec fn envelope_bytes(
    envelope: Envelope,
    file_header_bytes: nat,
    segment_header_bytes: Seq<nat>,
    payload_bytes: Seq<nat>,
) -> nat {
    match envelope {
        Envelope::Sequential =>
            file_header_bytes + sum(segment_header_bytes) + sum(payload_bytes),
        Envelope::RandomAccess =>
            file_header_bytes + sum(segment_header_bytes) + sum(payload_bytes),
        Envelope::Embedded => sum(segment_header_bytes) + sum(payload_bytes),
    }
}

pub open spec fn unknown_marker_fits(input_len: nat, marker_offset: nat) -> bool {
    18 <= marker_offset && marker_offset + 6 <= input_len
}

pub open spec fn unknown_marker_valid(
    input_len: nat,
    marker_offset: nat,
    declared_height: nat,
    row_count: nat,
) -> bool {
    unknown_marker_fits(input_len, marker_offset) && row_count <= declared_height
}

pub open spec fn bounded_reads(chunks: Seq<nat>, cap: nat) -> bool
    decreases chunks.len(),
{
    if chunks.len() == 0 {
        true
    } else {
        bounded_reads(chunks.drop_last(), cap) && sum(chunks) <= cap
    }
}

pub open spec fn count_limits(
    segments: nat,
    pages: nat,
    max_segments: nat,
    max_pages: nat,
) -> bool {
    segments <= max_segments && pages <= max_pages
}

pub open spec fn unique_numbers(segments: Seq<Segment>) -> bool {
    forall|i: int, j: int|
        0 <= i < segments.len() && 0 <= j < segments.len() && i != j
            ==> segments[i].number != segments[j].number
}

pub open spec fn contains_number(segments: Seq<Segment>, number: nat) -> bool {
    exists|i: int| 0 <= i < segments.len() && segments[i].number == number
}

pub open spec fn references_resolve_lower(
    segments: Seq<Segment>,
    global_numbers: Set<nat>,
) -> bool {
    forall|i: int, r: int|
        0 <= i < segments.len() && 0 <= r < segments[i].references.len()
            ==> {
                let reference = #[trigger] segments[i].references[r];
                &&& reference.number < segments[i].number
                &&& (contains_number(segments, reference.number)
                    || global_numbers.contains(reference.number))
            }
}

pub open spec fn page_association_allowed(class: SegmentClass, page: nat) -> bool {
    match class {
        SegmentClass::IntermediateTextRegion
        | SegmentClass::ImmediateTextRegion
        | SegmentClass::IntermediateHalftoneRegion
        | SegmentClass::ImmediateHalftoneRegion
        | SegmentClass::IntermediateGenericRegion
        | SegmentClass::ImmediateGenericRegion
        | SegmentClass::IntermediateRefinementRegion
        | SegmentClass::ImmediateRefinementRegion
        | SegmentClass::PageInformation
        | SegmentClass::EndOfPage
        | SegmentClass::EndOfStripe => page != 0,
        SegmentClass::EndOfFile => page == 0,
        _ => true,
    }
}

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

pub open spec fn segment_policy_allowed(segment: Segment) -> bool {
    page_association_allowed(segment.class, segment.page)
        && reference_class_allowed(segment.class, segment.references)
}

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

pub open spec fn is_last_use(
    uses: Seq<nat>,
    use_index: int,
    number: nat,
) -> bool {
    &&& 0 <= use_index < uses.len()
    &&& uses[use_index] == number
    &&& forall|later: int| use_index < later < uses.len() ==> uses[later] != number
}

pub open spec fn release_allowed(
    uses: Seq<nat>,
    use_index: int,
    number: nat,
) -> bool {
    is_last_use(uses, use_index, number)
}

pub open spec fn producer_retained_for_later_use(
    uses: Seq<nat>,
    producer_number: nat,
    retained: bool,
) -> bool {
    (exists|i: int| 0 <= i < uses.len() && uses[i] == producer_number)
        ==> retained
}

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

pub open spec fn contiguous_pages(pages: Seq<nat>) -> bool {
    forall|i: int| 0 <= i < pages.len() ==> pages[i] == i + 1
}

pub struct ProfileDeclaration {
    pub segment_index: nat,
    pub page: nat,
    pub values: Set<nat>,
}

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

// Preserve the logical owner and private-field access of the model bridges.
include!("../proofs/model_support/document.rs");

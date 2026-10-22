use vstd::prelude::*;

verus! {

pub closed spec fn page_number(page: &crate::decode::Page) -> u32 {
    page.number
}

pub closed spec fn page_bitmap(page: &crate::decode::Page) -> &crate::Bitmap {
    &page.bitmap
}

pub closed spec fn document_info(
    document: &crate::decode::DecodedDocument,
) -> &crate::DocumentInfo {
    &document.info
}

pub closed spec fn document_pages(
    document: &crate::decode::DecodedDocument,
) -> Seq<crate::decode::Page> {
    document.pages@
}

pub(super) open spec fn collector_pages(
    collector: &crate::decode::PageCollector,
) -> Seq<crate::decode::Page> {
    collector.pages@
}

pub(super) open spec fn collector_bitmap_bytes(
    collector: &crate::decode::PageCollector,
) -> u64 {
    collector.bitmap_bytes
}

pub(super) open spec fn collector_memory_limit(
    collector: &crate::decode::PageCollector,
) -> u64 {
    collector.memory_limit
}

pub(super) open spec fn collector_decoded_limit(
    collector: &crate::decode::PageCollector,
) -> u64 {
    collector.decoded_limit
}

pub(super) open spec fn retained_bytes(budget: &crate::decode::RetainedBudget) -> u64 {
    budget.bytes
}

pub(super) open spec fn work_used(work: &crate::decode::DecodeWork) -> u64 {
    work.used
}

pub(super) open spec fn runtime_last_stripe_end(
    page: &crate::decode::RuntimePage,
) -> Option<u32> {
    page.last_stripe_end
}

pub(super) open spec fn runtime_open_stripe_max_row(
    page: &crate::decode::RuntimePage,
) -> Option<u32> {
    page.open_stripe_max_row
}

pub(super) open spec fn runtime_pages_valid(
    pages: &crate::decode::RuntimePages,
) -> bool {
    forall|index: int| 0 <= index < pages@.len() ==>
        crate::spec::bitmap::storage_valid(&pages@[index].1.bitmap)
}

pub(super) open spec fn symbol_store_valid(
    symbols: &crate::decode::SymbolStore,
) -> bool {
    forall|index: int| 0 <= index < symbols@.len()
        ==> crate::halftone::halftone_spec::bitmaps_valid(
            #[trigger] symbols@[index].1.exported@
        )
}

pub(super) open spec fn bitmap_refs_valid(
    bitmaps: Seq<&crate::bitmap::Bitmap>,
) -> bool {
    forall|index: int| 0 <= index < bitmaps.len()
        ==> crate::spec::bitmap::storage_valid(#[trigger] bitmaps[index])
}

pub(super) open spec fn collected_symbols_valid(
    symbols: &crate::decode::CollectedSymbols,
) -> bool {
    match symbols {
        crate::decode::CollectedSymbols::Contiguous(bitmaps) =>
            crate::halftone::halftone_spec::bitmaps_valid(bitmaps@),
        crate::decode::CollectedSymbols::Borrowed(bitmaps) =>
            bitmap_refs_valid(bitmaps@),
    }
}

pub(super) open spec fn pattern_store_valid(
    patterns: &crate::decode::PatternStore,
) -> bool {
    forall|index: int| 0 <= index < patterns@.len()
        ==> crate::halftone::halftone_spec::bitmaps_valid(
            #[trigger] patterns@[index].1@
        )
}

pub(super) open spec fn region_buffers_valid(
    buffers: &crate::decode::RegionBuffers,
) -> bool {
    forall|index: int| 0 <= index < buffers@.len()
        ==> crate::spec::bitmap::storage_valid(
            #[trigger] &buffers@[index].1.bitmap
        )
}

pub(super) open spec fn initial_resources_valid(
    resources: &crate::decode::InitialResources,
) -> bool {
    pattern_store_valid(&resources.patterns)
        && symbol_store_valid(&resources.symbols)
}

pub open spec fn checked_sum_fits(left: u64, right: u64) -> bool {
    left as nat + right as nat <= u64::MAX as nat
}

pub open spec fn charge_succeeds(used: u64, units: u64, limit: u64) -> bool {
    checked_sum_fits(used, units) && used as nat + units as nat <= limit as nat
}

pub open spec fn release_succeeds(retained: u64, bytes: u64) -> bool {
    bytes <= retained
}

pub(super) open spec fn generic_template_code(
    template: crate::generic_region::GenericTemplate,
) -> nat {
    match template {
        crate::generic_region::GenericTemplate::Template0 => 0,
        crate::generic_region::GenericTemplate::Template1 => 1,
        crate::generic_region::GenericTemplate::Template2 => 2,
        crate::generic_region::GenericTemplate::Template3 => 3,
    }
}

pub open spec fn allocation_bytes<T>(count: usize) -> nat {
    count as nat * core::mem::size_of::<T>() as nat
}

pub open spec fn bitmap_byte_count(bitmap: &crate::Bitmap) -> nat {
    crate::views::bitmap_data(bitmap).len()
}

pub open spec fn region_intersects_page(
    region_width: u32,
    region_height: u32,
    region_x: u32,
    region_y: u32,
    page_width: u32,
    page_height: u32,
) -> bool {
    region_width != 0
        && region_height != 0
        && region_x < page_width
        && region_y < page_height
}

pub(super) open spec fn dispatch_class(segment_type: crate::segment_header::SegmentType) -> nat {
    match segment_type {
        crate::segment_header::SegmentType::SymbolDictionary => 0,
        crate::segment_header::SegmentType::IntermediateTextRegion
        | crate::segment_header::SegmentType::ImmediateTextRegion
        | crate::segment_header::SegmentType::ImmediateLosslessTextRegion => 1,
        crate::segment_header::SegmentType::PatternDictionary => 2,
        crate::segment_header::SegmentType::IntermediateHalftoneRegion
        | crate::segment_header::SegmentType::ImmediateHalftoneRegion
        | crate::segment_header::SegmentType::ImmediateLosslessHalftoneRegion => 3,
        crate::segment_header::SegmentType::IntermediateGenericRegion
        | crate::segment_header::SegmentType::ImmediateGenericRegion
        | crate::segment_header::SegmentType::ImmediateLosslessGenericRegion => 4,
        crate::segment_header::SegmentType::IntermediateGenericRefinementRegion
        | crate::segment_header::SegmentType::ImmediateGenericRefinementRegion
        | crate::segment_header::SegmentType::ImmediateLosslessGenericRefinementRegion => 5,
        crate::segment_header::SegmentType::PageInformation => 6,
        crate::segment_header::SegmentType::EndOfPage => 7,
        crate::segment_header::SegmentType::EndOfStripe => 8,
        crate::segment_header::SegmentType::EndOfFile => 9,
        crate::segment_header::SegmentType::Profiles => 10,
        crate::segment_header::SegmentType::Tables => 11,
        crate::segment_header::SegmentType::ColourPalette => 12,
        crate::segment_header::SegmentType::Extension => 13,
    }
}

}

// Preserve the logical owner and private-field access of the model bridges.
include!("../proofs/model_support/decode.rs");

use vstd::prelude::*;

verus! {

pub(super) proof fn prove_document_policy_bundle()
{
    super::document_properties::standalone_envelopes_have_equal_accounting(
        0,
        Seq::<nat>::empty(),
        Seq::<nat>::empty(),
    );
    super::document_properties::embedded_omits_only_file_header(
        0,
        Seq::<nat>::empty(),
        Seq::<nat>::empty(),
    );
    super::document_properties::reader_cap_is_inclusive(0, 0);
    super::document_properties::unknown_marker_consumption_is_bounded(24, 18);
}

pub(super) proof fn intermediate_region_runtime_fact(
    segment_type: crate::segment_header::SegmentType,
)
    ensures
        super::document_spec::production_is_intermediate_region(segment_type)
            == (
                segment_type == crate::segment_header::SegmentType::IntermediateTextRegion
                || segment_type
                    == crate::segment_header::SegmentType::IntermediateHalftoneRegion
                || segment_type
                    == crate::segment_header::SegmentType::IntermediateGenericRegion
                || segment_type
                    == crate::segment_header::SegmentType::IntermediateGenericRefinementRegion
            ),
{
    match segment_type {
        crate::segment_header::SegmentType::SymbolDictionary => {},
        crate::segment_header::SegmentType::IntermediateTextRegion => {},
        crate::segment_header::SegmentType::ImmediateTextRegion => {},
        crate::segment_header::SegmentType::ImmediateLosslessTextRegion => {},
        crate::segment_header::SegmentType::PatternDictionary => {},
        crate::segment_header::SegmentType::IntermediateHalftoneRegion => {},
        crate::segment_header::SegmentType::ImmediateHalftoneRegion => {},
        crate::segment_header::SegmentType::ImmediateLosslessHalftoneRegion => {},
        crate::segment_header::SegmentType::IntermediateGenericRegion => {},
        crate::segment_header::SegmentType::ImmediateGenericRegion => {},
        crate::segment_header::SegmentType::ImmediateLosslessGenericRegion => {},
        crate::segment_header::SegmentType::IntermediateGenericRefinementRegion => {},
        crate::segment_header::SegmentType::ImmediateGenericRefinementRegion => {},
        crate::segment_header::SegmentType::ImmediateLosslessGenericRefinementRegion => {},
        crate::segment_header::SegmentType::PageInformation => {},
        crate::segment_header::SegmentType::EndOfPage => {},
        crate::segment_header::SegmentType::EndOfStripe => {},
        crate::segment_header::SegmentType::EndOfFile => {},
        crate::segment_header::SegmentType::Profiles => {},
        crate::segment_header::SegmentType::Tables => {},
        crate::segment_header::SegmentType::ColourPalette => {},
        crate::segment_header::SegmentType::Extension => {},
    }
}

}

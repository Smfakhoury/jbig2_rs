// Included in document_spec to preserve closed-model and field visibility.
verus! {

pub(super) proof fn decoder_fields(decoder: &crate::document::Decoder)
    ensures decoder_limits(decoder) == decoder.limits,
{}

pub(super) proof fn document_fields(document: &crate::document::DocumentInfo)
    ensures
        document_organization(document) == document.organization,
        document_declared_page_count(document) == document.declared_page_count,
        document_segment_count(document) == document.segment_count,
        document_pages(document) == document.pages@,
{}

pub(super) proof fn global_fields(globals: &crate::document::GlobalSegments)
    ensures
        global_segment_count(globals) == globals.types@.len(),
        global_bytes(globals) == globals.bytes@,
{}

}

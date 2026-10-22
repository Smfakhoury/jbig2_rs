// Included in decode_spec to preserve closed-model and field visibility.
verus! {

pub(super) proof fn page_fields(page: &crate::decode::Page)
    ensures
        page_number(page) == page.number,
        page_bitmap(page) == &page.bitmap,
{}

pub(super) proof fn document_fields(document: &crate::decode::DecodedDocument)
    ensures
        document_info(document) == &document.info,
        document_pages(document) == document.pages@,
{}

}

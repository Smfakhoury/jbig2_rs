use vstd::prelude::*;

macro_rules! production_decoded_page_fields {
    ($page:ident) => {
        proof! {
            crate::views::decoded_page_fields($page);
        }
    };
}

macro_rules! production_decode_page_fields {
    ($page:expr) => {
        proof! {
            decode_spec::page_fields($page);
        }
    };
}

macro_rules! production_decode_document_fields {
    ($document:expr) => {
        proof! {
            decode_spec::document_fields($document);
        }
    };
}

macro_rules! production_collector_fields {
    ($collector:expr) => {
        proof! {
            decode_shared::collector_fields($collector);
        }
    };
}

macro_rules! production_retained_fields {
    ($budget:expr) => {
        proof! {
            decode_shared::retained_fields($budget);
        }
    };
}

macro_rules! production_work_fields {
    ($work:expr) => {
        proof! {
            decode_shared::work_fields($work);
        }
    };
}

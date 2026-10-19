use vstd::prelude::*;

macro_rules! production_page_info_fields {
    ($page:ident) => {
        proof! {
            crate::views::page_info_fields($page);
        }
    };
}

macro_rules! production_decoder_fields {
    ($decoder:expr) => {
        proof! {
            document_spec::decoder_fields($decoder);
        }
    };
}

macro_rules! production_document_fields {
    ($document:expr) => {
        proof! {
            document_spec::document_fields($document);
        }
    };
}

macro_rules! production_global_fields {
    ($globals:expr) => {
        proof! {
            document_spec::global_fields($globals);
        }
    };
}

macro_rules! production_document_policy_bundle {
    () => {
        proof! {
            document_shared::prove_document_policy_bundle();
        }
    };
}

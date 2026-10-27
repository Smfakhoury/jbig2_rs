// Source-only author navigation; named purposes are not normative approval.
// Guide, official acquisition and source identity: ../../specs/README.md
// All requirement links and inherited locators: ../../validation/traceability/requirements.json
// REQUIREMENT CONTEXT is not a direct normative equation, including for proof/policy helpers.

use vstd::prelude::*;

macro_rules! production_file_header_organization {
    ($header:ident) => {
        proof! {
            crate::views::file_header_fields($header);
        }
    };
}

macro_rules! production_file_header_page_count {
    ($header:ident) => {
        proof! {
            crate::views::file_header_fields($header);
        }
    };
}

macro_rules! production_file_header_extended_templates {
    ($header:ident) => {
        proof! {
            crate::views::file_header_fields($header);
        }
    };
}

macro_rules! production_file_header_colour_regions {
    ($header:ident) => {
        proof! {
            crate::views::file_header_fields($header);
        }
    };
}

macro_rules! production_file_header_byte_length {
    ($header:ident) => {
        proof! {
            crate::views::file_header_fields($header);
        }
    };
}

// Model navigation: macro::production_file_header_result [proof-support]
// Purpose: Private file-header fields exposed to the production result contract.
// Audit: header-result
// REQUIREMENT CONTEXT: T88-HEADER-SIGNATURE | ITU-T T.88 (08/2018) | D.4.1
// Fidelity: unresolved; no independent normative approval.
macro_rules! production_file_header_result {
    ($header:ident) => {
        proof! {
            crate::views::file_header_fields($header);
        }
    };
}

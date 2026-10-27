// Source-only author navigation; named purposes are not normative approval.
// Guide, official acquisition and source identity: ../../specs/README.md
// All requirement links and inherited locators: ../../validation/traceability/requirements.json
// REQUIREMENT CONTEXT is not a direct normative equation, including for proof/policy helpers.

use vstd::prelude::*;

// Model navigation: macro::production_page_information_count_step [proof-support]
// Purpose: Private fields, page counts and sequential/random window completion bridges.
// Audit: page-information-count-step
// REQUIREMENT CONTEXT: T88-DOCUMENT-COUNT-AGREEMENT | ITU-T T.88 (08/2018) | D.4.3
// Fidelity: unresolved; no independent normative approval.
macro_rules! production_page_information_count_step {
    ($segments:expr, $index:expr) => {
        proof! {
            reveal_with_fuel(document_spec::page_information_count, 1);
            assert(
                document_spec::page_information_count(($segments)@, ($index) as nat + 1)
                    == document_spec::page_information_count(($segments)@, ($index) as nat)
                        + if ($segments)@[($index) as int].header.page_association != 0
                            && ($segments)@[($index) as int].header.segment_type
                                == SegmentType::PageInformation {
                            1nat
                        } else {
                            0nat
                        }
            );
        }
    };
}

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

// Model navigation: macro::production_document_fields [proof-support]
// Purpose: Private fields, page counts and sequential/random window completion bridges.
// Audit: document-fields
// REQUIREMENT CONTEXT: T88-DOCUMENT-COUNT-AGREEMENT | ITU-T T.88 (08/2018) | D.4.3
// Fidelity: unresolved; no independent normative approval.
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

// Model navigation: macro::production_known_segment_data_fields [proof-support]
// Purpose: Private fields, page counts and sequential/random window completion bridges.
// Audit: known-segment-data-proof
// REQUIREMENT CONTEXT: T88-PAGE-KNOWN-DIMENSIONS | ITU-T T.88 (08/2018) | 7.4.8.1-7.4.8.2; 8.2 step 2
// Fidelity: unresolved; no independent normative approval.
macro_rules! production_known_segment_data_fields {
    () => {
        proof! {
            reveal(document_spec::known_segment_data_fields);
        }
    };
}

// Model navigation: macro::production_sequential_window_append [proof-support]
// Purpose: Private fields, page counts and sequential/random window completion bridges.
// Audit: document-window-append-proof
// REQUIREMENT CONTEXT: T88-SEQUENTIAL-SEGMENT-LAYOUT | ITU-T T.88 (08/2018) | D.1
// Fidelity: unresolved; no independent normative approval.
macro_rules! production_sequential_window_append {
    ($input:ident, $segments:ident, $old_segments:ident, $windows:ident,
        $header_start:ident, $payload_start:ident, $reader:ident, $ordered:ident) => {
        proof! {
            let segment = $segments@.last();
            document_spec::segment_metadata_append($old_segments, segment);
            document_spec::sequential_window_append(
                $input@, $old_segments, $windows, $header_start, segment,
                $payload_start, $reader.position as int, $ordered,
            );
            $windows = $windows.push((
                $header_start, $payload_start, $reader.position as int,
            ));
        }
    };
}

// Model navigation: macro::production_sequential_complete [proof-support]
// Purpose: Private fields, page counts and sequential/random window completion bridges.
// Audit: document-sequential-complete-proof
// REQUIREMENT CONTEXT: T88-SEQUENTIAL-SEGMENT-LAYOUT | ITU-T T.88 (08/2018) | D.1
// Fidelity: unresolved; no independent normative approval.
macro_rules! production_sequential_complete {
    ($input:ident, $segments:ident, $windows:ident, $reader:ident, $ordered:ident) => {
        proof! {
            assert($reader.position == $input@.len());
            document_spec::sequential_complete_fields($input@, $segments@, $windows, $ordered);
        }
    };
}

// Model navigation: macro::production_random_header_append [proof-support]
// Purpose: Private fields, page counts and sequential/random window completion bridges.
// Audit: document-random-header-proof
// REQUIREMENT CONTEXT: T88-RANDOM-HEADER-PAYLOAD-LAYOUT | ITU-T T.88 (08/2018) | D.2
// Fidelity: unresolved; no independent normative approval.
macro_rules! production_random_header_append {
    ($input:ident, $headers:ident, $windows:ident, $before:ident, $reader:ident) => {
        proof! {
            document_spec::random_header_window_append(
                $input@, $before.1, $windows, $before.0,
                $headers@.last(), $reader.position as int,
            );
            $windows = $windows.push(($before.0, $reader.position as int));
        }
    };
}

// Model navigation: macro::production_random_payload [proof-support]
// Purpose: Private fields, page counts and sequential/random window completion bridges.
// Audit: document-random-payload-proof
// REQUIREMENT CONTEXT: T88-RANDOM-HEADER-PAYLOAD-LAYOUT | ITU-T T.88 (08/2018) | D.2
// Fidelity: unresolved; no independent normative approval.
macro_rules! production_random_payload {
    ($headers:ident, $header_end:ident, $segments:ident,
        $windows:ident, $before:ident, $reader:ident) => {
        proof! {
            document_spec::segment_metadata_append($before.1, $segments@.last());
            document_spec::random_payload_window_append(
                $reader.bytes@, $headers, $header_end, $before.1, $windows, $before.0,
                $segments@.last(), $reader.position as int,
            );
            $windows = $windows.push(($before.0, $reader.position as int));
        }
    };
}

// Model navigation: macro::production_random_access_complete [proof-support]
// Purpose: Private fields, page counts and sequential/random window completion bridges.
// Audit: document-random-complete-proof
// REQUIREMENT CONTEXT: T88-RANDOM-HEADER-PAYLOAD-LAYOUT | ITU-T T.88 (08/2018) | D.2
// Fidelity: unresolved; no independent normative approval.
macro_rules! production_random_access_complete {
    ($input:ident, $headers:ident, $header_windows:ident, $header_end:ident,
        $segments:ident, $payload_windows:ident) => {
        proof! {
            document_spec::random_access_complete_fields(
                $input@, $headers, $header_windows, $header_end, $segments@, $payload_windows,
            );
        }
    };
}

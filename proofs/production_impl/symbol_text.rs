// Source-only author navigation; named purposes are not normative approval.
// Guide, official acquisition and source identity: ../../specs/README.md
// All requirement links and inherited locators: ../../validation/traceability/requirements.json
// REQUIREMENT CONTEXT is not a direct normative equation, including for proof/policy helpers.

use vstd::prelude::*;

macro_rules! production_symbol_tracker_fields {
    ($tracker:expr) => {
        proof! {
            crate::symbol_text::symbol_text_shared::tracker_fields($tracker);
        }
    };
}

macro_rules! production_symbol_clipped_geometry {
    ($destination:expr, $source:expr, $placement:expr, $width:expr, $height:expr) => {
        proof! {
            crate::views::bitmap_fields($destination);
            crate::views::bitmap_fields($source);
            reveal(crate::symbol_text::symbol_text_spec::placement_overlap);
            reveal(crate::symbol_text::symbol_text_spec::clipped_start);
            reveal(crate::symbol_text::symbol_text_spec::clipped_end);
            assert($placement.x != i64::MIN && $placement.y != i64::MIN);
            assert(
                crate::symbol_text::symbol_text_spec::placement_overlap(
                    $destination, $source, $placement,
                ).2 == $width as nat
            );
            assert(
                crate::symbol_text::symbol_text_spec::placement_overlap(
                    $destination, $source, $placement,
                ).3 == $height as nat
            );
        }
    };
}

// Model navigation: macro::production_symbol_current_placement_witness [proof-support]
// Purpose: Ghost placement witness from the native current strip coordinates.
// Audit: text-current-placement-witness
// REQUIREMENT CONTEXT: T88-TEXT-INSTANCE-COORDINATES | ITU-T T.88 (08/2018) | 6.4.5 step 3c; 6.4.7-6.4.9
// Fidelity: unresolved; no independent normative approval.
macro_rules! production_symbol_current_placement_witness {
    ($state:expr, $within:expr, $width:expr, $height:expr, $x:expr, $y:expr) => {
        proof! {
            let placement = crate::symbol_text::Placement { x: $x, y: $y };
            assert(crate::symbol_text::symbol_text_spec::placement_for_current(
                old($state), $within, $width, $height, placement,
            ));
            assert(exists|candidate: crate::symbol_text::Placement|
                crate::symbol_text::symbol_text_spec::placement_for_current(
                    old($state), $within, $width, $height, candidate,
                ));
        }
    };
}

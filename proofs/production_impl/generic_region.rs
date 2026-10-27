// Source-only author navigation; named purposes are not normative approval.
// Guide, official acquisition and source identity: ../../specs/README.md
// All requirement links and inherited locators: ../../validation/traceability/requirements.json
// REQUIREMENT CONTEXT is not a direct normative equation, including for proof/policy helpers.

use vstd::prelude::*;

// Model navigation: macro::production_generic_copied_row_pixels [proof-support]
// Purpose: Bitmap row-copy and generic-template implementation proof bridges.
// Audit: copied-row-pixels
// REQUIREMENT CONTEXT: T88-GENERIC-TYPICAL-ROW | ITU-T T.88 (08/2018) | 6.2.5.5; 6.2.5.7 step 3c
// Fidelity: unresolved; no independent normative approval.
macro_rules! production_generic_copied_row_pixels {
    ($bitmap:ident, $source:ident, $destination:ident) => {
        proof! {
            crate::views::bitmap_fields(old($bitmap));
            crate::views::bitmap_fields($bitmap);
            crate::bitmap::bitmap_shared::row_copy_pixels(
                old($bitmap), $bitmap, $source, $destination,
            );
        }
    };
}

// Model navigation: macro::production_generic_template_facts [proof-support]
// Purpose: Bitmap row-copy and generic-template implementation proof bridges.
// Audit: template-facts
// REQUIREMENT CONTEXT: T88-GENERIC-CONTEXT-SIZE | ITU-T T.88 (08/2018) | 6.2.5.3; Figures 3-6
// Fidelity: unresolved; no independent normative approval.
macro_rules! production_generic_template_facts {
    ($template:expr) => {
        proof! {
            generic_region_shared::generic_template_runtime_facts($template);
        }
    };
}

macro_rules! production_refinement_template_facts {
    ($template:expr) => {
        proof! {
            generic_region_shared::refinement_template_runtime_facts($template);
        }
    };
}

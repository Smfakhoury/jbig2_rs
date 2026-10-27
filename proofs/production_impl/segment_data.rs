// Source-only author navigation; named purposes are not normative approval.
// Guide, official acquisition and source identity: ../../specs/README.md
// All requirement links and inherited locators: ../../validation/traceability/requirements.json
// REQUIREMENT CONTEXT is not a direct normative equation, including for proof/policy helpers.

use vstd::prelude::*;

macro_rules! production_dimension_product {
    ($width:ident, $height:ident) => {
        proof! {
            segment_data_shared::dimension_product_runtime_fact($width, $height);
        }
    };
}

// Model navigation: macro::production_profiles_fields [proof-support]
// Purpose: Parsed profile count, extent and ordered identifier proof bridge.
// Audit: profile-field-proof
// REQUIREMENT CONTEXT: T88-SEQUENTIAL-SEGMENT-LAYOUT | ITU-T T.88 (08/2018) | D.1
// Fidelity: unresolved; no independent normative approval.
macro_rules! production_profiles_fields {
    ($payload:ident, $profiles:ident, $count:ident) => {
        proof! {
            assert($count == crate::spec::segments::be_u32_at($payload@, 0));
            assert($profiles@.len() == $count);
            assert forall|index: int| 0 <= index < $profiles@.len() implies
                #[trigger] $profiles@[index]
                    == crate::spec::segments::be_u32_at($payload@, 4 + index * 4) by {
                assert($profiles@[0 + index]
                    == crate::spec::segments::be_u32_at($payload@, 4 + index * 4));
            }
        }
    };
}

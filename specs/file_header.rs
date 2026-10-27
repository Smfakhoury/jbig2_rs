// Source-only author navigation; named purposes are not normative approval.
// Guide, official acquisition and source identity: README.md
// All requirement links and inherited locators: ../validation/traceability/requirements.json
// REQUIREMENT CONTEXT is not a direct normative equation, including for proof/policy helpers.

use vstd::prelude::*;

verus! {

// Model navigation: spec::has_signature [candidate-standard-facing-model]
// Purpose: Signature, reserved flags and minimum encoded-extent candidates.
// Audit: header-signature
// REQUIREMENT CONTEXT: T88-HEADER-SIGNATURE | ITU-T T.88 (08/2018) | D.4.1
// Fidelity: unresolved; no independent normative approval.
pub open spec fn has_signature(input: Seq<u8>) -> bool {
    input.len() >= 8
        && input[0] == 0x97
        && input[1] == 0x4A
        && input[2] == 0x42
        && input[3] == 0x32
        && input[4] == 0x0D
        && input[5] == 0x0A
        && input[6] == 0x1A
        && input[7] == 0x0A
}

// Model navigation: spec::valid [candidate-standard-facing-model]
// Purpose: Signature, reserved flags and minimum encoded-extent candidates.
// Audit: header-valid
// REQUIREMENT CONTEXT: T88-HEADER-SIGNATURE | ITU-T T.88 (08/2018) | D.4.1
// Fidelity: unresolved; no independent normative approval.
pub open spec fn valid(input: Seq<u8>) -> bool {
    input.len() >= 9
        && has_signature(input)
        && input[8] & 0xF0 == 0
        && (input[8] & 0x02 != 0 || input.len() >= 13)
}

// Model navigation: spec::organization [project-result-model]
// Purpose: Flag/count extraction and exact project file-header consumption results.
// Audit: header-organization
// REQUIREMENT CONTEXT: T88-HEADER-ORGANIZATION | ITU-T T.88 (08/2018) | D.4.2 bit 0
// Fidelity: unresolved; no independent normative approval.
pub open spec fn organization(input: Seq<u8>) -> crate::file_header::Organization
    recommends valid(input),
{
    if input[8] & 0x01 == 0 {
        crate::file_header::Organization::RandomAccess
    } else {
        crate::file_header::Organization::Sequential
    }
}

// Model navigation: spec::page_count [project-result-model]
// Purpose: Flag/count extraction and exact project file-header consumption results.
// Audit: header-count
// REQUIREMENT CONTEXT: T88-HEADER-COUNT-PRESENCE | ITU-T T.88 (08/2018) | D.4.2 bit 1; D.4.3
// Fidelity: unresolved; no independent normative approval.
pub open spec fn page_count(input: Seq<u8>) -> Option<u32>
    recommends valid(input),
{
    if input[8] & 0x02 != 0 {
        None
    } else {
        Some(
            ((input[9] as u32) << 24)
                | ((input[10] as u32) << 16)
                | ((input[11] as u32) << 8)
                | input[12] as u32,
        )
    }
}

// Model navigation: spec::extended_templates [project-result-model]
// Purpose: Flag/count extraction and exact project file-header consumption results.
// Audit: header-extended
// REQUIREMENT CONTEXT: T88-HEADER-EXTENDED-DECLARATION | ITU-T T.88 (08/2018) | D.4.2 bit 2
// Fidelity: unresolved; no independent normative approval.
pub open spec fn extended_templates(input: Seq<u8>) -> bool
    recommends valid(input),
{
    input[8] & 0x04 != 0
}

// Model navigation: spec::colour_regions [project-result-model]
// Purpose: Flag/count extraction and exact project file-header consumption results.
// Audit: header-colour
// REQUIREMENT CONTEXT: T88-HEADER-COLOUR-DECLARATION | ITU-T T.88 (08/2018) | D.4.2 bit 3
// Fidelity: unresolved; no independent normative approval.
pub open spec fn colour_regions(input: Seq<u8>) -> bool
    recommends valid(input),
{
    input[8] & 0x08 != 0
}

// Model navigation: spec::byte_length [project-result-model]
// Purpose: Flag/count extraction and exact project file-header consumption results.
// Audit: header-length
// REQUIREMENT CONTEXT: T88-HEADER-COUNT-PRESENCE | ITU-T T.88 (08/2018) | D.4.2 bit 1; D.4.3
// Fidelity: unresolved; no independent normative approval.
pub open spec fn byte_length(input: Seq<u8>) -> usize
    recommends valid(input),
{
    if input[8] & 0x02 != 0 {
        9
    } else {
        13
    }
}

}

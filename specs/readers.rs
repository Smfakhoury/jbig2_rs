// Source-only author navigation; named purposes are not normative approval.
// Guide, official acquisition and source identity: README.md
// All requirement links and inherited locators: ../validation/traceability/requirements.json
// REQUIREMENT CONTEXT is not a direct normative equation, including for proof/policy helpers.

use vstd::prelude::*;

verus! {

// Model navigation: spec::bit_at [project-result-model]
// Purpose: MSB-first bit extraction and ordered numeric accumulation.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn bit_at(bytes: Seq<u8>, position: nat) -> nat
    recommends
        position < bytes.len() * 8,
{
    ((bytes[(position / 8) as int] >> ((7 - position % 8) as u8)) & 1) as nat
}

// Model navigation: spec::bits_value [project-result-model]
// Purpose: MSB-first bit extraction and ordered numeric accumulation.
// Audit: bit-value
// REQUIREMENT CONTEXT: T6-MODE-TABLE-DISPATCH | ITU-T T.6 (1988) | 2.2.3; 2.2.4; Table 1/T.6
// Fidelity: unresolved; no independent normative approval.
pub open spec fn bits_value(bytes: Seq<u8>, start: nat, count: nat) -> nat
    decreases count,
{
    if count == 0 {
        0
    } else {
        bits_value(bytes, start, (count - 1) as nat) * 2
            + bit_at(bytes, (start + count - 1) as nat)
    }
}

// Model navigation: spec::eof_error [project-policy]
// Purpose: Project UnexpectedEof diagnostic payload.
// Audit: reader-eof-error
// REQUIREMENT CONTEXT: T88-HUFFMAN-NORMAL-VALUE | ITU-T T.88 (08/2018) | B.4 steps 2 and 5
// Fidelity: unresolved; no independent normative approval.
pub open spec fn eof_error(offset: usize, needed: usize, remaining: usize) -> crate::MalformedError {
    crate::MalformedError::UnexpectedEof {
        offset: offset as u64, needed: needed as u64, remaining: remaining as u64,
    }
}

// Model navigation: spec::bit_read_error [project-policy]
// Purpose: Project UnexpectedEof diagnostic payload.
// Audit: reader-bit-error
// REQUIREMENT CONTEXT: T88-HUFFMAN-NORMAL-VALUE | ITU-T T.88 (08/2018) | B.4 steps 2 and 5
// Fidelity: unresolved; no independent normative approval.
pub open spec fn bit_read_error(
    bytes: Seq<u8>, position: usize, count: u8,
) -> Option<crate::MalformedError>
    recommends position <= bytes.len() * 8, bytes.len() <= usize::MAX / 8,
{
    if count > 32 {
        Some(crate::MalformedError::InvalidBitCount(count))
    } else if count as nat > bytes.len() * 8 - position {
        Some(eof_error(position, count as usize, (bytes.len() * 8 - position) as usize))
    } else {
        None
    }
}

}

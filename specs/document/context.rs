// Source-only project API support, not normative approval.
// Guide: ../README.md; audit ledger: ../../validation/traceability/requirements.json

use super::*;

verus! {

// Model navigation: spec::decoder_context_state [project-result-model]
// Purpose: Supplied limits, logical scratch contents and stored retained-byte metadata.
// Scope: project API cleanup support only.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub closed spec fn decoder_context_state(
    context: &crate::document::DecoderContext,
) -> (crate::Limits, Seq<u32>, Seq<u32>, u64) {
    (
        decoder_limits(context.decoder),
        context.mmr.transitions@,
        context.mmr.row_transitions@,
        context.mmr.retained_bytes,
    )
}

// Model navigation: spec::decoder_context_finished [project-result-model]
// Purpose: Limits frame, empty logical scratch and retained-metadata admission after any return.
// Scope: project API cleanup support only.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn decoder_context_finished(
    before: &crate::document::DecoderContext,
    after: &crate::document::DecoderContext,
) -> bool {
    decoder_context_state(after).0 == decoder_context_state(before).0
        && decoder_context_state(after).1.len() == 0
        && decoder_context_state(after).2.len() == 0
        && decoder_context_state(after).3
            <= decoder_context_state(after).0.max_retained_state_bytes
}

pub(crate) proof fn decoder_context_fields()
    ensures forall|context: &crate::document::DecoderContext|
        #[trigger] decoder_context_state(context) == (
            decoder_limits(context.decoder),
            context.mmr.transitions@,
            context.mmr.row_transitions@,
            context.mmr.retained_bytes,
        ),
{
    reveal(decoder_context_state);
}

}

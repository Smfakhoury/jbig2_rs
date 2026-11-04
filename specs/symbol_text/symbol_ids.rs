// Source-only author navigation, not normative approval.
// Guide: ../README.md; audit ledger: ../../validation/traceability/requirements.json

use super::*;

verus! {

// Model navigation: spec::symbol_id_request_matches [project-result-model]
// Purpose: Exact caller-supplied codebook extent, enclosing-segment width and entropy-mode forwarding.
// Audit: symbol-id-request-matches
// REQUIREMENT CONTEXT: T88-PROFILE-SYMBOL-ID-PRESERVATION | ITU-T T.88 (08/2018) | 0.2 (informative context); 6.4.10 (index decoding context)
// Fidelity: unresolved; no independent normative approval.
pub(in crate::symbol_text) open spec fn symbol_id_request_matches(
    request: crate::symbol_text::SymbolIdRequest,
    available: nat,
    code_length: u8,
    huffman: bool,
) -> bool {
    &&& request.available_symbols as nat == available
    &&& request.code_length == code_length
    &&& request.huffman == huffman
}

// Model navigation: spec::pow2 [proof-support]
// Purpose: Private codec/bank projections, signed/clipped vocabulary and export measures.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub(crate) open spec fn pow2(exponent: nat) -> nat
    decreases exponent,
{
    if exponent == 0 { 1 } else { 2 * pow2((exponent - 1) as nat) }
}

// Model navigation: spec::symbol_code_length_valid [representation-invariant]
// Purpose: Codec banks/bitmaps, table slots, export counters and placement-bound invariants.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub(crate) open spec fn symbol_code_length_valid(count: u32, length: u8) -> bool {
    length <= 32
        && (count <= 1 ==> length == 0)
        && (count > 1 ==> length > 0)
}

// Model navigation: spec::symbol_code_length_expected [project-result-model]
// Purpose: Arithmetic codec result/error translation and encoded choice/length results.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub(crate) open spec fn symbol_code_length_expected(count: u32) -> u8 {
    if count <= 1 { 0 }
    else if count <= 2 { 1 }
    else if count <= 4 { 2 }
    else if count <= 8 { 3 }
    else if count <= 16 { 4 }
    else if count <= 32 { 5 }
    else if count <= 64 { 6 }
    else if count <= 128 { 7 }
    else if count <= 256 { 8 }
    else if count <= 512 { 9 }
    else if count <= 1_024 { 10 }
    else if count <= 2_048 { 11 }
    else if count <= 4_096 { 12 }
    else if count <= 8_192 { 13 }
    else if count <= 16_384 { 14 }
    else if count <= 32_768 { 15 }
    else if count <= 65_536 { 16 }
    else if count <= 131_072 { 17 }
    else if count <= 262_144 { 18 }
    else if count <= 524_288 { 19 }
    else if count <= 1_048_576 { 20 }
    else if count <= 2_097_152 { 21 }
    else if count <= 4_194_304 { 22 }
    else if count <= 8_388_608 { 23 }
    else if count <= 16_777_216 { 24 }
    else if count <= 33_554_432 { 25 }
    else if count <= 67_108_864 { 26 }
    else if count <= 134_217_728 { 27 }
    else if count <= 268_435_456 { 28 }
    else if count <= 536_870_912 { 29 }
    else if count <= 1_073_741_824 { 30 }
    else if count <= 2_147_483_648 { 31 }
    else { 32 }
}

// Model navigation: spec::checked_symbol_id_result [project-result-model]
// Purpose: Preserve a valid ID or return its exact supplied out-of-range payload.
// Audit: symbol-checked-id-result
// REQUIREMENT CONTEXT: T88-PROFILE-SYMBOL-ID-PRESERVATION | ITU-T T.88 (08/2018) | 0.2 (informative context); 6.4.10 (index decoding context)
// Fidelity: unresolved; no independent normative approval.
pub(crate) open spec fn checked_symbol_id_result(
    id: u32, available: u32,
) -> Result<u32, crate::symbol_text::SymbolTextError> {
    if id >= available {
        Err(crate::symbol_text::SymbolTextError::SymbolIdOutOfRange { id, available })
    } else {
        Ok(id)
    }
}

// Model navigation: spec::checked_symbol_decode_result [project-result-model]
// Purpose: Filter a decoded ID without changing an underlying entropy error.
// Audit: symbol-checked-id-decode-result
// REQUIREMENT CONTEXT: T88-PROFILE-SYMBOL-ID-PRESERVATION | ITU-T T.88 (08/2018) | 0.2 (informative context); 6.4.10 (index decoding context)
// Fidelity: unresolved; no independent normative approval.
pub(crate) open spec fn checked_symbol_decode_result(
    decoded: Result<u32, crate::symbol_text::SymbolTextError>, available: u32,
) -> Result<u32, crate::symbol_text::SymbolTextError> {
    match decoded {
        Ok(id) => checked_symbol_id_result(id, available),
        Err(error) => Err(error),
    }
}

// Model navigation: spec::checked_symbol_id_transition [project-result-model]
// Purpose: Exact empty-book preflight and arithmetic/literal-Huffman checked result and cursor effects.
// Audit: symbol-checked-id-transition
// REQUIREMENT CONTEXT: T88-PROFILE-SYMBOL-ID-PRESERVATION | ITU-T T.88 (08/2018) | 0.2 (informative context); 6.4.10 (index decoding context)
// Fidelity: unresolved; no independent normative approval.
pub(in crate::symbol_text) open spec fn checked_symbol_id_transition<'a>(
    before: &crate::symbol_text::SymbolCodec<'a>,
    after: &crate::symbol_text::SymbolCodec<'a>,
    request: crate::symbol_text::SymbolIdRequest,
    result: Result<u32, crate::symbol_text::SymbolTextError>,
) -> bool
    recommends codec_valid(before),
{
    if request.available_symbols == 0 {
        result == Err(crate::symbol_text::SymbolTextError::InvalidCount(
            "symbol ID with no available symbols",
        )) && *after == *before
    } else {
        match *before {
            crate::symbol_text::SymbolCodec::Arithmetic(prior) => {
                let decoded = arithmetic_symbol_id_result(
                    crate::arithmetic::arithmetic_spec::iaid_state(
                        prior.decoder, prior.iaid@, request.code_length,
                    ).0,
                );
                checked_symbol_decode_result(decoded, request.available_symbols) == result
                    && arithmetic_symbol_id_transition(before, after, request, decoded)
            },
            crate::symbol_text::SymbolCodec::Huffman(prior) =>
                {
                    let decoded = huffman_symbol_id_state(&prior, request).0;
                    checked_symbol_decode_result(decoded, request.available_symbols) == result
                        && huffman_symbol_id_transition(before, after, request, decoded)
                },
        }
    }
}

}

// Source-only author navigation, not normative approval.
// Guide: ../README.md; audit ledger: ../../validation/traceability/requirements.json

use super::*;

verus! {

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


}

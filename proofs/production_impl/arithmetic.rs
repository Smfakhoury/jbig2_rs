// Source-only author navigation; named purposes are not normative approval.
// Guide, official acquisition and source identity: ../../specs/README.md
// All requirement links and inherited locators: ../../validation/traceability/requirements.json
// REQUIREMENT CONTEXT is not a direct normative equation, including for proof/policy helpers.

use vstd::prelude::*;

// Model navigation: macro::production_qe_transition_valid [proof-support]
// Purpose: QE bounds, subtraction safety and integer-history/magnitude proof bridges.
// Audit: arithmetic-qe-transition-bounds
// REQUIREMENT CONTEXT: T88-ARITHMETIC-DECODE-QE-LOOKUP | ITU-T T.88 (08/2018) | E.3.2, Figure E.15, Qe(I(CX)) selection (p. 120)
// Fidelity: unresolved; no independent normative approval.
macro_rules! production_qe_transition_valid {
    ($index:expr, $state:ident) => {
        proof! {
            crate::arithmetic::arithmetic_shared::qe_transition_valid($index as int);
            assert($state.next_mps < 47);
            assert($state.next_lps < 47);
            assert($state.qe > 0);
            assert($state.qe <= 0x5601);
        }
    };
}

// Model navigation: macro::production_decode_interval_subtraction [proof-support]
// Purpose: QE bounds, subtraction safety and integer-history/magnitude proof bridges.
// Audit: arithmetic-interval-subtraction-bounds
// REQUIREMENT CONTEXT: T88-ARITHMETIC-DECODE-INTERVAL-SUBTRACT | ITU-T T.88 (08/2018) | E.3.2, Figure E.15, initial interval subtraction (p. 120)
// Fidelity: unresolved; no independent normative approval.
macro_rules! production_decode_interval_subtraction {
    ($decoder:ident, $qe:ident) => {
        proof! {
            assert($decoder.interval >= crate::arithmetic::MIN_INTERVAL);
            assert($qe <= 0x5601u32);
            assert($decoder.interval > $qe);
        }
    };
}

macro_rules! production_reveal_decoder_valid {
    () => {
        proof! {
            reveal(crate::arithmetic::arithmetic_spec::decoder_valid);
            reveal(crate::arithmetic::arithmetic_spec::decoder_state_valid);
            reveal(crate::arithmetic::arithmetic_spec::decoder_position_valid);
        }
    };
}

// Model navigation: macro::production_integer_history_index [proof-support]
// Purpose: QE bounds, subtraction safety and integer-history/magnitude proof bridges.
// Audit: arithmetic-history-index-bounds
// REQUIREMENT CONTEXT: T88-ARITHMETIC-INTEGER-CONTEXT-SELECT | ITU-T T.88 (08/2018) | A.2, step 2 (p. 89)
// Fidelity: unresolved; no independent normative approval.
macro_rules! production_integer_history_index {
    ($previous:expr) => {
        proof! {
            crate::arithmetic::arithmetic_properties::binary_history_index_is_valid(
                $previous,
            );
        }
    };
}

// Model navigation: macro::production_integer_magnitude [proof-support]
// Purpose: QE bounds, subtraction safety and integer-history/magnitude proof bridges.
// Audit: arithmetic-magnitude-bounds
// REQUIREMENT CONTEXT: T88-ARITHMETIC-INTEGER-MAGNITUDE-LEAF-1 | ITU-T T.88 (08/2018) | A.2, Figure A.1, magnitude leaf 1 (first zero branch) (p. 90)
// Fidelity: unresolved; no independent normative approval.
macro_rules! production_integer_magnitude {
    ($magnitude_bits:ident, $offset:ident) => {
        proof! {
            crate::arithmetic::arithmetic_properties::integer_magnitude_fits_i64(
                $magnitude_bits,
                $offset,
            );
        }
    };
}

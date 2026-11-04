// Source-only author navigation; named purposes are not normative approval.
// Guide, official acquisition and source identity: ../../specs/README.md
// All requirement links and inherited locators: ../../validation/traceability/requirements.json
// REQUIREMENT CONTEXT is not a direct normative equation, including for proof/policy helpers.

use vstd::prelude::*;

// Model navigation: macro::production_symbol_code_length_bounds [proof-support]
// Purpose: Prove the existing finite production projection has algebraic capacity and least-width bounds.
// Audit: symbol-code-length-bounds-witness
// REQUIREMENT CONTEXT: T88-PROFILE-SYMBOL-ID-PRESERVATION | ITU-T T.88 (08/2018) | 0.2 (informative context); 6.4.10 (index decoding context)
// Fidelity: unresolved; no independent normative approval.
macro_rules! production_symbol_code_length_bounds {
    ($count:expr) => {
        proof! {
            crate::symbol_text::symbol_text_spec::symbol_code_length_expected_bounds($count);
            crate::symbol_text::symbol_text_spec::symbol_code_length_least(
                $count,
                crate::symbol_text::symbol_text_spec::symbol_code_length_expected($count),
            );
        }
    };
}

// Model navigation: macro::production_symbol_id_request [proof-support]
// Purpose: Check actual text/dictionary request construction against the supplied codebook and segment parameters.
// Audit: symbol-id-request-witness
// REQUIREMENT CONTEXT: T88-PROFILE-SYMBOL-ID-PRESERVATION | ITU-T T.88 (08/2018) | 0.2 (informative context); 6.4.10 (index decoding context)
// Fidelity: unresolved; no independent normative approval.
macro_rules! production_symbol_id_request {
    ($request:expr, $available:expr, $length:expr, $huffman:expr) => {
        proof! {
            assert(crate::symbol_text::symbol_text_spec::symbol_id_request_matches(
                $request, ($available) as nat, $length, $huffman,
            ));
        }
    };
}

verus! {

pub(super) proof fn packed_row_end_in_bounds(
    row: nat,
    height: nat,
    stride: nat,
    final_byte: nat,
)
    requires
        row < height,
        stride > 0,
        final_byte == row * stride + (stride - 1),
    ensures
        final_byte < stride * height,
{
    vstd::arithmetic::mul::lemma_mul_inequality(
        row as int + 1,
        height as int,
        stride as int,
    );
    vstd::arithmetic::mul::lemma_mul_is_distributive_add_other_way(
        stride as int,
        row as int,
        1,
    );
    vstd::arithmetic::mul::lemma_mul_is_commutative(
        height as int,
        stride as int,
    );
    assert(final_byte + 1 == (row + 1) * stride);
}

}

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

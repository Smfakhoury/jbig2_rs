// Source-only author navigation, not normative approval.
// Guide: ../README.md; audit ledger: ../../validation/traceability/requirements.json

use super::*;

verus! {

// Model navigation: spec::huffman_error_result [project-result-model]
// Purpose: Reuse the existing symbol/text wrapper around the exact Huffman conversion.
// Audit: symbol-huffman-error-result
// REQUIREMENT CONTEXT: T88-PROFILE-SYMBOL-ID-PRESERVATION | ITU-T T.88 (08/2018) | 0.2 (informative context); 6.4.10 (index decoding context)
// Fidelity: unresolved; no independent normative approval.
pub closed spec fn huffman_error_result(
    error: &crate::huffman::HuffmanError,
) -> crate::symbol_text::SymbolTextError {
    crate::symbol_text::SymbolTextError::Decode(
        crate::huffman::huffman_spec::huffman_error_result(error),
    )
}

pub(in crate::symbol_text) proof fn huffman_error_result_definition(
    error: &crate::huffman::HuffmanError,
)
    ensures huffman_error_result(error) == crate::symbol_text::SymbolTextError::Decode(
        crate::huffman::huffman_spec::huffman_error_result(error),
    ),
{
    reveal(huffman_error_result);
}

// Model navigation: spec::huffman_symbol_id_result [project-result-model]
// Purpose: Exact signed-value, OOB and Huffman-error projection into symbol IDs.
// Audit: symbol-huffman-id-result
// REQUIREMENT CONTEXT: T88-PROFILE-SYMBOL-ID-PRESERVATION | ITU-T T.88 (08/2018) | 0.2 (informative context); 6.4.10 (index decoding context)
// Fidelity: unresolved; no independent normative approval.
pub(in crate::symbol_text) open spec fn huffman_symbol_id_result(
    decoded: Result<crate::huffman::HuffmanValue, crate::huffman::HuffmanError>,
) -> Result<u32, crate::symbol_text::SymbolTextError> {
    match decoded {
        Ok(crate::huffman::HuffmanValue::Integer(value)) =>
            if value < 0 {
                Err(crate::symbol_text::SymbolTextError::InvalidEntropyValue("symbol ID"))
            } else {
                Ok(value as u32)
            },
        Ok(crate::huffman::HuffmanValue::OutOfBand) =>
            Err(crate::symbol_text::SymbolTextError::UnexpectedOutOfBand("symbol ID")),
        Err(error) => Err(huffman_error_result(&error)),
    }
}

// Model navigation: spec::huffman_symbol_id_state [project-result-model]
// Purpose: Exact table-backed or literal decoded ID/error and consumed reader position.
// Audit: symbol-huffman-id-state
// REQUIREMENT CONTEXT: T88-PROFILE-SYMBOL-ID-PRESERVATION | ITU-T T.88 (08/2018) | 0.2 (informative context); 6.4.10 (index decoding context)
// Fidelity: unresolved; no independent normative approval.
pub(in crate::symbol_text) open spec fn huffman_symbol_id_state(
    codec: &crate::symbol_text::HuffmanCodec,
    request: crate::symbol_text::SymbolIdRequest,
) -> (Result<u32, crate::symbol_text::SymbolTextError>, usize)
    recommends crate::huffman::huffman_spec::reader_valid(&codec.reader),
{
    match &codec.symbol_id_table {
        Some(table) => {
            let decoded = crate::huffman::huffman_spec::decode_state(
                table, codec.reader.bytes@, codec.reader.bit_position,
            );
            (huffman_symbol_id_result(decoded.0), decoded.1)
        },
        None => {
            let error = crate::spec::readers::bit_read_error(
                codec.reader.bytes@, codec.reader.bit_position, request.code_length,
            );
            let result = match error {
                Some(error) => Err(crate::symbol_text::SymbolTextError::Decode(
                    crate::DecodeError::Malformed(error),
                )),
                None => Ok(crate::spec::readers::bits_value(
                    codec.reader.bytes@, codec.reader.bit_position as nat,
                    request.code_length as nat,
                ) as u32),
            };
            (result, (codec.reader.bit_position as nat
                + if error.is_none() { request.code_length as nat } else { 0nat }) as usize)
        },
    }
}

// Model navigation: spec::huffman_symbol_id_transition [project-result-model]
// Purpose: Preserve complete Huffman ID result/cursor and untouched codec metadata through enum dispatch.
// Audit: symbol-huffman-id-transition
// REQUIREMENT CONTEXT: T88-PROFILE-SYMBOL-ID-PRESERVATION | ITU-T T.88 (08/2018) | 0.2 (informative context); 6.4.10 (index decoding context)
// Fidelity: unresolved; no independent normative approval.
pub(in crate::symbol_text) open spec fn huffman_symbol_id_transition<'a>(
    before: &crate::symbol_text::SymbolCodec<'a>,
    after: &crate::symbol_text::SymbolCodec<'a>,
    request: crate::symbol_text::SymbolIdRequest,
    result: Result<u32, crate::symbol_text::SymbolTextError>,
) -> bool
    recommends codec_valid(before),
{
    match *before {
        crate::symbol_text::SymbolCodec::Huffman(prior) => match *after {
            crate::symbol_text::SymbolCodec::Huffman(next) => {
                let decoded = huffman_symbol_id_state(&prior, request);
                &&& result == decoded.0
                &&& next.reader.bit_position == decoded.1
                &&& next.reader.bytes@ == prior.reader.bytes@
                &&& crate::huffman::huffman_spec::reader_valid(&next.reader)
                &&& next.tables == prior.tables
                &&& next.log_strip_size == prior.log_strip_size
                &&& next.symbol_id_table == prior.symbol_id_table
                &&& next.refinement == prior.refinement
                &&& next.refinement_work == prior.refinement_work
            },
            _ => false,
        },
        _ => true,
    }
}

}

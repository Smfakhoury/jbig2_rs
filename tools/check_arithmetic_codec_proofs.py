#!/usr/bin/env python3
"""Check arithmetic caller result/error projection and context-bank isolation."""

import argparse
import json
from pathlib import Path
import subprocess
import sys

from proof_sensitivity import check_semantic_cases, replace_once
from rust_source import source_functions


ROOT = Path(__file__).resolve().parents[1]
ERROR_POST = "out == arithmetic_spec::decode_error(self),"
SLOT_POST = "out == symbol_text_spec::integer_field_slot(self),"
INTEGER_CALL = """crate::arithmetic::arithmetic_spec::integer_state(
    old(self).decoder,
    old(self).integers.slots[symbol_text_spec::integer_field_slot(field) as int]@,
)"""
INTEGER_RESULT = f"""out == symbol_text_spec::arithmetic_integer_result(
    {INTEGER_CALL}.0,
),"""
INTEGER_DECODER = f"final(self).decoder == {INTEGER_CALL}.1,"
INTEGER_FRAME = """forall|slot: int| 0 <= slot < INTEGER_FIELD_COUNT
    && slot != symbol_text_spec::integer_field_slot(field) ==>
        final(self).integers.slots[slot]@ == old(self).integers.slots[slot]@,"""
INTEGER_IAID_SITE = """final(self).iaid@ == old(self).iaid@,
final(self).refinement == old(self).refinement,
final(self).refinement_work == old(self).refinement_work,
out == symbol_text_spec::arithmetic_integer_result("""
IAID_FRAME_SITE = """final(self).integers.slots@ == old(self).integers.slots@,
final(self).refinement == old(self).refinement,
final(self).refinement_work == old(self).refinement_work,
out == symbol_text_spec::arithmetic_symbol_id_result("""
IAID_RESULT = """out == symbol_text_spec::arithmetic_symbol_id_result(
    crate::arithmetic::arithmetic_spec::iaid_state(
        old(self).decoder, old(self).iaid@, request.code_length,
    ).0,
),"""
DECODED_TAIL = """Err(error) => return Err(SymbolTextError::Decode(error.into_decode())),
};
match decoded {"""
IAID_HEAD = """fn decode_symbol_id(&mut self, request: SymbolIdRequest) -> Result<u32, SymbolTextError> {
    match self
        .decoder
        .decode_iaid(request.code_length, &mut self.iaid)"""
IAID_RETURN = """match self
    .decoder
    .decode_iaid(request.code_length, &mut self.iaid) {
    Ok(value) => Ok(value),
    Err(error) => Err(SymbolTextError::Decode(error.into_decode())),
}"""
FUNCTIONS = {row["id"]: row for row in source_functions(ROOT)}
VALIDATION_BODY = FUNCTIONS["generic_region::validate_probability_states"]["body_text"]
SIGNAL_BODY = FUNCTIONS["generic_region::probability_signal_error"]["body_text"]
EOF_BODY = FUNCTIONS["generic_region::unexpected_eof"]["body_text"]
OVERFLOW_BODY = FUNCTIONS["generic_region::integer_overflow"]["body_text"]
STATE_ERROR_POST = "&& error == ArithmeticError::InvalidContextState { state_index, mps }"
VALIDATION_POST = "generic_region_spec::probability_validation_result(contexts@, out),"
SIGNAL_POST = """ensures out == generic_region_spec::probability_error_for_signal(
    decoder, index, context_count, signal,
),"""
EOF_POST = """ensures out == GenericRegionError::Decode(
    crate::arithmetic::arithmetic_spec::decode_error(
        crate::arithmetic::arithmetic_spec::eof_error(offset, needed, remaining),
    ),
),"""
OVERFLOW_POST = """ensures out == GenericRegionError::Decode(
    DecodeError::Malformed(MalformedError::IntegerOverflow),
),"""
INVALID_STATE_RETURN = "return Err(ArithmeticError::InvalidContextState { state_index, mps });"
INVALID_STATE_VALUE = """DecodeError::Malformed(MalformedError::InvalidArithmeticContextState {
    state_index,
    mps,
})"""


def case(identity, module, function, site, replacement, control, weaker=""):
    return {
        "id": identity, "file": f"src/{module}.rs", "module": module,
        "function": function, "site": site, "replacement": replacement,
        "controls": ((control, weaker),),
    }


CASES = (
    case(
        "arithmetic_error_forwarding_loses_iaid_length", "arithmetic", "ArithmeticError::into_decode",
        """Self::InvalidIaidCodeLength(length) => {
            DecodeError::Malformed(MalformedError::InvalidIaidCodeLength(length))
        }""",
        """Self::InvalidIaidCodeLength(length) => {
            DecodeError::Malformed(MalformedError::InvalidIaidCodeLength(0))
        }""",
        ERROR_POST,
    ),
    case(
        "integer_field_uses_the_next_bank", "symbol_text", "IntegerField::slot",
        "const fn slot(self) -> usize { self as usize }",
        "const fn slot(self) -> usize { (self as usize + 1) % INTEGER_FIELD_COUNT }",
        SLOT_POST,
    ),
    case(
        "arithmetic_integer_caller_discards_value", "symbol_text", "ArithmeticCodec::decode_integer",
        "ArithmeticInteger::Value(value) => Ok(EntropyInteger::Value(value)),",
        "ArithmeticInteger::Value(value) => Ok(EntropyInteger::Value(0)),",
        INTEGER_RESULT,
    ),
    case(
        "arithmetic_integer_caller_replaces_decoder", "symbol_text", "ArithmeticCodec::decode_integer",
        DECODED_TAIL,
        DECODED_TAIL.replace("match decoded {", """if let Ok(replacement) = ArithmeticDecoder::new(&[0xff, 0x90]) {
            self.decoder = replacement;
        }
        match decoded {"""),
        INTEGER_DECODER,
    ),
    case(
        "arithmetic_integer_caller_changes_another_bank", "symbol_text", "ArithmeticCodec::decode_integer",
        DECODED_TAIL,
        DECODED_TAIL.replace(
            "match decoded {",
            "self.integers.slots[(slot + 1) % INTEGER_FIELD_COUNT] = [ArithmeticContext::new(); 512];\nmatch decoded {",
        ),
        INTEGER_FRAME,
    ),
    case(
        "arithmetic_integer_caller_clears_iaid_bank", "symbol_text", "ArithmeticCodec::decode_integer",
        DECODED_TAIL, DECODED_TAIL.replace("match decoded {", "self.iaid.clear();\nmatch decoded {"),
        INTEGER_IAID_SITE, INTEGER_IAID_SITE.replace("final(self).iaid@ == old(self).iaid@,", ""),
    ),
    case(
        "arithmetic_iaid_caller_discards_value", "symbol_text", "ArithmeticCodec::decode_symbol_id",
        IAID_RETURN, IAID_RETURN.replace("Ok(value) => Ok(value)", "Ok(value) => Ok(0)"), IAID_RESULT,
    ),
    case(
        "arithmetic_iaid_caller_changes_integer_bank", "symbol_text", "ArithmeticCodec::decode_symbol_id",
        IAID_HEAD, IAID_HEAD.replace(
            "match self",
            "self.integers.slots[0] = [ArithmeticContext::new(); 512];\nmatch self",
        ),
        IAID_FRAME_SITE, IAID_FRAME_SITE.replace("final(self).integers.slots@ == old(self).integers.slots@,", ""),
    ),
    case(
        "context_constructor_loses_invalid_state", "arithmetic", "ArithmeticContext::from_state",
        INVALID_STATE_RETURN,
        "return Err(ArithmeticError::InvalidContextState { state_index: 0, mps });",
        STATE_ERROR_POST,
    ),
    case(
        "context_constructor_loses_invalid_mps", "arithmetic", "ArithmeticContext::from_state",
        INVALID_STATE_RETURN,
        "return Err(ArithmeticError::InvalidContextState { state_index, mps: 0 });",
        STATE_ERROR_POST,
    ),
    case(
        "probability_validation_rejects_valid_bank", "generic_region", "validate_probability_states",
        VALIDATION_BODY, "Err(integer_overflow())", VALIDATION_POST,
    ),
    case(
        "probability_validation_loses_first_invalid_state", "generic_region", "validate_probability_states",
        VALIDATION_BODY,
        replace_once(
            VALIDATION_BODY, INVALID_STATE_VALUE,
            INVALID_STATE_VALUE.replace("state_index,", "state_index: 0,"),
        ),
        VALIDATION_POST,
    ),
    case(
        "probability_validation_loses_first_invalid_mps", "generic_region", "validate_probability_states",
        VALIDATION_BODY,
        replace_once(
            VALIDATION_BODY, INVALID_STATE_VALUE,
            INVALID_STATE_VALUE.replace("mps,", "mps: 0,"),
        ),
        VALIDATION_POST,
    ),
    case(
        "probability_validation_reclassifies_invalid_bank", "generic_region", "validate_probability_states",
        VALIDATION_BODY,
        replace_once(
            VALIDATION_BODY, "return Err(GenericRegionError::Decode(error));",
            "return Err(integer_overflow());",
        ),
        VALIDATION_POST,
    ),
    case(
        "probability_failure_loses_context_index", "generic_region", "probability_signal_error",
        SIGNAL_BODY,
        replace_once(SIGNAL_BODY, "index,\ncount: context_count,", "index: 0,\ncount: context_count,"),
        SIGNAL_POST, "ensures true,",
    ),
    case(
        "probability_failure_loses_context_count", "generic_region", "probability_signal_error",
        SIGNAL_BODY,
        replace_once(SIGNAL_BODY, "count: context_count,", "count: 0,"),
        SIGNAL_POST, "ensures true,",
    ),
    case(
        "probability_failure_reclassifies_eof", "generic_region", "probability_signal_error",
        SIGNAL_BODY,
        replace_once(
            SIGNAL_BODY, "decoder.signal_error(signal)",
            "decoder.signal_error(RASTER_SIGNAL_CONTEXT)",
        ),
        SIGNAL_POST, "ensures true,",
    ),
    case(
        "generic_eof_swaps_needed_and_remaining", "generic_region", "unexpected_eof",
        EOF_BODY,
        replace_once(
            EOF_BODY, "offset,\nneeded,\nremaining,",
            "offset,\nneeded: remaining,\nremaining: needed,",
        ),
        EOF_POST, "ensures true,",
    ),
    case(
        "generic_overflow_reclassified_as_eof", "generic_region", "integer_overflow",
        OVERFLOW_BODY,
        "GenericRegionError::Decode(DecodeError::Malformed(MalformedError::UnexpectedEof { offset: 0, needed: 1, remaining: 0 }))",
        OVERFLOW_POST, "ensures true,",
    ),
)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--verus-executable", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    try:
        report = check_semantic_cases(
            ROOT, arguments.verus_executable.resolve(), CASES,
            ("src/arithmetic.rs", "src/symbol_text.rs", "src/generic_region.rs",
             "specs/arithmetic.rs", "specs/symbol_text.rs", "specs/generic_region.rs"),
            "jbig2-arithmetic-codec-",
            "Exact project arithmetic error/value projection, integer field/bank identity, integer/IAID decoder/bank frames and generic raw-context first-failure/signal/EOF/overflow diagnostics; normative call-site identity, entropy/raster and full streams remain separate",
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"arithmetic codec proof sensitivity failed: {error}", file=sys.stderr)
        report = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

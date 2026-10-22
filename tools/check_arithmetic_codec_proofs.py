#!/usr/bin/env python3
"""Check arithmetic caller result/error projection and context-bank isolation."""

import argparse
import json
from pathlib import Path
import subprocess
import sys

from proof_sensitivity import check_semantic_cases


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
)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--verus-executable", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    try:
        report = check_semantic_cases(
            ROOT, arguments.verus_executable.resolve(), CASES,
            ("src/arithmetic.rs", "src/symbol_text.rs", "specs/arithmetic.rs", "specs/symbol_text.rs"),
            "jbig2-arithmetic-codec-",
            "Exact project arithmetic error/value projection, integer field/bank identity and integer/IAID decoder/bank frames; normative call-site identity and full streams remain separate",
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

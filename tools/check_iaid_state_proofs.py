#!/usr/bin/env python3
"""Check complete IAID result, decoder and context-bank composition."""

import argparse
import json
from pathlib import Path
import subprocess
import sys

from proof_sensitivity import check_semantic_cases


ROOT = Path(__file__).resolve().parents[1]
RESULT = "out == arithmetic_spec::iaid_state(*old(self), old(contexts)@, code_length).0,"
DECODER = "*final(self) == arithmetic_spec::iaid_state(*old(self), old(contexts)@, code_length).1,"
CONTEXTS = "final(contexts)@ == arithmetic_spec::iaid_state(*old(self), old(contexts)@, code_length).2,"
RETURN = """if value > u32::MAX as usize {
    return Err(ArithmeticError::Malformed(MalformedError::IntegerOverflow));
}
Ok(value as u32)"""
CALL = "let decision = self.decode(&mut contexts[previous])?;"


def case(identity, site, replacement, control):
    return {
        "id": identity, "file": "src/arithmetic.rs", "module": "arithmetic",
        "function": "ArithmeticDecoder::decode_iaid",
        "site": site, "replacement": replacement, "controls": ((control, ""),),
    }


CASES = (
    case("iaid_discards_the_decoded_value", RETURN, RETURN.replace("Ok(value as u32)", "Ok(0)"), RESULT),
    case(
        "iaid_nonzero_empty_value", RETURN,
        RETURN.replace("Ok(value as u32)", "if code_length == 0 { return Ok(1); }\nOk(value as u32)"), RESULT,
    ),
    case(
        "iaid_discards_final_code_register", RETURN,
        RETURN.replace("Ok(value as u32)", "self.code = 0;\nOk(value as u32)"), DECODER,
    ),
    case(
        "iaid_changes_unvisited_root_context", RETURN,
        RETURN.replace("Ok(value as u32)", "if !contexts.is_empty() { contexts[0] = ArithmeticContext::new(); }\nOk(value as u32)"),
        CONTEXTS,
    ),
    case(
        "iaid_masks_decision_eof_as_overflow", CALL,
        """let decision = match self.decode(&mut contexts[previous]) {
            Ok(decision) => decision,
            Err(_error) => return Err(ArithmeticError::Malformed(MalformedError::IntegerOverflow)),
        };""",
        RESULT,
    ),
    case(
        "iaid_rewinds_partial_error_input", CALL,
        """let decision = match self.decode(&mut contexts[previous]) {
            Ok(decision) => decision,
            Err(error) => {
                self.byte_position = 0;
                self.current_byte = self.bytes[0];
                self.marker = None;
                return Err(error);
            }
        };""",
        DECODER,
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
            ("src/arithmetic.rs", "specs/arithmetic.rs", "proofs/model_support/arithmetic.rs"),
            "jbig2-iaid-state-",
            "Exact project IAID result/decoder/context-bank relation including positive fold, zero length, and partial errors; normative caller length and standard interpretation remain separate",
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"IAID state proof sensitivity failed: {error}", file=sys.stderr)
        report = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Check exact decision/context outputs, not complete entropy-stream equivalence."""

import argparse
import json
from pathlib import Path
import subprocess
import sys

from proof_sensitivity import check_semantic_cases


ROOT = Path(__file__).resolve().parents[1]
CONTEXT_POST = """final(context).state_index == arithmetic_spec::entry_index(
    old(self).interval, old(self).code, old(context).state_index,
),
final(context).mps == arithmetic_spec::entry_mps(
    old(self).interval, old(self).code, old(context).state_index, old(context).mps,
),"""
SIGNAL_POST = """out <= 1 ==> out == arithmetic_spec::entry_decision(
    old(self).interval, old(self).code, old(context).state_index, old(context).mps,
),
""" + CONTEXT_POST
CHECKED_POST = """&& decision == arithmetic_spec::entry_decision(
    old(self).interval, old(self).code, old(context).state_index, old(context).mps,
)"""
CHECKED_CONTEXT = "final(self).bytes == old(self).bytes,\n" + CONTEXT_POST + "\nmatch out {"
FRAME = """final(self).bytes@ == old(self).bytes@,
final(self).bytes == old(self).bytes,"""
SIGNAL_FRAME = """out <= RASTER_SIGNAL_OVERFLOW,
arithmetic_spec::decoder_state_valid(final(self)),
arithmetic_spec::context_valid(*final(context)),
""" + FRAME + """
out <= 1 ==> arithmetic_spec::decoder_valid(final(self)),
""" + SIGNAL_POST
ERROR_SITE = "return self.decode_lower_signal(context, state, qe, old_mps);"


def error_case(identity, change, controls):
    return {
        "id": identity, "file": "src/arithmetic.rs", "module": "arithmetic",
        "function": "ArithmeticDecoder::decode_signal", "site": ERROR_SITE,
        "replacement": (
            "let signal = self.decode_lower_signal(context, state, qe, old_mps);\n"
            f"if signal > 1 {{ {change} }}\nreturn signal;"
        ),
        "controls": controls,
    }


def exchange_post(lower):
    return f"""final(context).state_index == arithmetic_spec::exchange_index(
    {lower}, old(self).interval, qe, state.next_mps, state.next_lps,
),
final(context).mps == arithmetic_spec::exchange_mps(
    {lower}, old(self).interval, qe, state.switch_mps, old(context).mps, old_mps,
),"""


CASES = (
    {
        "id": "signal_returns_wrong_decision_without_context_step",
        "file": "src/arithmetic.rs", "module": "arithmetic",
        "function": "ArithmeticDecoder::decode_signal",
        "site": "pub(crate) fn decode_signal(&mut self, context: &mut ArithmeticContext) -> u8 {",
        "replacement": "pub(crate) fn decode_signal(&mut self, context: &mut ArithmeticContext) -> u8 { return 0;",
        "controls": ((SIGNAL_POST, ""),),
    },
    {
        "id": "checked_wrapper_returns_wrong_successful_decision",
        "file": "src/arithmetic.rs", "module": "arithmetic",
        "function": "ArithmeticDecoder::decode",
        "site": """pub(crate) fn decode(
    &mut self, context: &mut ArithmeticContext,
) -> Result<u8, ArithmeticError> {""",
        "replacement": """pub(crate) fn decode(
    &mut self, context: &mut ArithmeticContext,
) -> Result<u8, ArithmeticError> { return Ok(0);""",
        "controls": (
            (CHECKED_POST, ""),
            (CHECKED_CONTEXT, "final(self).bytes == old(self).bytes,\nmatch out {"),
        ),
    },
) + tuple(
    {
        "id": f"{side}_exchange_uses_wrong_error_context",
        "file": "src/arithmetic.rs", "module": "arithmetic",
        "function": f"ArithmeticDecoder::decode_{side}_signal",
        "site": f"""fn decode_{side}_signal(
    &mut self, context: &mut ArithmeticContext, state: QeState, qe: u32, old_mps: u8,
) -> u8 {{""",
        "replacement": f"""fn decode_{side}_signal(
    &mut self, context: &mut ArithmeticContext, state: QeState, qe: u32, old_mps: u8,
) -> u8 {{ context.state_index = state.next_lps; return RASTER_SIGNAL_EOF;""",
        "controls": ((exchange_post(lower), ""),),
    }
    for side, lower in (("lower", "true"), ("upper", "false"))
) + (
    error_case(
        "entry_error_discards_context_index", "context.state_index = 0;",
        ((SIGNAL_POST, SIGNAL_POST.replace(
            "final(context).state_index ==", "out <= 1 ==> final(context).state_index ==",
        )),),
    ),
    error_case(
        "entry_error_discards_mps", "context.mps = 0;",
        ((SIGNAL_POST, SIGNAL_POST.replace(
            "final(context).mps ==", "out <= 1 ==> final(context).mps ==",
        )),),
    ),
    error_case(
        "entry_error_replaces_input",
        "self.bytes = &self.bytes[..1]; self.byte_position = 0; "
        "self.current_byte = self.bytes[0]; self.marker = None;",
        ((SIGNAL_FRAME, SIGNAL_FRAME.replace(FRAME, "")),),
    ),
)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--verus-executable", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        report = check_semantic_cases(
            ROOT, args.verus_executable.resolve(), CASES,
            ("src/arithmetic.rs", "specs/arithmetic.rs"),
            "jbig2-arithmetic-decision-",
            "Successful decision, unconditional probability-context update including errors, and unchanged input borrow/view; exact renormalized decision registers, stream semantics and normative translation remain separate",
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"arithmetic decision proof sensitivity failed: {error}", file=sys.stderr)
        report = {"schema_version": 2, "status": "failed", "error": str(error)}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

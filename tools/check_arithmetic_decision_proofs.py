#!/usr/bin/env python3
"""Check exact decision/context outputs, not complete entropy-stream equivalence."""

import argparse
import json
from pathlib import Path
import subprocess
import sys

from proof_sensitivity import check_semantic_cases


ROOT = Path(__file__).resolve().parents[1]
SIGNAL_POST = """out <= 1 ==> out == arithmetic_spec::entry_decision(
    old(self).interval, old(self).code, old(context).state_index, old(context).mps,
),
out <= 1 ==> final(context).state_index == arithmetic_spec::entry_index(
    old(self).interval, old(self).code, old(context).state_index,
),
out <= 1 ==> final(context).mps == arithmetic_spec::entry_mps(
    old(self).interval, old(self).code, old(context).state_index, old(context).mps,
),"""
CHECKED_POST = """&& decision == arithmetic_spec::entry_decision(
    old(self).interval, old(self).code, old(context).state_index, old(context).mps,
)
&& final(context).state_index == arithmetic_spec::entry_index(
    old(self).interval, old(self).code, old(context).state_index,
)
&& final(context).mps == arithmetic_spec::entry_mps(
    old(self).interval, old(self).code, old(context).state_index, old(context).mps,
)"""


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
        "controls": ((CHECKED_POST, ""),),
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
            "Successful decoded decision and probability-context step against project Qe model; exact renormalized registers, input consumption and normative translation remain separate",
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

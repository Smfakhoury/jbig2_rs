#!/usr/bin/env python3
"""Check exact value-bridge contracts against compiling wrong-result controls."""

import argparse
import json
import subprocess
import sys
from pathlib import Path

from proof_sensitivity import check_semantic_cases


ROOT = Path(__file__).resolve().parents[1]
BASELINE_GROUPS = (("symbol_text", (
    "widen_bool_i64", "widen_u32_i64", "widen_u8_i32", "widen_u8_i64",
)),)


def numeric_case(name, argument, result):
    contract = f"""#[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out as int == value as int,
    ))]
    fn {name}"""
    return {
        "id": f"{name}_changes_value", "file": "src/symbol_text.rs",
        "module": "symbol_text", "function": name,
        "site": f"fn {name}(value: {argument}) -> {result} {{ {result}::from(value) }}",
        "replacement": f"fn {name}(value: {argument}) -> {result} {{ {result}::from(value) + 1 }}",
        "controls": ((contract, contract.replace("out as int == value as int", "true")),),
    }


CASES = (
    numeric_case("widen_u32_i64", "u32", "i64"),
    numeric_case("widen_u8_i64", "u8", "i64"),
    numeric_case("widen_u8_i32", "u8", "i32"),
    {
        "id": "boolean_conversion_inverted", "file": "src/symbol_text.rs",
        "module": "symbol_text", "function": "widen_bool_i64",
        "site": "fn widen_bool_i64(value: bool) -> i64 { if value { 1 } else { 0 } }",
        "replacement": "fn widen_bool_i64(value: bool) -> i64 { if value { 0 } else { 1 } }",
        "controls": (("out == if value { 1i64 } else { 0i64 }", "true"),),
    },
    {
        "id": "operator_equality_changes_or_to_and", "file": "src/decode.rs",
        "module": "decode", "function": "same_combination_operator",
        "site": """crate::bitmap::CombinationOperator::Or => {
            matches!(right, crate::bitmap::CombinationOperator::Or)
        }""",
        "replacement": """crate::bitmap::CombinationOperator::Or => {
            matches!(right, crate::bitmap::CombinationOperator::And)
        }""",
        "controls": (("out == (left == right)", "true"),),
    },
)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--verus-executable", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    try:
        result = check_semantic_cases(
            ROOT, arguments.verus_executable.resolve(), CASES,
            ("src/symbol_text.rs", "src/decode.rs"), "jbig2-value-bridges-",
            "Exact lossless primitive widening and combination-operator equality only; no caller or normative conformance closure",
            baseline_groups=BASELINE_GROUPS,
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"value bridge proof sensitivity failed: {error}", file=sys.stderr)
        result = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2))
    return 0 if result["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

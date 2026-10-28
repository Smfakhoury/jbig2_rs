#!/usr/bin/env python3
"""Check per-bit operator output contracts without claiming raster correctness."""

import argparse
import json
from pathlib import Path
import subprocess
import sys

from proof_sensitivity import check_semantic_cases


ROOT = Path(__file__).resolve().parents[1]
BYTE_POST = """ensures forall|bit: u32| bit < 8 ==>
    crate::spec::bitmap::byte_operator_bit(out, bit)
        == crate::spec::bitmap::combine(
            self, crate::spec::bitmap::byte_operator_bit(destination, bit),
            crate::spec::bitmap::byte_operator_bit(source, bit),
        ),"""
WORD_POST = """ensures forall|bit: u32| bit < 64 ==>
    crate::spec::bitmap::word_operator_bit(out, bit)
        == crate::spec::bitmap::combine(
            self, crate::spec::bitmap::word_operator_bit(destination, bit),
            crate::spec::bitmap::word_operator_bit(source, bit),
        ),"""
CASES = tuple(
    {
        "id": identity, "file": "src/bitmap.rs", "module": "bitmap",
        "function": f"CombinationOperator::{function}",
        "site": f"const fn {function}(self, destination: {kind}, source: {kind}) -> {kind} {{",
        "replacement": f"const fn {function}(self, destination: {kind}, source: {kind}) -> {kind} {{ return {value};",
        "controls": ((post, "ensures true,"),),
    }
    for identity, function, kind, value, post in (
        ("byte_operator_zero", "apply_byte", "u8", "0u8", BYTE_POST),
        ("byte_operator_ones", "apply_byte", "u8", "u8::MAX", BYTE_POST),
        ("word_operator_zero", "apply_chunk", "u64", "0u64", WORD_POST),
        ("word_operator_ones", "apply_chunk", "u64", "u64::MAX", WORD_POST),
    )
)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--verus-executable", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    try:
        report = check_semantic_cases(
            ROOT, arguments.verus_executable.resolve(), CASES,
            ("src/bitmap.rs", "specs/bitmap.rs", "proofs/shared/bitmap.rs",
             "proofs/production_impl/bitmap.rs"),
            "jbig2-operator-bits-",
            "Every output bit of all five byte/word operators equals the existing Boolean combination model; placement, mask/shift integration and normative translation remain separate",
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"operator bit proof sensitivity failed: {error}", file=sys.stderr)
        report = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Check raw Huffman assignment contracts with compiling individual controls."""

import argparse
import json
from pathlib import Path
import subprocess
import sys

from proof_sensitivity import check_semantic_cases


ROOT = Path(__file__).resolve().parents[1]
RETURN = """Ok(Self {
    lines: owned_lines,
    ordered_lines,
    length_count,
    first_code,
    first_line,
    prefix_lookup,
    prefer_prefix_lookup,
    max_prefix_len,
})"""


def output_case(identity, field, replacement, predicate):
    return {
        "id": identity, "file": "src/huffman.rs", "module": "huffman",
        "function": "HuffmanTable::from_line_source",
        "site": RETURN,
        "replacement": RETURN.replace(f"\n    {field},", f"\n    {field}: {replacement},"),
        "controls": ((f"&& huffman_spec::{predicate}(huffman_spec::raw_lines(lines), &table)", ""),),
    }


CASES = (
    {
        "id": "zero_constructor_sets_low_bit", "file": "src/huffman.rs",
        "module": "huffman", "function": "WideCode::zero",
        "site": "const fn zero() -> Self { Self { limbs: [0; 4] } }",
        "replacement": "const fn zero() -> Self { Self { limbs: [1, 0, 0, 0] } }",
        "controls": ((
            "out.limbs@ == seq![0u64, 0u64, 0u64, 0u64]",
            "out.limbs@[0] <= 1 && out.limbs@[1] == 0 && out.limbs@[2] == 0 && out.limbs@[3] == 0",
        ),),
    },
    output_case(
        "returned_first_code_corrupted", "first_code",
        "{ first_code[usize::from(max_prefix_len)] = WideCode { limbs: [1, 0, 0, 0] }; first_code }",
        "assignment_codes",
    ),
    output_case(
        "returned_bucket_start_corrupted", "first_line",
        "{ first_line[usize::from(max_prefix_len)] = 1; first_line }",
        "assignment_starts",
    ),
    output_case(
        "returned_original_line_order_corrupted", "ordered_lines",
        "{ if !ordered_lines.is_empty() { ordered_lines[0] = 0; } ordered_lines }",
        "assignment_order",
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
            ("src/huffman.rs", "specs/huffman.rs", "proofs/model_support/huffman.rs",
             "proofs/shared/huffman.rs", "proofs/spec_properties/huffman.rs",
             "proofs/production_impl/huffman.rs"),
            "jbig2-huffman-assignment-",
            "Literal zero and exact raw-line canonical code/start/order results; each output control retains content, histogram, extent and the other assignment guarantees. No encoded parser, entropy decoding or normative approval.",
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"Huffman assignment proof sensitivity failed: {error}", file=sys.stderr)
        report = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

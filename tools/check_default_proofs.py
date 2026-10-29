#!/usr/bin/env python3
"""Verify every public default-limit field with isolated strong/weaker controls."""

import argparse
import json
from pathlib import Path
import subprocess
import sys

from proof_sensitivity import check_semantic_cases


ROOT = Path(__file__).resolve().parents[1]
FIELDS = (
    ("max_width", "1_000_000"),
    ("max_height", "1_000_000"),
    ("max_pixels_per_page", "1_000_000_000"),
    ("max_decoder_managed_memory_bytes", "1_073_741_824"),
    ("max_stream_bytes", "1_073_741_824"),
    ("max_pages", "10_000"),
    ("max_segments", "100_000"),
    ("max_symbols", "1_000_000"),
    ("max_references_per_segment", "4_096"),
    ("max_total_references", "1_000_000"),
    ("max_reference_graph_depth", "128"),
    ("max_nesting_depth", "128"),
    ("max_work_units", "2_000_000_000"),
    ("max_cumulative_decoded_bitmap_bytes", "4_294_967_296"),
    ("max_retained_state_bytes", "536_870_912"),
)


def contract(owner, clauses):
    return (
        "#[cfg_attr(verus_keep_ghost, verifier::verify)] "
        f"impl Default for {owner} {{ "
        "#[cfg_attr(verus_keep_ghost, verus_spec(out => ensures "
        + " ".join(clauses) + " ))]"
    )


def cases(owner, module):
    projection = "out" if owner == "Limits" else "document_spec::decoder_limits(&out)"
    clauses = tuple(f"{projection}.{name} == {value}," for name, value in FIELDS)
    original = contract(
        owner,
        clauses if owner == "Limits" else (
            "document_spec::default_limits( document_spec::decoder_limits(&out), ),",
        ),
    )
    site = "fn default() -> Self { Self::project_default() }"
    constructor = "Self" if owner == "Limits" else "Limits"
    result = "limits" if owner == "Limits" else "Self::new(limits)"
    return tuple({
        "id": f"{module}-default-{name}",
        "file": f"src/{module}.rs",
        "module": module,
        "verification_module": module,
        "function": f"{owner}::default",
        "impl_trait": "Default",
        "site": site,
        "replacement": (
            "fn default() -> Self { "
            f"let mut limits = {constructor}::project_default(); "
            f"limits.{name} = 0; {result} }}"
        ),
        "controls": ((original, contract(owner, (
            clause for index, clause in enumerate(clauses) if index != omitted
        ))),),
    } for omitted, (name, _) in enumerate(FIELDS))


LIMITS_CASES = cases("Limits", "limits")
DECODER_CASES = cases("Decoder", "document")
CASE_GROUPS = {
    "default-limits-1": LIMITS_CASES[:3],
    "default-limits-2": LIMITS_CASES[3:6],
    "default-limits-3": LIMITS_CASES[6:9],
    "default-limits-4": LIMITS_CASES[9:12],
    "default-limits-5": LIMITS_CASES[12:],
    "default-decoder-1": DECODER_CASES[:3],
    "default-decoder-2": DECODER_CASES[3:6],
    "default-decoder-3": DECODER_CASES[6:9],
    "default-decoder-4": DECODER_CASES[9:12],
    "default-decoder-5": DECODER_CASES[12:],
}
FILES = ("src/limits.rs", "src/document.rs")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--verus-executable", type=Path, required=True)
    parser.add_argument("--case-group", choices=CASE_GROUPS, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    try:
        report = check_semantic_cases(
            ROOT, arguments.verus_executable.resolve(), CASE_GROUPS[arguments.case_group], FILES,
            "jbig2-default-policy-",
            "Exact public default resource policy; each control omits only its mutated field. "
            "No new standard-library specification or normative/resource-capacity approval.",
        )
        report["case_group"] = arguments.case_group
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"default-policy proof sensitivity failed: {error}", file=sys.stderr)
        report = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

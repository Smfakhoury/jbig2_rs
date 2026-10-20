#!/usr/bin/env python3
"""Check bitmap copying, zero initialization and padding semantic contracts."""

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path

from check_distribution import run
from proof_sensitivity import proof_workspace, replace_once, selected_proof


ROOT = Path(__file__).resolve().parents[1]
ROW_COPY = """if source < destination {
            let (before, after) = self.data.split_at_mut(destination_start);
            copy_row_bytes(&mut after[..self.stride], &before[source_start..source_end]);
        } else {
            let (before, after) = self.data.split_at_mut(source_start);
            copy_row_bytes(
                &mut before[destination_start..destination_end],
                &after[..self.stride],
            );
        }"""
ROW_RESULT = "crate::spec::bitmap::row_copy_result(old(self), final(self), source, destination),"
PREFIX_COPY = """if byte_count == 8 {
            self.data.as_mut_slice()[..8] .copy_from_slice(&source.data.as_slice()[..8]);
        } else {
            copy_row_bytes(
                &mut self.data.as_mut_slice()[..byte_count],
                &source.data.as_slice()[..byte_count],
            );
        }"""
PREFIX_RESULT = """&& crate::spec::bitmap::prefix_copy_result(
                        old(self), final(self), source, byte_count,
                    )"""
PAD_RESULT = """&& forall|index: int| 0 <= index < crate::views::bitmap_data(&bitmap).len() ==>
                crate::views::bitmap_data(&bitmap)[index]
                    == if index < crate::views::bitmap_data(&decoded).len() {
                        crate::views::bitmap_data(&decoded)[index]
                    } else { 0u8 }"""
CASES = (
    {
        "id": "skip_row_copy", "file": "src/bitmap.rs", "module": "bitmap",
        "function": "Bitmap::copy_row", "site": ROW_COPY, "replacement": ";",
        "control": ROW_RESULT,
    },
    {
        "id": "corrupt_other_row", "file": "src/bitmap.rs", "module": "bitmap",
        "function": "Bitmap::copy_row", "site": ROW_COPY,
        "replacement": ROW_COPY + "\n        if destination_start != 0 { self.data[0] ^= 1; }",
        "control": ROW_RESULT,
    },
    {
        "id": "skip_prefix_copy", "file": "src/bitmap.rs", "module": "bitmap",
        "function": "Bitmap::copy_prefix_from", "site": PREFIX_COPY, "replacement": ";",
        "control": PREFIX_RESULT,
    },
    {
        "id": "corrupt_uncopied_suffix", "file": "src/bitmap.rs", "module": "bitmap",
        "function": "Bitmap::copy_prefix_from", "site": PREFIX_COPY,
        "replacement": PREFIX_COPY + "\n        if byte_count < self.data.len() { self.data[byte_count] ^= 1; }",
        "control": PREFIX_RESULT,
    },
    {
        "id": "nonzero_blank_bitmap", "file": "src/bitmap.rs", "module": "bitmap",
        "function": "Bitmap::blank", "site": "Self::filled(width, height, false, limits)",
        "replacement": "Self::filled(width, height, true, limits)",
        "control": "&& crate::spec::bitmap::zeroed_storage(&bitmap)",
    },
    {
        "id": "discard_decoded_rows", "file": "src/generic_region.rs", "module": "generic_region",
        "function": "pad_bitmap_height", "site": "padded.copy_prefix_from(&decoded, byte_count)?;",
        "replacement": ";", "control": PAD_RESULT,
    },
)


def mutated_copy(source: str, case: dict) -> str:
    return replace_once(source, case["site"], case["replacement"])


def weaker_contract(source: str, case: dict) -> str:
    return replace_once(source, case["control"], "")


def check_bitmap_copy_proofs(executable: Path) -> dict:
    files = (
        "src/bitmap.rs", "src/generic_region.rs", "specs/bitmap.rs",
        "proofs/shared/bitmap.rs", "proofs/production_impl/bitmap.rs",
        "proofs/production_impl/generic_region.rs",
    )
    sources = {name: (ROOT / name).read_bytes() for name in files}
    results = []
    with proof_workspace(ROOT, executable, "jbig2-bitmap-proof-") as (
        directory, environment, expected, rust,
    ):
        for case in CASES:
            path = directory / case["file"]
            original = path.read_text(encoding="utf-8")
            baseline = selected_proof(
                executable, directory, environment, False, case["module"], case["function"],
            )
            source = mutated_copy(original, case)
            path.write_text(source, encoding="utf-8")
            run(["cargo", f"+{rust}", "check", "--offline", "--locked", "--lib"],
                directory, environment, capture=True)
            rejected = selected_proof(
                executable, directory, environment, True, case["module"], case["function"],
            )
            path.write_text(weaker_contract(source, case), encoding="utf-8")
            control = selected_proof(
                executable, directory, environment, False, case["module"], case["function"],
            )
            results.append({
                "id": case["id"], "function": case["module"] + "::" + case["function"],
                "normal_rust_compiles": True,
                "strong_contract": "rejected", "weaker_control": "accepted",
                "strong_verifier_errors": rejected["errors"],
                "control_verifier_errors": control["errors"],
                "baseline_selected_verification": baseline,
            })
            path.write_text(original, encoding="utf-8")
    if any((ROOT / name).read_bytes() != data for name, data in sources.items()):
        raise ValueError("production source changed during bitmap sensitivity checks")
    return {
        "schema_version": 2, "status": "passed", "verus_version": expected,
        "scope": "Selected bitmap materialization proofs only; whole-crate verification is separate",
        "production_sources_modified": False, "counterexamples": results,
        "source_bindings": {
            name: hashlib.sha256(data).hexdigest() for name, data in sources.items()
        },
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--verus-executable", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    try:
        result = check_bitmap_copy_proofs(arguments.verus_executable.resolve())
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"bitmap proof sensitivity failed: {error}", file=sys.stderr)
        result = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2))
    return 0 if result["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

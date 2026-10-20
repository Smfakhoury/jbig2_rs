#!/usr/bin/env python3
"""Check semantic sensitivity of the executing row-copy contract."""

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path

from check_distribution import run
from proof_sensitivity import proof_workspace, replace_once, selected_proof


ROOT = Path(__file__).resolve().parents[1]
COPY = """if source < destination {
            let (before, after) = self.data.split_at_mut(destination_start);
            copy_row_bytes(&mut after[..self.stride], &before[source_start..source_end]);
        } else {
            let (before, after) = self.data.split_at_mut(source_start);
            copy_row_bytes(
                &mut before[destination_start..destination_end],
                &after[..self.stride],
            );
        }"""
RESULT = "crate::spec::bitmap::row_copy_result(old(self), final(self), source, destination),"
CASES = (
    ("skip_copy", ";"),
    ("corrupt_other_row", COPY + "\n        if destination_start != 0 { self.data[0] ^= 1; }"),
)


def mutated_copy(source: str, replacement: str) -> str:
    return replace_once(source, COPY, replacement)


def storage_only_contract(source: str) -> str:
    return replace_once(source, RESULT, "")


def check_row_copy_proof(executable: Path) -> dict:
    files = (
        "src/bitmap.rs", "src/generic_region.rs", "specs/bitmap.rs",
        "proofs/shared/bitmap.rs", "proofs/production_impl/bitmap.rs",
        "proofs/production_impl/generic_region.rs",
    )
    sources = {name: (ROOT / name).read_bytes() for name in files}
    results = []
    with proof_workspace(ROOT, executable, "jbig2-row-proof-") as (
        directory, environment, expected, rust,
    ):
        path = directory / "src/bitmap.rs"
        original = path.read_text(encoding="utf-8")
        baseline = selected_proof(
            executable, directory, environment, False, "bitmap", "Bitmap::copy_row",
        )
        for identity, replacement in CASES:
            source = mutated_copy(original, replacement)
            path.write_text(source, encoding="utf-8")
            run(["cargo", f"+{rust}", "check", "--offline", "--locked", "--lib"],
                directory, environment, capture=True)
            rejected = selected_proof(
                executable, directory, environment, True, "bitmap", "Bitmap::copy_row",
            )
            path.write_text(storage_only_contract(source), encoding="utf-8")
            control = selected_proof(
                executable, directory, environment, False, "bitmap", "Bitmap::copy_row",
            )
            results.append({
                "id": identity, "normal_rust_compiles": True,
                "strong_byte_contract": "rejected", "storage_only_control": "accepted",
                "strong_verifier_errors": rejected["errors"],
                "control_verifier_errors": control["errors"],
            })
            path.write_text(original, encoding="utf-8")
    if any((ROOT / name).read_bytes() != data for name, data in sources.items()):
        raise ValueError("production source changed during row-copy sensitivity checks")
    return {
        "schema_version": 1, "status": "passed", "verus_version": expected,
        "scope": "Selected executing row-copy proof only; whole-crate verification is separate",
        "production_sources_modified": False, "counterexamples": results,
        "baseline_selected_verification": baseline,
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
        result = check_row_copy_proof(arguments.verus_executable.resolve())
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"row-copy proof sensitivity failed: {error}", file=sys.stderr)
        result = {"schema_version": 1, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2))
    return 0 if result["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

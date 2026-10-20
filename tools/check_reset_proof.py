#!/usr/bin/env python3
"""Demonstrate semantic strength of the selected batch-reset proof."""

import argparse
import hashlib
import json
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from check_distribution import isolated_environment, run


ROOT = Path(__file__).resolve().parents[1]
FUNCTION = "jbig2_rs_production::arithmetic::reset_contexts"
ASSIGNMENT = "contexts[index] = ArithmeticContext::new();"
ZERO_CLAUSES = (
    """forall|index: int| 0 <= index < final(contexts)@.len() ==>
        final(contexts)@[index].state_index == 0
        && final(contexts)@[index].mps == 0,""",
    """forall|completed: int| 0 <= completed < index ==>
        contexts@[completed].state_index == 0
        && contexts@[completed].mps == 0,""",
)


def mutated_reset(source: str, state_index: int, mps: int) -> str:
    if source.count(ASSIGNMENT) != 1:
        raise ValueError("batch-reset assignment is missing or ambiguous")
    replacement = f"contexts[index] = ArithmeticContext {{ state_index: {state_index}, mps: {mps} }};"
    return source.replace(ASSIGNMENT, replacement, 1)


def validity_only_contract(source: str) -> str:
    for clause in ZERO_CLAUSES:
        pattern = re.compile(r"\s*".join(re.escape(token) for token in clause.split()))
        if len(pattern.findall(source)) != 1:
            raise ValueError("zero-state contract/invariant changed; review sensitivity fixtures")
        source = pattern.sub("", source, count=1)
    return source


def selected_proof(executable: Path, directory: Path, environment: dict, expect_failure: bool):
    process = subprocess.run(
        [
            str(executable), "src/lib.rs", "--crate-name", "jbig2_rs_production",
            "--crate-type", "rlib", "--cfg", "verus_keep_ghost",
            "--verify-only-module", "arithmetic", "--verify-function", "reset_contexts",
            "--num-threads", "1", "--rlimit", "30", "--triggers-mode", "silent", "--output-json",
        ],
        cwd=directory, env=environment, text=True, capture_output=True, check=False, timeout=180,
    )
    try:
        report = json.loads(process.stdout)
    except ValueError as error:
        raise ValueError("verifier did not produce semantic proof evidence:\n" + process.stderr) from error
    result = report["verification-results"]
    if result["is-verifying-entire-crate"] is not False or FUNCTION not in report["func-details"]:
        raise ValueError("selected verification did not target the reset proof")
    if expect_failure:
        if (
            process.returncode == 0 or result["errors"] < 1
            or not any(text in process.stderr for text in (
                "loop invariant not preserved", "postcondition not satisfied",
                "invariant not satisfied at end of loop body",
            ))
        ):
            raise ValueError("nonzero reset was not rejected by semantic obligations:\n" + process.stderr)
    elif process.returncode != 0 or result["errors"] != 0 or result["verified"] < 1:
        raise ValueError("baseline/control proof did not pass:\n" + process.stderr)
    return result


def check_reset_proof(executable: Path) -> dict:
    manifest = json.loads((ROOT / "toolchain.json").read_text())
    expected = manifest["verus"]["version"]
    rust = manifest["verus"]["rust_toolchain"]
    production_bytes = (ROOT / "src/arithmetic.rs").read_bytes()
    with tempfile.TemporaryDirectory(prefix="jbig2-reset-proof-") as temporary:
        directory = Path(temporary).resolve()
        if directory.is_relative_to(ROOT):
            raise ValueError("proof sensitivity checks must run outside the checkout")
        for name in ("src", "specs", "proofs"):
            shutil.copytree(ROOT / name, directory / name)
        for name in ("Cargo.toml", "Cargo.lock", "README.md", "SECURITY.md", "LICENSE"):
            shutil.copyfile(ROOT / name, directory / name)
        environment = isolated_environment(directory)
        environment["RUSTUP_TOOLCHAIN"] = rust
        version = run([str(executable), "--version"], directory, environment, capture=True)
        if f"Version: {expected}" not in version:
            raise ValueError("reset proof sensitivity requires the pinned Verus version")
        baseline = selected_proof(executable, directory, environment, expect_failure=False)
        path = directory / "src/arithmetic.rs"
        original = path.read_text(encoding="utf-8")
        results = []
        for state_index, mps in ((1, 0), (0, 1)):
            source = mutated_reset(original, state_index, mps)
            path.write_text(source, encoding="utf-8")
            run(["cargo", f"+{rust}", "check", "--offline", "--locked", "--lib"],
                directory, environment, capture=True)
            rejected = selected_proof(executable, directory, environment, expect_failure=True)
            path.write_text(validity_only_contract(source), encoding="utf-8")
            control = selected_proof(executable, directory, environment, expect_failure=False)
            results.append({
                "state_index": state_index, "mps": mps,
                "normal_rust_compiles": True,
                "strong_zero_contract": "rejected",
                "validity_only_control": "accepted",
                "strong_verifier_errors": rejected["errors"],
                "control_verifier_errors": control["errors"],
            })
            path.write_text(original, encoding="utf-8")
    if (ROOT / "src/arithmetic.rs").read_bytes() != production_bytes:
        raise ValueError("production source changed during proof sensitivity checks")
    return {
        "schema_version": 1, "status": "passed", "verus_version": expected,
        "scope": "Selected reset proof only; normal whole-crate verification is required separately",
        "production_sources_modified": False, "counterexamples": results,
        "source_sha256": hashlib.sha256(production_bytes).hexdigest(),
        "baseline_selected_verification": baseline,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--verus-executable", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    try:
        result = check_reset_proof(arguments.verus_executable.resolve())
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"reset proof sensitivity failed: {error}", file=sys.stderr)
        result = {"schema_version": 1, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2))
    return 0 if result["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

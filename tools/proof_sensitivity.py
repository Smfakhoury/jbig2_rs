"""Shared isolated, pinned selected-proof sensitivity checks."""

import hashlib
import json
import re
import shutil
import subprocess
import tempfile
from contextlib import contextmanager
from pathlib import Path

from check_distribution import isolated_environment, run


def replace_once(source: str, original: str, replacement: str) -> str:
    pattern = re.compile(r"\s*".join(re.escape(token) for token in original.split()))
    if len(pattern.findall(source)) != 1:
        raise ValueError("proof sensitivity site is missing or ambiguous")
    return pattern.sub(lambda _: replacement, source, count=1)


@contextmanager
def proof_workspace(root: Path, executable: Path, prefix: str):
    manifest = json.loads((root / "toolchain.json").read_text())
    expected = manifest["verus"]["version"]
    rust = manifest["verus"]["rust_toolchain"]
    with tempfile.TemporaryDirectory(prefix=prefix) as temporary:
        directory = Path(temporary).resolve()
        if directory.is_relative_to(root):
            raise ValueError("proof sensitivity checks must run outside the checkout")
        for name in ("src", "specs", "proofs"):
            shutil.copytree(root / name, directory / name)
        for name in ("Cargo.toml", "Cargo.lock", "README.md", "SECURITY.md", "LICENSE"):
            shutil.copyfile(root / name, directory / name)
        environment = isolated_environment(directory)
        environment["RUSTUP_TOOLCHAIN"] = rust
        version = run([str(executable), "--version"], directory, environment, capture=True)
        if f"Version: {expected}" not in version:
            raise ValueError("proof sensitivity requires the pinned Verus version")
        yield directory, environment, expected, rust


def selected_proof(
    executable: Path, directory: Path, environment: dict, expect_failure: bool,
    module: str, function: str,
):
    process = subprocess.run(
        [
            str(executable), "src/lib.rs", "--crate-name", "jbig2_rs_production",
            "--crate-type", "rlib", "--cfg", "verus_keep_ghost",
            "--verify-only-module", module, "--verify-function", function,
            "--num-threads", "1", "--rlimit", "30", "--triggers-mode", "silent", "--output-json",
        ],
        cwd=directory, env=environment, text=True, capture_output=True, check=False, timeout=180,
    )
    try:
        report = json.loads(process.stdout)
    except ValueError as error:
        raise ValueError("verifier did not produce semantic proof evidence:\n" + process.stderr) from error
    result = report["verification-results"]
    identity = f"jbig2_rs_production::{module}::{function}"
    if result["is-verifying-entire-crate"] is not False or identity not in report["func-details"]:
        raise ValueError(f"selected verification did not target {identity}")
    if expect_failure:
        if (
            process.returncode == 0 or result["errors"] < 1
            or not any(text in process.stderr for text in (
                "loop invariant not preserved", "postcondition not satisfied",
                "invariant not satisfied at end of loop body", "loop invariant not satisfied",
            ))
        ):
            raise ValueError("mutant was not rejected by semantic obligations:\n" + process.stderr)
    elif process.returncode != 0 or result["errors"] != 0 or result["verified"] < 1:
        raise ValueError("baseline/control proof did not pass:\n" + process.stderr)
    return result


def weakened_contract(source: str, case: dict) -> str:
    controls = case["controls"] if "controls" in case else ((case["control"], ""),)
    if not controls:
        raise ValueError("semantic counterexample has no weaker control")
    for original, replacement in controls:
        source = replace_once(source, original, replacement)
    return source


def check_semantic_cases(
    root: Path, executable: Path, cases: tuple, files: tuple, prefix: str, scope: str,
) -> dict:
    sources = {name: (root / name).read_bytes() for name in files}
    results = []
    with proof_workspace(root, executable, prefix) as (
        directory, environment, expected, rust,
    ):
        baselines = {}
        for case in cases:
            if case["file"] not in sources:
                raise ValueError("semantic counterexample edits an unbound source")
            path = directory / case["file"]
            original = path.read_text(encoding="utf-8")
            target = (case["module"], case["function"])
            if target not in baselines:
                baselines[target] = selected_proof(
                    executable, directory, environment, False, *target,
                )
            source = replace_once(original, case["site"], case["replacement"])
            path.write_text(source, encoding="utf-8")
            run(["cargo", f"+{rust}", "check", "--offline", "--locked", "--lib"],
                directory, environment, capture=True)
            rejected = selected_proof(executable, directory, environment, True, *target)
            path.write_text(weakened_contract(source, case), encoding="utf-8")
            control = selected_proof(executable, directory, environment, False, *target)
            results.append({
                "id": case["id"], "function": "::".join(target),
                "normal_rust_compiles": True, "strong_contract": "rejected",
                "weaker_control": "accepted", "strong_verifier_errors": rejected["errors"],
                "control_verifier_errors": control["errors"],
                "baseline_selected_verification": baselines[target],
            })
            path.write_text(original, encoding="utf-8")
    if any((root / name).read_bytes() != data for name, data in sources.items()):
        raise ValueError("production source changed during semantic sensitivity checks")
    return {
        "schema_version": 2, "status": "passed", "verus_version": expected,
        "scope": scope, "production_sources_modified": False, "counterexamples": results,
        "source_bindings": {
            name: hashlib.sha256(data).hexdigest() for name, data in sources.items()
        },
    }

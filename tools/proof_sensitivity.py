"""Shared isolated, pinned selected-proof sensitivity checks."""

import json
import shutil
import subprocess
import tempfile
from contextlib import contextmanager
from pathlib import Path

from check_distribution import isolated_environment, run


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

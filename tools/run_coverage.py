#!/usr/bin/env python3
"""Collect pinned Linux runtime coverage and check the reviewed baseline."""

import argparse
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path

from coverage_report import runtime_sources, test_source_hash
from evidence import load_json


ROOT = Path(__file__).resolve().parents[1]


def run(command, environment, capture=False):
    process = subprocess.run(
        command, cwd=ROOT, env=environment, text=True,
        capture_output=capture, check=False,
    )
    if process.returncode and capture:
        print(process.stdout, file=sys.stderr)
        print(process.stderr, file=sys.stderr)
    process.check_returncode()
    return process.stdout.strip() if capture else None


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output-dir", type=Path, default=ROOT / "target/coverage")
    parser.add_argument("--base-ref")
    arguments = parser.parse_args()
    if sys.platform != "linux":
        parser.error("the pinned baseline is Linux-only; native platform tests run separately")
    output = arguments.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    try:
        for name in (
            "raw.json", "raw.lcov", "runtime.json", "runtime.md", "collection.json",
            "failure.json", "mutations.json",
        ):
            (output / name).unlink(missing_ok=True)
        configuration = load_json(ROOT / "toolchain.json")["coverage"]
        cargo = ["cargo", f"+{configuration['rust_toolchain']}", "llvm-cov"]
        environment = {
            key: value for key, value in os.environ.items()
            if key not in {
                "RUSTFLAGS", "RUSTDOCFLAGS", "CARGO_ENCODED_RUSTFLAGS",
                "CARGO_ENCODED_RUSTDOCFLAGS", "RUSTUP_TOOLCHAIN", "RUSTC", "RUSTDOC",
                "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "CARGO_BUILD_RUSTC",
                "CARGO_BUILD_RUSTC_WRAPPER", "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
                "CARGO_BUILD_RUSTDOC", "CARGO_BUILD_RUSTFLAGS", "CARGO_BUILD_TARGET",
                "CARGO_TARGET_DIR", "LLVM_COV", "LLVM_PROFDATA", "LLVM_COV_FLAGS",
                "LLVM_PROFDATA_FLAGS", "CARGO_LLVM_COV_FLAGS", "CARGO_LLVM_PROFDATA_FLAGS",
            } and not key.startswith("CARGO_LLVM_COV_")
            and not (key.startswith("CARGO_TARGET_") and key.endswith("_RUSTFLAGS"))
        }
        environment["CARGO_LLVM_COV_TARGET_DIR"] = str(output / "build")
        version = run(cargo + ["--version"], environment, capture=True)
        if version != f"cargo-llvm-cov {configuration['cargo_llvm_cov_version']}":
            raise ValueError(f"unexpected coverage tool version: {version}")
        _, source_hash = runtime_sources(ROOT)
        tests_hash = test_source_hash(ROOT)
        run(cargo + [
            "--branch", "--tests", "--all-features", "--json",
            "--output-path", str(output / "raw.json"),
        ], environment)
        run(cargo + [
            "report", "--branch", "--lcov", "--output-path", str(output / "raw.lcov"),
        ], environment)
        run(cargo + [
            "report", "--branch", "--html", "--output-dir", str(output),
        ], environment)
        if runtime_sources(ROOT)[1] != source_hash:
            raise ValueError("source changed during coverage collection")
        if test_source_hash(ROOT) != tests_hash:
            raise ValueError("test sources changed during coverage collection")
        collection = {
            "schema_version": 1,
            "source_revision": run(["git", "rev-parse", "HEAD"], environment, True),
            "source_sha256": source_hash,
            "test_source_sha256": tests_hash,
            "toolchain": configuration,
            "rustc": run(["rustc", f"+{configuration['rust_toolchain']}", "--version"], environment, True),
            "test_scope": "unit_and_integration_tests_no_ignored_microbenchmarks",
            "report_sha256": {
                "json": hashlib.sha256((output / "raw.json").read_bytes()).hexdigest(),
                "lcov": hashlib.sha256((output / "raw.lcov").read_bytes()).hexdigest(),
            },
        }
        metadata = output / "collection.json"
        metadata.write_text(json.dumps(collection, indent=2) + "\n", encoding="utf-8")
        command = [
            sys.executable, "-B", str(ROOT / "tools/coverage_report.py"),
            "--json", str(output / "raw.json"), "--lcov", str(output / "raw.lcov"),
            "--collection", str(metadata), "--baseline",
            str(ROOT / "validation/coverage/baseline.json"),
            "--output", str(output / "runtime.json"),
        ]
        if arguments.base_ref:
            command += ["--base-ref", arguments.base_ref]
        run(command, environment)
        return 0
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        print(f"coverage collection failed: {error}", file=sys.stderr)
        (output / "failure.json").write_text(json.dumps({
            "schema_version": 1, "status": "failed", "error": str(error),
        }, indent=2) + "\n", encoding="utf-8")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())

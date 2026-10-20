#!/usr/bin/env python3
"""Check that critical public-API regressions detect isolated semantic mutations."""

import argparse
import json
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from check_distribution import isolated_environment, run


ROOT = Path(__file__).resolve().parents[1]
MUTATIONS = (
    {
        "id": "io-kind-misclassification",
        "file": "src/error.rs",
        "before": "io::ErrorKind::PermissionDenied => Self::PermissionDenied,",
        "after": "io::ErrorKind::PermissionDenied => Self::Other,",
        "target": "error_contract",
        "test": "io_error_conversion_preserves_public_kind_without_retaining_private_message",
    },
    {
        "id": "reader-inclusive-limit-off-by-one",
        "file": "src/document.rs",
        "before": "if new_length_u64 > limits.max_stream_bytes {",
        "after": "if new_length_u64 >= limits.max_stream_bytes {",
        "target": "public_api",
        "test": "stream_byte_limit_is_inclusive_for_slice_and_reader_apis",
    },
    {
        "id": "swallowed-page-sink-error",
        "file": "src/decode.rs",
        "before": "    sink.write_page(page)\n",
        "after": "    let _ = sink.write_page(page);\n    Ok(())\n",
        "target": "public_api",
        "test": "sink_failure_is_preserved_and_does_not_poison_the_decoder",
    },
)


def mutate(source: str, mutation: dict) -> str:
    if source.count(mutation["before"]) != 1:
        raise ValueError(f"mutation site is missing or ambiguous: {mutation['id']}")
    return source.replace(mutation["before"], mutation["after"], 1)


def check_mutations(toolchain: str) -> dict:
    results = []
    cargo = ["cargo", f"+{toolchain}"]
    with tempfile.TemporaryDirectory(prefix="jbig2-contract-mutations-") as temporary:
        directory = Path(temporary).resolve()
        if directory.is_relative_to(ROOT):
            raise ValueError("mutation checks must run outside the checkout")
        for name in ("src", "tests"):
            shutil.copytree(ROOT / name, directory / name)
        (directory / "tools").mkdir()
        shutil.copyfile(ROOT / "tools/distribution_consumer.rs",
                        directory / "tools/distribution_consumer.rs")
        for name in ("Cargo.toml", "Cargo.lock", "README.md", "SECURITY.md", "LICENSE"):
            shutil.copyfile(ROOT / name, directory / name)
        environment = isolated_environment(directory)
        for mutation in MUTATIONS:
            command = cargo + [
                "test", "--offline", "--locked", "--test", mutation["target"],
                mutation["test"], "--", "--exact",
            ]
            baseline = run(command, directory, environment, capture=True)
            if not re.search(r"test result: ok\. 1 passed; 0 failed;", baseline):
                raise ValueError(f"baseline did not run exactly one passing test: {mutation['id']}")
            path = directory / mutation["file"]
            original = path.read_bytes()
            try:
                path.write_text(mutate(original.decode("utf-8"), mutation), encoding="utf-8")
                run(cargo + [
                    "test", "--offline", "--locked", "--test",
                    mutation["target"], "--no-run",
                ], directory, environment, capture=True)
                process = subprocess.run(
                    command, cwd=directory, env=environment,
                    capture_output=True, text=True, check=False,
                )
                if process.returncode == 0 or not re.search(
                    r"test result: FAILED\. 0 passed; 1 failed;", process.stdout
                ):
                    raise ValueError(
                        f"mutation survived or failed for a non-test reason: {mutation['id']}\n"
                        + process.stdout + process.stderr
                    )
                results.append({
                    "id": mutation["id"], "test_target": mutation["target"],
                    "test_name": mutation["test"], "baseline": "passed",
                    "mutant_compiles": True, "mutant": "detected_by_failing_test",
                })
            finally:
                path.write_bytes(original)
    return {
        "schema_version": 1, "status": "passed", "toolchain": toolchain,
        "scope": "Three targeted API contracts, not exhaustive codec mutation coverage",
        "runtime_source_modified": False, "mutations": results,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--toolchain", default="1.89.0")
    parser.add_argument("--output", type=Path)
    arguments = parser.parse_args()
    try:
        result = check_mutations(arguments.toolchain)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"contract mutation validation failed: {error}", file=sys.stderr)
        result = {"schema_version": 1, "status": "failed", "error": str(error)}
    text = json.dumps(result, indent=2) + "\n"
    if arguments.output:
        arguments.output.parent.mkdir(parents=True, exist_ok=True)
        arguments.output.write_text(text, encoding="utf-8")
    print(text, end="")
    return 0 if result["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Build once, then run the native fixed-ID regression with existing LLVM coverage."""

import json
import os
from pathlib import Path
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target" / "reproduce-fixed-dictionary-ids"
TEST = (
    "symbol_text::tests::dictionary_dispatch::"
    "huffman_dictionary_aggregation_preserves_standard_fixed_symbol_ids"
)
CARGO = ["cargo", "+nightly-2026-10-12", "llvm-cov"]


def main():
    os.chdir(ROOT)
    environment = os.environ.copy()
    environment["CARGO_LLVM_COV_TARGET_DIR"] = str(TARGET)
    environment["LLVM_PROFILE_FILE"] = str(TARGET / "fixed-ids-%p.profraw")
    source = (ROOT / "src/symbol_text/tests/dictionary_dispatch.rs").read_text()
    for guard in (
        "fn huffman_dictionary_aggregation_preserves_standard_fixed_symbol_ids()",
        "RFC_FIXED_DICTIONARY_IDS",
        "assert_eq!(checked, 119)",
        "fn single_instance_refinement_reaches_the_existing_raw_callback()",
        "fn direct_coded_symbol_rejects_an_aggregation_count_before_decoding()",
        "fn refinement_still_rejects_missing_and_zero_instance_counts()",
    ):
        if guard not in source:
            raise RuntimeError(f"replay source is missing {guard!r}")

    if sys.argv[1:] == ["build"]:
        result = subprocess.run(
            CARGO + ["--branch", "--no-report", "--lib", "--", TEST, "--exact", "--list"],
            env=environment, text=True, stdout=subprocess.PIPE, check=True,
        )
        print(result.stdout, end="")
        if f"{TEST}: test" not in result.stdout:
            raise RuntimeError("native test registration is absent")
        return 0
    if sys.argv[1:] != ["run"]:
        raise RuntimeError("expected build or run")

    binaries = [
        path for path in TARGET.rglob("jbig2_decoder-*")
        if path.is_file() and os.access(path, os.X_OK) and not path.suffix
    ]
    if len(binaries) != 1:
        raise RuntimeError(f"expected exactly one prebuilt native test binary: {binaries}")
    result = subprocess.run(
        [str(binaries[0]), TEST, "--exact", "--nocapture"], env=environment, check=False,
    )
    report = TARGET / "fixed-ids-coverage.json"
    subprocess.run(
        CARGO + ["report", "--branch", "--json", "--output-path", str(report)],
        env=environment, check=True,
    )
    coverage = json.loads(report.read_text())
    for unit in coverage["data"]:
        for file in unit["files"]:
            if file["filename"].endswith("/src/symbol_text/dictionary/huffman.rs"):
                hit_lines = sorted({
                    segment[0] for segment in file["segments"]
                    if segment[2] > 0 and segment[3]
                })
                print("SUBJECT_COVERAGE", json.dumps({
                    "path": file["filename"],
                    "hit_segment_lines": hit_lines,
                    "branches": file.get("branches", []),
                }), flush=True)
    return result.returncode


if __name__ == "__main__":
    sys.exit(main())

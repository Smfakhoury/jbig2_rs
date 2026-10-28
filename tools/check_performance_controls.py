#!/usr/bin/env python3
"""Check real slowdown, work-quota regression and exact-output failure detection."""

import argparse
import hashlib
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from types import SimpleNamespace

from run_performance import HARNESS_FILES, ROOT, comparison
from source_group import SourceGroup, bound_source_groups


def options(base, output, enforce=False):
    return SimpleNamespace(
        base_ref=base, output_dir=output, iterations=20, pairs=3,
        case="white-64", cpu=None, enforce_timing=enforce,
    )


def check_controls(output):
    output.mkdir(parents=True, exist_ok=False)
    _, originals, _ = bound_source_groups(ROOT, ("src/decode.rs", "src/bitmap.rs"))
    with tempfile.TemporaryDirectory(prefix="jbig2-perf-control-") as temporary:
        fixture = Path(temporary) / "source"
        subprocess.run(
            ["git", "clone", "--quiet", "--no-local", "--single-branch", str(ROOT), str(fixture)],
            check=True,
        )
        for name in HARNESS_FILES:
            path = fixture / name
            path.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / name, path)
        base = subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=fixture, text=True, capture_output=True, check=True,
        ).stdout.strip()
        decode = SourceGroup(fixture, "src/decode.rs")
        original = decode.text
        site = "        self.context().decode(input)"
        if original.count(site) != 1:
            raise ValueError("decoder slowdown control site is missing or ambiguous")
        decode.write(fixture, original.replace(
            site,
            "        std::thread::sleep(std::time::Duration::from_millis(25));\n" + site,
            1,
        ))
        slowdown = comparison(options(base, output / "slowdown", True), fixture)
        if (
            slowdown.get("correctness") != "exact_generated_outputs_and_typed_error_boundaries_passed"
            or slowdown["status"] == "passed" or not slowdown.get("cases")
            or slowdown["cases"][0]["median_ratio"] <= 1.2
        ):
            raise ValueError("real decoder slowdown was not observed and blocked by timing enforcement")
        decode.restore(fixture)
        site = "    structural_work_totals(input, segment_count, references)\n"
        if original.count(site) != 1:
            raise ValueError("work-quota control site is missing or ambiguous")
        decode.write(fixture, original.replace(
            site,
            "    checked_work_add(structural_work_totals(input, segment_count, references)?, 1)\n",
            1,
        ))
        work = comparison(options(base, output / "work-quota"), fixture)
        if (
            work.get("correctness") != "exact_generated_outputs_and_typed_error_boundaries_passed"
            or work["status"] != "failed" or work.get("resource_gate") != "failed"
            or work.get("timing_gate") != "not_requested"
            or len(work.get("cases", [])) != 1
            or work["cases"][0]["head_minimum_work_limit"]
                != work["cases"][0]["base_minimum_work_limit"] + 1
        ):
            raise ValueError("one-unit work regression was not blocked independently of timing")
        decode.restore(fixture)
        restored = comparison(options(base, output / "restored-work"), fixture)
        if restored["status"] != "passed" or restored.get("resource_gate") != "passed":
            raise ValueError("restored decoder did not pass the work-quota weaker control")
        bitmap = SourceGroup(fixture, "src/bitmap.rs")
        contents = bitmap.text
        site = "        &self.data\n    }"
        if contents.count(site) != 1:
            raise ValueError("bitmap shape control site is missing or ambiguous")
        bitmap.write(fixture, contents.replace(site, "        &self.data[1..]\n    }", 1))
        corrupted = comparison(options(base, output / "output"), fixture)
        if corrupted["status"] != "failed" or "correctness" in corrupted:
            raise ValueError("truncated packed output was not rejected before successful timing")
        log = (output / "output/head-0.json").read_text()
        if "exact output mismatch" not in log:
            raise ValueError("output control failed for an unrelated reason")
    for name, expected in originals.items():
        if (ROOT / name).read_bytes() != expected:
            raise ValueError("production source changed during performance controls")
    return {
        "schema_version": 2, "status": "passed", "production_sources_modified": False,
        "controlled_decoder_slowdown": "observed_and_not_accepted",
        "slowdown_timing_gate": slowdown["timing_gate"],
        "exact_output_corruption": "rejected",
        "one_unit_work_quota_regression": "rejected_without_timing_enforcement",
        "restored_work_quota_control": "accepted",
        "source_bindings": {
            name: hashlib.sha256(contents).hexdigest() for name, contents in originals.items()
        },
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output-dir", type=Path, required=True)
    arguments = parser.parse_args()
    output = arguments.output_dir.resolve()
    if output.exists():
        parser.error("performance controls require a fresh output directory")
    try:
        report = check_controls(output)
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"performance control failed: {error}", file=sys.stderr)
        report = {"schema_version": 1, "status": "failed", "error": str(error)}
    output.mkdir(parents=True, exist_ok=True)
    (output / "control-report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

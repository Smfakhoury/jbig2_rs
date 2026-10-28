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


def options(base, output, enforce=False, measure_rss=False):
    return SimpleNamespace(
        base_ref=base, output_dir=output, iterations=20, pairs=3,
        case="white-64", cpu=None, enforce_timing=enforce, measure_rss=measure_rss,
    )


def check_controls(output, measure_rss=False):
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
        memory = None
        if measure_rss:
            site = "        self.context().decode(input)"
            if original.count(site) != 1:
                raise ValueError("RSS inflation control site is missing or ambiguous")
            decode.write(fixture, original.replace(
                site,
                "        let resident_probe = vec![0xA5_u8; 64 * 1024 * 1024];\n"
                "        std::hint::black_box(&resident_probe);\n"
                "        let result = self.context().decode(input);\n"
                "        std::hint::black_box(resident_probe);\n"
                "        result",
                1,
            ))
            memory = comparison(options(base, output / "rss-inflation", measure_rss=True), fixture)
            if (
                memory["status"] != "passed" or memory.get("resource_gate") != "passed"
                or memory["rss"]["status"] != "observed"
                or memory["rss"]["policy"] != "informational_only"
                or memory["rss"]["cases"][0]["head_median_kib"]
                    - memory["rss"]["cases"][0]["base_median_kib"] < 32 * 1024
            ):
                raise ValueError("real 64 MiB inflation was not observed independently of release gating")
            decode.restore(fixture)
        launcher_probe = None
        if measure_rss:
            # A child must not inherit the launcher's high-water RSS as its measurement.
            launcher_probe = bytearray(64 * 1024 * 1024)
            launcher_probe[::4096] = b"\xA5" * (len(launcher_probe) // 4096)
        restored = comparison(options(base, output / "restored-work", measure_rss=measure_rss), fixture)
        del launcher_probe
        if restored["status"] != "passed" or restored.get("resource_gate") != "passed":
            raise ValueError("restored decoder did not pass the work-quota weaker control")
        if measure_rss and (
            restored["rss"]["status"] != "observed"
            or memory["rss"]["cases"][0]["head_median_kib"]
                - restored["rss"]["cases"][0]["head_median_kib"] < 32 * 1024
            or abs(restored["rss"]["cases"][0]["base_median_kib"]
                   - memory["rss"]["cases"][0]["base_median_kib"]) >= 32 * 1024
        ):
            raise ValueError("restored RSS retained decoder inflation or inherited launcher high-water")
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
        "schema_version": 3, "status": "passed", "production_sources_modified": False,
        "controlled_decoder_slowdown": "observed_and_not_accepted",
        "slowdown_timing_gate": slowdown["timing_gate"],
        "exact_output_corruption": "rejected",
        "one_unit_work_quota_regression": "rejected_without_timing_enforcement",
        "restored_work_quota_control": "accepted",
        "rss_control": {
            "status": "observed" if measure_rss else "not_requested",
            "real_64_mib_inflation_observed_and_restored": measure_rss,
            "inflated_launcher_highwater_excluded": measure_rss,
            "release_memory_gate_enforced": False,
        },
        "source_bindings": {
            name: hashlib.sha256(contents).hexdigest() for name, contents in originals.items()
        },
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--measure-rss", action="store_true")
    arguments = parser.parse_args()
    output = arguments.output_dir.resolve()
    if output.exists():
        parser.error("performance controls require a fresh output directory")
    try:
        report = check_controls(output, measure_rss=arguments.measure_rss)
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"performance control failed: {error}", file=sys.stderr)
        report = {"schema_version": 3, "status": "failed", "error": str(error)}
    output.mkdir(parents=True, exist_ok=True)
    (output / "control-report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

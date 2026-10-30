#!/usr/bin/env python3
"""Observe allocator-requested payloads separately from RSS and managed counters."""

import argparse
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import tomllib

from evidence import load_json
from run_performance import (
    CASES, ROOT, build_library, digest, environment, execute, library_name,
    resolve_base, runtime_identity,
)


FILES = (
    "benchmarks/allocation-probe/Cargo.toml", "benchmarks/allocation-probe/Cargo.lock",
    "benchmarks/allocation-probe/main.rs", "benchmarks/cases.rs", "benchmarks/generated.rs",
    "benchmarks/allocation-probe/phases.rs",
    "tools/run_allocation.py", "tools/run_performance.py", "tools/audit_package.py",
    "config/toolchain.json",
)
COUNTERS = (
    "tracked_allocations_total", "peak_live_allocations", "final_live_allocations",
    "total_requested_payload_bytes", "peak_requested_payload_bytes",
    "final_requested_payload_bytes",
)
INFLATION = 64 * 1024
PHASES = ("decode", "output_validation", "output_drop")
PHASE_COUNTERS = (
    "allocations_total", "live_allocations_delta", "live_allocations_peak",
    "requested_payload_bytes_total", "live_requested_payload_bytes_delta",
    "live_requested_payload_bytes_peak",
)
PHASE_NET = ("live_allocations_delta", "live_requested_payload_bytes_delta")


def validate_phase_counters(row):
    if not isinstance(row, dict) or set(row) != set(PHASE_COUNTERS):
        raise ValueError("phase counter has missing or unknown fields")
    for field in PHASE_COUNTERS:
        lower, upper = (-2**63, 2**63) if field in PHASE_NET else (0, 2**64)
        if type(row[field]) is not int or not lower <= row[field] < upper:
            raise ValueError("invalid phase counter type or range")
    if (
        row["live_allocations_peak"] > row["allocations_total"]
        or row["live_requested_payload_bytes_peak"] > row["requested_payload_bytes_total"]
        or row["live_allocations_delta"] > row["live_allocations_peak"]
        or row["live_requested_payload_bytes_delta"] > row["live_requested_payload_bytes_peak"]
    ):
        raise ValueError("invalid phase-local allocation peak")


def validate_phase_sample(row, case, inflation=0):
    if not isinstance(row, dict) or set(row) != {
        "schema_version", "id", "outcome", "input_bytes", "output_bytes",
        "exact_output_checked", "inflation_phase", "inflation_bytes", "decode_scope",
        "phase_peaks_additive", "measurement_windows_nested", "decode_executions",
        "inspection_executions", "inspection_scope", "inspection_not_additive_with_decode",
        "inspection", "overall", "phases",
    }:
        raise ValueError("phase sample has missing or unknown fields")
    if (
        type(row["schema_version"]) is not int or row["schema_version"] != 2
        or row["id"] != case or case not in CASES or row["exact_output_checked"] is not True
        or row["decode_scope"] != "public_decode_includes_envelope_parsing"
        or row["inspection_scope"] != "public_inspect_envelope_metadata_validation_and_drop_no_entropy"
        or row["phase_peaks_additive"] is not False
        or row["measurement_windows_nested"] is not False
        or row["inspection_not_additive_with_decode"] is not True
        or type(row["decode_executions"]) is not int or row["decode_executions"] != 2
        or type(row["inspection_executions"]) is not int or row["inspection_executions"] != 1
        or row["inflation_phase"] != ("output_validation" if inflation else None)
        or row["inflation_bytes"] != inflation
        or any(type(row[name]) is not int or not 0 <= row[name] < 2**64
               for name in ("input_bytes", "output_bytes", "inflation_bytes"))
        or row["input_bytes"] == 0
    ):
        raise ValueError("invalid phase identity, scope, execution or correctness evidence")
    expected = "work-limit" if case == "work-boundary" else (
        "eof" if case == "late-truncation" else "decoded"
    )
    if row["outcome"] != expected or (row["output_bytes"] > 0) != (expected == "decoded"):
        raise ValueError("phase sample changed the expected decode outcome")
    if not isinstance(row["phases"], dict) or set(row["phases"]) != set(PHASES):
        raise ValueError("missing or unknown measurement phases")
    for counters in [row["overall"], row["inspection"], *row["phases"].values()]:
        validate_phase_counters(counters)
    for name in ("overall", "inspection"):
        if any(row[name][field] != 0 for field in PHASE_NET):
            raise ValueError("complete measurement window retained tracked allocations")
    for total, net, peak in (
        ("allocations_total", "live_allocations_delta", "live_allocations_peak"),
        ("requested_payload_bytes_total", "live_requested_payload_bytes_delta",
         "live_requested_payload_bytes_peak"),
    ):
        allocated = live = high = 0
        for phase in PHASES:
            counters = row["phases"][phase]
            allocated += counters[total]
            high = max(high, live + counters[peak])
            live += counters[net]
            if live < 0:
                raise ValueError("phase released memory outside the measured decode")
        if (allocated, live, high) != tuple(row["overall"][field] for field in (total, net, peak)):
            raise ValueError("phase lifetime accounting does not reconstruct the independent flat replay")
    return row


def validate_phase_control(baseline, inflated, restored):
    validate_phase_sample(baseline, "white-64")
    validate_phase_sample(inflated, "white-64", INFLATION)
    validate_phase_sample(restored, "white-64")
    for field in ("input_bytes", "output_bytes", "outcome", "inspection"):
        if baseline[field] != inflated[field] or baseline[field] != restored[field]:
            raise ValueError("validation inflation changed input, output or inspection")
    for phase in ("decode", "output_drop"):
        if baseline["phases"][phase] != inflated["phases"][phase]:
            raise ValueError("validation inflation leaked into another phase")
    before, after = (row["phases"]["output_validation"] for row in (baseline, inflated))
    for field in PHASE_COUNTERS:
        difference = 0 if field in PHASE_NET else (
            1 if field in ("allocations_total", "live_allocations_peak") else INFLATION
        )
        if after[field] != before[field] + difference:
            raise ValueError("validation-local allocation control was not observed exactly")
    if baseline != restored:
        raise ValueError("restored phase measurement differs from its original sample")


def validate_phase_matches_combined(phase, combined):
    for field in ("id", "outcome", "input_bytes", "output_bytes", "exact_output_checked"):
        if phase[field] != combined[field]:
            raise ValueError("phase replay changed the combined driver's result")
    for phase_field, combined_field in zip(PHASE_COUNTERS, (
        "tracked_allocations_total", "final_live_allocations", "peak_live_allocations",
        "total_requested_payload_bytes", "final_requested_payload_bytes",
        "peak_requested_payload_bytes",
    )):
        if phase["overall"][phase_field] != combined[combined_field]:
            raise ValueError("phase flat replay differs from the original combined measurement")


def validate_sample(row, case, inflation=0):
    if set(row) != {
        "schema_version", "id", "outcome", "input_bytes", "output_bytes",
        "exact_output_checked", "inflation_bytes", *COUNTERS,
    }:
        raise ValueError("allocator sample has missing or unknown fields")
    if (
        type(row["schema_version"]) is not int or row["schema_version"] != 1
        or row["id"] != case or case not in CASES
        or row["exact_output_checked"] is not True or row["inflation_bytes"] != inflation
        or any(type(row[name]) is not int or not 0 <= row[name] < 2**64
               for name in (*COUNTERS, "input_bytes", "output_bytes", "inflation_bytes"))
        or row["input_bytes"] == 0
    ):
        raise ValueError("allocator sample identity, correctness or counters are invalid")
    expected = "work-limit" if case == "work-boundary" else (
        "eof" if case == "late-truncation" else "decoded"
    )
    if row["outcome"] != expected or (row["output_bytes"] > 0) != (expected == "decoded"):
        raise ValueError("allocator sample does not have the exact expected outcome")
    if (
        row["peak_live_allocations"] > row["tracked_allocations_total"]
        or row["peak_requested_payload_bytes"] > row["total_requested_payload_bytes"]
        or row["final_live_allocations"] != 0 or row["final_requested_payload_bytes"] != 0
    ):
        raise ValueError("impossible peak or unreleased allocator payload")
    return row


def validate_control(baseline, inflated, restored):
    validate_sample(baseline, "white-64")
    validate_sample(inflated, "white-64", INFLATION)
    validate_sample(restored, "white-64")
    for field in ("input_bytes", "output_bytes", "outcome"):
        if baseline[field] != inflated[field] or baseline[field] != restored[field]:
            raise ValueError("allocator inflation changed the exact decode outcome")
    for field, difference in (
        ("tracked_allocations_total", 1), ("peak_live_allocations", 1),
        ("total_requested_payload_bytes", INFLATION),
        ("peak_requested_payload_bytes", INFLATION),
    ):
        if inflated[field] != baseline[field] + difference:
            raise ValueError(f"real live inflation not observed exactly: {field}")
    if any(restored[field] != baseline[field] for field in COUNTERS):
        raise ValueError("restored allocator measurement retained the inflation")


def comparison(base_ref, output, root=ROOT, measure_phases=False):
    output.mkdir(parents=True, exist_ok=False)
    if sys.platform != "linux":
        raise ValueError("the observed allocator lane is currently Linux-only")
    rust = load_json(root / "config/toolchain.json")["cargo_rust_toolchain"]
    base, selection = resolve_base(base_ref, root)
    harness = digest(root, FILES)
    runtime = runtime_identity(root)
    env = environment()
    probe = root / "benchmarks/allocation-probe"
    lock = tomllib.loads((probe / "Cargo.lock").read_text())
    dependencies = [row for row in lock["package"] if "source" in row]
    if (
        len(dependencies) != 1 or dependencies[0]["name"] != "allocation-counter"
        or dependencies[0]["version"] != "0.8.1"
        or dependencies[0]["source"] != "registry+https://github.com/rust-lang/crates.io-index"
        or len(dependencies[0]["checksum"]) != 64
    ):
        raise ValueError("allocator probe must retain its one exact locked tool dependency")
    execute(
        ["cargo", "+" + rust, "fetch", "--locked", "--manifest-path", str(probe / "Cargo.toml")],
        root, env, output / "tool-fetch.log", 180,
    )
    tool_build = output / "tool-build"
    execute(
        ["cargo", "+" + rust, "build", "--offline", "--locked", "--release",
         "--manifest-path", str(probe / "Cargo.toml"), "--target-dir", str(tool_build)],
        root, env, output / "tool-build.log", 600,
    )
    counter_libraries = list((tool_build / "release/deps").glob("liballocation_counter-*.rlib"))
    if len(counter_libraries) != 1:
        raise ValueError("allocator tool build did not produce exactly one pinned counter library")
    binaries = {}
    library_names = {}
    with tempfile.TemporaryDirectory(prefix="jbig2-allocation-base-") as temporary:
        base_root = Path(temporary) / "source"
        base_root.mkdir()
        archive = Path(temporary) / "base.tar"
        with archive.open("wb") as stream:
            subprocess.run(
                ["git", "archive", base], cwd=root, stdout=stream, check=True, timeout=60,
            )
        with tarfile.open(archive) as stream:
            stream.extractall(base_root, filter="data")
        base_runtime = runtime_identity(base_root)
        for side, source in (("base", base_root), ("head", root)):
            library_names[side] = library_name(source)
            library = build_library(
                source, output / (side + "-build"), rust, root, env, output / (side + "-build.log"),
            )
            binary = output / ("allocation-" + side)
            execute(
                ["rustc", "+" + rust, "--edition=2021", "-O", str(probe / "main.rs"),
                 "--crate-name", "jbig2_allocation_probe",
                 "--extern", f"jbig2_decoder={library}",
                 "--extern", f"allocation_counter={counter_libraries[0]}",
                 "-L", f"dependency={tool_build / 'release/deps'}", "-o", str(binary)],
                root, env, output / (side + "-driver.log"), 120,
            )
            binaries[side] = binary
        measurements = {side: {case: [] for case in CASES} for side in binaries}

        def sample(side, case, name, inflate=False):
            path = output / (name + ".json")
            execute(
                [str(binaries[side]), case, *(["--inflate"] if inflate else [])],
                root, env, path, 60,
            )
            return validate_sample(load_json(path), case, INFLATION if inflate else 0)

        for repeat in range(3):
            for case in CASES:
                for side in binaries:
                    measurements[side][case].append(sample(side, case, f"{side}-{repeat}-{case}"))
        controls = []
        for repeat in range(3):
            inflated = sample("head", "white-64", f"inflated-{repeat}-white-64", True)
            restored = sample("head", "white-64", f"restored-{repeat}-white-64")
            validate_control(measurements["head"]["white-64"][repeat], inflated, restored)
            controls.append({"inflated": inflated, "restored": restored})
        phase_report = None
        if measure_phases:
            phase_dir = output / "phases"
            phase_dir.mkdir()
            phase_measurements = {side: {case: [] for case in CASES} for side in binaries}

            def phase_sample(side, case, name, inflate=False):
                path = phase_dir / (name + ".json")
                execute(
                    [str(binaries[side]), case, "--phases",
                     *(["--inflate-validation"] if inflate else [])],
                    root, env, path, 60,
                )
                return validate_phase_sample(load_json(path), case, INFLATION if inflate else 0)

            for repeat in range(3):
                for case in CASES:
                    for side in binaries:
                        measured = phase_sample(side, case, f"{side}-{repeat}-{case}")
                        validate_phase_matches_combined(measured, measurements[side][case][repeat])
                        phase_measurements[side][case].append(measured)
            phase_controls = []
            for repeat in range(3):
                inflated = phase_sample("head", "white-64", f"inflated-{repeat}-white-64", True)
                restored = phase_sample("head", "white-64", f"restored-{repeat}-white-64")
                validate_phase_control(phase_measurements["head"]["white-64"][repeat], inflated, restored)
                phase_controls.append({"inflated": inflated, "restored": restored})
            phase_rows = []
            for case in CASES:
                for side in binaries:
                    samples = phase_measurements[side][case]
                    if any(row != samples[0] for row in samples):
                        raise ValueError(f"phase measurements are not repeatable: {side}/{case}")
                before, after = (phase_measurements[side][case][0] for side in ("base", "head"))
                for field in ("id", "outcome", "input_bytes", "output_bytes", "exact_output_checked"):
                    if before[field] != after[field]:
                        raise ValueError("base/head phase observations changed exact output")
                phase_rows.append({"id": case, "base": before, "head": after})
            phase_report = {
                "schema_version": 1, "status": "observed_with_independent_flat_replay",
                "raw_paired_samples": 48, "validation_inflation_restoration_pairs": 3,
                "non_nested_windows": True, "phase_peaks_additive": False,
                "inspection_is_separate_overlapping_workload": True,
                "cases": phase_rows, "controls": phase_controls,
            }
    rows = []
    for case in CASES:
        for side in measurements:
            if any(row != measurements[side][case][0] for row in measurements[side][case]):
                raise ValueError(f"allocator counters are not repeatable: {side}/{case}")
        before, after = (measurements[side][case][0] for side in ("base", "head"))
        if any(before[field] != after[field] for field in (
            "input_bytes", "output_bytes", "outcome", "exact_output_checked",
        )):
            raise ValueError("base/head allocator runs disagree on exact decode results")
        rows.append({"id": case, "base": before, "head": after})
    if digest(root, FILES) != harness or runtime_identity(root) != runtime:
        raise ValueError("source changed during allocator measurement")
    result = {
        "schema_version": 1, "status": "observed_with_exact_controls",
        "policy": "informational_only_no_release_memory_threshold",
        "measurement": "current_thread_global_allocator_requested_layout_payload",
        "scope": "single_decode_exact_validation_and_drop_after_fixture_preparation",
        "exclusions": ["fixture_preparation", "allocator_metadata_and_usable_size_slack",
                       "other_threads", "process_startup", "rss"],
        "not_proved": ["allocator_correctness", "managed_capacity_monotonicity", "release_memory_bound"],
        "base_revision": base, "baseline_selection": selection,
        "head_revision": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=root, text=True,
        ).strip(),
        "working_tree_dirty": bool(subprocess.check_output(
            ["git", "status", "--porcelain"], cwd=root, text=True,
        ).strip()),
        "rustc": subprocess.check_output(
            ["rustc", "+" + rust, "--version"], cwd=root, env=env, text=True,
        ).strip(),
        "base_runtime_sha256": base_runtime, "head_runtime_sha256": runtime,
        "harness_sha256": harness, "rust_toolchain": rust,
        "dependency": dependencies[0], "cases": rows,
        "base_library_name": library_names["base"], "head_library_name": library_names["head"],
        "raw_paired_samples": 48, "live_64_kib_controls": controls,
        "release_memory_gate_enforced": False, "normative_approval": False,
    }
    if phase_report is not None:
        result["phase_observations"] = phase_report
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--base-ref", required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--measure-phases", action="store_true")
    arguments = parser.parse_args()
    output = arguments.output_dir.resolve()
    if output.exists():
        parser.error("allocator measurement requires a fresh output directory")
    try:
        report = comparison(arguments.base_ref, output, measure_phases=arguments.measure_phases)
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
        print(f"allocator measurement failed: {error}", file=sys.stderr)
        report = {"schema_version": 1, "status": "failed", "error": str(error)}
    output.mkdir(parents=True, exist_ok=True)
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if report["status"] == "observed_with_exact_controls" else 1


if __name__ == "__main__":
    raise SystemExit(main())

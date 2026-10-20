#!/usr/bin/env python3
"""Compare identical generated drivers against base/head decoder libraries."""

import argparse
import hashlib
import json
import math
import os
import statistics
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "benchmarks/hayro-comparison"))
from metrics import relative_spread


CASES = (
    "white-64", "white-512", "white-2048", "transitions-64", "transitions-512",
    "many-pages-32", "work-boundary", "late-truncation",
)
HARNESS_FILES = (
    "examples/performance_ci.rs", "benchmarks/generated.rs", "benchmarks/timing.rs",
    "benchmarks/hayro-comparison/metrics.py", "tools/run_performance.py",
    "validation/performance/scenarios.json",
)


def digest(root: Path, names) -> str:
    identity = hashlib.sha256()
    for name in sorted(names):
        identity.update(name.encode() + b"\0" + (root / name).read_bytes() + b"\0")
    return identity.hexdigest()


def runtime_identity(root: Path) -> str:
    names = [path.relative_to(root).as_posix() for path in (root / "src").rglob("*.rs")]
    return digest(root, names + ["Cargo.toml", "Cargo.lock"])


def validate_measurement(report: dict, iterations: int, selected: list[str]) -> dict:
    if report["schema_version"] != 1 or report["iterations"] != iterations:
        raise ValueError("unexpected performance report schema or sample count")
    rows = report["cases"]
    if len(rows) != len(selected) or {row["id"] for row in rows} != set(selected):
        raise ValueError("missing, duplicate or unexpected performance cases")
    for row in rows:
        samples = row["samples_ns"]
        if (
            row["exact_output_checked"] is not True
            or type(row["input_bytes"]) is not int or row["input_bytes"] <= 0
            or type(row["output_bytes"]) is not int or row["output_bytes"] < 0
            or len(samples) != iterations
            or any(type(value) is not int or value < 0 for value in samples)
        ):
            raise ValueError("performance samples or exact-output evidence are invalid")
        ordered = sorted(samples)
        if (
            type(row["median_ns"]) is not int or type(row["p95_ns"]) is not int
            or
            row["median_ns"] != ordered[len(ordered) // 2]
            or row["p95_ns"] != ordered[(len(ordered) * 95 + 99) // 100 - 1]
            or row["median_ns"] <= 0
        ):
            raise ValueError("performance percentiles do not match raw samples")
        if row["outcome"] not in {"decoded", "work-limit", "eof"}:
            raise ValueError("unexpected generated outcome")
        if (row["outcome"] == "decoded") != (row["output_bytes"] > 0):
            raise ValueError("generated output-byte shape is invalid")
    return {row["id"]: row for row in rows}


def compare_runs(base: list[dict], head: list[dict], threshold: float, maximum_spread=0.03) -> list[dict]:
    if not all(math.isfinite(value) and value >= 0 for value in (threshold, maximum_spread)):
        raise ValueError("timing thresholds must be finite and nonnegative")
    if len(base) < 3 or len(base) != len(head):
        raise ValueError("at least three complete paired runs are required")
    result = []
    for identity in base[0]:
        for before, after in zip(base, head):
            if set(before) != set(base[0]) or set(after) != set(base[0]):
                raise ValueError("paired case identities differ")
            for field in ("input_bytes", "output_bytes", "outcome"):
                if before[identity][field] != after[identity][field]:
                    raise ValueError(f"{identity}: correctness descriptor differs: {field}")
        before = [run[identity]["median_ns"] for run in base]
        after = [run[identity]["median_ns"] for run in head]
        stable = max(relative_spread(before), relative_spread(after)) <= maximum_spread
        ratio = statistics.median(after) / statistics.median(before)
        result.append({
            "id": identity, "base_median_ns": statistics.median(before),
            "head_median_ns": statistics.median(after), "median_ratio": ratio,
            "p95_ratio": statistics.median(run[identity]["p95_ns"] for run in head)
                / statistics.median(run[identity]["p95_ns"] for run in base),
            "base_relative_spread": relative_spread(before),
            "head_relative_spread": relative_spread(after),
            "timing_status": "inconclusive" if not stable else (
                "regression_observed" if ratio > 1 + threshold / 100 else "within_policy"
            ),
        })
    return result


def execute(command, root, environment, log, timeout):
    with log.open("w", encoding="utf-8") as output:
        process = subprocess.run(
            command, cwd=root, env=environment, stdout=output,
            stderr=subprocess.STDOUT, check=False, timeout=timeout,
        )
    if process.returncode:
        raise ValueError(f"command failed with exit {process.returncode}; see {log.name}")


def environment():
    excluded = {
        "RUSTFLAGS", "RUSTDOCFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_ENCODED_RUSTDOCFLAGS",
        "RUSTUP_TOOLCHAIN", "RUSTC", "RUSTDOC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER",
        "RUSTC_BOOTSTRAP", "CARGO_BUILD_RUSTFLAGS", "CARGO_BUILD_TARGET", "CARGO_TARGET_DIR",
        "CARGO_BUILD_RUSTC", "CARGO_BUILD_RUSTDOC", "CARGO_BUILD_RUSTC_WRAPPER",
        "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
    }
    result = {
        key: value for key, value in os.environ.items()
        if key not in excluded
        and not key.startswith("CARGO_PROFILE_")
        and not (key.startswith("CARGO_TARGET_") and key.endswith("_RUSTFLAGS"))
    }
    result.update({"CARGO_INCREMENTAL": "0", "CARGO_TERM_COLOR": "never"})
    return result


def markdown_report(report):
    lines = [
        "# Generated correctness and informational performance",
        "",
        "**Hosted timing is informational, not a release-performance pass.**",
        "Unstable measurements are inconclusive. Allocation/RSS and broader corpus gaps remain.",
        "",
        "Run status: " + report["status"],
    ]
    if "correctness" in report:
        lines.append("Correctness: " + report["correctness"])
    else:
        lines.append("Correctness: not established; inspect the failure diagnostics.")
    if "error" in report:
        lines.append("Failure: " + report["error"])
    if "cases" in report:
        lines += [
            "", "| Case | Median head/base | p95 head/base | Timing classification |",
            "|---|---:|---:|---|",
        ]
        for case in report["cases"]:
            lines.append(
                f"| {case['id']} | {case['median_ratio']:.3f} | {case['p95_ratio']:.3f} | "
                f"{case['timing_status']} |"
            )
    return "\n".join(lines) + "\n"


def comparison(arguments, root=ROOT):
    output = arguments.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=False)
    result = {"schema_version": 1, "status": "failed"}
    try:
        selected = [arguments.case] if arguments.case else list(CASES)
        if arguments.iterations not in range(20, 201) or arguments.pairs not in range(3, 8):
            raise ValueError("iterations must be 20..200 and paired runs 3..7")
        reference = arguments.base_ref
        if not reference or set(reference) == {"0"}:
            reference = "origin/main"
            result["baseline_selection"] = "first-push baseline explicitly resolved to origin/main"
        base = subprocess.run(
            ["git", "rev-parse", "--verify", "--end-of-options", reference + "^{commit}"],
            cwd=root, text=True, capture_output=True, check=True,
        ).stdout.strip()
        head = subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=root, text=True, capture_output=True, check=True,
        ).stdout.strip()
        manifest = json.loads((root / "toolchain.json").read_text())
        rust = manifest["cargo_rust_toolchain"]
        policy = json.loads((root / "validation/performance/scenarios.json").read_text())["threshold_approval"]
        if policy["status"] != "approved_delegated_default":
            raise ValueError("review the performance threshold policy before using it")
        harness_hash = digest(root, HARNESS_FILES)
        head_hash = runtime_identity(root)
        result.update({
            "base_revision": base, "head_revision": head, "rust_toolchain": rust,
            "harness_sha256": harness_hash, "head_runtime_sha256": head_hash,
            "policy": policy, "timing_enforced": arguments.enforce_timing,
            "maximum_relative_spread": 0.03,
            "p95_gate": "informational only; no owner-approved p95 threshold applied",
            "working_tree_dirty": bool(subprocess.run(
                ["git", "status", "--porcelain"], cwd=root, text=True,
                capture_output=True, check=True,
            ).stdout.strip()),
            "rustc": subprocess.run(
                ["rustc", "+" + rust, "--version"], cwd=root, env=environment(),
                text=True, capture_output=True, check=True,
            ).stdout.strip(),
            "build_scope": "locked offline release libraries, opt-level=3/codegen-units=1/lto=false/debug=false; identical rustc -O driver; caller overrides removed",
            "scope": "Generated MMR/envelope/error subset; no normative assets, allocation counts or dedicated-runner claim",
        })
        env = environment()
        with tempfile.TemporaryDirectory(prefix="jbig2-performance-base-") as temporary:
            base_root = Path(temporary) / "source"
            base_root.mkdir()
            archive = Path(temporary) / "base.tar"
            with archive.open("wb") as contents:
                subprocess.run(["git", "archive", base], cwd=root, stdout=contents, check=True, timeout=60)
            with tarfile.open(archive) as contents:
                contents.extractall(base_root, filter="data")
            result["base_runtime_sha256"] = runtime_identity(base_root)
            binaries = {}
            for name, source in (("base", base_root), ("head", root)):
                build = output / (name + "-build")
                execute(
                    ["cargo", "+" + rust, "build", "--offline", "--locked", "--release",
                     "--lib", "--manifest-path", str(source / "Cargo.toml"),
                     "--target-dir", str(build),
                     "--config", "profile.release.opt-level=3",
                     "--config", "profile.release.codegen-units=1",
                     "--config", "profile.release.lto=false",
                     "--config", "profile.release.debug=false"],
                    root, env, output / (name + "-build.log"), 600,
                )
                binary = output / ("driver-" + name)
                execute(
                    ["rustc", "+" + rust, "--edition=2021", "-O",
                     str(root / "examples/performance_ci.rs"), "--crate-name", "jbig2_perf_driver",
                     "--extern", "jbig2_rs=" + str(build / "release/libjbig2_rs.rlib"),
                     "-L", "dependency=" + str(build / "release/deps"), "-o", str(binary)],
                    root, env, output / (name + "-driver.log"), 120,
                )
                binaries[name] = binary
            if hasattr(os, "sched_getaffinity"):
                allowed = os.sched_getaffinity(0)
                cpu = min(allowed) if arguments.cpu is None else arguments.cpu
                if cpu not in allowed:
                    raise ValueError("selected CPU is outside the runner affinity set")
                os.sched_setaffinity(0, {cpu})
                result["cpu_affinity"] = cpu
            elif arguments.cpu is not None:
                raise ValueError("CPU affinity is unavailable on this host")
            runs = {"base": [], "head": []}
            for pair in range(arguments.pairs):
                for name in (("base", "head") if pair % 2 == 0 else ("head", "base")):
                    path = output / f"{name}-{pair}.json"
                    command = [str(binaries[name]), str(arguments.iterations)]
                    if arguments.case:
                        command.append(arguments.case)
                    execute(command, root, env, path, 120)
                    runs[name].append(validate_measurement(
                        json.loads(path.read_text()), arguments.iterations, selected,
                    ))
            result["cases"] = compare_runs(
                runs["base"], runs["head"], policy["wall_time_regression_percent"],
            )
        if digest(root, HARNESS_FILES) != harness_hash or runtime_identity(root) != head_hash:
            raise ValueError("benchmark source changed during measurement")
        statuses = {case["timing_status"] for case in result["cases"]}
        result["timing_gate"] = "not_requested"
        if arguments.enforce_timing:
            result["timing_gate"] = (
                "failed" if "regression_observed" in statuses
                else "inconclusive" if "inconclusive" in statuses else "passed"
            )
        result["status"] = (
            "passed" if result["timing_gate"] in ("not_requested", "passed") else result["timing_gate"]
        )
        result["correctness"] = "exact_generated_outputs_and_typed_error_boundaries_passed"
        result["unmeasured"] = [
            "allocation counts, allocated peak bytes and per-case process RSS",
            "cumulative work-unit telemetry beyond the fixed guard checks",
            "representative PDFs and arithmetic/text/halftone corpus",
            "dedicated-host latency/RSS gate", "owner ratification of proposed median/p95 thresholds",
        ]
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        result["error"] = str(error)
        print(f"performance validation failed: {error}", file=sys.stderr)
    (output / "report.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    (output / "report.md").write_text(markdown_report(result), encoding="utf-8")
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--base-ref", default="origin/main")
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--iterations", type=int, default=30)
    parser.add_argument("--pairs", type=int, default=3)
    parser.add_argument("--case", choices=CASES)
    parser.add_argument("--cpu", type=int)
    parser.add_argument("--enforce-timing", action="store_true")
    arguments = parser.parse_args()
    try:
        report = comparison(arguments)
    except (OSError, ValueError) as error:
        print(f"performance setup failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps({"status": report["status"], "timing_gate": report.get("timing_gate", "unavailable")}))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

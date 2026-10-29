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
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "benchmarks/hayro-comparison"))
from metrics import relative_spread
from audit_package import LIBRARY_NAME


CASES = (
    "white-64", "white-512", "white-2048", "transitions-64", "transitions-512",
    "many-pages-32", "work-boundary", "late-truncation",
)
HARNESS_FILES = (
    "examples/performance_ci.rs", "benchmarks/generated.rs", "benchmarks/timing.rs",
    "benchmarks/hayro-comparison/metrics.py", "tools/run_performance.py",
    "tools/audit_package.py",
    "validation/performance/scenarios.json",
)
RSS_SOURCE = "linux_proc_self_status_VmHWM"
RSS_SCOPE = "fresh_process_startup_selected_fixture_single_decode_and_exact_validation"


def digest(root: Path, names) -> str:
    identity = hashlib.sha256()
    for name in sorted(names):
        identity.update(name.encode() + b"\0" + (root / name).read_bytes() + b"\0")
    return identity.hexdigest()


def runtime_identity(root: Path) -> str:
    names = [path.relative_to(root).as_posix() for path in (root / "src").rglob("*.rs")]
    return digest(root, names + ["Cargo.toml", "Cargo.lock"])

def library_name(root: Path) -> str:
    manifest = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    package = manifest["package"]["name"]
    if not isinstance(package, str) or not package:
        raise ValueError("benchmark source needs a Cargo package name")
    name = manifest.get("lib", {}).get("name", package.replace("-", "_"))
    if not isinstance(name, str) or not name.isidentifier() or not name.isascii():
        raise ValueError("benchmark source needs a valid Rust library name")
    return name


def resolve_base(reference: str, root: Path) -> tuple[str, str | None]:
    selection = None
    if not reference or set(reference) == {"0"}:
        reference = "origin/main"
        selection = "first-push baseline explicitly resolved to origin/main"
    command = ["git", "rev-parse", "--verify", "--end-of-options", reference + "^{commit}"]
    process = subprocess.run(command, cwd=root, text=True, capture_output=True, check=False)
    if process.returncode:
        if reference == "origin/main":
            process.check_returncode()
        selection = "requested baseline unavailable; fell back to origin/main"
        process = subprocess.run(
            ["git", "rev-parse", "--verify", "--end-of-options", "origin/main^{commit}"],
            cwd=root, text=True, capture_output=True, check=False,
        )
        process.check_returncode()
    return process.stdout.strip(), selection


def validate_measurement(report: dict, iterations: int, selected: list[str]) -> dict:
    if not selected or len(set(selected)) != len(selected):
        raise ValueError("performance measurement needs unique nonempty case selection")
    if report["schema_version"] != 2 or report["iterations"] != iterations:
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
        quota = row.get("work_quota")
        if (
            not isinstance(quota, dict)
            or set(quota) != {"admitted_limit", "rejected_limit", "rejected_actual", "rejected_kind"}
            or quota["rejected_kind"] != "Work"
            or any(
                type(quota[name]) is not int or not 0 <= quota[name] <= (1 << 64) - 1
                for name in ("rejected_limit", "rejected_actual")
            )
            or quota["rejected_actual"] <= quota["rejected_limit"]
        ):
            raise ValueError("missing or invalid typed work-quota boundary")
        admitted = quota["admitted_limit"]
        if row["outcome"] == "work-limit":
            if (
                admitted is not None or quota["rejected_actual"] != row["input_bytes"]
                or quota["rejected_limit"] + 1 != row["input_bytes"]
            ):
                raise ValueError("fixed input-work control differs from its exact boundary")
        elif (
            type(admitted) is not int or not 0 < admitted <= (1 << 64) - 1
            or admitted != quota["rejected_limit"] + 1
            or quota["rejected_actual"] != admitted
        ):
            raise ValueError("work-quota result lacks an admitted/one-below rejection pair")
    return {row["id"]: row for row in rows}


def compare_runs(base: list[dict], head: list[dict], threshold: float, maximum_spread=0.03) -> list[dict]:
    if not all(math.isfinite(value) and value >= 0 for value in (threshold, maximum_spread)):
        raise ValueError("timing thresholds must be finite and nonnegative")
    if len(base) < 3 or len(base) != len(head):
        raise ValueError("at least three complete paired runs are required")
    if any(not run for run in (*base, *head)):
        raise ValueError("paired runs need nonempty case measurements")
    result = []
    for identity in base[0]:
        for side in (base, head):
            for run in side:
                if set(run) != set(side[0]) or any(
                    run[identity][field] != side[0][identity][field]
                    for field in ("input_bytes", "output_bytes", "outcome", "work_quota")
                ):
                    raise ValueError(f"{identity}: repeated correctness/work-quota measurements differ")
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
        base_quota = base[0][identity]["work_quota"]["admitted_limit"]
        head_quota = head[0][identity]["work_quota"]["admitted_limit"]
        if (base_quota is None) != (head_quota is None):
            raise ValueError(f"{identity}: incompatible work-quota scopes")
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
            "base_minimum_work_limit": base_quota,
            "head_minimum_work_limit": head_quota,
            "work_quota_status": (
                "fixed_input_guard_control" if base_quota is None
                else "regression_observed" if head_quota > base_quota else "within_policy"
            ),
        })
    return result


def validate_rss(report, expected):
    if (
        not isinstance(report, dict)
        or set(report) != {
            "schema_version", "id", "input_bytes", "output_bytes", "outcome",
            "exact_output_checked", "rss_source", "rss_unit", "peak_rss_kib", "scope",
        }
        or type(report.get("schema_version")) is not int or report["schema_version"] != 1
        or report.get("rss_source") != RSS_SOURCE or report.get("rss_unit") != "KiB"
        or report.get("scope") != RSS_SCOPE or report.get("exact_output_checked") is not True
        or any(report.get(field) != expected[field]
               for field in ("id", "input_bytes", "output_bytes", "outcome"))
        or any(type(report[field]) is not int for field in ("input_bytes", "output_bytes"))
        or type(report.get("peak_rss_kib")) is not int
        or not 0 < report["peak_rss_kib"] <= (1 << 64) - 1
    ):
        raise ValueError("missing, incompatible or invalid exact-output per-case RSS measurement")
    return report["peak_rss_kib"]


def summarize_rss(runs, selected, pairs):
    if not selected or len(set(selected)) != len(selected) or not 3 <= pairs <= 7:
        raise ValueError("RSS summary needs unique nonempty cases and three to seven pairs")
    if set(runs) != {"base", "head"} or any(set(side) != set(selected) for side in runs.values()):
        raise ValueError("RSS summary has missing or unexpected base/head cases")
    cases = []
    for name in selected:
        samples = {side: runs[side][name] for side in ("base", "head")}
        if any(
            not isinstance(values, list) or len(values) != pairs
            or any(type(value) is not int or not 0 < value <= (1 << 64) - 1 for value in values)
            for values in samples.values()
        ):
            raise ValueError("RSS summary requires every positive per-process sample")
        base = statistics.median(samples["base"])
        head = statistics.median(samples["head"])
        cases.append({
            "id": name, "base_samples_kib": samples["base"], "head_samples_kib": samples["head"],
            "base_median_kib": base, "head_median_kib": head, "median_ratio": head / base,
        })
    return {
        "status": "observed", "policy": "informational_only",
        "source": RSS_SOURCE, "unit": "KiB", "scope": RSS_SCOPE,
        "sample_point": "after exact output validation, before report serialization",
        "includes": "process startup, selected fixture construction, one decode, validation and measurement overhead",
        "excludes": "other cases, work-quota probes, repeated timings and inherited launcher high-water RSS",
        "not_claimed": "allocator counts/peak bytes, isolated decoder allocation, or a release-memory gate",
        "cases": cases,
    }


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
        "Unstable measurements are inconclusive. Allocator, dedicated-host memory and broader corpus gaps remain.",
        "Work-quota boundaries are deterministic public admission limits, not internal work counters.",
        "",
        "Run status: " + report["status"],
    ]
    if "correctness" in report:
        lines.append("Correctness: " + report["correctness"])
    else:
        lines.append("Correctness: not established; inspect the failure diagnostics.")
    if "error" in report:
        lines.append("Failure: " + report["error"])
    if "resource_gate" in report:
        lines.append("Work-quota gate: " + report["resource_gate"])
    rss = report.get("rss", {"status": "not_established"})
    lines.append("Process RSS: " + rss["status"])
    if "cases" in report:
        lines += [
            "", "| Case | Median head/base | p95 head/base | Timing classification | Work quota base/head | Work quota classification |",
            "|---|---:|---:|---|---:|---|",
        ]
        for case in report["cases"]:
            quota = (
                "fixed input guard" if case["base_minimum_work_limit"] is None
                else f"{case['base_minimum_work_limit']} / {case['head_minimum_work_limit']}"
            )
            lines.append(
                f"| {case['id']} | {case['median_ratio']:.3f} | {case['p95_ratio']:.3f} | "
                f"{case['timing_status']} | "
                f"{quota} | "
                f"{case['work_quota_status']} |"
            )
    if rss["status"] == "observed":
        lines += [
            "", "**Process peak RSS is observational, not an allocator or release-memory gate.**",
            "Each sample includes startup, one selected fixture/decode, validation and measurement overhead; no quota probes or repeated timings.",
            "", "| Case | Base median KiB | Head median KiB | Head/base |",
            "|---|---:|---:|---:|",
        ]
        for case in rss["cases"]:
            lines.append(
                f"| {case['id']} | {case['base_median_kib']} | {case['head_median_kib']} | "
                f"{case['median_ratio']:.3f} |"
            )
    return "\n".join(lines) + "\n"


def comparison(arguments, root=ROOT):
    output = arguments.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=False)
    result = {"schema_version": 3, "status": "failed", "rss": {"status": "not_requested"}}
    try:
        if arguments.measure_rss:
            result["rss"] = {"status": "requested_not_established", "policy": "informational_only"}
            if sys.platform != "linux":
                raise ValueError("requested per-case RSS collection requires Linux /proc/self/status")
        selected = [arguments.case] if arguments.case else list(CASES)
        if arguments.iterations not in range(20, 201) or arguments.pairs not in range(3, 8):
            raise ValueError("iterations must be 20..200 and paired runs 3..7")
        base, baseline_selection = resolve_base(arguments.base_ref, root)
        if baseline_selection:
            result["baseline_selection"] = baseline_selection
        head = subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=root, text=True, capture_output=True, check=True,
        ).stdout.strip()
        manifest = json.loads((root / "config/toolchain.json").read_text())
        rust = manifest["cargo_rust_toolchain"]
        policy = json.loads((root / "validation/performance/scenarios.json").read_text())["threshold_approval"]
        if policy["status"] != "approved_delegated_default":
            raise ValueError("review the performance threshold policy before using it")
        if type(policy["work_unit_regression"]) is not int or policy["work_unit_regression"] != 0:
            raise ValueError("work-quota comparison requires the existing zero-work-regression policy")
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
            "resource_scope": "Minimum public max_work_units for the exact expected outcome; not an internal work counter",
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
                crate = library_name(source)
                result[name + "_library_name"] = crate
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
                     "--extern", LIBRARY_NAME + "=" + str(build / f"release/lib{crate}.rlib"),
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
            rss_runs = {side: {case: [] for case in selected} for side in ("base", "head")}
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
                    if arguments.measure_rss:
                        for case in selected:
                            path = output / f"{name}-{pair}-rss-{case}.json"
                            execute([str(binaries[name]), "--rss-case", case], root, env, path, 120)
                            rss_runs[name][case].append(validate_rss(
                                json.loads(path.read_text()), runs[name][-1][case],
                            ))
            result["cases"] = compare_runs(
                runs["base"], runs["head"], policy["wall_time_regression_percent"],
            )
            if arguments.measure_rss:
                result["rss"] = summarize_rss(rss_runs, selected, arguments.pairs)
        if digest(root, HARNESS_FILES) != harness_hash or runtime_identity(root) != head_hash:
            raise ValueError("benchmark source changed during measurement")
        statuses = {case["timing_status"] for case in result["cases"]}
        result["resource_gate"] = (
            "failed" if any(case["work_quota_status"] == "regression_observed" for case in result["cases"])
            else "passed"
        )
        result["timing_gate"] = "not_requested"
        if arguments.enforce_timing:
            result["timing_gate"] = (
                "failed" if "regression_observed" in statuses
                else "inconclusive" if "inconclusive" in statuses else "passed"
            )
        result["status"] = (
            "failed" if result["resource_gate"] == "failed"
            else "passed" if result["timing_gate"] in ("not_requested", "passed")
            else result["timing_gate"]
        )
        result["correctness"] = "exact_generated_outputs_and_typed_error_boundaries_passed"
        result["unmeasured"] = [
            "allocation counts and allocated peak bytes",
            "internal cumulative work counters and a monotonicity/accounting proof; quota boundaries are measured separately",
            "representative PDFs and arithmetic/text/halftone corpus",
            "dedicated-host latency/RSS gate", "owner ratification of proposed median/p95 thresholds",
        ]
        if not arguments.measure_rss:
            result["unmeasured"].append("per-case process RSS (not requested)")
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
    parser.add_argument("--measure-rss", action="store_true", help="Observe fresh-process per-case peak RSS on Linux; never enforce a memory threshold")
    arguments = parser.parse_args()
    try:
        report = comparison(arguments)
    except (OSError, ValueError) as error:
        print(f"performance setup failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps({
        "status": report["status"], "timing_gate": report.get("timing_gate", "unavailable"),
        "resource_gate": report.get("resource_gate", "not_established"),
        "rss_status": report["rss"]["status"],
    }))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

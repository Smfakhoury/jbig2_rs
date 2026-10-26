#!/usr/bin/env python3
"""Scope LLVM coverage to ordinary runtime code, excluding inline test items."""

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

from evidence import load_json, read_evidence
from rust_source import production_code, source_functions


ROOT = Path(__file__).resolve().parents[1]
METRICS = ("lines", "regions", "branches")
EXCLUDED_FILES = {"src/lib.rs": "Module declarations and re-exports; no executing functions."}
SCOPE_VERSION = 1
TEST_FIXTURES = ("tools/distribution_consumer.rs", "benchmarks/generated.rs")


def test_source_hash(root: Path) -> str:
    paths = sorted((root / "tests").rglob("*.rs"))
    paths += [root / name for name in TEST_FIXTURES if (root / name).is_file()]
    identity = hashlib.sha256()
    for path in paths:
        identity.update(path.relative_to(root).as_posix().encode() + b"\0" + path.read_bytes() + b"\0")
    return identity.hexdigest()


def test_driver_source(path: Path, root: Path) -> bool:
    return path.is_relative_to(root / "tests") or path in {root / name for name in TEST_FIXTURES}


def ratio(covered: int, count: int) -> dict:
    return {
        "covered": covered, "count": count,
        "percent": 100 * covered / count if count else None,
    }


def runtime_sources(root: Path):
    root = root.resolve()
    functions = source_functions(root)
    if any(entry["file"] in EXCLUDED_FILES for entry in functions):
        raise ValueError("an excluded source file gained an executing function")
    paths = sorted((root / "src").rglob("*.rs"))
    masks = {}
    identity = hashlib.sha256()
    for path in paths:
        relative = path.relative_to(root).as_posix()
        data = path.read_bytes()
        identity.update(relative.encode() + b"\0" + data + b"\0")
        if relative not in EXCLUDED_FILES:
            masks[path.resolve()] = production_code(data, mask_noncode=False).encode().splitlines()
    return masks, identity.hexdigest()


def lcov_lines(text: str) -> dict:
    files = {}
    current = None
    for line in text.splitlines():
        if line.startswith("SF:"):
            if current is not None:
                raise ValueError("LCOV record is not terminated")
            current = Path(line[3:]).resolve()
            if current in files:
                raise ValueError("duplicate LCOV file")
            files[current] = {}
        elif line == "end_of_record":
            current = None
        elif line.startswith("DA:"):
            if current is None:
                raise ValueError("LCOV line outside a source record")
            fields = line[3:].split(",")
            if len(fields) not in (2, 3):
                raise ValueError("malformed LCOV line count")
            number, count = int(fields[0]), int(fields[1])
            if number < 1 or count < 0 or number in files[current]:
                raise ValueError("invalid or duplicate LCOV line")
            files[current][number] = count
    if current is not None or not files:
        raise ValueError("incomplete or empty LCOV report")
    return files


def active_line(mask, number):
    if not 1 <= number <= len(mask):
        raise ValueError("coverage location is outside its source")
    return bool(mask[number - 1].strip())


def active_region(mask, region):
    line, column, end_line, end_column = region[:4]
    if (
        not 1 <= line <= end_line <= len(mask)
        or not 1 <= column <= len(mask[line - 1]) + 1
        or not 1 <= end_column <= len(mask[end_line - 1]) + 1
        or (line == end_line and column > end_column)
    ):
        raise ValueError("invalid coverage source region")
    if line == end_line:
        return bool(mask[line - 1][column - 1:end_column - 1].strip())
    return any(part.strip() for part in [
        mask[line - 1][column - 1:], *mask[line:end_line - 1],
        mask[end_line - 1][:end_column - 1],
    ])


def excluded_lines(region):
    return range(region[0], region[2] + (region[3] != 1))


def scoped_report(document: dict, lcov: str, root: Path) -> dict:
    if (
        document.get("type") != "llvm.coverage.json.export"
        or document.get("version") != "3.1.0"
        or len(document.get("data", [])) != 1
    ):
        raise ValueError("unsupported or empty LLVM export")
    root = root.resolve()
    masks, source_hash = runtime_sources(root)
    lines = lcov_lines(lcov)
    data = document["data"][0]
    exported = {Path(entry["filename"]).resolve() for entry in data["files"]}
    extras = exported - set(masks)
    if any(not test_driver_source(path, root) for path in extras):
        raise ValueError("unexpected non-runtime coverage source file")
    if not masks or not set(masks).issubset(exported) or exported != set(lines):
        raise ValueError("coverage file set differs from the declared runtime source set")
    records = {}
    for path, mask in masks.items():
        executable = {number: count for number, count in lines[path].items()
                      if active_line(mask, number)}
        if not executable:
            raise ValueError(f"runtime file has no executable line coverage: {path}")
        records[path] = {
            "line_counts": executable, "regions": {}, "branches": {},
            "selected_instantiations": 0, "excluded_test_instantiations": 0,
            "excluded_test_lines": set(),
        }
    excluded_drivers = set(extras)
    for function in data["functions"]:
        if len(function["filenames"]) != 1 or not function["regions"]:
            raise ValueError("unsupported multi-file or empty function mapping")
        path = Path(function["filenames"][0]).resolve()
        if test_driver_source(path, root):
            excluded_drivers.add(path)
            continue
        if path not in records:
            raise ValueError("function maps outside runtime report files")
        record = records[path]
        if not active_region(masks[path], function["regions"][0]):
            record["excluded_test_instantiations"] += 1
            for region in function["regions"]:
                record["excluded_test_lines"].update(excluded_lines(region))
            continue
        record["selected_instantiations"] += 1
        for region in function["regions"]:
            if len(region) != 8 or region[7] != 0 or region[5:7] != [0, 0] or region[4] < 0:
                raise ValueError("unsupported LLVM code region")
            if not active_region(masks[path], region):
                record["excluded_test_lines"].update(excluded_lines(region))
                continue
            key = tuple(region[:4])
            record["regions"][key] = record["regions"].get(key, False) or region[4] > 0
        for branch in function["branches"]:
            if len(branch) != 9 or branch[8] != 4 or branch[6:8] != [0, 0] or min(branch[4:6]) < 0:
                raise ValueError("unsupported LLVM branch region")
            if not active_region(masks[path], branch):
                continue
            key = tuple(branch[:4])
            previous = record["branches"].get(key, (False, False))
            record["branches"][key] = (
                previous[0] or branch[4] > 0, previous[1] or branch[5] > 0,
            )
    files = {}
    for path, record in sorted(records.items()):
        if record["excluded_test_lines"] & record["line_counts"].keys():
            raise ValueError("mixed runtime/test lines need explicit coverage scope support")
        branches = record["branches"]
        files[path.relative_to(root).as_posix()] = {
            "lines": ratio(sum(count > 0 for count in record["line_counts"].values()),
                           len(record["line_counts"])),
            "regions": ratio(sum(record["regions"].values()), len(record["regions"])),
            "branches": ratio(sum(sum(outcomes) for outcomes in branches.values()), 2 * len(branches)),
            "uncovered_lines": sorted(number for number, count in record["line_counts"].items() if not count),
            "selected_instantiations": record["selected_instantiations"],
            "excluded_test_instantiations": record["excluded_test_instantiations"],
            "line_counts": {str(number): count for number, count in record["line_counts"].items()},
        }
    totals = {
        metric: ratio(sum(entry[metric]["covered"] for entry in files.values()),
                      sum(entry[metric]["count"] for entry in files.values()))
        for metric in METRICS
    }
    if any(not totals[metric]["count"] for metric in METRICS):
        raise ValueError("coverage export is missing runtime line/region/branch instrumentation")
    return {
        "schema_version": 1, "scope_version": SCOPE_VERSION,
        "source_sha256": source_hash, "coverage_platform": "linux",
        "totals": totals, "files": files, "excluded_files": EXCLUDED_FILES,
        "excluded_integration_test_sources": sorted(path.relative_to(root).as_posix() for path in excluded_drivers),
    }


def regressions(report: dict, baseline: dict) -> list[str]:
    if baseline["scope_version"] != report["scope_version"]:
        raise ValueError("coverage scope version changed; review a new baseline")
    failures = []
    comparisons = [("total", report["totals"], baseline["totals"])]
    for path, before in baseline["files"].items():
        if path not in report["files"]:
            failures.append(f"baseline runtime file disappeared: {path}")
        else:
            comparisons.append((path, report["files"][path], before))
    for label, current, before in comparisons:
        for metric in METRICS:
            new, old = current[metric], before[metric]
            for value in (new, old):
                if (
                    type(value["count"]) is not int or type(value["covered"]) is not int
                    or not 0 <= value["covered"] <= value["count"]
                ):
                    raise ValueError("invalid baseline/report coverage counts")
            if old["count"] and (
                not new["count"] or new["covered"] * old["count"] < old["covered"] * new["count"]
            ):
                failures.append(f"{label}: {metric} below reviewed baseline")
    return failures


def changed_lines(root: Path, reference: str, files: dict) -> dict:
    covered = count = 0
    uncovered = {}
    for path, entry in files.items():
        process = subprocess.run(
            ["git", "diff", "--no-ext-diff", "--unified=0", reference, "--", path],
            cwd=root, capture_output=True, text=True, check=True,
        )
        changed = set()
        for start, length in re.findall(r"^@@ .*? \+(\d+)(?:,(\d+))? @@", process.stdout, re.MULTILINE):
            changed.update(range(int(start), int(start) + (int(length) if length else 1)))
        executable = {number for number in changed if str(number) in entry["line_counts"]}
        missing = sorted(number for number in executable if entry["line_counts"][str(number)] == 0)
        covered += len(executable) - len(missing)
        count += len(executable)
        if missing:
            uncovered[path] = missing
    return {
        **ratio(covered, count), "status": "measured" if count else "not_applicable",
        "gate": "informational_pending_threshold_review", "uncovered": uncovered,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json", type=Path, required=True)
    parser.add_argument("--lcov", type=Path, required=True)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--collection", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--base-ref")
    arguments = parser.parse_args()
    try:
        document = load_json(arguments.json)
        report = scoped_report(document, arguments.lcov.read_text(), ROOT)
        baseline = read_evidence(arguments.baseline, "coverage_baseline")
        configuration = load_json(ROOT / "config/toolchain.json")["coverage"]
        collection = load_json(arguments.collection)
        if (
            collection["source_sha256"] != report["source_sha256"]
            or collection["test_source_sha256"] != test_source_hash(ROOT)
            or collection["toolchain"] != configuration
            or collection["report_sha256"] != {
                "json": hashlib.sha256(arguments.json.read_bytes()).hexdigest(),
                "lcov": hashlib.sha256(arguments.lcov.read_bytes()).hexdigest(),
            }
        ):
            raise ValueError("coverage collection is stale or does not match report inputs")
        if baseline["toolchain"] != configuration:
            raise ValueError("coverage toolchain baseline mismatch")
        if document["cargo_llvm_cov"]["version"] != configuration["cargo_llvm_cov_version"]:
            raise ValueError("coverage exporter version mismatch")
        report["failures"] = regressions(report, baseline)
        report["status"] = "failed" if report["failures"] else "baseline_nonregression_passed"
        report["toolchain"] = configuration
        report["collection"] = collection
        report["release_goals"] = baseline["release_goals"]
        report["release_goals_met"] = all(
            report["totals"][metric]["percent"] >= goal
            for metric, goal in baseline["release_goals"].items()
        )
        if arguments.base_ref:
            report["changed_lines"] = changed_lines(ROOT, arguments.base_ref, report["files"])
        arguments.output.parent.mkdir(parents=True, exist_ok=True)
        arguments.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        table = [
            "# Runtime coverage", "",
            f"Baseline non-regression: **{report['status']}**.",
            f"Proposed release coverage goals met: **{report['release_goals_met']}**.", "",
            "| Metric | Covered / total | Runtime coverage |",
            "|---|---:|---:|",
        ]
        for metric, value in report["totals"].items():
            table.append(
                f"| {metric} | {value['covered']} / {value['count']} | {value['percent']:.2f}% |"
            )
        table += [
            "", "Inline tests/oracles and ghost code are excluded from these metrics.",
            "Raw HTML is diagnostic and includes test code; runtime.json is the scoped result.",
            "90% line / 85% branch remain proposed release goals, not a claim of current readiness.",
        ]
        arguments.output.with_suffix(".md").write_text("\n".join(table) + "\n", encoding="utf-8")
        print(json.dumps({key: report[key] for key in (
            "status", "totals", "release_goals_met", "failures",
        )}, indent=2))
        return 1 if report["failures"] else 0
    except (OSError, ValueError, KeyError, TypeError, subprocess.CalledProcessError) as error:
        print(f"coverage validation failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())

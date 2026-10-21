#!/usr/bin/env python3
"""Validate, minimize and exactly replay existing first-party fuzz corpora."""

import argparse
import collections
import hashlib
import json
import os
import re
import stat
import subprocess
import sys
import time
from pathlib import Path

from run_fuzz import (
    INPUT_TIMEOUT, MAX_CORPUS_BYTES, MAX_INPUT_BYTES, ROOT, RSS_LIMIT_MB, TARGETS,
    corpus_summary, execute, execution_count, fuzz_environment, inventory,
    source_identity, validate_targets,
)


MAINTENANCE_STEP_SECONDS = 3 * 60
TOTAL_SECONDS = MAINTENANCE_STEP_SECONDS - 5
BUILD_SECONDS = 120
MERGE_SECONDS = 30
REPLAY_SECONDS = 20
MAX_FILES_PER_TARGET = 4096
MAX_TOTAL_FILES = 16_384
MAX_DIAGNOSTIC_BYTES = 16 << 20
NAME = re.compile(r"[A-Za-z0-9_.-]{1,128}")


def real_path(path: Path) -> Path:
    path = path.absolute()
    for component in (path, *path.parents):
        if component.is_symlink():
            raise ValueError(f"symlink in corpus lifecycle path: {component}")
    return path.resolve()


def collection(path: Path) -> dict:
    real_path(path)
    if not path.is_dir():
        raise ValueError("corpus must contain exactly the existing nine target directories")
    names = set()
    for entry in path.iterdir():
        if entry.name not in TARGETS:
            raise ValueError("unsupported target in corpus collection")
        names.add(entry.name)
    if names != set(TARGETS):
        raise ValueError("corpus must contain exactly the existing nine target directories")
    result = {}
    for target in TARGETS:
        entries = inventory(path / target, MAX_INPUT_BYTES, MAX_FILES_PER_TARGET)
        if not entries:
            raise ValueError(f"empty target corpus: {target}")
        for entry in entries:
            if not NAME.fullmatch(entry["name"]) or entry["bytes"] == 0:
                raise ValueError(f"unsupported empty input or corpus filename: {target}/{entry['name']}")
        result[target] = entries
    if sum(len(entries) for entries in result.values()) > MAX_TOTAL_FILES:
        raise ValueError("corpus exceeds total file work bound")
    if sum(entry["bytes"] for entries in result.values() for entry in entries) > MAX_CORPUS_BYTES:
        raise ValueError("corpus exceeds total byte bound")
    return result


def copy_collection(source: Path, destination: Path, expected: dict) -> None:
    destination.mkdir()
    for target, entries in expected.items():
        real_path(source / target)
        (destination / target).mkdir()
        for entry in entries:
            path = source / target / entry["name"]
            descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
            with os.fdopen(descriptor, "rb") as stream:
                if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
                    raise ValueError("non-regular input during owned corpus copy")
                contents = stream.read(MAX_INPUT_BYTES + 1)
            if len(contents) != entry["bytes"] or hashlib.sha256(contents).hexdigest() != entry["sha256"]:
                raise ValueError("input changed during bounded owned corpus copy")
            with (destination / target / entry["name"]).open("xb") as output:
                output.write(contents)


def diagnostic(path: Path) -> None:
    real_path(path)
    if not path.is_file() or path.stat().st_size > MAX_DIAGNOSTIC_BYTES:
        raise ValueError("missing, nonregular or oversized corpus diagnostic")


def one_match(pattern: str, text: str, label: str) -> tuple:
    matches = re.findall(pattern, text, re.MULTILINE)
    if len(matches) != 1:
        raise ValueError(f"missing or ambiguous minimizer {label}")
    values = matches[0] if isinstance(matches[0], tuple) else (matches[0],)
    return tuple(int(value) for value in values)


def merge_evidence(log: Path, before: list, after: list) -> dict:
    diagnostic(log)
    text = log.read_text(encoding="utf-8")
    count = len(before)
    outer = one_match(
        r"^MERGE-OUTER: ([0-9]+) files, ([0-9]+) in the initial corpus, ([0-9]+) processed earlier$",
        text, "input inventory",
    )
    inner = one_match(
        r"^MERGE-INNER: ([0-9]+) total files; ([0-9]+) processed earlier; will process ([0-9]+) files now$",
        text, "processed inventory",
    )
    attempts = one_match(r"^MERGE-OUTER: successful in ([0-9]+) attempt\(s\)$", text, "completion")
    done = one_match(
        r"^#([0-9]+)\s+DONE\s+cov: ([0-9]+) ft: ([0-9]+) .+$", text, "processed units",
    )
    added = one_match(
        r"^MERGE-OUTER: ([0-9]+) new files with ([0-9]+) new features added; ([0-9]+) new coverage edges$",
        text, "result",
    )
    if (
        outer != (count, 0, 0) or inner != (count, 0, count) or attempts != (1,)
        or done[0] != count or min(done[1:]) < 1
        or added[0] != len(after) or min(added) < 1
        or added[1:] != (done[2], done[1])
    ):
        raise ValueError("minimizer evidence does not match complete bounded input/output inventories")
    before_hashes = collections.Counter(entry["sha256"] for entry in before)
    after_hashes = collections.Counter(entry["sha256"] for entry in after)
    if (
        not after or after_hashes - before_hashes
        or sum(entry["bytes"] for entry in after) > sum(entry["bytes"] for entry in before)
    ):
        raise ValueError("minimizer output is empty, enlarged or not a subset of input contents")
    return {
        "processed_units": done[0], "retained_files": added[0],
        "instrumented_features": added[1], "instrumented_edges": added[2], "attempts": attempts[0],
    }


def replay_evidence(log: Path, files: list[Path]) -> int:
    diagnostic(log)
    count = execution_count(log)
    text = log.read_text(encoding="utf-8")
    running = re.findall(r"^Running: (.+)$", text, re.MULTILINE)
    executed = re.findall(r"^Executed (.+) in [0-9]+ ms$", text, re.MULTILINE)
    expected = [str(path) for path in files]
    if count != len(files) or running != expected or executed != expected:
        raise ValueError("exact replay requires one completed execution of every inventoried input")
    return count


def check_campaign(path: Path, before: dict, identity: str, configuration: dict, revision: str) -> dict:
    real_path(path)
    if not path.is_file() or path.stat().st_size > 1 << 20:
        raise ValueError("campaign provenance must be a bounded regular report")
    report = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(report, dict):
        raise ValueError("campaign provenance must be a report mapping")
    targets = report.get("targets", [])
    if (
        report.get("schema_version") != 1
        or report.get("status") != "passed" or report.get("mode") != "run"
        or report.get("source_sha256") != identity
        or report.get("source_revision") != revision
        or report.get("toolchain") != configuration["fuzz_rust_toolchain"]
        or report.get("cargo_fuzz") != "cargo-fuzz " + configuration["cargo_fuzz_version"]
        or not isinstance(targets, list) or len(targets) != len(TARGETS)
        or any(not isinstance(item, dict) for item in targets)
        or [item.get("target") for item in targets] != list(TARGETS)
    ):
        raise ValueError("missing, stale, unsupported or failed campaign provenance")
    evidence = []
    for name in ("seeds", "build"):
        item = report.get(name, {})
        log = path.parent / (name + ".log")
        diagnostic(log)
        if (
            not isinstance(item, dict)
            or item.get("exit_code") != 0 or item.get("timed_out") is not False
            or item.get("log") != log.name or not log.is_file()
        ):
            raise ValueError("missing or failed generated seed/build provenance")
        evidence.append(log)
    total = 0
    for item in targets:
        count = item.get("executions")
        if (
            item.get("status") != "passed" or item.get("exit_code") != 0
            or item.get("timed_out") is not False or item.get("artifacts") != []
            or type(count) is not int or count < 1
            or item.get("corpus") != corpus_summary(before[item["target"]])
        ):
            raise ValueError("campaign evidence does not match current corpus")
        log = path.parent / (item["target"] + ".log")
        diagnostic(log)
        if item.get("log") != log.name or not log.is_file() or execution_count(log) != count:
            raise ValueError("campaign raw execution evidence is missing or does not match report")
        evidence.append(log)
        total += count
    if report.get("executions") != total:
        raise ValueError("campaign execution total is missing or ambiguous")
    return {"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
            "source_revision": report["source_revision"], "executions": total,
            "evidence": [{
                "name": log.name, "bytes": log.stat().st_size,
                "sha256": hashlib.sha256(log.read_bytes()).hexdigest(),
            } for log in evidence]}


def maintain(arguments, root: Path = ROOT) -> dict:
    if sys.platform != "linux":
        raise ValueError("bounded corpus maintenance requires Linux process groups")
    output = real_path(arguments.output_dir)
    if not arguments.generated:
        inputs = real_path(arguments.corpus_dir)
        if output == inputs or output in inputs.parents or inputs in output.parents:
            raise ValueError("input and maintenance output directories must not overlap")
    output.mkdir(parents=True, exist_ok=False)
    started = time.monotonic()
    deadline = started + TOTAL_SECONDS
    report = {
        "schema_version": 1, "status": "failed", "operations": [], "targets": [],
        "limits": {
            "total_seconds": TOTAL_SECONDS, "build_seconds": BUILD_SECONDS,
            "merge_seconds": MERGE_SECONDS, "replay_seconds": REPLAY_SECONDS,
            "input_seconds": INPUT_TIMEOUT, "rss_mb": RSS_LIMIT_MB,
            "max_input_bytes": MAX_INPUT_BYTES, "max_files_per_target": MAX_FILES_PER_TARGET,
            "max_total_files": MAX_TOTAL_FILES, "max_total_bytes": MAX_CORPUS_BYTES,
        },
        "coverage_claim": "libFuzzer merge feature/edge feedback only; not source-line coverage preservation",
    }
    try:
        validate_targets(root, list(TARGETS))
        configuration = json.loads((root / "toolchain.json").read_text())
        identity = source_identity(root)
        cargo = ["cargo", "+" + configuration["fuzz_rust_toolchain"]]
        environment = fuzz_environment()

        def operation(command: list, name: str, seconds: int) -> Path:
            remaining = int(deadline - time.monotonic()) - 5
            if remaining < 1:
                raise ValueError("corpus maintenance total deadline exhausted")
            log = output / (name + ".log")
            result = execute(command, root, environment, log, min(seconds, remaining))
            report["operations"].append(result)
            if result["exit_code"] != 0 or result["timed_out"] or not log.is_file():
                raise ValueError(f"corpus operation failed or timed out: {name}; diagnostics retained")
            return log

        version_log = operation(cargo + ["fuzz", "--version"], "cargo-fuzz-version", 10)
        version = version_log.read_text().strip()
        if version != "cargo-fuzz " + configuration["cargo_fuzz_version"]:
            raise ValueError("unexpected cargo-fuzz version")
        rustc = operation(
            ["rustc", "+" + configuration["fuzz_rust_toolchain"], "-vV"], "rustc-version", 10,
        ).read_text()
        cargo_version = operation(cargo + ["-V"], "cargo-version", 10).read_text().strip()
        operation(cargo + ["fuzz", "cmin", "--help"], "cmin-help", 10)
        report.update({
            "source_sha256": identity, "toolchain": configuration["fuzz_rust_toolchain"],
            "cargo_fuzz": version, "rustc_verbose": rustc, "cargo_version": cargo_version,
            "source_revision": subprocess.run(
                ["git", "rev-parse", "HEAD"], cwd=root, capture_output=True,
                text=True, check=True, timeout=10,
            ).stdout.strip(),
            "working_tree_dirty": bool(subprocess.run(
                ["git", "status", "--porcelain"], cwd=root, capture_output=True,
                text=True, check=True, timeout=10,
            ).stdout.strip()),
        })
        if arguments.generated:
            if arguments.install or arguments.campaign_report is not None:
                raise ValueError("generated validation cannot install or consume a campaign report")
            inputs = output / "generated-input"
            operation(
                cargo + ["run", "--quiet", "--example", "fuzz_seeds", "--", str(inputs)],
                "generated-seeds", 60,
            )
            report["provenance"] = "existing first-party fuzz_seeds example; no additional input generator"
        elif arguments.campaign_report is None:
            raise ValueError("maintenance requires a successful matching generated-campaign report")
        before = collection(inputs)
        report["before"] = before
        report["before_summary"] = {target: corpus_summary(entries) for target, entries in before.items()}
        if not arguments.generated:
            report["campaign"] = check_campaign(
                arguments.campaign_report, before, identity, configuration, report["source_revision"],
            )
        preserved = output / "preserved-input"
        copy_collection(inputs, preserved, before)
        staged = output / "minimized"
        copy_collection(preserved, staged, before)
        if collection(preserved) != before or collection(staged) != before or collection(inputs) != before:
            raise ValueError("input corpus changed during owned staging")
        report["preserved_input_path"] = str(preserved)
        report["staged_before"] = collection(staged)
        operation(cargo + ["fuzz", "build"], "build", BUILD_SECONDS)
        for target in TARGETS:
            artifacts = output / "artifacts" / target
            artifacts.mkdir(parents=True)
            limits = [
                f"-timeout={INPUT_TIMEOUT}", f"-rss_limit_mb={RSS_LIMIT_MB}",
                f"-max_len={MAX_INPUT_BYTES}", f"-artifact_prefix={artifacts}{os.sep}",
                "-print_final_stats=1",
            ]
            outcome = {"target": target, "status": "failed"}
            report["targets"].append(outcome)

            def replay(directory: Path, entries: list, phase: str) -> int:
                files = [directory / entry["name"] for entry in entries]
                log = operation(
                    cargo + ["fuzz", "run", target] + [str(path) for path in files]
                    + ["--"] + limits + ["-runs=1", f"-max_total_time={REPLAY_SECONDS}"],
                    target + "-" + phase, REPLAY_SECONDS,
                )
                return replay_evidence(log, files)

            outcome["before_replay_executions"] = replay(preserved / target, before[target], "before")
            log = operation(
                cargo + ["fuzz", "cmin", target, str(staged / target), "--"] + limits
                + [f"-max_total_time={MERGE_SECONDS}", f"-runs={MAX_FILES_PER_TARGET + 1}"],
                target + "-cmin", MERGE_SECONDS,
            )
            after = inventory(staged / target, MAX_INPUT_BYTES, MAX_FILES_PER_TARGET)
            outcome["after"] = after
            outcome["minimizer"] = merge_evidence(log, before[target], after)
            outcome["after_replay_executions"] = replay(staged / target, after, "after")
            if inventory(artifacts, MAX_INPUT_BYTES, MAX_FILES_PER_TARGET):
                raise ValueError(f"unexpected reproducer during corpus maintenance: {target}")
            outcome["status"] = "passed"
        after = collection(staged)
        if after != {item["target"]: item["after"] for item in report["targets"]}:
            raise ValueError("minimized corpus changed during replay")
        if (
            collection(inputs) != before or collection(preserved) != before
            or source_identity(root) != identity
        ):
            raise ValueError("input corpus or source changed during maintenance")
        if not arguments.generated and check_campaign(
            arguments.campaign_report, before, identity, configuration, report["source_revision"],
        ) != report["campaign"]:
            raise ValueError("campaign provenance changed during maintenance")
        if time.monotonic() >= deadline:
            raise ValueError("corpus maintenance total deadline exhausted")
        report["after"] = after
        report["after_summary"] = {target: corpus_summary(entries) for target, entries in after.items()}
        report["input_preserved"] = True
        report["installed"] = False
        if arguments.install:
            replaced = output / "replaced-input"
            inputs.rename(replaced)
            try:
                staged.rename(inputs)
            except OSError:
                replaced.rename(inputs)
                raise
            report["installed"] = True
            report["replaced_input_path"] = str(replaced)
        report["replay_executions"] = sum(
            item["before_replay_executions"] + item["after_replay_executions"]
            for item in report["targets"]
        )
        report["merge_processed_units"] = sum(
            item["minimizer"]["processed_units"] for item in report["targets"]
        )
        report["status"] = "passed"
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
        report["error"] = str(error)
        print(f"corpus maintenance failed: {error}", file=sys.stderr)
    report["seconds"] = time.monotonic() - started
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--generated", action="store_true", help="validate only existing generated seeds")
    source.add_argument("--corpus-dir", type=Path)
    parser.add_argument("--campaign-report", type=Path)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--install", action="store_true", help="install only after all nine targets pass")
    arguments = parser.parse_args()
    try:
        report = maintain(arguments)
    except (OSError, ValueError) as error:
        print(f"corpus maintenance setup failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps({key: report[key] for key in (
        "status", "seconds", "replay_executions", "merge_processed_units",
    ) if key in report}, indent=2))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Run bounded semantic checks, or strictly collect same-run CI evidence."""

import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time


ROOT = Path(__file__).resolve().parents[1]
WORKERS = 2
CI_SHARDS = 6
WHOLE_GROUPS = ("arithmetic-codec", "value-bridge")
COLLECTOR_GROUPS = ("arithmetic-decision", "document-count", "reset")
RECEIPT_SCHEMA = 3
CHECKS = (
    ("arithmetic-codec", "check_arithmetic_codec_proofs.py", 19, "strong_contract", "weaker_control"),
    ("value-bridge", "check_value_bridge_proofs.py", 5, "strong_contract", "weaker_control"),
    ("arithmetic-decision", "check_arithmetic_decision_proofs.py", 15, "strong_contract", "weaker_control"),
    ("document-count", "check_document_count_proof.py", 13, "strong_count_contract", "weaker_control"),
    ("reset", "check_reset_proof.py", 2, "strong_zero_contract", "validity_only_control"),
    ("mmr-reference", "check_mmr_reference_proofs.py", 11, "strong_contract", "weaker_control"),
    ("document-layout", "check_document_count_proof.py", 12, "strong_count_contract", "weaker_control"),
    ("page-state", "check_page_state_proofs.py", 13, "strong_contract", "weaker_control"),
    ("text-batch", "check_text_batch_proofs.py", 11, "strong_contract", "weaker_control"),
    ("symbol-cardinality", "check_symbol_cardinality_proofs.py", 11, "strong_contract", "weaker_control"),
    ("segment-data", "check_segment_data_proofs.py", 10, "strong_contract", "weaker_control"),
    ("reader-error", "check_reader_error_proofs.py", 9, "strong_contract", "weaker_control"),
    ("page-public", "check_page_state_proofs.py", 9, "strong_contract", "weaker_control"),
    ("arithmetic-integer", "check_arithmetic_integer_proofs.py", 9, "strong_contract", "weaker_control"),
    ("halftone-header", "check_halftone_header_proofs.py", 9, "strong_contract", "weaker_control"),
    ("integer-state", "check_integer_state_proofs.py", 9, "strong_contract", "weaker_control"),
    ("segment-header", "check_segment_header_proofs.py", 10, "strong_contract", "weaker_control"),
    ("black-span", "check_black_span_proofs.py", 6, "strong_contract", "weaker_control"),
    ("page-finish", "check_page_state_proofs.py", 7, "strong_contract", "weaker_control"),
    ("mmr-horizontal", "check_mmr_horizontal_proofs.py", 6, "strong_contract", "weaker_control"),
    ("bitmap-compose", "check_bitmap_compose_proofs.py", 7, "strong_contract", "weaker_control"),
    ("bitmap-copy", "check_bitmap_copy_proofs.py", 8, "strong_contract", "weaker_control"),
    ("mmr-span", "check_mmr_span_proofs.py", 8, "strong_contract", "weaker_control"),
    ("arithmetic-refill", "check_arithmetic_refill_proofs.py", 6, "strong_contract", "weaker_control"),
    ("arithmetic-normalization", "check_arithmetic_normalization_proofs.py", 6, "strong_contract", "weaker_control"),
    ("iaid-state", "check_iaid_state_proofs.py", 6, "strong_contract", "weaker_control"),
    ("arithmetic-initialization", "check_arithmetic_initialization_proofs.py", 6, "strong_contract", "weaker_control"),
    ("mmr-run", "check_mmr_run_sequence_proofs.py", 4, "strong_contract", "weaker_control"),
    ("huffman-assignment", "check_huffman_assignment_proofs.py", 4, "strong_contract", "weaker_control"),
    ("region-dispatch", "check_region_dispatch_proofs.py", 3, "strong_contract", "weaker_control"),
    ("operator-bit", "check_operator_bit_proofs.py", 4, "strong_contract", "weaker_control"),
    ("retention", "check_retention_proofs.py", 3, "strong_contract", "weaker_control"),
    ("bitmap-chunk", "check_bitmap_chunk_proofs.py", 4, "strong_contract", "weaker_control"),
    ("mmr-prefix", "check_mmr_prefix_proofs.py", 4, "strong_contract", "weaker_control"),
)

SUMMARY = "semantic-proof-summary.json"
WHOLE_SUMMARY = "whole-proof-summary.json"
TRACE = Path("target/traceability")


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read_json(path):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError(f"duplicate JSON key: {key}")
            result[key] = value
        return result
    def invalid_constant(value):
        raise ValueError(f"non-JSON numeric constant: {value}")
    return json.loads(
        path.read_text(encoding="utf-8"), object_pairs_hook=unique, parse_constant=invalid_constant,
    )


def write_json(path, document):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")


def configured_checks(only=None):
    identities = [row[0] for row in CHECKS]
    if not identities or len(set(identities)) != len(identities):
        raise ValueError("missing or duplicate configured semantic checks")
    if any(
        not re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*", identity) or identity == "whole"
        or type(count) is not int or count < 1
        for identity, _, count, _, _ in CHECKS
    ):
        raise ValueError("invalid configured semantic group identity/count")
    if only is not None and only not in identities:
        raise ValueError(f"unknown semantic group: {only}")
    return tuple(row for row in CHECKS if only is None or row[0] == only)


def configured_shards():
    checks = tuple(
        check for check in configured_checks()
        if check[0] not in (*WHOLE_GROUPS, *COLLECTOR_GROUPS)
    )
    count = min(CI_SHARDS, len(checks))
    return tuple(checks[index::count] for index in range(count))


def configured_whole_checks():
    return tuple(check for check in configured_checks() if check[0] in WHOLE_GROUPS)


def configured_collector_checks():
    return tuple(check for check in configured_checks() if check[0] in COLLECTOR_GROUPS)


def run_identity():
    names = ("GITHUB_REPOSITORY", "GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT", "GITHUB_SHA")
    values = {name: os.environ.get(name, "local") for name in names}
    if any(value != "local" for value in values.values()) and "local" in values.values():
        raise ValueError("incomplete workflow run identity")
    return values


def artifact_prefix():
    identity = run_identity()
    return f"verus-partial-{identity['GITHUB_RUN_ID']}-{identity['GITHUB_RUN_ATTEMPT']}-"


def agent_profile_blobs(root):
    if not (root / ".git").exists():
        return {}
    status = subprocess.run(
        ["git", "status", "--porcelain", "--", ".github/agents"],
        cwd=root, capture_output=True, text=True, check=True,
    )
    if status.stdout:
        raise ValueError("agent profiles must match their committed opaque identities")
    tree = subprocess.run(
        ["git", "ls-tree", "-r", "HEAD", "--", ".github/agents"],
        cwd=root, capture_output=True, text=True, check=True,
    )
    return {
        name: metadata.split()[2]
        for line in tree.stdout.splitlines()
        for metadata, name in [line.split("\t", 1)]
    }


def manifest(root):
    checks = configured_checks()
    paths = set()
    for directory, patterns in (
        ("config", ("*.json",)), ("verification", ("*.json",)),
        ("src", ("*.rs",)), ("specs", ("*.rs",)), ("proofs", ("*.rs",)),
        ("tools", ("*.py", "*.txt", "*.json")), ("docs", ("*.md",)),
        ("validation", ("*.json", "*.md")), (".github", ("*.yml", "*.yaml", "*.md")),
    ):
        for pattern in patterns:
            paths.update(
                path for path in (root / directory).rglob(pattern)
                if not any(part in {"target", "__pycache__", "corpus", "artifacts"}
                           for part in path.relative_to(root).parts)
                and path.relative_to(root).parts[:2] != (".github", "agents")
            )
    paths.update(root / name for name in (
        "Cargo.toml", "Cargo.lock", "README.md", "SECURITY.md", "LICENSE",
        "verification/Cargo.toml", "verification/Cargo.lock",
    ) if (root / name).is_file())
    return {
        "schema_version": RECEIPT_SCHEMA,
        "groups": [
            {"id": identity, "checker": script, "counterexamples": expected,
             "strong": strong, "weak": weak}
            for identity, script, expected, strong, weak in checks
        ],
        "shards": [[check[0] for check in shard] for shard in configured_shards()],
        "whole_groups": [check[0] for check in configured_whole_checks()],
        "collector_groups": [check[0] for check in configured_collector_checks()],
        "verus": read_json(root / "config/toolchain.json")["verus"],
        "agent_profile_blobs": agent_profile_blobs(root),
        "source_bindings": {
            path.relative_to(root).as_posix(): digest(path) for path in sorted(paths)
        },
    }


def baseline_passed(result, whole=None, targets=()):
    if isinstance(result, dict) and result.get("is-verifying-entire-crate") is True:
        if whole is None:
            return False
        try:
            names = result["whole_positive"]["functions"]
            return (
                set(targets) <= set(names)
                and result == baseline_from_whole(whole, names)
            )
        except (ValueError, KeyError, TypeError):
            return False
    return (
        isinstance(result, dict) and result.get("success", True) is True
        and result.get("encountered-error") is False and result.get("encountered-vir-error") is False
        and type(result.get("errors")) is int and result["errors"] == 0
        and result.get("is-verifying-entire-crate") is False
        and type(result.get("verified")) is int and result["verified"] > 0
    )


def check_report(report, expected, strong, weak, version):
    if (
        not isinstance(report, dict)
        or report.get("status") != "passed"
        or report.get("verus_version") != version
        or report.get("production_sources_modified") is not False
        or not isinstance(report.get("counterexamples"), list)
        or len(report["counterexamples"]) != expected
    ):
        raise ValueError("missing or incompatible semantic proof evidence")
    records = report["counterexamples"]
    if len({json.dumps(record, sort_keys=True) for record in records}) != expected:
        raise ValueError("duplicate semantic proof evidence")
    for record in records:
        if (
            not isinstance(record, dict)
            or record.get("normal_rust_compiles") is not True
            or record.get(strong) != "rejected"
            or record.get(weak) != "accepted"
            or type(record.get("strong_verifier_errors")) is not int
            or record["strong_verifier_errors"] < 1
            or type(record.get("control_verifier_errors")) is not int
            or record["control_verifier_errors"] != 0
        ):
            raise ValueError("semantic violation/control evidence did not pass")


def check_bound_report(root, check, report, bindings, whole=None):
    identity, script, expected, strong, weak = check
    check_report(report, expected, strong, weak, read_json(root / "config/toolchain.json")["verus"]["version"])
    if report.get("schema_version") != (1 if identity in {"reset", "document-count", "document-layout"} else 2):
        raise ValueError("incompatible semantic report schema")
    if identity == "reset":
        if (
            report.get("source_sha256") != bindings.get("src/arithmetic.rs")
            or not baseline_passed(
                report.get("baseline_selected_verification"), whole,
                ("jbig2_rs_production::arithmetic::reset_contexts",),
            )
            or [(row.get("state_index"), row.get("mps")) for row in report["counterexamples"]]
            != [(1, 0), (0, 1)]
        ):
            raise ValueError("stale reset source or missing selected baseline")
    else:
        sources = report.get("source_bindings")
        if not isinstance(sources, dict) or not sources or any(
            name not in bindings or value != bindings[name] for name, value in sources.items()
        ):
            raise ValueError("missing or stale semantic source bindings")
        spec = importlib.util.spec_from_file_location("semantic_checker", root / "tools" / script)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        if hasattr(module, "CASE_GROUPS"):
            if report.get("case_group") != identity:
                raise ValueError("missing or misrouted checker case group")
            cases = module.CASE_GROUPS[identity]
        else:
            cases = module.CASES
        if len(cases) != expected or [row.get("id") for row in report["counterexamples"]] != [
            case["id"] for case in cases
        ] or any(
            not baseline_passed(
                row.get("baseline_selected_verification"), whole,
                ("jbig2_rs_production::" + (
                    "::".join([case["module"], case["function"]])
                    if "module" in case else "document::" + case["function"]
                ),),
            )
            for case, row in zip(cases, report["counterexamples"])
        ) or [row.get("function") for row in report["counterexamples"]] != [
            "::".join([case["module"], case["function"]]) if "module" in case else case["function"]
            for case in cases
        ]:
            raise ValueError("missing, duplicate or stale selected cases/baselines")
        if identity in {"document-count", "document-layout"}:
            functions = sorted({case["function"] for case in cases})
            if (
                report.get("baseline_verification_scope") != "document::*"
                or report.get("baseline_selected_functions") != functions
                or any(
                    row["baseline_selected_verification"]["verified"] < len(functions)
                    for row in report["counterexamples"]
                )
            ):
                raise ValueError("missing or incomplete shared document-module baseline")
        baseline_groups = getattr(module, "BASELINE_GROUPS", ())
        if baseline_groups:
            expected_groups = [
                {"module": name, "functions": list(functions)}
                for name, functions in baseline_groups
            ]
            if report.get("baseline_groups") != expected_groups:
                raise ValueError("missing or stale shared positive baseline groups")
            for case, record in zip(cases, report["counterexamples"]):
                expected_functions = next(
                    (list(functions) for name, functions in baseline_groups
                     if name == case["module"] and case["function"] in functions),
                    [case["function"]],
                )
                if (
                    record.get("baseline_selected_functions") != expected_functions
                    or record["baseline_selected_verification"]["verified"] < len(expected_functions)
                ):
                    raise ValueError("shared positive baseline omits a selected function")
        if any(case["file"] not in sources for case in cases if "file" in case):
            raise ValueError("semantic case edits an unbound production source")


def run_checks(
    root, executable, output, only=None, shard=None, whole_baseline=None,
    whole_groups=False, collector_groups=False,
):
    output.mkdir(parents=True, exist_ok=True)
    (output / SUMMARY).unlink(missing_ok=True)
    if sum((only is not None, shard is not None, whole_groups, collector_groups)) > 1:
        raise ValueError("group and shard selectors are mutually exclusive")
    checks = configured_checks(only)
    if whole_groups:
        checks = configured_whole_checks()
        if not checks:
            raise ValueError("no configured whole-job semantic groups")
    if collector_groups:
        checks = configured_collector_checks()
        if not checks:
            raise ValueError("no configured collector-job semantic groups")
    if shard is not None:
        shards = configured_shards()
        if type(shard) is not int or not 0 <= shard < len(shards):
            raise ValueError(f"unknown semantic shard: {shard}")
        checks = shards[shard]
    current = manifest(root)
    whole = load_whole_baseline(root, whole_baseline) if whole_baseline is not None else None
    environment = os.environ.copy()
    environment.pop("JBIG2_WHOLE_BASELINE", None)
    if whole_baseline is not None:
        environment["JBIG2_WHOLE_BASELINE"] = str(whole_baseline.resolve())
    identity_binding = run_identity()
    version = current["verus"]["version"]
    workers = 1 if only is not None or shard is not None or whole_groups or collector_groups else WORKERS

    def execute(check):
        identity, script, expected, strong, weak = check
        report = output / f"{identity}-proof-sensitivity.json"
        log = output / f"{identity}-proof-sensitivity.log"
        started = time.monotonic()
        result = {"id": identity, "status": "failed", "report": report.name, "log": log.name}
        try:
            report.unlink(missing_ok=True)
            source = root / "tools" / script
            result["checker_sha256"] = digest(source)
            selection = ["--case-group", identity] if script in {
                "check_document_count_proof.py", "check_page_state_proofs.py",
            } else []
            with log.open("w", encoding="utf-8") as stream:
                process = subprocess.run(
                    [sys.executable, "-B", str(source), "--verus-executable",
                     str(executable), *selection, "--output", str(report)],
                    cwd=root, stdout=stream, stderr=subprocess.STDOUT, check=False, env=environment,
                )
            result["exit_code"] = process.returncode
            if process.returncode != 0:
                raise ValueError(f"checker exited {process.returncode}; see {log}")
            check_bound_report(root, check, read_json(report), current["source_bindings"], whole)
            result.update(
                status="passed", counterexamples=expected,
                report_sha256=digest(report), log_sha256=digest(log),
            )
        except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
            result["error"] = str(error)
        result["seconds"] = time.monotonic() - started
        return result

    with ThreadPoolExecutor(max_workers=workers) as executor:
        results = list(executor.map(execute, checks))
    if manifest(root) != current:
        raise ValueError("source changed during semantic checks")
    summary = {
        "schema_version": RECEIPT_SCHEMA,
        "status": "passed" if all(row["status"] == "passed" for row in results) else "failed",
        "workers": workers,
        "verus_version": version,
        "manifest": current, "run_identity": identity_binding,
        "selected_groups": [row[0] for row in checks],
        "shard": shard,
        "checks": results,
        "scope": (
            "All configured isolated semantic contrasts; whole-crate verification remains separate."
            if only is None and shard is None and not whole_groups and not collector_groups else
            "Complete selected groups; whole-crate and all-shard collection remain separate."
        ),
    }
    write_json(output / SUMMARY, summary)
    return summary


def whole_files(directory):
    paths = [
        path for path in sorted(directory.rglob("*"))
        if path.is_file() and path != directory / WHOLE_SUMMARY
        and path.relative_to(directory).parts[0] != "semantic-groups"
    ]
    if any(path.is_symlink() for path in directory.rglob("*")):
        raise ValueError("whole-crate artifact must not contain symlinks")
    if not (directory / TRACE / "verifier-report.json").is_file() or not any(
        path.stat().st_size > 0 for path in (directory / "target/verus-production").glob("*.rlib")
    ):
        raise ValueError("missing whole-crate traceability or native compilation")
    return {path.relative_to(directory).as_posix(): digest(path) for path in paths}


def validate_whole(root, directory):
    from evidence import validate
    from jsonschema import ValidationError

    report = read_json(directory / "verus-production.json")
    inventory = directory / "production-proof-coverage-ci.json"
    if inventory.read_bytes() != (root / "verification/production-proof-coverage.json").read_bytes():
        raise ValueError("whole-crate production inventory differs from checked-in inventory")
    coverage = read_json(inventory)
    try:
        validate(report, "verus")
        validate(coverage, "coverage")
    except ValidationError as error:
        raise ValueError(f"invalid whole-crate evidence: {error.message}") from error
    if any(report["verification-results"].get(flag, False) is not False for flag in (
        "encountered-error", "encountered-vir-error",
    )):
        raise ValueError("whole-crate verifier encountered an error")
    tool = read_json(root / "config/toolchain.json")["verus"]
    actual_tool = report.get("verus", {})
    if (
        actual_tool.get("version") != tool["version"]
        or actual_tool.get("commit") != tool.get("commit")
        or ("rust_toolchain" in tool and not actual_tool.get("toolchain", "").startswith(tool["rust_toolchain"] + "-"))
    ):
        raise ValueError("whole-crate verifier tool identity differs from pins")
    if (
        coverage["verus_verified_obligations"] != report["verification-results"]["verified"]
        or coverage["counts"]["incomplete"] != 0 or coverage["native_assumptions"] != []
        or any(
            name not in report["func-details"] for row in coverage["functions"]
            for name in row["verus_functions"]
        )
    ):
        raise ValueError("whole-crate verification/inventory mismatch")
    trace = read_json(directory / TRACE / "verifier-report.json")
    ledger_path = root / "validation/traceability/requirements.json"
    ledger = read_json(ledger_path)
    if trace.get("verifier_report_checked") is not True or trace.get("status") not in {
        "references_valid_scope_complete", "references_valid_scope_incomplete",
    } or (
        not trace.get("requirements") or trace.get("ledger_sha256") != digest(ledger_path)
        or (run_identity()["GITHUB_SHA"] != "local" and trace.get("source_revision") != run_identity()["GITHUB_SHA"])
        or trace.get("requirement_count") != len(ledger["requirements"])
        or trace.get("feature_count") != len(ledger["features"])
        or [row.get("id") for row in trace["requirements"]] != [row["id"] for row in ledger["requirements"]]
        or any(
            actual.get(key) != expected.get(key)
            for actual, expected in zip(trace["requirements"], ledger["requirements"])
            for key in ("strength", "review", "gaps")
        )
    ):
        raise ValueError("whole-crate traceability was not checked")
    return whole_files(directory)


def seal_whole(root, directory):
    directory.mkdir(parents=True, exist_ok=True)
    (directory / WHOLE_SUMMARY).unlink(missing_ok=True)
    current = manifest(root)
    files = validate_whole(root, directory)
    summary = {
        "schema_version": RECEIPT_SCHEMA, "status": "passed", "manifest": current,
        "run_identity": run_identity(), "files": files,
    }
    write_json(directory / WHOLE_SUMMARY, summary)
    return summary


def require_summary(summary, current):
    if (
        summary.get("schema_version") != RECEIPT_SCHEMA or summary.get("status") != "passed"
        or summary.get("manifest") != current or summary.get("run_identity") != run_identity()
    ):
        raise ValueError("failed, stale or incompatible artifact manifest/run/source/tool identity")


def load_whole_baseline(root, directory):
    summary_path = directory / WHOLE_SUMMARY
    summary = read_json(summary_path)
    require_summary(summary, manifest(root))
    if summary.get("files") != validate_whole(root, directory):
        raise ValueError("stale or modified whole-positive baseline")
    return {
        "receipt_sha256": digest(summary_path),
        "report_sha256": digest(directory / "verus-production.json"),
        "report": read_json(directory / "verus-production.json"),
        "manifest": summary["manifest"],
        "files": summary["files"],
    }


def baseline_from_whole(whole, functions):
    if (
        not isinstance(functions, (list, tuple)) or not functions
        or any(not isinstance(name, str) for name in functions)
        or len(set(functions)) != len(functions)
        or not set(functions) <= set(whole["report"]["func-details"])
    ):
        raise ValueError("whole-positive baseline omits exact selected targets")
    return {
        **whole["report"]["verification-results"],
        "whole_positive": {
            "receipt_sha256": whole["receipt_sha256"],
            "report_sha256": whole["report_sha256"], "functions": list(functions),
        },
    }


def measured_shard_seconds(rows):
    if len(rows) != len(configured_checks()) or {row["id"] for row in rows} != {
        check[0] for check in configured_checks()
    }:
        raise ValueError("timing requires every complete current-profile group")
    seconds = {}
    for row in rows:
        elapsed = row.get("seconds")
        if (
            row.get("status") != "passed" or type(elapsed) not in (int, float)
            or not math.isfinite(elapsed) or elapsed <= 0
        ):
            raise ValueError("timing requires successful complete measured groups")
        seconds[row["id"]] = elapsed
    measured = [sum(seconds[check[0]] for check in shard) for shard in configured_shards()]
    whole_seconds = sum(seconds[check[0]] for check in configured_whole_checks())
    collector_seconds = sum(seconds[check[0]] for check in configured_collector_checks())
    if max([whole_seconds, collector_seconds, *measured], default=0) > 1050:
        raise ValueError("complete measured semantic profile exceeds the 1050-second guard")
    return measured


def aggregate(root, inputs, output, whole_result, semantic_result):
    output.mkdir(parents=True, exist_ok=True)
    (output / TRACE / SUMMARY).unlink(missing_ok=True)
    if whole_result != "success" or semantic_result != "success":
        raise ValueError("whole and semantic jobs must both succeed (missing/skipped/failed is fatal)")
    current = manifest(root)
    prefix = artifact_prefix()
    checks = configured_checks()
    shards = configured_shards()
    collector_checks = configured_collector_checks()
    expected = {prefix + "whole", *(prefix + f"shard-{index}" for index in range(len(shards)))}
    if collector_checks:
        expected.add(prefix + "collector")
    if {path.name for path in inputs.iterdir()} != expected or any(
        not path.is_dir() or path.is_symlink() for path in inputs.iterdir()
    ):
        raise ValueError("missing, duplicate or unexpected whole/shard artifacts")
    whole = inputs / (prefix + "whole")
    whole_summary = read_json(whole / WHOLE_SUMMARY)
    require_summary(whole_summary, current)
    if whole_summary.get("files") != validate_whole(root, whole):
        raise ValueError("stale or modified whole-crate evidence")
    positive = load_whole_baseline(root, whole)
    rows_by_id = {}
    directories = {}
    producers = [
        (index, shard, inputs / (prefix + f"shard-{index}"))
        for index, shard in enumerate(shards)
    ]
    if collector_checks:
        producers.append((None, collector_checks, inputs / (prefix + "collector")))
    whole_checks = configured_whole_checks()
    if whole_checks:
        producers.append((None, whole_checks, whole / "semantic-groups"))
    whole_semantic_files = set()
    for index, shard, shard_root in producers:
        directory = shard_root / TRACE
        if any(path.is_symlink() for path in shard_root.rglob("*")):
            raise ValueError("semantic artifact must not contain symlinks")
        summary = read_json(directory / SUMMARY)
        require_summary(summary, current)
        if (
            summary.get("selected_groups") != [check[0] for check in shard]
            or type(summary.get("workers")) is not int or summary["workers"] != 1
            or "shard" not in summary or summary["shard"] != index
            or (index is not None and type(summary["shard"]) is not int)
            or summary.get("verus_version") != current["verus"]["version"]
            or not isinstance(summary.get("checks"), list) or len(summary["checks"]) != len(shard)
        ):
            raise ValueError(f"missing, duplicate or wrong shard summary: {index}")
        expected_files = {SUMMARY}
        for check, row in zip(shard, summary["checks"]):
            identity, script, expected_count, *_ = check
            report = directory / f"{identity}-proof-sensitivity.json"
            log = directory / f"{identity}-proof-sensitivity.log"
            if (
                row.get("id") != identity or row.get("status") != "passed"
                or type(row.get("exit_code")) is not int or row["exit_code"] != 0
                or type(row.get("counterexamples")) is not int or row["counterexamples"] != expected_count
                or row.get("checker_sha256") != digest(root / "tools" / script)
                or row.get("report") != report.name or row.get("log") != log.name
                or row.get("report_sha256") != digest(report) or row.get("log_sha256") != digest(log)
                or identity in rows_by_id
            ):
                raise ValueError(f"failed, duplicate or stale semantic group: {identity}")
            expected_files.update((report.name, log.name))
            check_bound_report(root, check, read_json(report), current["source_bindings"], positive)
            rows_by_id[identity] = row
            directories[identity] = directory
        if {path.name for path in directory.iterdir()} != expected_files:
            raise ValueError(f"unexpected, missing or duplicate shard evidence: {index}")
        if {
            path.relative_to(shard_root) for path in shard_root.rglob("*") if path.is_file()
        } != {TRACE / name for name in expected_files}:
            raise ValueError(f"unexpected files outside semantic shard evidence: {index}")
        if shard_root == whole / "semantic-groups":
            whole_semantic_files = {
                Path("semantic-groups") / TRACE / name for name in expected_files
            }
    if {
        path.relative_to(whole) for path in whole.rglob("*") if path.is_file()
    } != {Path(WHOLE_SUMMARY), *(Path(name) for name in whole_summary["files"]), *whole_semantic_files}:
        raise ValueError("unexpected files outside whole proof and whole-job semantic evidence")
    rows = [rows_by_id[check[0]] for check in checks]
    shard_seconds = measured_shard_seconds(rows)
    for name in whole_summary["files"]:
        destination = output / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(whole / name, destination)
    shutil.copyfile(whole / WHOLE_SUMMARY, output / WHOLE_SUMMARY)
    for row in rows:
        for name in (row["report"], row["log"]):
            shutil.copyfile(directories[row["id"]] / name, output / TRACE / name)
    result = {
        "schema_version": RECEIPT_SCHEMA, "status": "passed", "manifest": current,
        "run_identity": run_identity(), "verus_version": current["verus"]["version"],
        "selected_groups": [row[0] for row in checks], "checks": rows,
        "whole_crate": whole_summary, "workers": 1, "matrix_max_parallel": 4,
        "matrix_shards": len(shards),
        "measured_shard_seconds": shard_seconds,
        "measured_whole_group_seconds": sum(
            row["seconds"] for row in rows if row["id"] in WHOLE_GROUPS
        ),
        "measured_collector_group_seconds": sum(
            row["seconds"] for row in rows if row["id"] in COLLECTOR_GROUPS
        ),
        "scope": "Complete same-run whole-crate and all configured isolated semantic evidence.",
    }
    write_json(output / TRACE / SUMMARY, result)
    return result


def main():
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--only")
    mode.add_argument("--shard", type=int)
    mode.add_argument("--whole-groups", action="store_true")
    mode.add_argument("--collector-groups", action="store_true")
    mode.add_argument("--matrix", action="store_true")
    mode.add_argument("--seal-whole", type=Path)
    mode.add_argument("--aggregate", type=Path)
    parser.add_argument("--verus-executable", type=Path)
    parser.add_argument("--whole-baseline", type=Path)
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--whole-result")
    parser.add_argument("--semantic-result")
    arguments = parser.parse_args()
    if arguments.whole_baseline is not None and (
        arguments.matrix or arguments.seal_whole or arguments.aggregate
    ):
        parser.error("--whole-baseline is only valid for semantic execution")
    try:
        if arguments.matrix:
            print(json.dumps({"shard": list(range(len(configured_shards())))}))
            return 0
        if arguments.seal_whole:
            seal_whole(ROOT, arguments.seal_whole.resolve())
            return 0
        if arguments.output_dir is None:
            parser.error("--output-dir is required for execution/aggregation")
        if arguments.aggregate:
            result = aggregate(
                ROOT, arguments.aggregate.resolve(), arguments.output_dir.resolve(),
                arguments.whole_result, arguments.semantic_result,
            )
        else:
            if arguments.verus_executable is None:
                parser.error("--verus-executable is required for execution")
            result = run_checks(
                ROOT, arguments.verus_executable.resolve(), arguments.output_dir.resolve(),
                only=arguments.only, shard=arguments.shard,
                whole_baseline=arguments.whole_baseline,
                whole_groups=arguments.whole_groups,
                collector_groups=arguments.collector_groups,
            )
    except (OSError, ValueError, KeyError, TypeError, AttributeError, subprocess.SubprocessError) as error:
        print(f"semantic proof orchestration failed: {error}", file=sys.stderr)
        if arguments.aggregate and arguments.output_dir:
            write_json(arguments.output_dir / "aggregation-failure.json", {"status": "failed", "error": str(error)})
        return 1
    for check in result["checks"]:
        if check["status"] != "passed":
            print(f"semantic proof check {check['id']} failed: {check['error']}", file=sys.stderr)
    print(json.dumps({"status": result["status"], "checks": len(result["checks"]), "workers": result["workers"]}))
    return 0 if result["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

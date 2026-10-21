#!/usr/bin/env python3
"""Run all existing isolated semantic proof checks with bounded concurrency."""

import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time


ROOT = Path(__file__).resolve().parents[1]
WORKERS = 2
CHECKS = (
    ("reset", "check_reset_proof.py", 2, "strong_zero_contract", "validity_only_control"),
    ("document-count", "check_document_count_proof.py", 5, "strong_count_contract", "weaker_control"),
    ("bitmap-copy", "check_bitmap_copy_proofs.py", 6, "strong_contract", "weaker_control"),
    ("retention", "check_retention_proofs.py", 3, "strong_contract", "weaker_control"),
    ("value-bridge", "check_value_bridge_proofs.py", 5, "strong_contract", "weaker_control"),
    ("mmr-span", "check_mmr_span_proofs.py", 3, "strong_contract", "weaker_control"),
    ("region-dispatch", "check_region_dispatch_proofs.py", 3, "strong_contract", "weaker_control"),
    ("text-batch", "check_text_batch_proofs.py", 4, "strong_contract", "weaker_control"),
    ("bitmap-chunk", "check_bitmap_chunk_proofs.py", 4, "strong_contract", "weaker_control"),
    ("bitmap-compose", "check_bitmap_compose_proofs.py", 5, "strong_contract", "weaker_control"),
    ("black-span", "check_black_span_proofs.py", 6, "strong_contract", "weaker_control"),
    ("operator-bit", "check_operator_bit_proofs.py", 4, "strong_contract", "weaker_control"),
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


def run_checks(root, executable, output):
    output.mkdir(parents=True, exist_ok=True)
    (output / "semantic-proof-summary.json").unlink(missing_ok=True)
    version = json.loads((root / "toolchain.json").read_text())["verus"]["version"]
    if not CHECKS or len({entry[0] for entry in CHECKS}) != len(CHECKS):
        raise ValueError("missing or duplicate configured semantic checks")

    def execute(check):
        identity, script, expected, strong, weak = check
        report = output / f"{identity}-proof-sensitivity.json"
        log = output / f"{identity}-proof-sensitivity.log"
        started = time.monotonic()
        result = {"id": identity, "status": "failed", "report": report.name, "log": log.name}
        try:
            report.unlink(missing_ok=True)
            source = root / "tools" / script
            result["checker_sha256"] = hashlib.sha256(source.read_bytes()).hexdigest()
            with log.open("w", encoding="utf-8") as stream:
                process = subprocess.run(
                    [sys.executable, "-B", str(source), "--verus-executable",
                     str(executable), "--output", str(report)],
                    cwd=root, stdout=stream, stderr=subprocess.STDOUT, check=False,
                )
            result["exit_code"] = process.returncode
            if process.returncode != 0:
                raise ValueError(f"checker exited {process.returncode}; see {log}")
            check_report(json.loads(report.read_text()), expected, strong, weak, version)
            result.update(status="passed", counterexamples=expected)
        except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
            result["error"] = str(error)
        result["seconds"] = time.monotonic() - started
        return result

    with ThreadPoolExecutor(max_workers=WORKERS) as executor:
        results = list(executor.map(execute, CHECKS))
    summary = {
        "schema_version": 1,
        "status": "passed" if all(row["status"] == "passed" for row in results) else "failed",
        "workers": WORKERS,
        "verus_version": version,
        "checks": results,
        "scope": "All configured isolated semantic contrasts; whole-crate verification remains separate.",
    }
    (output / "semantic-proof-summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    return summary


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--verus-executable", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    arguments = parser.parse_args()
    try:
        result = run_checks(ROOT, arguments.verus_executable.resolve(), arguments.output_dir.resolve())
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"semantic proof orchestration failed: {error}", file=sys.stderr)
        return 1
    for check in result["checks"]:
        if check["status"] != "passed":
            print(f"semantic proof check {check['id']} failed: {check['error']}", file=sys.stderr)
    print(json.dumps({"status": result["status"], "checks": len(result["checks"]), "workers": WORKERS}))
    return 0 if result["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Seal and replay original project-only expanded IR from the pinned whole proof build."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import sys

from collect_compiler_inventory import validate_existing as validate_native
from evidence import load_json, read_evidence
from proof_mode_inventory import reconcile


ROOT = Path(__file__).resolve().parents[1]
TOOLS = (
    "tools/collect_proof_mode_inventory.py", "tools/proof_mode_inventory.py",
    "tools/check_traceability.py", "tools/rust_source.py", "tools/compiler_inventory.py",
    "tools/implementation_identity.py", "tools/evidence.py", "tools/evidence.schema.json",
    "config/toolchain.json", "verification/Cargo.toml", "verification/Cargo.lock",
    "tools/check_workflows.py", "tools/run_semantic_proofs.py", ".github/workflows/verus.yml",
)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def bindings(root):
    files = [root / name for name in TOOLS]
    files.extend(path for directory in ("src", "specs", "proofs")
                 for path in (root / directory).rglob("*.rs"))
    if any(path.is_symlink() or not path.resolve().is_relative_to(root.resolve()) for path in files):
        raise ValueError("expanded inventory input is symlinked or outside the checkout")
    return {path.relative_to(root).as_posix(): digest(path) for path in sorted(files)}


def reconstruct(root, export, whole_path, coverage_path, library_path, native_directory=None):
    if library_path.is_symlink() or library_path.stat().st_size == 0:
        raise ValueError("expanded inventory requires the original nonempty production library")
    whole = read_evidence(whole_path, "verus")
    coverage = read_evidence(coverage_path, "coverage")
    pins = load_json(root / "config/toolchain.json")["verus"]
    if (
        whole["verus"]["version"] != pins["version"] or whole["verus"]["commit"] != pins["commit"]
        or not whole["verus"]["toolchain"].startswith(pins["rust_toolchain"] + "-")
        or coverage != read_evidence(root / "verification/production-proof-coverage.json", "coverage")
    ):
        raise ValueError("expanded inventory requires exact pinned whole source/accounting evidence")
    native = validate_native(root, native_directory) if native_directory is not None else None
    return reconcile(root, export, native, whole, coverage)


def collect(root, vir, whole, coverage, library, output):
    text = vir.read_text()
    recomputed = reconstruct(root, text, whole, coverage, library)
    wanted = {row["expanded_form_sha256"] for row in recomputed["records"]}
    forms = [
        chunk.strip() for chunk in text.strip().split("\n\n")
        if hashlib.sha256(chunk.strip().encode()).hexdigest() in wanted
    ]
    if len(forms) != len(wanted):
        raise ValueError("project export did not retain each exact original function form")
    selected = "\n\n".join(forms) + "\n"
    if reconstruct(root, selected, whole, coverage, library) != recomputed:
        raise ValueError("project-only export differs from original expanded functions")
    output.mkdir(parents=True, exist_ok=True)
    export = output / "project-functions.vir"
    export.write_text(selected)
    report = {
        **recomputed, "source_bindings": bindings(root),
        "original_raw_export_sha256": digest(vir),
        "original_raw_export_bytes": vir.stat().st_size,
        "artifacts": {
            "project-functions.vir": {"sha256": digest(export), "bytes": export.stat().st_size},
            "verus-production.json": {"sha256": digest(whole), "bytes": whole.stat().st_size},
            "production-proof-coverage-ci.json": {"sha256": digest(coverage), "bytes": coverage.stat().st_size},
            "production-library.rlib": {"sha256": digest(library), "bytes": library.stat().st_size},
        },
        "profile": {
            "verus": load_json(root / "config/toolchain.json")["verus"],
            "unfiltered_whole": True, "raw_export_rule": "--log vir",
            "retention": "exact project function forms only; dependency function bodies not copied",
        },
    }
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    return report


def validate_existing(root, output, whole, coverage, library, native_directory=None):
    report = load_json(output / "report.json")
    if (
        not isinstance(report["original_raw_export_sha256"], str)
        or re.fullmatch(r"[0-9a-f]{64}", report["original_raw_export_sha256"]) is None
        or type(report["original_raw_export_bytes"]) is not int
        or report["original_raw_export_bytes"] <= 0
    ):
        raise ValueError("invalid original raw export identity metadata")
    export = output / "project-functions.vir"
    if report["source_bindings"] != bindings(root) or export.is_symlink():
        raise ValueError("stale or symlinked expanded inventory source bindings")
    paths = {"project-functions.vir": export, "verus-production.json": whole,
             "production-proof-coverage-ci.json": coverage, "production-library.rlib": library}
    if report["artifacts"] != {
        name: {"sha256": digest(path), "bytes": path.stat().st_size} for name, path in paths.items()
    }:
        raise ValueError("stale or altered expanded inventory artifacts")
    expected_profile = {
        "verus": load_json(root / "config/toolchain.json")["verus"],
        "unfiltered_whole": True, "raw_export_rule": "--log vir",
        "retention": "exact project function forms only; dependency function bodies not copied",
    }
    if report["profile"] != expected_profile:
        raise ValueError("wrong expanded inventory production profile")
    recomputed = reconstruct(root, export.read_text(), whole, coverage, library)
    if any(report.get(key) != value for key, value in recomputed.items()):
        raise ValueError("expanded inventory claims differ from original function reconstruction")
    if native_directory is not None:
        return reconstruct(root, export.read_text(), whole, coverage, library, native_directory)
    return report


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--vir", type=Path)
    parser.add_argument("--verus-json", type=Path, required=True)
    parser.add_argument("--coverage-json", type=Path, required=True)
    parser.add_argument("--compiled-library", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--check-existing", action="store_true")
    parser.add_argument("--native-inventory", type=Path)
    arguments = parser.parse_args()
    try:
        if arguments.check_existing:
            report = validate_existing(ROOT, arguments.output_dir, arguments.verus_json,
                                       arguments.coverage_json, arguments.compiled_library,
                                       arguments.native_inventory)
        else:
            if arguments.vir is None:
                raise ValueError("collection requires the original raw --log vir export")
            report = collect(ROOT, arguments.vir, arguments.verus_json, arguments.coverage_json,
                             arguments.compiled_library, arguments.output_dir)
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(f"expanded proof inventory failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps({"status": report["status"], "counts": report["counts"],
                      "ready_for_external_validation": False}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

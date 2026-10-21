#!/usr/bin/env python3
"""Check executable catalog links, not semantic faithfulness or conformance."""

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path

from jsonschema import ValidationError

from evidence import load_json, validate
from rust_tests import discover_tests, test_inventory


ROOT = Path(__file__).resolve().parents[1]
CATALOGS = {"negative": "NEG-", "metamorphic": "META-"}


def check(documents, root, discovered=None):
    if set(documents) != set(CATALOGS):
        raise ValueError("both negative and metamorphic catalogs are required")
    inventory = test_inventory(root)
    identifiers = set()
    rows = []
    bindings = {}
    for kind, prefix in CATALOGS.items():
        document = documents[kind]
        validate(document, "test_catalog")
        for case in document["cases"]:
            identity = case["id"]
            if not identity.startswith(prefix) or identity in identifiers:
                raise ValueError(f"wrong catalog kind or duplicate case: {identity}")
            identifiers.add(identity)
            references = set()
            locations = []
            for reference in case["tests"]:
                key = (reference["target"], reference["name"])
                if key in references:
                    raise ValueError(f"duplicate executable reference: {identity}: {key}")
                references.add(key)
                if key not in inventory or inventory[key]["ignored"]:
                    raise ValueError(f"missing or ignored executable test: {identity}: {key}")
                if discovered is not None and key not in discovered:
                    raise ValueError(f"test absent from Cargo discovery: {identity}: {key}")
                record = inventory[key]
                locations.append(record)
                path = root / record["file"]
                bindings[record["file"]] = hashlib.sha256(path.read_bytes()).hexdigest()
            rows.append({"catalog": kind, **case, "test_locations": locations})
    return {
        "schema_version": 1, "status": "references_valid",
        "case_count": len(rows), "cases": rows, "source_bindings": bindings,
        "reference_count": sum(len(row["tests"]) for row in rows),
        "unique_test_count": len({
            (test["target"], test["name"]) for row in rows for test in row["tests"]
        }),
        "cargo_test_discovery_checked": discovered is not None,
        "tests_executed_by_this_command": False,
        "note": "Exact links and discovery are checked, not English-to-assertion faithfulness, code coverage or normative conformance. Native CI separately executes the ordinary tests.",
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--discover-tests", action="store_true")
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    try:
        paths = {kind: ROOT / "validation" / kind / "cases.json" for kind in CATALOGS}
        documents = {kind: load_json(path) for kind, path in paths.items()}
        for document in documents.values():
            validate(document, "test_catalog")
        targets = {
            test["target"] for document in documents.values()
            for case in document["cases"] for test in case["tests"]
        }
        discovered = discover_tests(ROOT, targets) if arguments.discover_tests else None
        report = check(documents, ROOT, discovered)
        report["source_revision"] = subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True, text=True, check=True,
        ).stdout.strip()
        report["catalog_sha256"] = {
            kind: hashlib.sha256(path.read_bytes()).hexdigest() for kind, path in paths.items()
        }
        arguments.output.parent.mkdir(parents=True, exist_ok=True)
        arguments.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({key: report[key] for key in (
            "status", "case_count", "reference_count", "unique_test_count",
            "cargo_test_discovery_checked", "tests_executed_by_this_command",
        )}, indent=2))
        return 0
    except (OSError, ValueError, KeyError, ValidationError, subprocess.CalledProcessError) as error:
        print(f"test catalog validation failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())

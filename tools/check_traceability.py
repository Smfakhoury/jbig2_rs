#!/usr/bin/env python3
"""Check provisional normative links, not the correctness of standard translation."""

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path, PurePosixPath

from jsonschema import ValidationError

from evidence import load_json, read_evidence
from rust_source import items, parse, production_code, source_functions
from rust_tests import discover_tests as discover_cargo_tests, test_inventory


ROOT = Path(__file__).resolve().parents[1]
PREFIX = "jbig2_rs_production::"


def source_path(root, relative):
    parts = PurePosixPath(relative)
    if parts.is_absolute() or ".." in parts.parts or "\\" in relative:
        raise ValueError(f"invalid traceability source path: {relative}")
    path = (root / relative).resolve()
    if not path.is_relative_to(root.resolve()) or not path.is_file():
        raise ValueError(f"missing or external source: {relative}")
    return path


def resolve_module(root, names):
    path = root / "src/lib.rs"
    visited = {path.relative_to(root).as_posix()}
    for name in names:
        candidates = [
            (node, attributes) for node, attributes in items(parse(path.read_bytes()), False)
            if node.type == "mod_item"
            and node.child_by_field_name("name").text.decode() == name
        ]
        if len(candidates) != 1:
            raise ValueError(f"missing or ambiguous module: {'::'.join(names)}")
        node, attributes = candidates[0]
        if node.child_by_field_name("body") is not None:
            raise ValueError("inline model module bindings need explicit support")
        paths = [
            re.fullmatch(r'#\[\s*path\s*=\s*"([^"]+)"\s*\]', attribute.text.decode())
            for attribute in attributes
        ]
        paths = [match[1] for match in paths if match]
        directory = path.parent
        if paths:
            choices = [directory / paths[0]]
        else:
            if path.name not in {"lib.rs", "mod.rs"}:
                directory /= path.stem
            choices = [directory / f"{name}.rs", directory / name / "mod.rs"]
        existing = [choice.resolve() for choice in choices if choice.is_file()]
        if len(existing) != 1 or not existing[0].is_relative_to(root):
            raise ValueError(f"missing, ambiguous or external module file: {name}")
        path = existing[0]
        visited.add(path.relative_to(root).as_posix())
    return path, visited


def check_definition(root, definition, verifier_names):
    path, used = resolve_module(root, definition["module_path"])
    if path != source_path(root, definition["file"]):
        raise ValueError(f"definition module points to another file: {definition['name']}")
    if definition["kind"] == "macro":
        declarations = [
            node for node in parse(path.read_bytes()).named_children
            if node.type == "macro_definition"
            and node.child_by_field_name("name").text.decode() == definition["name"]
        ]
        if len(declarations) != 1:
            raise ValueError(f"missing or ambiguous proof macro: {definition['name']}")
    else:
        code = production_code(path.read_bytes(), exclude_proofs=False)
        pattern = (
            r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?(?:(?:open|closed|opaque)\s+)?"
            + definition["kind"] + r"\s+fn\s+" + re.escape(definition["name"]) + r"\b"
        )
        if len(re.findall(pattern, code)) != 1:
            raise ValueError(f"missing or ambiguous Verus declaration: {definition['name']}")
        qualified = PREFIX + "::".join([*definition["module_path"], definition["name"]])
        if verifier_names is not None and qualified not in verifier_names:
            raise ValueError(f"definition absent from verifier report: {qualified}")
    return used


def runtime_inventory(root):
    records = {}
    counts = {}
    for record in source_functions(root):
        base = record["id"]
        counts[base] = counts.get(base, 0) + 1
        identity = base if counts[base] == 1 else f"{base}#{counts[base]}"
        records[identity] = record
    return records


def compact(text):
    return re.sub(r"\s+", "", text)


def check_contract(record, section, tokens):
    sections = []
    for attribute in record["attributes"]:
        if not re.search(r"\bverus_spec\s*\(", attribute):
            continue
        start = re.search(rf"\b{section}\b", attribute)
        if start:
            text = attribute[start.end():]
            end = re.search(r"\b(?:requires|ensures|recommends|decreases|returns)\b", text)
            sections.append(compact(text[:end.start()] if end else text))
    if not sections or not all(any(compact(token) in text for text in sections) for token in tokens):
        raise ValueError(f"claimed {section} evidence is not present on {record['id']}")


def check(ledger, root, verifier_names=None, discovered=None):
    root = root.resolve()
    matrix = load_json(root / "support-matrix.json")
    provenance = load_json(root / "source-provenance.json")
    documents = {entry["id"]: entry for entry in provenance["normative_documents"]}
    if len({entry["id"] for entry in ledger["standards"]}) != len(ledger["standards"]):
        raise ValueError("duplicate normative standard identity")
    for standard in ledger["standards"]:
        if documents[standard["id"]]["document_sha256"] != standard["document_sha256"]:
            raise ValueError("normative document identity differs from provenance")
        if documents[standard["id"]]["edition"] != standard["edition"]:
            raise ValueError("normative edition differs from provenance")
    features = {entry["feature"]: entry for entry in matrix["rows"]}
    if len(features) != len(matrix["rows"]):
        raise ValueError("duplicate support-matrix feature")
    inventory = {entry["feature"]: entry for entry in ledger["features"]}
    if len(inventory) != len(ledger["features"]) or set(features) != set(inventory):
        raise ValueError("feature worklist must cover the exact support matrix")
    for name, feature in features.items():
        if inventory[name]["support_status"] != feature["status"]:
            raise ValueError(f"stale feature scope: {name}")
        if inventory[name]["decomposition"] == "excluded" and feature["status"] != "deferred":
            raise ValueError(f"required/rejection/prohibited behavior cannot be excluded: {name}")
    coverage = read_evidence(root / "production-proof-coverage.json", "coverage")
    coverage = {entry["id"]: entry for entry in coverage["functions"]}
    runtime = runtime_inventory(root)
    tests = test_inventory(root)
    used = set()
    for definition in ledger["definitions"].values():
        used.update(check_definition(root, definition, verifier_names))
    rows = []
    identifiers = set()
    for requirement in ledger["requirements"]:
        identity = requirement["id"]
        if identity in identifiers:
            raise ValueError(f"duplicate requirement ID: {identity}")
        identifiers.add(identity)
        if requirement["feature"] not in inventory:
            raise ValueError(f"unknown feature: {identity}")
        if requirement["standard"] not in {entry["id"] for entry in ledger["standards"]}:
            raise ValueError(f"unknown normative standard: {identity}")
        for definition in requirement["models"]:
            if ledger["definitions"][definition]["kind"] != "spec":
                raise ValueError(f"model reference is not a spec definition: {identity}")
        for definition in requirement["proof_macros"]:
            if ledger["definitions"][definition]["kind"] != "macro":
                raise ValueError(f"proof reference is not a macro definition: {identity}")
            macro = ledger["definitions"][definition]["name"]
            if not any(re.search(
                rf"\b{re.escape(macro)}\s*!",
                production_code(runtime[name]["body_text"].encode(), exclude_proofs=False),
            ) for name in requirement["runtime"]):
                raise ValueError(f"proof macro is not wired to recorded runtime: {identity}")
        for function in requirement["runtime"]:
            record = runtime[function]
            evidence = coverage[function]
            if record["file"] != evidence["file"] or record["file_sha256"] != evidence["file_sha256"]:
                raise ValueError(f"stale cached production evidence: {function}")
            used.add(record["file"])
            if verifier_names is not None and evidence["status"] == "verified":
                if not evidence["verus_functions"] or any(
                    name not in verifier_names for name in evidence["verus_functions"]
                ):
                    raise ValueError(f"runtime missing verifier-report evidence: {function}")
            if requirement["strength"] in {"functional_contract", "state_preservation", "safety_only"}:
                if evidence["status"] != "verified" or record["runtime_proof_divergence"]:
                    raise ValueError(f"formal claim has an unverified or divergent body: {identity}")
            if requirement["strength"] == "runtime_divergence" and evidence["status"] != "runtime-proof-divergence":
                raise ValueError(f"divergence classification changed: {identity}")
        if requirement["strength"] == "functional_contract" and not requirement["functional_evidence"]:
            raise ValueError(f"functional claim needs explicit postcondition evidence: {identity}")
        if requirement["strength"] == "state_preservation" and not requirement["invariant_evidence"]:
            raise ValueError(f"state claim needs explicit invariant evidence: {identity}")
        if any(claim["section"] != "ensures" for claim in requirement["functional_evidence"]):
            raise ValueError(f"preconditions cannot satisfy a functional result claim: {identity}")
        for claim in requirement["functional_evidence"] + requirement["invariant_evidence"]:
            if claim["runtime"] not in requirement["runtime"]:
                raise ValueError(f"contract points outside requirement runtime scope: {identity}")
            check_contract(runtime[claim["runtime"]], claim["section"], claim["tokens"])
        for reference in requirement["tests"]:
            key = (reference["target"], reference["name"])
            test = tests[key]
            if test["ignored"]:
                raise ValueError(f"ignored test cannot back a requirement: {key}")
            used.add(test["file"])
            if discovered is not None and key not in discovered:
                raise ValueError(f"test not present in Cargo discovery: {key}")
        rows.append({
            "id": identity, "clause": requirement["clause"], "title": requirement["title"],
            "strength": requirement["strength"], "review": requirement["review"],
            "runtime": requirement["runtime"], "gaps": requirement["gaps"],
            "scope_of_formal_result": requirement["scope_of_formal_result"],
            "assumptions": requirement["assumptions"],
            "runtime_locations": [
                {"name": name, "file": runtime[name]["file"], "line": runtime[name]["line"]}
                for name in requirement["runtime"]
            ],
            "models": [ledger["definitions"][name] for name in requirement["models"]],
            "proof_macros": [ledger["definitions"][name] for name in requirement["proof_macros"]],
            "tests": [tests[(test["target"], test["name"])] for test in requirement["tests"]],
        })
    for path in used:
        if path not in ledger["bindings"]:
            raise ValueError(f"unbound traceability source: {path}")
    if set(ledger["bindings"]) != used:
        raise ValueError("unused or missing source bindings")
    for path, expected in ledger["bindings"].items():
        if hashlib.sha256(source_path(root, path).read_bytes()).hexdigest() != expected:
            raise ValueError(f"stale reviewed source binding: {path}")
    mapped_features = {entry["feature"] for entry in ledger["requirements"]}
    if any(
        entry["decomposition"] == "complete" and entry["feature"] not in mapped_features
        for entry in ledger["features"]
    ):
        raise ValueError("a declared complete feature has no requirement records")
    complete = all(
        entry["decomposition"] in {"complete", "excluded"} for entry in ledger["features"]
    ) and all(
        entry["review"] == "approved" and not entry["gaps"]
        and entry["strength"] == "functional_contract" for entry in ledger["requirements"]
    )
    return {
        "schema_version": 1,
        "status": "references_valid_scope_complete" if complete else "references_valid_scope_incomplete",
        "normative_decomposition_complete": complete,
        "scope_completeness_is_review_declared_not_proved": True,
        "requirements": rows, "requirement_count": len(rows),
        "feature_count": len(features),
        "evidence_strength_counts": {
            strength: sum(row["strength"] == strength for row in rows)
            for strength in ("functional_contract", "state_preservation", "safety_only", "runtime_divergence")
        },
        "decomposition_counts": {
            state: sum(feature["decomposition"] == state for feature in ledger["features"])
            for state in ("not_started", "partial", "complete", "excluded")
        },
        "features_without_requirement_records": sorted(set(features) - mapped_features),
        "verifier_report_checked": verifier_names is not None,
        "cargo_test_discovery_checked": discovered is not None,
        "release_gate": "complete_scope_review_required" if not complete else "owner_release_review_required",
        "note": "Links and declared evidence strength are checked, not semantic truth. Report symbols do not establish report freshness: the caller must run pinned Verus on the same source, as CI does. Standard-to-model translation remains a review obligation.",
    }


def discover_tests(root, ledger):
    return discover_cargo_tests(
        root, {test["target"] for req in ledger["requirements"] for test in req["tests"]},
    )


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--ledger", type=Path, default=ROOT / "validation/traceability/requirements.json")
    parser.add_argument("--verus-json", type=Path)
    parser.add_argument("--discover-tests", action="store_true")
    parser.add_argument("--standard-pdf", type=Path)
    parser.add_argument("--require-complete", action="store_true")
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    if arguments.require_complete and (not arguments.verus_json or not arguments.discover_tests):
        parser.error("--require-complete needs Verus evidence and Cargo test discovery")
    try:
        ledger = read_evidence(arguments.ledger, "traceability")
        verifier = read_evidence(arguments.verus_json, "verus") if arguments.verus_json else None
        discovered = discover_tests(ROOT, ledger) if arguments.discover_tests else None
        report = check(ledger, ROOT, set(verifier["func-details"]) if verifier else None, discovered)
        revision = subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True, text=True, check=True,
        ).stdout.strip()
        report["source_revision"] = revision
        report["ledger_sha256"] = hashlib.sha256(arguments.ledger.read_bytes()).hexdigest()
        report["normative_document_hash_checked"] = False
        if arguments.standard_pdf:
            if hashlib.sha256(arguments.standard_pdf.read_bytes()).hexdigest() != ledger["standards"][0]["document_sha256"]:
                raise ValueError("local normative document hash mismatch")
            report["normative_document_hash_checked"] = True
        arguments.output.parent.mkdir(parents=True, exist_ok=True)
        arguments.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        table = [
            "# Provisional normative traceability", "",
            f"Status: **{report['status']}**. No full-conformance or complete-proof claim.", "",
            "| Requirement | Clause | Scope | Evidence strength | Open gaps |",
            "|---|---|---|---|---|",
        ]
        for row in report["requirements"]:
            clean = lambda value: str(value).replace("|", "\\|").replace("\n", " ")
            table.append("| " + " | ".join(clean(value) for value in (
                row["id"], row["clause"], row["title"], row["strength"], "; ".join(row["gaps"]),
            )) + " |")
        def link(location, name):
            line = f"#L{location['line']}" if "line" in location else ""
            return f"[`{name}`](https://github.com/microsoft/jbig2_rs/blob/{revision}/{location['file']}{line})"
        for row in report["requirements"]:
            table += [
                "", f"## {row['id']}: {row['title']}", "",
                f"Clause: {row['clause']}; review: {row['review']}; strength: {row['strength']}.",
                "", row["scope_of_formal_result"], "",
                "Runtime: " + ", ".join(link(item, item["name"]) for item in row["runtime_locations"]),
                "Models (definitions, not independent correctness theorems): "
                + (", ".join(link(item, item["name"]) for item in row["models"]) or "none"),
                "Proof support: "
                + (", ".join(link(item, item["name"]) for item in row["proof_macros"]) or "runtime declaration/contract only"),
                "Tests: " + ", ".join(link(item, f"{item['target']}::{item['name']}") for item in row["tests"]),
                "Assumptions: " + "; ".join(row["assumptions"]),
                "Open gaps: " + ("; ".join(row["gaps"]) or "none within this recorded property scope; owner review pending"),
            ]
        table += ["", report["note"]]
        arguments.output.with_suffix(".md").write_text("\n".join(table) + "\n", encoding="utf-8")
        print(json.dumps({key: report[key] for key in (
            "status", "requirement_count", "feature_count", "verifier_report_checked",
            "cargo_test_discovery_checked", "normative_document_hash_checked",
        )}, indent=2))
        if arguments.require_complete and not report["normative_decomposition_complete"]:
            raise ValueError("normative decomposition and approval remain incomplete")
        return 0
    except (OSError, ValueError, KeyError, TypeError, ValidationError, subprocess.CalledProcessError) as error:
        print(f"traceability validation failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Check provisional normative links, not the correctness of standard translation."""

import argparse
import copy
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path, PurePosixPath
from urllib.parse import parse_qs, urlsplit

from jsonschema import ValidationError

from evidence import load_json, read_evidence
from rust_source import items, parse, production_code, source_functions
from rust_tests import discover_tests as discover_cargo_tests, test_inventory


ROOT = Path(__file__).resolve().parents[1]
PREFIX = "jbig2_rs_production::"


def audit_inventory(root, ledger):
    references = {
        identity: [
            {"id": row["id"], "standard": row["standard"], "clause": row["clause"],
             "review": row["review"], "obligation_kind": row.get("obligation_kind", "normative_rule")}
            for row in ledger["requirements"]
            if identity in row["models"] + row["proof_macros"]
        ]
        for identity in ledger["definitions"]
    }
    definitions = []
    registered = set()
    for identity, definition in ledger["definitions"].items():
        registered.add((definition["file"], definition["name"]))
        definitions.append({
            "id": identity, **copy.deepcopy(definition),
            "classification": "proof-support" if definition["kind"] in {"proof", "macro"} else "classification-pending",
            "requirements": references[identity],
            "normative_approval": False,
        })
    unregistered = []
    inline_references = set()
    implementation_dependencies = {}
    declaration = re.compile(
        r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?(?:(?:open|closed|opaque)\s+)?spec\s+fn\s+([A-Za-z_]\w*)\b"
    )
    for directory in ("specs", "proofs/model_support"):
        for path in sorted((root / directory).rglob("*.rs")):
            relative = path.relative_to(root).as_posix()
            source = source_path(root, relative).read_bytes()
            for identity in re.findall(r"(?m)^\s*// Audit: ([A-Za-z0-9_-]+)\s*$", source.decode("utf-8")):
                if (
                    identity not in ledger["definitions"]
                    or ledger["definitions"][identity]["file"] != relative
                    or identity in inline_references
                ):
                    raise ValueError(f"unknown, duplicate or misrouted inline audit reference: {identity}")
                inline_references.add(identity)
            code = production_code(source, exclude_proofs=False)
            for match in declaration.finditer(code):
                if (relative, match[1]) not in registered:
                    unregistered.append({"file": relative, "name": match[1]})
            names = sorted(set(re.findall(r"\bsuper::([A-Z][A-Z_0-9]+)\b", code)))
            if names:
                implementation_dependencies[relative] = names
    for definition in definitions:
        definition["inline_audit_reference"] = definition["id"] in inline_references
    return {
        "scope": "Registered ledger definitions and spec-function discovery; not an exhaustive normative decomposition or transitive dependency analysis.",
        "registered_definition_count": len(definitions),
        "registered_spec_count": sum(row["kind"] == "spec" for row in definitions),
        "definitions": definitions,
        "unregistered_spec_functions": unregistered,
        "implementation_constant_references": implementation_dependencies,
        "classification_complete": not unregistered and all(
            row["classification"] != "classification-pending" for row in definitions
        ),
        "normative_approval": False,
    }


def audit_packet(root, ledger, requirement_ids, ledger_path):
    if (
        not isinstance(requirement_ids, list) or not requirement_ids
        or not all(isinstance(value, str) for value in requirement_ids)
        or len(set(requirement_ids)) != len(requirement_ids)
    ):
        raise ValueError("audit packets need nonempty unique requirement IDs")
    if load_json(source_path(root, ledger_path)) != ledger:
        raise ValueError("audit ledger changed before snapshot creation")
    selected = {row["id"]: row for row in ledger["requirements"]}
    if any(identity not in selected for identity in requirement_ids):
        raise ValueError("unknown audit requirement ID")
    requirements = [selected[identity] for identity in requirement_ids]
    definitions = {
        identity: ledger["definitions"][identity]
        for row in requirements for identity in row["models"] + row["proof_macros"]
    }
    for directory in ("src", "specs", "proofs", "tools", ".github/agents"):
        if any(path.is_symlink() for path in (root / directory).rglob("*")):
            raise ValueError("audit source snapshots must not contain symlinks")
    paths = {
        path.relative_to(root).as_posix()
        for directory in ("src", "specs", "proofs")
        for path in (root / directory).rglob("*.rs")
    }
    paths.update(
        path.relative_to(root).as_posix()
        for pattern in ("*.py", "*.json", "*.txt")
        for path in (root / "tools").rglob(pattern)
        if "__pycache__" not in path.relative_to(root).parts
    )
    paths.update((
        ledger_path, "validation/source-provenance.json",
        "config/support-matrix.json", "config/toolchain.json",
        "tools/check_traceability.py", "tools/evidence.schema.json",
        "Cargo.toml", "Cargo.lock", "verification/Cargo.toml", "verification/Cargo.lock",
    ))
    paths.update(
        path.relative_to(root).as_posix()
        for path in (root / ".github/agents").glob("*.agent.md")
    )
    for name in ("specs/README.md", "docs/specification-review.md"):
        if (root / name).is_file():
            paths.add(name)
    sources = {}
    for relative in sorted(paths):
        unresolved = root / relative
        if unresolved.is_symlink() or any(parent.is_symlink() for parent in unresolved.parents if parent != root):
            raise ValueError("audit source snapshots must not contain symlinks")
        sources[relative] = hashlib.sha256(source_path(root, relative).read_bytes()).hexdigest()
    packet = {
        "schema_version": 1,
        "state": "unreviewed",
        "scope": "Conservative all-Rust-source review snapshot, not an approved normative-only model freeze.",
        "requirement_ids": list(requirement_ids),
        "standards": [
            copy.deepcopy(row) for row in ledger["standards"]
            if row["id"] in {requirement["standard"] for requirement in requirements}
        ],
        "requirements": copy.deepcopy(requirements),
        "definitions": copy.deepcopy(definitions),
        "source_bindings": sources,
        "human_adjudicator": None,
        "normative_approval": False,
        "document_processing_authorized": False,
    }
    packet["packet_sha256"] = hashlib.sha256(json.dumps(
        packet, sort_keys=True, separators=(",", ":"), allow_nan=False,
    ).encode("utf-8")).hexdigest()
    if any(
        hashlib.sha256(source_path(root, name).read_bytes()).hexdigest() != value
        for name, value in sources.items()
    ):
        raise ValueError("audit source changed during snapshot creation")
    return packet


def check_audit_packet(root, ledger, packet, expected_sha256, ledger_path):
    if not isinstance(expected_sha256, str) or not re.fullmatch(r"[0-9a-f]{64}", expected_sha256):
        raise ValueError("an independently supplied SHA-256 pin is required")
    if not isinstance(packet, dict):
        raise ValueError("audit packet must be a mapping")
    payload = dict(packet)
    declared = payload.pop("packet_sha256", None)
    actual = hashlib.sha256(json.dumps(
        payload, sort_keys=True, separators=(",", ":"), allow_nan=False,
    ).encode("utf-8")).hexdigest()
    if declared != expected_sha256 or actual != expected_sha256:
        raise ValueError("modified or unpinned audit packet")
    expected = audit_packet(root, ledger, packet.get("requirement_ids"), ledger_path)
    if packet != expected or expected["packet_sha256"] != expected_sha256:
        raise ValueError("stale, modified or unpinned audit packet; no normative approval is inferred")


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


def module_sources(root, path):
    sources = set()

    def visit(current):
        if current in sources:
            raise ValueError(f"cyclic or duplicate source inclusion: {current}")
        sources.add(current)
        source = current.read_bytes()
        includes = []
        for node, _ in items(parse(source), False):
            if node.type == "expression_statement" and len(node.named_children) == 1:
                node = node.named_children[0]
            if node.type != "macro_invocation":
                continue
            name = node.child_by_field_name("macro")
            if name is not None and name.text == b"include":
                includes.append(node)
        code = production_code(source, exclude_proofs=False)
        if len(re.findall(r"\binclude\s*!\s*[\(\[{]", code)) != len(includes):
            raise ValueError("only module-scope source inclusion is supported")
        for node in includes:
            literal = re.fullmatch(
                r'include\s*!\s*\(\s*"([^"\\\n]+)"\s*\)', node.text.decode(),
            )
            if literal is None or (
                PurePosixPath(literal[1]).is_absolute() or Path(literal[1]).is_absolute()
            ):
                raise ValueError("source inclusion requires a literal relative path")
            included = (current.parent / literal[1]).resolve()
            if not included.is_relative_to(root.resolve()) or not included.is_file():
                raise ValueError("missing or external included source")
            if any(node.type == "mod_item" for node, _ in items(parse(included.read_bytes()), False)):
                raise ValueError("included module declarations need explicit support")
            visit(included)

    visit(path.resolve())
    return sources


def check_definition(root, definition, verifier_names):
    path, used = resolve_module(root, definition["module_path"])
    sources = module_sources(root, path)
    declared = source_path(root, definition["file"])
    if declared not in sources:
        raise ValueError(f"definition module points to another file: {definition['name']}")
    used.update(source.relative_to(root).as_posix() for source in sources)
    if definition["kind"] == "macro":
        declarations = [
            source for source in sources
            for node in parse(source.read_bytes()).named_children
            if node.type == "macro_definition"
            and node.child_by_field_name("name").text.decode() == definition["name"]
        ]
        if declarations != [declared]:
            raise ValueError(f"missing or ambiguous proof macro: {definition['name']}")
    else:
        pattern = (
            r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?(?:(?:open|closed|opaque)\s+)?"
            + definition["kind"] + r"\s+fn\s+" + re.escape(definition["name"]) + r"\b"
        )
        declarations = [
            source for source in sources
            for _ in re.finditer(
                pattern, production_code(source.read_bytes(), exclude_proofs=False),
            )
        ]
        if declarations != [declared]:
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


def policy_row_hash(row):
    return hashlib.sha256(json.dumps(
        row, sort_keys=True, separators=(",", ":"),
    ).encode("utf-8")).hexdigest()


def resolve_standards(ledger, provenance):
    if len({entry["id"] for entry in ledger["standards"]}) != len(ledger["standards"]):
        raise ValueError("duplicate normative standard identity")
    documents = {}
    for standard in ledger["standards"]:
        candidates = [
            (collection, entry)
            for collection in ("normative_documents", "normative_references")
            for entry in provenance.get(collection, [])
            if entry["id"] == standard["id"]
        ]
        if len(candidates) != 1:
            raise ValueError(f"missing or ambiguous normative provenance: {standard['id']}")
        collection, document = candidates[0]
        if collection != standard.get("provenance_collection", "normative_documents"):
            raise ValueError(f"normative provenance collection differs: {standard['id']}")
        if not document.get("document_sha256"):
            raise ValueError(f"normative source has no approved document identity: {standard['id']}")
        if document["document_sha256"] != standard["document_sha256"]:
            raise ValueError("normative document identity differs from provenance")
        if document.get("edition") != standard["edition"]:
            raise ValueError("normative edition differs from provenance")
        documents[standard["id"]] = document
    return documents


def local_document_checks(ledger, standard_pdf=None, standard_documents=()):
    hashes = {standard["id"]: standard["document_sha256"] for standard in ledger["standards"]}
    checks = dict.fromkeys(hashes, False)
    selections = []
    if standard_pdf is not None:
        selections.append((ledger["standards"][0]["id"], Path(standard_pdf)))
    for selection in standard_documents:
        identity, separator, path = selection.partition("=")
        if not separator or not identity or not path:
            raise ValueError("--standard-document needs STANDARD_ID=PATH")
        selections.append((identity, Path(path)))
    seen = set()
    for identity, path in selections:
        if identity not in hashes:
            raise ValueError(f"unknown local normative document identity: {identity}")
        if identity in seen:
            raise ValueError(f"duplicate local normative document selection: {identity}")
        seen.add(identity)
        if hashlib.sha256(path.read_bytes()).hexdigest() != hashes[identity]:
            raise ValueError(f"local normative document hash mismatch: {identity}")
        checks[identity] = True
    return checks


def check_contract(record, section, tokens):
    if not tokens or any(not compact(token) for token in tokens):
        raise ValueError(f"empty contract evidence on {record['id']}")
    if any(compact(token).strip("()") == "true" for token in tokens):
        raise ValueError(f"tautological contract evidence on {record['id']}")
    if len({compact(token) for token in tokens}) != len(tokens):
        raise ValueError(f"duplicate contract token on {record['id']}")
    sections = []
    for attribute in record["attributes"]:
        source = attribute.encode("utf-8")
        masked = bytearray(source)
        pending = [parse(source + b"\nfn traceability_attribute_probe() {}")]
        while pending:
            node = pending.pop()
            if node.type in {"line_comment", "block_comment"}:
                for index in range(node.start_byte, min(node.end_byte, len(masked))):
                    if masked[index] not in (10, 13):
                        masked[index] = 32
            else:
                pending.extend(node.named_children)
        attribute = masked.decode("utf-8")
        if not re.match(
            r"^\s*#\[\s*cfg_attr\s*\(\s*verus_keep_ghost\s*,\s*verus_spec\s*\(",
            attribute,
        ):
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
    matrix = load_json(root / "config/support-matrix.json")
    provenance = load_json(root / "validation/source-provenance.json")
    documents = resolve_standards(ledger, provenance)
    consultations = ledger["source_consultations"]
    consulted = {entry["standard"] for entry in consultations}
    if len(consulted) != len(consultations) or consulted != {
        entry["id"] for entry in ledger["standards"]
    }:
        raise ValueError("source consultations must cover the exact normative standards once")
    for consultation in consultations:
        document = documents[consultation["standard"]]
        families = consultation["clause_families"]
        if (
            not families or any(not compact(family) for family in families)
            or len({compact(family) for family in families}) != len(families)
        ):
            raise ValueError("source consultation needs nonempty distinct clause families")
        if consultation["document_sha256"] != document["document_sha256"]:
            raise ValueError("source consultation document_sha256 differs from provenance")
        url = urlsplit(consultation["acquisition_url"])
        if (
            url.scheme != "https" or url.hostname != "www.itu.int"
            or url.hostname != urlsplit(document["source_url"]).hostname
            or url.path != "/rec/dologin_pub.asp"
            or url.username is not None or url.password is not None or url.fragment
        ):
            raise ValueError("source consultation is not the recorded official publication host")
        if consultation["acquisition_kind"] == "official_package":
            if not document.get("package_sha256") or consultation.get("package_sha256") != document["package_sha256"]:
                raise ValueError("source consultation package_sha256 differs from provenance")
            publication = urlsplit(document["source_url"]).path.rstrip("/").split("/")[-2]
            if parse_qs(url.query) != {
                "lang": ["e"], "id": [publication + "!!SOFT-ZST-E"], "type": ["items"],
            }:
                raise ValueError("source consultation URL identifies a different publication/package")
        elif consultation["acquisition_kind"] == "official_pdf":
            if "package_sha256" in consultation:
                raise ValueError("direct PDF consultation cannot invent a package identity")
            if consultation["acquisition_url"] != document["source_url"]:
                raise ValueError("direct PDF consultation URL differs from recorded provenance")
        else:
            raise ValueError("unsupported source consultation acquisition kind")
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
    coverage = read_evidence(root / "verification/production-proof-coverage.json", "coverage")
    coverage = {entry["id"]: entry for entry in coverage["functions"]}
    runtime = runtime_inventory(root)
    tests = test_inventory(root)
    used = set()
    referenced_definitions = {
        name for requirement in ledger["requirements"]
        for name in requirement["models"] + requirement["proof_macros"]
    }
    if referenced_definitions != set(ledger["definitions"]):
        raise ValueError("unused or missing model/proof definitions")
    definition_identities = [
        (entry["kind"], entry["file"], tuple(entry["module_path"]), entry["name"])
        for entry in ledger["definitions"].values()
    ]
    if len(definition_identities) != len(set(definition_identities)):
        raise ValueError("duplicate model/proof definition identity")
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
        obligation_kind = requirement.get("obligation_kind", "normative_rule")
        policy = features[requirement["feature"]]
        if obligation_kind == "project_profile_policy":
            policy_source = requirement.get("policy_source", "feature_row")
            if policy_source == "feature_row":
                if not (policy.get("note") or policy.get("failure")):
                    raise ValueError(f"project policy has no explicit support-matrix basis: {identity}")
            elif policy_source == "public_api":
                policy = matrix.get("public_api", {})
                if policy.get("status") != "approved" or (
                    "Typed malformed, unsupported, resource-limit, input/output, and allocation errors."
                    not in policy.get("requirements", [])
                ):
                    raise ValueError(f"project error policy has no approved public API basis: {identity}")
            else:
                raise ValueError(f"unknown project policy source: {identity}")
            if requirement.get("policy_row_sha256") != policy_row_hash(policy):
                raise ValueError(f"stale or missing project policy evidence: {identity}")
        elif obligation_kind != "normative_rule" or "policy_row_sha256" in requirement or "policy_source" in requirement:
            raise ValueError(f"normative evidence cannot hide a project policy binding: {identity}")
        for field in ("runtime", "models", "proof_macros", "tests", "functional_evidence", "invariant_evidence"):
            values = [json.dumps(value, sort_keys=True) for value in requirement[field]]
            if len(values) != len(set(values)):
                raise ValueError(f"duplicate {field} evidence: {identity}")
        if requirement["standard"] not in {entry["id"] for entry in ledger["standards"]}:
            raise ValueError(f"unknown normative standard: {identity}")
        if identity.startswith("T6-") != (requirement["standard"] == "ITU-T T.6 (1988)"):
            raise ValueError(f"requirement prefix differs from normative source: {identity}")
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
        if requirement["strength"] != "functional_contract" and requirement["functional_evidence"]:
            raise ValueError(f"functional evidence has a weaker strength classification: {identity}")
        if any(claim["section"] != "ensures" for claim in requirement["functional_evidence"]):
            raise ValueError(f"preconditions cannot satisfy a functional result claim: {identity}")
        if any(claim["section"] != "ensures" for claim in requirement["invariant_evidence"]):
            raise ValueError(f"preconditions cannot satisfy a state preservation claim: {identity}")
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
            "standard": requirement["standard"],
            "feature": requirement["feature"],
            "obligation_kind": obligation_kind,
            "policy_row_sha256": requirement.get("policy_row_sha256"),
            "policy_source": requirement.get("policy_source", "feature_row") if obligation_kind == "project_profile_policy" else None,
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
    feature_decomposition = []
    for feature in ledger["features"]:
        name = feature["feature"]
        selected = [row for row in rows if row["feature"] == name]
        expected = {row["id"] for row in selected}
        state = feature["decomposition"]
        if state == "complete" and not selected:
            raise ValueError(f"a declared complete feature has no requirement records: {name}")
        declared = feature["requirement_ids"]
        if len(declared) != len(set(declared)) or set(declared) != expected:
            raise ValueError(f"feature requirement accounting differs from records: {name}")
        if bool(selected) != (state in {"partial", "complete"}):
            raise ValueError(f"feature decomposition state disagrees with records: {name}")
        if state == "complete":
            if feature["remaining_scope"] or any(
                row["review"] != "approved" for row in selected
            ):
                raise ValueError(f"complete feature has unresolved scope or evidence: {name}")
        elif not feature["remaining_scope"] or any(
            not note.strip() for note in feature["remaining_scope"]
        ):
            raise ValueError(f"unfinished/excluded feature needs explicit remaining scope: {name}")
        feature_decomposition.append({
            "feature": name, "support_status": feature["support_status"],
            "decomposition": state, "requirement_ids": sorted(expected),
            "requirement_count": len(selected), "remaining_scope": feature["remaining_scope"],
            "evidence_strength_counts": {
                strength: sum(row["strength"] == strength for row in selected)
                for strength in ("functional_contract", "state_preservation", "safety_only", "runtime_divergence")
            },
        })
    decomposition_complete = all(
        entry["decomposition"] in {"complete", "excluded"} for entry in ledger["features"]
    ) and all(entry["review"] == "approved" for entry in ledger["requirements"])
    semantic_complete = decomposition_complete and all(
        entry["review"] == "approved" and not entry["gaps"]
        and entry["strength"] == "functional_contract" for entry in ledger["requirements"]
    )
    return {
        "schema_version": 1,
        "status": "references_valid_scope_complete" if semantic_complete else "references_valid_scope_incomplete",
        # Preserve the legacy full-strength gate while exposing the P04-only milestone.
        "normative_decomposition_complete": semantic_complete,
        "decomposition_review_complete": decomposition_complete,
        "semantic_claims_complete": semantic_complete,
        "scope_completeness_is_review_declared_not_proved": True,
        "requirements": rows, "requirement_count": len(rows),
        "feature_count": len(features),
        "feature_decomposition": feature_decomposition,
        "standards": ledger["standards"],
        "source_consultations": consultations,
        "obligation_kind_counts": {
            kind: sum(row["obligation_kind"] == kind for row in rows)
            for kind in ("normative_rule", "project_profile_policy")
        },
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
        "release_gate": (
            "complete_scope_review_required" if not decomposition_complete
            else "semantic_claims_review_required" if not semantic_complete
            else "owner_release_review_required"
        ),
        "note": "Links and declared evidence strength are checked, not semantic truth. Report symbols do not establish report freshness: the caller must run pinned Verus on the same source, as CI does. Standard-to-model translation remains a review obligation.",
    }


def discover_tests(root, ledger):
    return discover_cargo_tests(
        root, {test["target"] for req in ledger["requirements"] for test in req["tests"]},
    )


def enforce_completion(report, require_decomposition=False, require_complete=False):
    if require_decomposition and not report["decomposition_review_complete"]:
        raise ValueError("reviewed normative requirement decomposition remains incomplete")
    if require_complete and not report["semantic_claims_complete"]:
        raise ValueError("normative decomposition, semantic claims and approval remain incomplete")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--ledger", type=Path, default=ROOT / "validation/traceability/requirements.json")
    parser.add_argument("--verus-json", type=Path)
    parser.add_argument("--discover-tests", action="store_true")
    parser.add_argument("--standard-pdf", type=Path, help="Legacy hash check for the first declared standard.")
    parser.add_argument(
        "--standard-document", action="append", default=[], metavar="STANDARD_ID=PATH",
        help="Hash-check a named declared standard; repeat for multiple local documents.",
    )
    parser.add_argument("--require-complete", action="store_true")
    parser.add_argument(
        "--require-reviewed-decomposition", action="store_true",
        help="Require reviewed requirement enumeration without treating recorded semantic gaps as closed.",
    )
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--audit-packet", type=Path, help="Write an explicitly unreviewed, hash-bound review snapshot.")
    parser.add_argument("--audit-requirement", action="append", default=[], metavar="REQUIREMENT_ID")
    parser.add_argument("--check-audit-packet", type=Path)
    parser.add_argument("--expected-audit-sha256")
    arguments = parser.parse_args()
    if arguments.audit_packet and arguments.check_audit_packet:
        parser.error("audit packet creation and validation are separate operations")
    if bool(arguments.check_audit_packet) != bool(arguments.expected_audit_sha256):
        parser.error("packet validation requires a separately supplied --expected-audit-sha256")
    if bool(arguments.audit_packet) != bool(arguments.audit_requirement):
        parser.error("packet creation requires explicit --audit-requirement selections")
    if arguments.audit_packet and (
        not arguments.audit_packet.resolve().is_relative_to((ROOT / "target").resolve())
        or arguments.audit_packet.resolve() == arguments.output.resolve()
    ):
        parser.error("create audit packets separately from reports under target/")
    if (arguments.require_complete or arguments.require_reviewed_decomposition) and (
        not arguments.verus_json or not arguments.discover_tests
    ):
        parser.error("completion gates need Verus evidence and Cargo test discovery")
    try:
        arguments.output.unlink(missing_ok=True)
        if arguments.output.with_suffix(".md") != arguments.output:
            arguments.output.with_suffix(".md").unlink(missing_ok=True)
        if arguments.audit_packet:
            arguments.audit_packet.unlink(missing_ok=True)
        ledger = read_evidence(arguments.ledger, "traceability")
        verifier = read_evidence(arguments.verus_json, "verus") if arguments.verus_json else None
        discovered = discover_tests(ROOT, ledger) if arguments.discover_tests else None
        report = check(ledger, ROOT, set(verifier["func-details"]) if verifier else None, discovered)
        report["model_audit_inventory"] = audit_inventory(ROOT, ledger)
        if arguments.audit_packet or arguments.check_audit_packet:
            ledger_path = arguments.ledger.resolve().relative_to(ROOT.resolve()).as_posix()
            if arguments.check_audit_packet:
                check_audit_packet(
                    ROOT, ledger, load_json(arguments.check_audit_packet),
                    arguments.expected_audit_sha256, ledger_path,
                )
                report["audit_packet_identity_checked"] = True
            else:
                packet = audit_packet(ROOT, ledger, arguments.audit_requirement, ledger_path)
                arguments.audit_packet.parent.mkdir(parents=True, exist_ok=True)
                arguments.audit_packet.write_text(json.dumps(packet, indent=2) + "\n", encoding="utf-8")
                report["audit_packet_sha256"] = packet["packet_sha256"]
        revision = subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True, text=True, check=True,
        ).stdout.strip()
        report["source_revision"] = revision
        report["ledger_sha256"] = hashlib.sha256(arguments.ledger.read_bytes()).hexdigest()
        report["normative_document_hash_checks"] = local_document_checks(
            ledger, arguments.standard_pdf, arguments.standard_document,
        )
        report["normative_document_hash_checked"] = all(report["normative_document_hash_checks"].values())
        arguments.output.parent.mkdir(parents=True, exist_ok=True)
        arguments.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        table = [
            "# Provisional normative traceability", "",
            f"Status: **{report['status']}**. No full-conformance or complete-proof claim.", "",
            f"Reviewed decomposition complete: **{report['decomposition_review_complete']}**.",
            f"Semantic claims complete: **{report['semantic_claims_complete']}**.", "",
            "| Requirement | Standard | Clause/context | Basis | Scope | Evidence strength | Open gaps |",
            "|---|---|---|---|---|---|---|",
        ]
        for row in report["requirements"]:
            clean = lambda value: str(value).replace("|", "\\|").replace("\n", " ")
            table.append("| " + " | ".join(clean(value) for value in (
                row["id"], row["standard"], row["clause"], row["obligation_kind"], row["title"],
                row["strength"], "; ".join(row["gaps"]),
            )) + " |")
        table += ["", "## Consulted source identity", ""]
        for consultation in report["source_consultations"]:
            table += [
                f"- {consultation['standard']}: {consultation['acquisition_url']}",
                f"  Document SHA-256: `{consultation['document_sha256']}`.",
                "  Consulted clause families: " + "; ".join(consultation["clause_families"]) + ".",
            ]
            if consultation["acquisition_kind"] == "official_package":
                table.append(f"  Package SHA-256: `{consultation['package_sha256']}`.")
            else:
                table.append("  Direct official PDF; no package identity is recorded.")
        table += [
            "", "Acquisition identity and declared consultation scope are checked, not independent proof of translation or owner approval.",
            "Project-profile policy rows use their cited standard only as context; selected feature/API policy hashes are checked separately and do not increase the normative-rule count.",
        ]
        table += [
            "", "## Feature decomposition accounting", "",
            "Selected records are not an exhaustive normative denominator. Remaining scope is a worklist, not a completeness certificate.",
            "", "| Feature | State | Selected records | Remaining scope |",
            "|---|---|---:|---|",
        ]
        for feature in report["feature_decomposition"]:
            table.append("| " + " | ".join(clean(value) for value in (
                feature["feature"], feature["decomposition"], feature["requirement_count"],
                "; ".join(feature["remaining_scope"]),
            )) + " |")
        def link(location, name):
            line = f"#L{location['line']}" if "line" in location else ""
            return f"[`{name}`](https://github.com/microsoft/jbig2_rs/blob/{revision}/{location['file']}{line})"
        for row in report["requirements"]:
            table += [
                "", f"## {row['id']}: {row['title']}", "",
                f"Standard: {row['standard']}.",
                f"Clause: {row['clause']}; review: {row['review']}; strength: {row['strength']}.",
                f"Obligation basis: {row['obligation_kind']}.",
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
            if row["obligation_kind"] == "project_profile_policy":
                table.append(
                    f"Policy basis: `config/support-matrix.json` {row['policy_source']} "
                    f"(feature `{row['feature']}`), reviewed source SHA-256 `{row['policy_row_sha256']}`."
                )
        table += ["", report["note"]]
        arguments.output.with_suffix(".md").write_text("\n".join(table) + "\n", encoding="utf-8")
        print(json.dumps({key: report[key] for key in (
            "status", "requirement_count", "feature_count", "verifier_report_checked",
            "cargo_test_discovery_checked", "normative_document_hash_checked", "normative_document_hash_checks",
            "decomposition_review_complete", "semantic_claims_complete",
        )}, indent=2))
        enforce_completion(
            report, arguments.require_reviewed_decomposition, arguments.require_complete,
        )
        return 0
    except (OSError, ValueError, KeyError, TypeError, ValidationError, subprocess.CalledProcessError) as error:
        print(f"traceability validation failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())

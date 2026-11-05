"""Conservative source/contract and declared-statement gap measurements."""

from collections import Counter, defaultdict
from pathlib import PurePosixPath
import re

from audit_production_coverage import verified_matches
from rust_source import contract_sections, items, parse


def direct_postconditions(record):
    declarations = [
        node for node, _ in items(parse(record["body_text"].encode()))
        if node.type in {"function_item", "function_signature_item"}
    ]
    if len(declarations) != 1:
        raise ValueError(f"missing or ambiguous function body: {record['id']}")
    body = declarations[0].child_by_field_name("body")
    if body is None:
        return 0
    count = 0
    for node, _ in items(body, exclude_proofs=False):
        if node.type == "expression_statement" and node.named_children:
            node = node.named_children[0]
        if node.type == "call_expression":
            function = node.child_by_field_name("function")
            count += function is not None and function.text == b"ensures"
    return count


def assess(ledger, runtime, verifier_names, accounting=None):
    if not runtime:
        raise ValueError("readiness assessment needs actual production declarations")
    links, functional, invariants = (defaultdict(list) for _ in range(3))
    for requirement in ledger["requirements"]:
        for identity in requirement["runtime"]:
            if identity not in runtime:
                raise ValueError(f"unknown readiness runtime link: {identity}")
            links[identity].append(requirement["id"])
        for field, destination in (
            ("functional_evidence", functional), ("invariant_evidence", invariants),
        ):
            for claim in requirement[field]:
                if claim["runtime"] not in requirement["runtime"]:
                    raise ValueError("readiness evidence escapes its owning requirement")
                destination[claim["runtime"]].append(requirement["id"])
    matches = {
        identity: verified_matches(record, verifier_names or set())
        for identity, record in runtime.items()
    }
    owners = defaultdict(list)
    for identity, names in matches.items():
        for name in names:
            owners[name].append(identity)
    records = []
    for identity, record in runtime.items():
        evidence = accounting.get(identity) if accounting is not None else None
        if accounting is None:
            accounting_status = "not_supplied"
        elif evidence is None or (
            evidence["file"] != record["file"]
            or evidence["file_sha256"] != record["file_sha256"]
        ):
            accounting_status = "stale_or_missing_accounting_identity"
        else:
            accounting_status = evidence["status"]
        sections = contract_sections(record, "ensures")
        prolog = direct_postconditions(record)
        if sections or prolog:
            surface = "local_postcondition_surface"
        elif record["impl_trait"]:
            surface = "trait_implementation_without_local_postcondition"
        else:
            surface = "no_local_postcondition_surface"
        names = matches[identity]
        if verifier_names is None:
            proof = "not_checked"
        elif any(len(owners[name]) > 1 for name in names):
            proof = "shared_verifier_name_requires_disambiguation"
        elif names:
            proof = "matched_verifier_name"
        else:
            proof = "no_verified_name_match"
        records.append({
            "id": identity, "file": record["file"], "line": record["line"],
            "file_sha256": record["file_sha256"],
            "module": PurePosixPath(record["file"]).parts[1].removesuffix(".rs"),
            "visibility": record["visibility"], "impl_trait": record["impl_trait"],
            "verification_requested": record["verification_requested"],
            "runtime_proof_divergence": record["runtime_proof_divergence"],
            "contract_surface": surface,
            "attached_postcondition_sections": len(sections),
            "direct_legacy_ensures_calls": prolog,
            "literal_true_postcondition_candidate": any(
                re.fullmatch(r"\s*true[\s,)\]]*", text) is not None for text in sections
            ),
            "proof_identity_status": proof, "verifier_names": names,
            "accounting_status": accounting_status,
            "reviewed_boundary_reason": (
                evidence["disposition_reason"]
                if accounting_status == "reviewed-verifier-limitation" else None
            ),
            "requirement_links": sorted(set(links[identity])),
            "direct_functional_evidence_links": sorted(set(functional[identity])),
            "direct_invariant_evidence_links": sorted(set(invariants[identity])),
        })
    modules = []
    for module in sorted({record["module"] for record in records}):
        selected = [record for record in records if record["module"] == module]
        modules.append({
            "module": module, "explicit_declarations": len(selected),
            "accounting_statuses": dict(sorted(Counter(
                record["accounting_status"] for record in selected
            ).items())),
            "contract_surfaces": dict(sorted(Counter(
                record["contract_surface"] for record in selected
            ).items())),
            "with_requirement_links": sum(bool(record["requirement_links"]) for record in selected),
            "with_direct_functional_evidence_links": sum(
                bool(record["direct_functional_evidence_links"]) for record in selected
            ),
        })
    requirements = ledger["requirements"]
    return {
        "schema_version": 1, "status": "gap_assessment_only",
        "ready_for_external_validation": False,
        "readiness_percentage": None,
        "code": {
            "denominator": "selected explicit production declarations from the existing cfg/module parser",
            "explicit_declarations": len(records),
            "complete_production_inventory_qualified": False,
            "semantic_contract_quality_qualified": False,
            "public_entry_reachability_qualified": False,
            "contract_surfaces": dict(sorted(Counter(
                record["contract_surface"] for record in records
            ).items())),
            "proof_identity_statuses": dict(sorted(Counter(
                record["proof_identity_status"] for record in records
            ).items())),
            "accounting_statuses": dict(sorted(Counter(
                record["accounting_status"] for record in records
            ).items())),
            "no_local_postcondition_by_accounting_status": dict(sorted(Counter(
                record["accounting_status"] for record in records
                if record["contract_surface"] != "local_postcondition_surface"
            ).items())),
            "with_requirement_links": sum(bool(record["requirement_links"]) for record in records),
            "with_direct_functional_evidence_links": sum(
                bool(record["direct_functional_evidence_links"]) for record in records
            ),
            "shared_verifier_names": {
                name: sorted(identities) for name, identities in sorted(owners.items())
                if len(identities) > 1
            },
            "modules": modules, "declarations": records,
        },
        "statements": {
            "denominator": "selected ledger records, not an exhaustive source-statement inventory",
            "selected_records": len(requirements),
            "declared_evidence_strengths": dict(sorted(Counter(
                row["strength"] for row in requirements
            ).items())),
            "with_explicit_gap_notes": sum(bool(row["gaps"]) for row in requirements),
            "declared_feature_decomposition": dict(sorted(Counter(
                feature["decomposition"] for feature in ledger["features"]
            ).items())),
            "exhaustive_statement_count": None,
            "statement_coverage_percentage": None,
            "statement_inventory_qualified": False,
            "remaining_feature_worklists": [
                {"feature": feature["feature"], "requirement_ids": feature["requirement_ids"],
                 "remaining_scope": feature["remaining_scope"]}
                for feature in ledger["features"]
                if feature["decomposition"] not in {"complete", "excluded"}
            ],
        },
        "limitations": [
            "A local postcondition surface is syntax, not a judgment of meaningful functional coverage.",
            "Trait-provided contracts and specification bridges need ownership review; absent local syntax is not proof of no contract.",
            "Direct requirement/evidence links do not credit unmeasured transitive caller coverage.",
            "Derived/generated items, executing macro expansion and closures are not independently inventoried here.",
            "Visibility is measured, not public/private/stage reachability or admissible call domains.",
            "Verifier-name matches are not fresh execution or a qualified source-function percentage.",
            "Reviewed boundary dispositions remain separate from contract-ownership review; absent local syntax does not invalidate a reviewed boundary.",
            "Selected records and broad feature worklists do not establish an exhaustive specification denominator.",
            "This assessment cannot grant readiness, independent normative approval or full functional proof.",
        ],
    }

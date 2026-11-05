"""Qualify shared verifier names using exact, already-validated semantic receipts."""

import hashlib
import json


def source_key(record):
    return (
        record["file"], record["verification_scope"], record["verifier_module"],
        record["owner"], record["name"], record["impl_trait"],
    )


def function_name(record):
    return "::".join(
        part for part in (record["verifier_module"], record["owner"], record["name"]) if part
    )


def exact_count(value, expected):
    return type(value) is int and value == expected


def qualify(runtime, assessment, reports):
    declarations = assessment["code"]["declarations"]
    if {row["id"] for row in declarations} != set(runtime) or len(declarations) != len(runtime):
        raise ValueError("identity assessment must cover the exact production inventory")
    ambiguous = [
        row["id"] for row in declarations
        if row["proof_identity_status"] == "shared_verifier_name_requires_disambiguation"
    ]
    selected = {}
    for identity in ambiguous:
        record = runtime[identity]
        if (
            not record["impl_trait"] or not record["verification_requested"]
            or record["runtime_proof_divergence"] or source_key(record) in selected
        ):
            raise ValueError("shared verifier identity needs one exact verified source implementation")
        selected[source_key(record)] = identity
    evidence = {identity: [] for identity in ambiguous}
    for group, document in reports.items():
        for case in document["counterexamples"]:
            matches = [
                (identity, record) for identity, record in runtime.items()
                if identity in evidence and case.get("source_file") == record["file"]
                and case.get("verification_module") == record["verification_scope"]
                and case.get("function") == function_name(record)
                and case.get("impl_trait") == record["impl_trait"]
            ]
            if not matches:
                continue
            if len(matches) != 1:
                raise ValueError("semantic receipt aliases distinct source implementations")
            if document["status"] != "passed" or document["production_sources_modified"] is not False:
                raise ValueError("identity evidence must come from passed unmodified semantic reports")
            identity, record = matches[0]
            peers = sorted(
                peer["impl_trait"] for peer in runtime.values()
                if peer["file"] == record["file"]
                and peer["verification_scope"] == record["verification_scope"]
                and function_name(peer) == function_name(record)
                and peer["verification_requested"] and not peer["runtime_proof_divergence"]
            )
            if (
                not peers or any(not isinstance(trait, str) or not trait for trait in peers)
                or len(set(peers)) != len(peers)
                or case.get("selected_impl_traits") != peers
                or document["source_bindings"].get(record["file"]) != record["file_sha256"]
                or case.get("normal_rust_compiles") is not True
                or case.get("strong_contract") != "rejected"
                or case.get("weaker_control") != "accepted"
                or not exact_count(case.get("strong_verifier_errors"), 1)
                or not exact_count(case.get("control_verifier_errors"), 0)
                or not exact_count(case.get("strong_verified_functions"), len(peers) - 1)
                or not exact_count(case.get("control_verified_functions"), len(peers))
                or case.get("baseline_selected_functions") != [
                    "::".join(part for part in (record["owner"], record["name"]) if part)
                ]
            ):
                raise ValueError(f"incomplete implementation-specific semantic control: {identity}")
            baseline = case["baseline_selected_verification"]
            if (
                "whole_positive" in baseline
                or not exact_count(baseline.get("verified"), len(peers))
                or not exact_count(baseline.get("errors"), 0)
                or any(baseline.get(flag) is not False for flag in (
                    "encountered-error", "encountered-vir-error", "is-verifying-entire-crate",
                ))
            ):
                raise ValueError(f"shared name needs a fresh exact selected positive: {identity}")
            evidence[identity].append({
                "group": group, "case_id": case["id"],
                "case_sha256": hashlib.sha256(json.dumps(
                    case, sort_keys=True, separators=(",", ":"),
                ).encode()).hexdigest(),
                "selected_impl_traits": peers,
            })
    missing = sorted(identity for identity, cases in evidence.items() if not cases)
    if missing:
        raise ValueError(f"shared verifier implementations lack exact semantic evidence: {missing}")
    records = []
    for identity in sorted(evidence):
        record = runtime[identity]
        records.append({
            "id": identity, "file": record["file"], "line": record["line"],
            "file_sha256": record["file_sha256"],
            "verification_scope": record["verification_scope"],
            "verifier_module": record["verifier_module"],
            "owner": record["owner"], "method": record["name"],
            "impl_trait": record["impl_trait"],
            "status": "exact_selected_implementation_evidence_checked",
            "semantic_controls": evidence[identity],
        })
    return {
        "schema_version": 1, "status": "shared_implementation_identity_evidence_checked",
        "shared_declarations": len(ambiguous),
        "qualified_shared_implementations": len(records),
        "remaining_unqualified_shared_implementations": [],
        "implementations": records,
        "evidence_boundary": (
            "Exact source/file/scope/trait identities plus fresh selected pristine, compiling strong/"
            "weak controls with all peers preserved. Requires the enclosing same-run collection's "
            "source/tool/receipt validation; this is not public reachability, generated-code, "
            "statement completeness, semantic contract quality or external-validation readiness."
        ),
        "ready_for_external_validation": False,
    }

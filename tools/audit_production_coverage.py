#!/usr/bin/env python3
import argparse
import json
from pathlib import Path

from rust_source import source_functions
from evidence import read_evidence, validate


ROOT = Path(__file__).resolve().parents[1]


def verified_matches(entry: dict, details: set[str]) -> list[str]:
    if not entry["verification_requested"] or entry["runtime_proof_divergence"]:
        return []
    module = entry["verifier_module"]
    prefix = "jbig2_rs_production::" + (f"{module}::" if module else "")
    if entry["owner"]:
        prefix += f"{entry['owner']}::"
    expected = prefix + entry["name"]
    return [expected] if expected in details else []


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--verus-json", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--dispositions", type=Path, default=ROOT / "verification/proof-dispositions.json")
    parser.add_argument("--require-complete", action="store_true")
    parser.add_argument("--require-exact", action="store_true",
                        help="also reject reviewed runtime/proof substitutions")
    arguments = parser.parse_args()

    report = read_evidence(arguments.verus_json, "verus")
    verification = report.get("verification-results", {})
    if (
        verification.get("success") is not True
        or verification.get("errors") != 0
        or verification.get("is-verifying-entire-crate") is not True
        or not report.get("func-details")
    ):
        raise ValueError("production Verus report is not a successful whole-crate verification")
    details = set(report["func-details"])
    dispositions = {}
    if arguments.dispositions.exists():
        document = read_evidence(arguments.dispositions, "dispositions")
        for entry in document["functions"]:
            if entry["id"] in dispositions:
                raise ValueError(f"duplicate proof disposition: {entry['id']}")
            dispositions[entry["id"]] = entry

    records = []
    identifiers: dict[str, int] = {}
    consumed_dispositions = set()
    for entry in source_functions(ROOT):
        base_identifier = entry["id"]
        identifiers[base_identifier] = identifiers.get(base_identifier, 0) + 1
        if identifiers[base_identifier] > 1:
            entry["id"] = f"{base_identifier}#{identifiers[base_identifier]}"
        matches = verified_matches(entry, details)
        disposition = dispositions.get(entry["id"])
        if matches:
            status = "verified"
            reason = None
        elif disposition is not None:
            if disposition["file_sha256"] != entry["file_sha256"]:
                raise ValueError(f"stale proof disposition for {entry['id']}")
            status = disposition["status"]
            if status not in {
                "reviewed-verifier-limitation", "audited-forwarder", "runtime-proof-divergence",
            }:
                raise ValueError(f"invalid proof disposition for {entry['id']}: {status}")
            if entry["runtime_proof_divergence"] != (status == "runtime-proof-divergence"):
                raise ValueError(f"runtime/proof divergence disposition mismatch: {entry['id']}")
            reason = disposition["reason"]
            if not reason.strip():
                raise ValueError(f"empty proof disposition reason for {entry['id']}")
            consumed_dispositions.add(entry["id"])
        else:
            status = "incomplete"
            reason = None
        records.append({
            "id": entry["id"], "file": entry["file"], "visibility": entry["visibility"],
            "status": status, "disposition_reason": reason,
            "file_sha256": entry["file_sha256"], "verus_functions": matches,
        })
    if not records:
        raise ValueError("production function inventory is empty")
    unused = sorted(set(dispositions) - consumed_dispositions)
    if unused:
        raise ValueError(f"unused proof dispositions: {unused}")
    counts = {
        status: sum(record["status"] == status for record in records)
        for status in (
            "verified", "reviewed-verifier-limitation", "audited-forwarder",
            "runtime-proof-divergence", "native-assumption", "incomplete",
        )
    }
    output = {
        "schema_version": 1, "source": "src/**/*.rs",
        "verus_verified_obligations": verification["verified"],
        "native_assumptions": [], "counts": counts, "functions": records,
    }
    validate(output, "coverage")
    arguments.output.write_text(json.dumps(output, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(counts, sort_keys=True))
    if arguments.require_complete and counts["incomplete"]:
        raise ValueError(f"{counts['incomplete']} production functions remain incomplete")
    if arguments.require_exact and (counts["runtime-proof-divergence"] or counts["incomplete"]):
        raise ValueError("exact runtime proof gate blocked by divergences or incomplete functions")


if __name__ == "__main__":
    main()

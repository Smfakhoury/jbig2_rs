"""Strict JSON loading and schemas for the evidence consumed by audit gates."""

import hashlib
import json
import tomllib
from pathlib import Path

from jsonschema import Draft202012Validator


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def invalid_constant(value):
    raise ValueError(f"non-JSON numeric constant: {value}")


def load_json(path: Path):
    return json.loads(
        path.read_text(encoding="utf-8"),
        object_pairs_hook=unique_object,
        parse_constant=invalid_constant,
    )


def validate(document, kind: str) -> None:
    schema = load_json(Path(__file__).with_name("evidence.schema.json"))
    schema["$ref"] = f"#/$defs/{kind}"
    Draft202012Validator.check_schema(schema)
    Draft202012Validator(schema).validate(document)
    if kind == "conformance_differences":
        identifiers = [entry["id"] for entry in document["cases"]]
        if len(identifiers) != len(set(identifiers)):
            raise ValueError("duplicate conformance expectation identity")
    if kind == "coverage_baseline":
        for metric in ("lines", "regions", "branches"):
            for entry in [document["totals"], *document["files"].values()]:
                if entry[metric]["covered"] > entry[metric]["count"]:
                    raise ValueError("coverage exceeds its denominator")
            for field in ("covered", "count"):
                if document["totals"][metric][field] != sum(
                    entry[metric][field] for entry in document["files"].values()
                ):
                    raise ValueError("coverage baseline totals do not match files")
            if not document["totals"][metric]["count"]:
                raise ValueError("coverage baseline lacks runtime instrumentation")
    if kind in {"coverage", "dispositions"}:
        identifiers = [entry["id"] for entry in document["functions"]]
        if len(identifiers) != len(set(identifiers)):
            raise ValueError("duplicate function evidence identity")
    if kind == "coverage":
        for entry in document["functions"]:
            if entry["status"] == "verified":
                if not entry["verus_functions"] or entry["disposition_reason"] is not None:
                    raise ValueError(f"invalid verified evidence for {entry['id']}")
            elif entry["verus_functions"]:
                raise ValueError(f"unverified function has proof matches: {entry['id']}")
            elif entry["status"] != "incomplete" and not (
                isinstance(entry["disposition_reason"], str) and entry["disposition_reason"].strip()
            ):
                raise ValueError(f"missing review reason for {entry['id']}")
        for status, count in document["counts"].items():
            if count != sum(entry["status"] == status for entry in document["functions"]):
                raise ValueError(f"inconsistent coverage count for {status}")


def read_evidence(path: Path, kind: str):
    document = load_json(path)
    validate(document, kind)
    return document


def reviewed_standard_interface(root: Path):
    path = root / "verification/stdlib/review.json"
    if not path.is_file():
        return None
    review = read_evidence(path, "standard_interface_review")
    for field in ("source", "manifest"):
        if hashlib.sha256((root / review[field]).read_bytes()).hexdigest() != review[field + "_sha256"]:
            raise ValueError(f"stale standard-interface review: {review[field]}")
    package = tomllib.loads((root / review["manifest"]).read_text())
    proof = tomllib.loads((root / "verification/Cargo.toml").read_text())
    if (
        package["package"]["name"] != review["package"]
        or package["package"].get("publish") is not False
        or package["package"].get("build") is not False
        or package.get("lib", {}).get("path") != "lib.rs"
        or package["package"].get("metadata", {}).get("verus", {}).get("verify") is not True
        or proof["dependencies"].get(review["package"]) != {"path": "stdlib/vec-capacity-interface"}
    ):
        raise ValueError("reviewed standard interface must remain the exact proof-only dependency")
    return review

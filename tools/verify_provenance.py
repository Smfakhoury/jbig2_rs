#!/usr/bin/env python3
import hashlib
import sys
from pathlib import Path

from jsonschema import Draft202012Validator, ValidationError, SchemaError

from evidence import load_json, read_evidence

ROOT = Path(__file__).resolve().parents[1]
JSON_FILES = (
    "support-matrix.json",
    "source-provenance.json",
    "proof-attestation.json",
    "conformance-attestation.json",
    "fuzz-attestation.json",
    "release-attestation.schema.json",
    "toolchain.json",
)
FORBIDDEN_SUFFIXES = {".pdf", ".doc", ".docx"}
FORBIDDEN_ORACLE_SUFFIXES = {".c", ".cc", ".cpp", ".js"}
IGNORED_PARTS = {".git", "target"}


def fail(message: str) -> None:
    print(f"provenance audit failed: {message}", file=sys.stderr)
    raise SystemExit(1)


for relative_path in JSON_FILES:
    path = ROOT / relative_path
    try:
        load_json(path)
    except (OSError, ValueError) as error:
        fail(f"{relative_path}: {error}")

for path in sorted(ROOT.rglob("*")):
    if not path.is_file() or any(part in IGNORED_PARTS for part in path.parts):
        continue
    if path.suffix.lower() in FORBIDDEN_SUFFIXES:
        fail(f"standards/document asset must not be committed: {path.relative_to(ROOT)}")
    if path.suffix.lower() in FORBIDDEN_ORACLE_SUFFIXES:
        fail(f"native/third-party oracle source must not be committed: {path.relative_to(ROOT)}")
    if "scratch" in path.parts:
        fail(f"scratch artifact must not be committed: {path.relative_to(ROOT)}")
    if path.suffix == ".json" and not any(
        part in {"corpus", "artifacts", "__pycache__"} for part in path.parts
    ):
        try:
            document = load_json(path)
            if not isinstance(document, (dict, list)):
                fail(f"JSON document must be an object or array: {path.relative_to(ROOT)}")
            if isinstance(document, dict) and "$schema" in document and "$defs" in document:
                Draft202012Validator.check_schema(document)
        except (OSError, ValueError, SchemaError) as error:
            fail(f"{path.relative_to(ROOT)}: {error}")

for filename, kind in (
    ("proof-dispositions.json", "dispositions"),
    ("production-proof-coverage.json", "coverage"),
    ("toolchain.json", "toolchain"),
    ("proof-attestation.json", "proof_attestation"),
):
    try:
        read_evidence(ROOT / filename, kind)
    except (OSError, ValueError, ValidationError) as error:
        fail(f"{filename}: {error}")

coverage = read_evidence(ROOT / "production-proof-coverage.json", "coverage")
attestation = read_evidence(ROOT / "proof-attestation.json", "proof_attestation")
for filename, key in (
    ("production-proof-coverage.json", "production_proof_coverage_sha256"),
    ("proof-dispositions.json", "proof_dispositions_sha256"),
):
    if hashlib.sha256((ROOT / filename).read_bytes()).hexdigest() != attestation[key]:
        fail(f"stale proof-attestation hash: {filename}")
for result_key, count_key in (
    ("verified_production_functions", "verified"),
    ("reviewed_verifier_limitations", "reviewed-verifier-limitation"),
    ("audited_forwarders", "audited-forwarder"),
    ("incomplete_functions", "incomplete"),
    ("runtime_proof_divergences", "runtime-proof-divergence"),
):
    if attestation["result"].get(result_key, 0) != coverage["counts"].get(count_key, 0):
        fail(f"stale proof-attestation count: {result_key}")
if attestation["result"]["verified_obligations"] != coverage["verus_verified_obligations"]:
    fail("stale proof-attestation obligation count")
if coverage["counts"].get("runtime-proof-divergence", 0):
    if attestation["result"]["algorithm_bodies_verified"] or attestation["release_gate"] != "blocked":
        fail("runtime/proof divergences must block exact-source release claims")

provenance = load_json(ROOT / "source-provenance.json")
if provenance["policy"]["standards_text_committed"]:
    fail("standards_text_committed must remain false")
if provenance["policy"]["non_redistributable_assets_committed"]:
    fail("non_redistributable_assets_committed must remain false")

print("provenance audit passed")

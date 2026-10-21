#!/usr/bin/env python3
import argparse
import hashlib
import sys
from pathlib import Path

from jsonschema import Draft202012Validator, ValidationError, SchemaError

from evidence import load_json, read_evidence, validate

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

parser = argparse.ArgumentParser(description="Validate reference identities, not vector execution.")
parser.add_argument(
    "--require-exact-conformance", action="store_true",
    help="Also require every recorded official case to be an exact output pass; does not execute vectors.",
)
arguments = parser.parse_args()


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
            elif path.relative_to(ROOT).as_posix() == "validation/coverage/baseline.json":
                validate(document, "coverage_baseline")
            elif path.relative_to(ROOT).as_posix() == "validation/traceability/requirements.json":
                validate(document, "traceability")
            elif path.relative_to(ROOT).as_posix() == "validation/conformance/expected-differences.json":
                validate(document, "conformance_differences")
            elif path.relative_to(ROOT).as_posix() in {
                "validation/negative/cases.json", "validation/metamorphic/cases.json",
            }:
                validate(document, "test_catalog")
            elif isinstance(document, dict) and "schema_version" in document:
                validate(document, "repository")
        except (OSError, ValueError, SchemaError, ValidationError) as error:
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
differences = read_evidence(
    ROOT / "validation/conformance/expected-differences.json", "conformance_differences"
)
manifest = load_json(ROOT / "validation/conformance/itu-t88-2018-manifest.json")
streams = {
    Path(entry["path"]).stem: entry["sha256"]
    for entry in manifest["files"] if entry["kind"] == "encoded_stream"
}
for entry in differences["cases"]:
    if entry["id"] not in streams:
        fail(f"unknown conformance expectation: {entry['id']}")
    if entry["classification"] == "pending_normative_review":
        if entry["stream_sha256"] != streams[entry["id"]]:
            fail(f"pending review stream hash mismatch: {entry['id']}")
        for path, expected in entry["source_files_sha256"].items():
            if hashlib.sha256((ROOT / path).read_bytes()).hexdigest() != expected:
                fail(f"pending review source hash mismatch: {entry['id']}: {path}")
conformance = load_json(ROOT / "conformance-attestation.json")
normative = conformance["normative_vectors"]
for relative, key in (
    ("validation/conformance/itu-t88-2018-results.json", "results_sha256"),
    ("validation/conformance/seven-rejection-review.json", "review_sha256"),
):
    if hashlib.sha256((ROOT / relative).read_bytes()).hexdigest() != normative[key]:
        fail(f"stale conformance attestation hash: {relative}")
results = load_json(ROOT / "validation/conformance/itu-t88-2018-results.json")
for key in ("total", "passed", "reviewed_strict_rejections", "unsupported_profile",
            "pending_normative_reviews", "unresolved_failures"):
    if normative[key] != results[key]:
        fail(f"stale conformance attestation count: {key}")
pending_ids = {
    entry["id"] for entry in differences["cases"]
    if entry["classification"] == "pending_normative_review"
}
if (
    {entry["id"] for entry in results["cases"]} != set(streams)
    or len(results["cases"]) != len(streams)
    or results["total"] != len(streams)
    or {entry["id"] for entry in results["cases"] if entry["status"] == "pending_normative_review"}
        != pending_ids
):
    fail("conformance result case identities do not match the manifest and review ledger")
for key, status in (
    ("passed", "passed"), ("reviewed_strict_rejections", "reviewed_strict_rejection"),
    ("unsupported_profile", "unsupported_profile"),
    ("pending_normative_reviews", "pending_normative_review"),
):
    if results[key] != sum(entry["status"] == status for entry in results["cases"]):
        fail(f"inconsistent conformance result count: {key}")
if results["unresolved_failures"] != sum(
    entry["status"] not in {"passed", "reviewed_strict_rejection", "unsupported_profile"}
    for entry in results["cases"]
):
    fail("unresolved conformance cases must not receive pass credit")
expectations = {entry["id"]: entry for entry in differences["cases"]}
for entry in results["cases"]:
    if entry["stream_sha256"] != streams[entry["id"]]:
        fail(f"conformance result stream identity mismatch: {entry['id']}")
    if entry["id"] in pending_ids and entry["stderr"] != expectations[entry["id"]]["expected_error_exact"]:
        fail(f"pending review typed error changed: {entry['id']}")
review = load_json(ROOT / "validation/conformance/seven-rejection-review.json")
if hashlib.sha256((ROOT / ".gitattributes").read_bytes()).hexdigest() != review["checkout_policy_sha256"]:
    fail("stale conformance checkout policy identity")
if (
    {entry["id"] for entry in review["cases"]} != pending_ids
    or len(review["cases"]) != len(pending_ids)
    or any(entry["normative_approval"] is not False for entry in review["cases"])
    or review["release_gate"] != "blocked"
    or review["source_revision"] != conformance["source_revision"]
    or review["normal_conformance_results_sha256"] != normative["results_sha256"]
):
    fail("pending review evidence must not imply normative approval")
if set(review["source_files_sha256"]) != {
    path.relative_to(ROOT).as_posix() for path in (ROOT / "src").rglob("*.rs")
}:
    fail("conformance review must identify the complete runtime source inventory")
for field in ("source_files_sha256", "validation_files_sha256"):
    for path, expected in review[field].items():
        actual = ROOT / path
        if (
            not actual.resolve().is_relative_to(ROOT)
            or hashlib.sha256(actual.read_bytes()).hexdigest() != expected
        ):
            fail(f"stale conformance review identity: {path}")
if results["unresolved_failures"] and (
    conformance["status"] != "blocked_pending_normative_review"
    or conformance["release_gate"] != "blocked"
):
    fail("unresolved official cases must block conformance approval")
if provenance["policy"]["standards_text_committed"]:
    fail("standards_text_committed must remain false")
if provenance["policy"]["non_redistributable_assets_committed"]:
    fail("non_redistributable_assets_committed must remain false")
if arguments.require_exact_conformance and any(
    entry["status"] != "passed" for entry in results["cases"]
):
    fail("exact conformance gate blocked by non-passing official cases")

print("provenance audit passed")

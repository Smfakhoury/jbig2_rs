#!/usr/bin/env python3
import json
import sys
from pathlib import Path


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
        with path.open(encoding="utf-8") as source:
            json.load(source)
    except (OSError, json.JSONDecodeError) as error:
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

with (ROOT / "source-provenance.json").open(encoding="utf-8") as source:
    provenance = json.load(source)
if provenance["policy"]["standards_text_committed"]:
    fail("standards_text_committed must remain false")
if provenance["policy"]["non_redistributable_assets_committed"]:
    fail("non_redistributable_assets_committed must remain false")

print("provenance audit passed")

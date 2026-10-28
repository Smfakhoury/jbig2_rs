#!/usr/bin/env python3
"""Check exact project-owned typed error conversions, not normative rules."""

import argparse
import json
from pathlib import Path
import subprocess
import sys

from proof_sensitivity import check_semantic_cases


ROOT = Path(__file__).resolve().parents[1]
OBEYS = "open spec fn obeys_from_spec() -> bool { true }"
CASES = tuple(
    {
        "id": identity, "file": f"src/{scope}.rs", "module": "error",
        "verification_module": scope, "function": "DecodeError::from",
        "site": site, "replacement": replacement,
        "controls": ((OBEYS, OBEYS.replace("true", "false")),),
    }
    for identity, scope, site, replacement in (
        (
            "generic-row-count-fields", "generic_region",
            "GenericRegionError::RowCountMismatch { expected, actual } => { GenericRegionErrorKind::RowCountMismatch { expected, actual } }",
            "GenericRegionError::RowCountMismatch { expected, actual } => { GenericRegionErrorKind::RowCountMismatch { expected: actual, actual: expected } }",
        ),
        (
            "generic-context-fields", "generic_region",
            "GenericRegionError::ContextIndex { index, count } => { GenericRegionErrorKind::ContextIndex { index, count } }",
            "GenericRegionError::ContextIndex { index, count } => { GenericRegionErrorKind::ContextIndex { index: count, count: index } }",
        ),
        (
            "huffman-error-class", "huffman",
            "Self::Malformed(MalformedError::InvalidHuffman(kind))",
            "Self::Malformed(MalformedError::IntegerOverflow)",
        ),
        (
            "huffman-wrapped-error", "huffman",
            "fn from(error: HuffmanError) -> Self { match error { HuffmanError::Decode(error) => error,",
            "fn from(error: HuffmanError) -> Self { match error { HuffmanError::Decode(error) => DecodeError::Malformed(MalformedError::IntegerOverflow),",
        ),
        (
            "mmr-allocation-bytes", "mmr",
            "return Self::Allocation { requested_bytes };",
            "return Self::Allocation { requested_bytes: 0 };",
        ),
        (
            "mmr-run-color", "mmr",
            "MmrError::InvalidRunCode { bit_offset, black } => { MmrErrorKind::InvalidRunCode { bit_offset, black } }",
            "MmrError::InvalidRunCode { bit_offset, black } => { MmrErrorKind::InvalidRunCode { bit_offset, black: !black } }",
        ),
        (
            "symbol-id-fields", "symbol_text",
            "Self::Malformed(MalformedError::SymbolIdOutOfRange { id, available })",
            "Self::Malformed(MalformedError::SymbolIdOutOfRange { id: available, available: id })",
        ),
        (
            "symbol-dimension-class", "symbol_text",
            "Self::Malformed(MalformedError::InvalidSymbolDimensions)",
            "Self::Malformed(MalformedError::IntegerOverflow)",
        ),
    )
)
FILES = (
    "src/error.rs", "src/generic_region.rs", "src/huffman.rs", "src/mmr.rs",
    "src/symbol_text.rs",
)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--verus-executable", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    try:
        report = check_semantic_cases(
            ROOT, arguments.verus_executable.resolve(), CASES, FILES,
            "jbig2-error-conversion-",
            "Four executing project-owned From bodies preserve exact error variants and payloads under the pinned trait contract; no normative or resource-accounting approval",
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"typed error proof sensitivity failed: {error}", file=sys.stderr)
        report = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

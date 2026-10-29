#!/usr/bin/env python3
"""Verify exact owned error-wrapper mappings with implementation-specific controls."""

import argparse
import json
from pathlib import Path
import subprocess
import sys

from proof_sensitivity import check_semantic_cases


ROOT = Path(__file__).resolve().parents[1]


def wrapper_case(scope, target, source, body, replacement):
    model_trait = (
        "vstd::std_specs::convert::FromSpecImpl"
        if scope in {"arithmetic", "halftone"} else "FromSpecImpl"
    )
    contract = (
        f"impl {model_trait}<{source}> for {target} {{ "
        "open spec fn obeys_from_spec() -> bool { true }"
    )
    site = f"impl From<{source}> for {target} {{ fn from(error: {source}) -> Self {{ {body} }} }}"
    return {
        "id": f"{scope}-{target}-{source}", "file": f"src/{scope}.rs",
        "module": "error" if target == "DecodeError" else scope,
        "verification_module": scope, "function": f"{target}::from",
        "impl_trait": f"From<{source}>", "site": site,
        "replacement": site.replace(body, replacement),
        "controls": ((contract, contract.replace("true", "false")),),
    }


def wrapping_cases(scope, target, sources):
    return tuple(
        wrapper_case(
            scope, target, source,
            "Self::Decode(error)" if source == "DecodeError" else "Self::Decode(error.into())",
            "Self::Decode(DecodeError::Malformed(MalformedError::IntegerOverflow))",
        )
        for source in sources
    )


CASE_GROUPS = {
    "error-wrappers-basic": (
        wrapper_case(
            "arithmetic", "ArithmeticError", "MalformedError", "Self::Malformed(error)",
            "Self::Malformed(MalformedError::IntegerOverflow)",
        ),
        wrapper_case(
            "arithmetic", "DecodeError", "ArithmeticError", "error.into_decode()",
            "Self::Malformed(MalformedError::IntegerOverflow)",
        ),
        *wrapping_cases("huffman", "HuffmanError", ("DecodeError", "MalformedError", "LimitExceeded")),
        wrapper_case(
            "mmr", "MmrError", "MalformedError", "Self::Reader(error)",
            "Self::Reader(MalformedError::IntegerOverflow)",
        ),
    ),
    "error-wrappers-regions": (
        *wrapping_cases("generic_region", "GenericRegionError", ("DecodeError", "MalformedError", "MmrError")),
        *wrapping_cases("halftone", "HalftoneError", ("DecodeError", "MalformedError", "LimitExceeded")),
        wrapper_case(
            "halftone", "DecodeError", "HalftoneError", "halftone_error_into_decode(error)",
            "Self::Malformed(MalformedError::IntegerOverflow)",
        ),
    ),
    "error-wrappers-symbol": wrapping_cases(
        "symbol_text", "SymbolTextError",
        ("DecodeError", "MalformedError", "LimitExceeded", "HuffmanError", "GenericRegionError", "MmrError", "ArithmeticError"),
    ),
}
CASES = tuple(case for cases in CASE_GROUPS.values() for case in cases)
FILES = (
    "src/arithmetic.rs", "src/error.rs", "src/generic_region.rs", "src/halftone.rs",
    "src/huffman.rs", "src/mmr.rs", "src/symbol_text.rs",
)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--verus-executable", type=Path, required=True)
    parser.add_argument("--case-group", choices=CASE_GROUPS, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    try:
        report = check_semantic_cases(
            ROOT, arguments.verus_executable.resolve(), CASE_GROUPS[arguments.case_group], FILES,
            "jbig2-error-wrappers-",
            "Exact owned error wrapper variants, fields and composition through existing proved conversions; no normative or resource-accounting approval",
        )
        report["case_group"] = arguments.case_group
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"owned error-wrapper proof sensitivity failed: {error}", file=sys.stderr)
        report = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

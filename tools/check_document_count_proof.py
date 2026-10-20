#!/usr/bin/env python3
"""Check semantic sensitivity of document cardinality and metadata contracts."""

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

from check_distribution import run
from proof_sensitivity import proof_workspace, selected_proof


ROOT = Path(__file__).resolve().parents[1]
CASES = (
    {
        "id": "accept_inconsistent_declared_count",
        "function": "build_document_info",
        "site": "if declared != actual_page_count {",
        "replacement": "if false {",
        "control_site": """&& match declared_page_count {
                Some(count) => document_spec::document_pages(&info).len() == count as nat,
                None => true,
            }""",
        "control_replacement": "",
    },
    {
        "id": "drop_finalized_page",
        "function": "finalize_pages",
        "site": "validate_public_page_numbers(&pages)?;\n    Ok(pages)",
        "replacement": "validate_public_page_numbers(&pages)?;\n    pages.pop();\n    Ok(pages)",
        "control_site": "Ok(pages) => pages@.len() == page_order@.len(),",
        "control_replacement": "Ok(pages) => true,",
    },
    {
        "id": "collect_non_page_information_header",
        "function": "collect_page_states_into",
        "site": "if !matches!(segment.header.segment_type, SegmentType::PageInformation) {",
        "replacement": "if false {",
        "control_site": """&& final(page_states)@.len()
            == document_spec::page_information_count(segments@, segments@.len()),""",
        "control_replacement": ",",
    },
    {
        "id": "discard_standalone_header_count",
        "function": "parse_standalone_document",
        "site": "file_header.page_count(),\n        &segments,",
        "replacement": "None,\n        &segments,",
        "control_file": "specs/document.rs",
        "control_site": "&&& document_declared_page_count(document) == file_header_spec::page_count(input)",
        "control_replacement": "",
    },
    {
        "id": "invent_embedded_header_count",
        "function": "parse_embedded_document",
        "site": "build_document_info(Organization::Sequential, None, &segments, limits, globals)?",
        "replacement": "build_document_info(Organization::Sequential, Some(0), &segments, limits, globals)?",
        "control_file": "specs/document.rs",
        "control_site": "&&& document_declared_page_count(document) == None",
        "control_replacement": "",
    },
)
PREFIX_INVARIANT = """page_states@.len()
                == document_spec::page_information_count(segments@, segment_index as nat),"""


def replace_once(source: str, original: str, replacement: str) -> str:
    pattern = re.compile(r"\s*".join(re.escape(token) for token in original.split()))
    if len(pattern.findall(source)) != 1:
        raise ValueError("document-count sensitivity site is missing or ambiguous")
    return pattern.sub(lambda _: replacement, source, count=1)


def mutated_document(source: str, case: dict) -> str:
    return replace_once(source, case["site"], case["replacement"])


def weaker_contract(source: str, case: dict) -> str:
    source = replace_once(source, case["control_site"], case["control_replacement"])
    if case["function"] == "collect_page_states_into":
        source = replace_once(source, PREFIX_INVARIANT, "")
    return source


def check_document_count_proof(executable: Path) -> dict:
    files = (
        "src/document.rs", "specs/document.rs", "proofs/production_impl/document.rs",
        "src/file_header.rs", "specs/file_header.rs",
    )
    sources = {name: (ROOT / name).read_bytes() for name in files}
    results = []
    with proof_workspace(ROOT, executable, "jbig2-count-proof-") as (
        directory, environment, expected, rust,
    ):
        path = directory / "src/document.rs"
        original = path.read_text(encoding="utf-8")
        for case in CASES:
            control_path = directory / case.get("control_file", "src/document.rs")
            control_original = control_path.read_text(encoding="utf-8")
            baseline = selected_proof(
                executable, directory, environment, False, "document", case["function"],
            )
            source = mutated_document(original, case)
            path.write_text(source, encoding="utf-8")
            run(["cargo", f"+{rust}", "check", "--offline", "--locked", "--lib"],
                directory, environment, capture=True)
            rejected = selected_proof(
                executable, directory, environment, True, "document", case["function"],
            )
            control_source = source if control_path == path else control_original
            control_path.write_text(weaker_contract(control_source, case), encoding="utf-8")
            control = selected_proof(
                executable, directory, environment, False, "document", case["function"],
            )
            results.append({
                "id": case["id"], "function": case["function"],
                "normal_rust_compiles": True, "strong_count_contract": "rejected",
                "weaker_control": "accepted", "strong_verifier_errors": rejected["errors"],
                "control_verifier_errors": control["errors"],
                "baseline_selected_verification": baseline,
            })
            path.write_text(original, encoding="utf-8")
            if control_path != path:
                control_path.write_text(control_original, encoding="utf-8")
    if any((ROOT / name).read_bytes() != data for name, data in sources.items()):
        raise ValueError("production source changed during document proof sensitivity checks")
    return {
        "schema_version": 1, "status": "passed", "verus_version": expected,
        "scope": "Selected document count/metadata proofs only; whole-crate verification is separate",
        "production_sources_modified": False, "counterexamples": results,
        "source_bindings": {
            name: hashlib.sha256(data).hexdigest() for name, data in sources.items()
        },
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--verus-executable", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    try:
        result = check_document_count_proof(arguments.verus_executable.resolve())
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"document count proof sensitivity failed: {error}", file=sys.stderr)
        result = {"schema_version": 1, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2))
    return 0 if result["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

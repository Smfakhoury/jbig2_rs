#!/usr/bin/env python3
"""Check semantic sensitivity of document cardinality and metadata contracts."""

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path

from check_distribution import run
from proof_sensitivity import proof_workspace, replace_once, selected_proof
from rust_source import source_functions


ROOT = Path(__file__).resolve().parents[1]
FUNCTIONS = {entry["id"]: entry for entry in source_functions(ROOT)}
PAYLOAD_FIELDS = """document_spec::payload_window(
    header, old(reader).bytes@, old(reader).position as int,
    final(reader).position as int, payload@,
)"""
SEQUENTIAL_FIELDS = """document_spec::sequential_container_fields(
    input@, segments@, require_local_references,
)"""
RANDOM_FIELDS = "document_spec::random_access_container_fields(input@, segments@)"
STANDALONE_WINDOWS = """Organization::Sequential => document_spec::sequential_container_fields(
        input@.subrange(
            crate::file_header::file_header_spec::byte_length(input@) as int,
            input@.len() as int,
        ),
        result.1@, true,
    ),"""
STANDALONE_RANDOM_WINDOWS = """Organization::RandomAccess => document_spec::random_access_container_fields(
        input@.subrange(
            crate::file_header::file_header_spec::byte_length(input@) as int,
            input@.len() as int,
        ),
        result.1@,
    ),"""
EMBEDDED_WINDOWS = "&& document_spec::sequential_container_fields(input@, result.1@, false)"


def quota_cases(function, comparison, postcondition):
    original = FUNCTIONS[f"document::{function}"]["body_text"]
    return (
        {
            "id": f"{function}_rejects_inclusive_boundary",
            "function": function,
            "site": original,
            "replacement": replace_once(
                original, f"if actual > {comparison} {{",
                f"if actual >= {comparison} {{",
            ),
            "control_site": postcondition,
            "control_replacement": "",
        },
        {
            "id": f"{function}_discards_error_actual",
            "function": function,
            "site": original,
            "replacement": replace_once(original, "actual,\n}", "actual: 0,\n}"),
            "control_site": postcondition,
            "control_replacement": "",
        },
    )


def result_case(identity, function, contract, replacement):
    original = FUNCTIONS[f"document::{function}"]["body_text"]
    return {
        "id": identity, "function": function, "site": original,
        "replacement": replace_once(original, "Ok(segments)", replacement),
        "control_site": contract, "control_replacement": "true",
    }


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
    *quota_cases(
        "enforce_stream_limit", "limits.max_stream_bytes",
        """out == document_spec::quota_result(
            length as nat, limits.max_stream_bytes, LimitKind::StreamBytes,
        ),""",
    ),
    *quota_cases(
        "enforce_input_work", "limits.max_work_units",
        """out == document_spec::quota_result(
            input_length as nat, limits.max_work_units, LimitKind::Work,
        ),""",
    ),
    *quota_cases(
        "enforce_segment_limit", "u64::from(limits.max_segments)",
        """out == document_spec::next_count_quota_result(
            count, limits.max_segments, LimitKind::Segments,
        ),""",
    ),
    *quota_cases(
        "enforce_page_limit", "u64::from(limits.max_pages)",
        """out == document_spec::next_count_quota_result(
            count, limits.max_pages, LimitKind::Pages,
        ),""",
    ),
    {
        "id": "known_payload_returns_wrong_bytes",
        "function": "read_payload",
        "site": "return Ok(reader.read_bytes(length)?);",
        "replacement": "reader.read_bytes(length)?;\n        return Ok(&[]);",
        "control_site": PAYLOAD_FIELDS, "control_replacement": "true",
    },
    {
        "id": "unknown_payload_returns_wrong_bytes",
        "function": "read_payload",
        "site": """let length = find_unknown_generic_length(reader.remaining_bytes())?;
            Ok(reader.read_bytes(length)?)""",
        "replacement": """let length = find_unknown_generic_length(reader.remaining_bytes())?;
            reader.read_bytes(length)?;
            Ok(&[])""",
        "control_site": PAYLOAD_FIELDS, "control_replacement": "true",
    },
    result_case("sequential_discards_consumed_segments", "parse_sequential", SEQUENTIAL_FIELDS, "Ok(Vec::new())"),
    result_case("sequential_rebinds_payload_after_validation", "parse_sequential", SEQUENTIAL_FIELDS, """if !segments.is_empty() {
        let mut first = segments.remove(0);
        first.payload = &[];
        segments.insert(0, first);
    }
    Ok(segments)"""),
    result_case("sequential_corrupts_header_after_validation", "parse_sequential", SEQUENTIAL_FIELDS, """if !segments.is_empty() {
        let mut first = segments.remove(0);
        first.header.number ^= 1;
        segments.insert(0, first);
    }
    Ok(segments)"""),
    {
        "id": "standalone_disables_sequential_number_order",
        "function": "parse_standalone_document",
        "site": "Organization::Sequential => parse_sequential(body, limits, true)?,",
        "replacement": "Organization::Sequential => parse_sequential(body, limits, false)?,",
        "control_site": STANDALONE_WINDOWS,
        "control_replacement": "Organization::Sequential => true,",
    },
    {
        "id": "embedded_discards_first_input_byte",
        "function": "parse_embedded_document",
        "site": "let segments = parse_sequential(input, limits, false)?;",
        "replacement": """let segments = if input.is_empty() {
            parse_sequential(input, limits, false)?
        } else {
            parse_sequential(&input[1..], limits, false)?
        };""",
        "control_site": EMBEDDED_WINDOWS, "control_replacement": "",
    },
    result_case("random_access_discards_consumed_segments", "parse_random_access", RANDOM_FIELDS, "Ok(Vec::new())"),
    result_case("random_access_rebinds_payload_after_validation", "parse_random_access", RANDOM_FIELDS, """if !segments.is_empty() {
        let mut first = segments.remove(0);
        first.payload = &[];
        segments.insert(0, first);
    }
    Ok(segments)"""),
    result_case("random_access_corrupts_header_after_validation", "parse_random_access", RANDOM_FIELDS, """if !segments.is_empty() {
        let mut first = segments.remove(0);
        first.header.number ^= 1;
        segments.insert(0, first);
    }
    Ok(segments)"""),
    result_case("random_access_drops_eof_after_validation", "parse_random_access", RANDOM_FIELDS, """segments.pop();
    Ok(segments)"""),
    {
        "id": "standalone_random_access_discards_first_body_byte",
        "function": "parse_standalone_document",
        "site": "Organization::RandomAccess => parse_random_access(body, limits)?,",
        "replacement": """Organization::RandomAccess => if body.is_empty() {
            parse_random_access(body, limits)?
        } else {
            parse_random_access(&body[1..], limits)?
        },""",
        "control_site": STANDALONE_RANDOM_WINDOWS,
        "control_replacement": "Organization::RandomAccess => true,",
    },
)
PREFIX_INVARIANT = """page_states@.len()
                == document_spec::page_information_count(segments@, segment_index as nat),"""


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
        "src/file_header.rs", "specs/file_header.rs", "proofs/model_support/document.rs",
        "src/segment_header.rs", "specs/segments.rs",
    )
    sources = {name: (ROOT / name).read_bytes() for name in files}
    results = []
    with proof_workspace(ROOT, executable, "jbig2-count-proof-") as (
        directory, environment, expected, rust,
    ):
        path = directory / "src/document.rs"
        original = path.read_text(encoding="utf-8")
        functions = tuple(sorted({case["function"] for case in CASES}))
        if any(not FUNCTIONS[f"document::{name}"]["verification_requested"] for name in functions):
            raise ValueError("document baseline contains an unverified production function")
        baseline = selected_proof(
            executable, directory, environment, False, "document", functions,
        )
        for case in CASES:
            print(f"document contrast: {case['id']}", file=sys.stderr, flush=True)
            control_path = directory / case.get("control_file", "src/document.rs")
            control_original = control_path.read_text(encoding="utf-8")
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
        "baseline_verification_scope": "document::*",
        "baseline_selected_functions": list(functions),
        "scope": "Document count/metadata, accepted sequential/random-access raw byte windows/order/EOF and exact project numeric preflight quotas; not complete input acceptance/errors, decoded ParsedData, unknown-length termination, allocator/RSS or normative approval",
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

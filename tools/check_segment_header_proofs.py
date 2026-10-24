#!/usr/bin/env python3
"""Check complete successful segment-header correspondence with raw input."""

import argparse
import json
from pathlib import Path
import subprocess
import sys

from proof_sensitivity import check_semantic_cases


ROOT = Path(__file__).resolve().parents[1]
HEADER = """Ok(SegmentHeader {
    number, segment_type, deferred_non_retain, referred_segments, retention_flags,
    page_association, data_length, byte_length,
})"""
FIELDS = """crate::spec::segments::segment_header_fields(
    &header, old(reader).bytes@, old(reader).position as int,
    final(reader).position as int,
)"""
REFERENCES = """let (referred_segments, retention_flags) =
    parse_referred_segments(reader, number, limits)?;"""


def case(identity, site, replacement):
    return {
        "id": identity, "file": "src/segment_header.rs", "module": "segment_header",
        "function": "parse_segment_header", "site": site, "replacement": replacement,
        "controls": ((FIELDS, "true"),),
    }


CASES = (
    case("number_discarded", HEADER, HEADER.replace("number,", "number: 0,")),
    case("type_discarded", HEADER, HEADER.replace(
        "segment_type,", "segment_type: SegmentType::SymbolDictionary,",
    )),
    case("deferred_flag_discarded", HEADER, HEADER.replace(
        "deferred_non_retain,", "deferred_non_retain: false,",
    )),
    case("reference_list_discarded", HEADER, HEADER.replace(
        "referred_segments,", "referred_segments: Vec::new(),",
    )),
    case("retention_list_discarded", HEADER, HEADER.replace(
        "retention_flags,", "retention_flags: RetentionFlags { values: Vec::new() },",
    )),
    case("page_association_discarded", HEADER, HEADER.replace(
        "page_association,", "page_association: 0,",
    )),
    case("data_length_discarded", HEADER, HEADER.replace(
        "data_length,", "data_length: Some(0),",
    )),
    case("header_extent_discarded", HEADER, HEADER.replace(
        "byte_length,", "byte_length: 0,",
    )),
    case("first_retention_wire_bit_flipped", REFERENCES, """let (referred_segments, mut retention_flags) =
        parse_referred_segments(reader, number, limits)?;
        retention_flags.values[0] ^= 1;"""),
    case("first_reference_changed_within_range", REFERENCES, """let (mut referred_segments, retention_flags) =
        parse_referred_segments(reader, number, limits)?;
        if number > 1 && !referred_segments.is_empty() {
            referred_segments[0] = if referred_segments[0] == 0 { 1 } else { 0 };
        }"""),
)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--verus-executable", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    try:
        report = check_semantic_cases(
            ROOT, arguments.verus_executable.resolve(), CASES,
            ("src/segment_header.rs", "specs/segments.rs",
             "proofs/production_impl/segment_header.rs",
             "proofs/shared/segment_header.rs", "proofs/spec_properties/segment_header.rs"),
            "jbig2-segment-header-",
            "Exact successful raw header fields, reference/retention contents and byte extent; "
            "not complete valid-input acceptance, typed partial errors, resource accounting or normative approval",
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"segment header proof sensitivity failed: {error}", file=sys.stderr)
        report = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Check exact successful pattern/halftone header fields and consumed lengths."""

import argparse
import json
from pathlib import Path
import subprocess
import sys

from proof_sensitivity import check_semantic_cases


ROOT = Path(__file__).resolve().parents[1]
PATTERN = """Ok(PatternDictionaryHeader {
    mmr, template, pattern_width, pattern_height, gray_max,
})"""
HALFTONE = """Ok(HalftoneHeader {
    mmr, template, enable_skip, operator, default_pixel: flags & 0x80 != 0,
    grid_width, grid_height, grid_x, grid_y, grid_vector_x, grid_vector_y,
})"""
PATTERN_FIELDS = """halftone_spec::pattern_header_fields(
    header, old(reader).bytes@, old(reader).position as int,
)"""
HALFTONE_FIELDS = """halftone_spec::halftone_header_fields(
    header, old(reader).bytes@, old(reader).position as int,
)"""


def case(identity, function, site, replacement, control, weaker="true"):
    return {
        "id": identity, "file": "src/halftone.rs", "module": "halftone",
        "function": function, "site": site, "replacement": replacement,
        "controls": ((control, weaker),),
    }


CASES = (
    case(
        "valid_pattern_header_is_rejected", "parse_pattern_dictionary_header",
        """pub(crate) fn parse_pattern_dictionary_header(
            reader: &mut ByteReader<'_>,
        ) -> Result<PatternDictionaryHeader, HalftoneError> {""",
        """pub(crate) fn parse_pattern_dictionary_header(
            reader: &mut ByteReader<'_>,
        ) -> Result<PatternDictionaryHeader, HalftoneError> {
            if reader.remaining() >= 7 {
                return Err(HalftoneError::InvalidPatternDictionaryFlags(0));
            }""",
        """out.is_ok() == halftone_spec::pattern_header_accepted(
            old(reader).bytes@, old(reader).position as int,
        ),""", "",
    ),
    case(
        "valid_halftone_header_is_rejected", "parse_halftone_header",
        """pub(crate) fn parse_halftone_header(
            reader: &mut ByteReader<'_>,
        ) -> Result<HalftoneHeader, HalftoneError> {""",
        """pub(crate) fn parse_halftone_header(
            reader: &mut ByteReader<'_>,
        ) -> Result<HalftoneHeader, HalftoneError> {
            if reader.remaining() >= 21 {
                return Err(HalftoneError::InvalidHalftoneFlags(0));
            }""",
        """out.is_ok() == halftone_spec::halftone_header_accepted(
            old(reader).bytes@, old(reader).position as int,
        ),""", "",
    ),
    case(
        "pattern_header_discards_width", "parse_pattern_dictionary_header",
        PATTERN, PATTERN.replace("pattern_width,", "pattern_width: 1,"), PATTERN_FIELDS,
    ),
    case(
        "pattern_header_discards_gray_max", "parse_pattern_dictionary_header",
        PATTERN, PATTERN.replace("gray_max,", "gray_max: 0,"), PATTERN_FIELDS,
    ),
    case(
        "pattern_header_rewinds_consumed_byte", "parse_pattern_dictionary_header",
        PATTERN, "reader.position -= 1;\n" + PATTERN,
        "&& final(reader).position == old(reader).position + 7", "",
    ),
    case(
        "halftone_header_discards_grid_width", "parse_halftone_header",
        HALFTONE, HALFTONE.replace("grid_width,", "grid_width: 0,"), HALFTONE_FIELDS,
    ),
    case(
        "halftone_header_discards_signed_origin", "parse_halftone_header",
        HALFTONE, HALFTONE.replace("grid_x,", "grid_x: 0,"), HALFTONE_FIELDS,
    ),
    case(
        "halftone_header_discards_operator", "parse_halftone_header",
        HALFTONE, HALFTONE.replace("operator,", "operator: CombinationOperator::Or,"), HALFTONE_FIELDS,
    ),
    case(
        "halftone_header_discards_default_pixel", "parse_halftone_header",
        HALFTONE, HALFTONE.replace("flags & 0x80 != 0", "false"), HALFTONE_FIELDS,
    ),
)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--verus-executable", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    try:
        report = check_semantic_cases(
            ROOT, arguments.verus_executable.resolve(), CASES,
            ("src/halftone.rs", "specs/halftone.rs", "specs/segments.rs"),
            "jbig2-halftone-headers-",
            "Exact project header acceptance/rejection, successful fields and consumed lengths; typed partial errors and independent normative translation remain separate",
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"halftone header proof sensitivity failed: {error}", file=sys.stderr)
        report = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

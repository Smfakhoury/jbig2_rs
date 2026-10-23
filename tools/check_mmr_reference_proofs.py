#!/usr/bin/env python3
"""Check reference selection and actual raster-loop cache/pixel correspondence."""

import argparse
import json
from pathlib import Path
import subprocess
import sys

from proof_sensitivity import check_semantic_cases, replace_once
from rust_source import source_functions


ROOT = Path(__file__).resolve().parents[1]
FUNCTIONS = {row["id"]: row for row in source_functions(ROOT)}
FULL = FUNCTIONS["mmr::reference_elements"]["body_text"]
FROM = FUNCTIONS["mmr::reference_elements_from"]["body_text"]
RASTER = FUNCTIONS["mmr::decode_bitmap"]["body_text"]
FULL_PAIR = """&& mmr_spec::reference_pair_matches(
    transitions@, x, black, width, initial, 0, b1, b2,
)"""
SUFFIX_PAIR = """&& mmr_spec::reference_pair_matches(
    transitions@, x, black, width, initial,
    *old(search_start) as int, b1, b2,
)"""
CURSOR = """&& mmr_spec::reference_cursor_matches(
    transitions@, x, initial,
    *old(search_start) as int, *final(search_start) as int,
)"""
GLOBAL_PAIR = """&& (mmr_properties::reference_prefix_skipped(
    transitions@, x, initial, *old(search_start) as int,
) ==> mmr_spec::reference_pair_matches(
        transitions@, x, black, width, initial, 0, b1, b2,
    ))"""
FULL_ACCEPT = "ensures out.is_ok(), match out { Ok((b1, b2)) =>"
FULL_ACCEPT_CONTROL = "ensures match out { Ok((b1, b2)) =>"
FROM_ACCEPT = "*final(search_start) < transitions@.len(), out.is_ok(),"
FROM_ACCEPT_CONTROL = "*final(search_start) < transitions@.len(),"
RASTER_TAIL = "production_mmr_row_termination!(row, height);"
ROW_REFERENCE = """row > 0 ==> mmr_spec::row_cache_matches(
    &bitmap, (row - 1) as u32, transitions@,
),"""
WHITE_REFERENCE = "row == 0 ==> mmr_spec::white_reference_cache(transitions@, width),"
ROW_PREFIX = """mmr_spec::row_cache_prefix(
    &bitmap, row, row_transitions@, emitted_black, x,
),"""
RASTER_CONTROLS = (
    (WHITE_REFERENCE + "\n" + ROW_REFERENCE, WHITE_REFERENCE),
    (ROW_PREFIX + "\n" + ROW_REFERENCE, ROW_PREFIX),
)


def case(identity, function, source, replacement, controls):
    return {
        "id": identity, "file": "src/mmr.rs", "module": "mmr",
        "function": function, "site": source, "replacement": replacement,
        "controls": controls,
    }


CASES = (
    case(
        "full_reference_discards_second_element", "reference_elements", FULL,
        replace_once(FULL, "return Ok((position, b2));", "return Ok((position, width));"),
        ((FULL_PAIR, ""),),
    ),
    case(
        "full_reference_falsely_rejects_valid_row_end", "reference_elements", FULL,
        replace_once(
            FULL, "if transitions.len() < 2 {",
            """if x == width { return Err(MmrError::InvalidTransition); }
            if transitions.len() < 2 {""",
        ),
        ((FULL_ACCEPT, FULL_ACCEPT_CONTROL),),
    ),
    case(
        "cursor_reference_discards_first_candidate", "reference_elements_from", FROM,
        replace_once(FROM, "return Ok((position, b2));", "return Ok((width, width));"),
        ((SUFFIX_PAIR, ""), (GLOBAL_PAIR, "")),
    ),
    case(
        "cursor_reference_collapses_second_element", "reference_elements_from", FROM,
        replace_once(FROM, "return Ok((position, b2));", "return Ok((position, position));"),
        ((SUFFIX_PAIR, ""), (GLOBAL_PAIR, "")),
    ),
    case(
        "cursor_reference_loses_terminal_cache_position", "reference_elements_from", FROM,
        replace_once(FROM, "Ok((width, width))", "*search_start = 0;\nOk((width, width))"),
        ((CURSOR, ""),),
    ),
    case(
        "cursor_reference_falsely_rejects_valid_row_end", "reference_elements_from", FROM,
        replace_once(
            FROM, "let search_end = transitions",
            """if x == width { return Err(MmrError::InvalidTransition); }
            let search_end = transitions""",
        ),
        ((FROM_ACCEPT, FROM_ACCEPT_CONTROL),),
    ),
    case(
        "raster_reference_pixels_corrupted_after_row_finish", "decode_bitmap", RASTER,
        replace_once(
            RASTER, RASTER_TAIL,
            RASTER_TAIL + """
            if row == 0 && height == 1 && width > 0 && !bitmap.data.is_empty() {
                bitmap.data[0] ^= 0x80;
            }
            #[cfg(verus_keep_ghost)]
            proof! { crate::views::bitmap_fields(&bitmap); }""",
        ),
        RASTER_CONTROLS,
    ),
    case(
        "raster_reference_cache_corrupted_after_row_swap", "decode_bitmap", RASTER,
        replace_once(
            RASTER, RASTER_TAIL,
            RASTER_TAIL + """
            if row == 0 && transitions.len() > 2 {
                transitions[0] = 0;
            }""",
        ),
        RASTER_CONTROLS,
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
            ("src/mmr.rs", "specs/mmr.rs", "proofs/model_support/mmr.rs",
             "proofs/spec_properties/mmr.rs", "proofs/shared/bitmap.rs"),
            "jbig2-mmr-reference-",
            "Exact project reference pair/cursor/acceptance and actual raster-loop reference-cache correspondence to the completed physical row. Shape-preserving pixel/cache corruption is rejected without deleting proof support; controls remove only the two previous-row correspondence invariants. Normative/input-to-raster conformance remains separate",
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"MMR reference proof sensitivity failed: {error}", file=sys.stderr)
        report = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

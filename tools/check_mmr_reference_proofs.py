#!/usr/bin/env python3
"""Check reference selection and actual raster-loop cache/pixel correspondence."""

import argparse
import json
from pathlib import Path
import re
import subprocess
import sys

from proof_sensitivity import check_semantic_cases, replace_once
from source_group import runtime_function


ROOT = Path(__file__).resolve().parents[1]
FULL = runtime_function(ROOT, "mmr", "reference_elements")["body_text"]
FROM = runtime_function(ROOT, "mmr", "reference_elements_from")["body_text"]
RASTER = runtime_function(ROOT, "mmr", "decode_bitmap")["body_text"]
FINISH = runtime_function(ROOT, "mmr", "finish_required_prefix")["body_text"]
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
TRACE_RESULT = """&& mmr_spec::bitmap_decodes(
    data@, width, height, &bitmap, reader.bit_position as nat,
),"""
TRACE_OUTER = """mmr_spec::raster_outer(
    data@, width, row, &bitmap, traced_rows, transitions@,
    reader.bit_position as nat,
),"""
TRACE_INNER = """mmr_spec::raster_inner(
    data@, width, row, &bitmap, traced_rows, transitions@,
    traced_start, traced_steps, initial,
    mmr_spec::raster_state(
        reader.bit_position as nat, x, black, row_transitions@, emitted_black,
    ),
),"""
TRACE_OUTPUT = "production_mmr_trace_output!(data, width, height, bitmap, reader, traced_rows);"
TRACE_MACRO = re.compile(
    r"#\[\s*cfg\s*\(\s*verus_keep_ghost\s*\)\s*\]\s*"
    r"production_mmr_trace_[a-z_]+!\([^;]*?\);",
)
PREFIX_MARKER_RESULT = """&& final(reader).bit_position % 8 == 0
    && mmr_spec::exact_eofb(
        old(reader).bytes@, old(reader).bit_position as nat,
    )"""
PREFIX_FRAME_RESULT = PREFIX_MARKER_RESULT + """
&& mmr_spec::required_eofb_prefix(
    old(reader).bytes@, old(reader).bit_position as nat, final(reader).bit_position as nat,
),"""


def case(identity, function, source, replacement, controls):
    return {
        "id": identity, "file": "src/mmr.rs", "module": "mmr",
        "function": function, "site": source, "replacement": replacement,
        "controls": controls,
    }

def cache_raster_case(identity, corruption):
    replacement = replace_once(RASTER, RASTER_TAIL, RASTER_TAIL + corruption)
    weaker = replacement
    for original, control in RASTER_CONTROLS:
        weaker = replace_once(weaker, original, control)
    weaker = replace_once(weaker, TRACE_OUTER, "")
    weaker = replace_once(weaker, TRACE_INNER, "")
    weaker, count = TRACE_MACRO.subn("", weaker)
    if count != 12:
        raise ValueError("raster cache control must remove exactly the new trace-only bridges")
    return case(
        identity, "decode_bitmap", RASTER, replacement,
        ((replacement, weaker), (TRACE_RESULT, ",")),
    )


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
    cache_raster_case(
        "raster_reference_pixels_corrupted_after_row_finish",
        """
            if row == 0 && height == 1 && width > 0 && !bitmap.data.is_empty() {
                bitmap.data[0] ^= 0x80;
            }
            #[cfg(verus_keep_ghost)]
            proof! { crate::views::bitmap_fields(&bitmap); }""",
    ),
    cache_raster_case(
        "raster_reference_cache_corrupted_after_row_swap",
        """
            if row == 0 && transitions.len() > 2 {
                transitions[0] = 0;
            }""",
    ),
    case(
        "raster_output_discards_actual_input_extent", "decode_bitmap", RASTER,
        replace_once(RASTER, TRACE_OUTPUT, TRACE_OUTPUT + "\nreader.bit_position = 0;"),
        ((TRACE_RESULT, ","),),
    ),
    case(
        "raster_output_blackens_a_white_pixel", "decode_bitmap", RASTER,
        replace_once(
            RASTER, TRACE_OUTPUT,
            TRACE_OUTPUT + """
            #[cfg(verus_keep_ghost)]
            proof! {
                crate::views::bitmap_fields(&bitmap);
                reveal(crate::spec::bitmap::storage_valid);
                assert(bitmap.stride as nat
                    == crate::spec::bitmap::stride_for_width(bitmap.width as nat));
                assert(bitmap.data@.len() == bitmap.stride as nat * bitmap.height as nat);
            }
            if bitmap.width > 0 && bitmap.height > 0 {
                bitmap.fill_black_span(0, 0, 1)
                    .map_err(|_error| MmrError::InvalidTransition)?;
            }
            #[cfg(verus_keep_ghost)]
            proof! {
                crate::views::bitmap_fields(&bitmap);
                reveal(crate::spec::bitmap::storage_valid);
                assert(crate::spec::bitmap::storage_valid(&bitmap));
            }""",
        ),
        ((TRACE_RESULT, ","),),
    ),
    case(
        "required_prefix_accepts_nonzero_alignment_bits", "finish_required_prefix", FINISH,
        replace_once(
            FINISH,
            """if reader.read_bits(padding_count)? != 0 {
                return Err(MmrError::NonZeroEofbPadding);
            }""",
            "let _padding_value = reader.read_bits(padding_count)?;",
        ),
        ((PREFIX_FRAME_RESULT, PREFIX_MARKER_RESULT + ","),),
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
             "proofs/spec_properties/mmr.rs", "proofs/shared/bitmap.rs",
             "proofs/production_impl/mmr.rs"),
            "jbig2-mmr-reference-",
            "Exact project reference selection, actual row/cache composition and accepted input-to-raster/extent/EOFB-padding results. Output corruptions retain all loop/trace proofs and remove only the new result; earlier cache controls additionally remove only logically coupled trace-result support in the weaker variant. Independent normative table/coordinate fidelity, complete valid-input acceptance/errors and resource accounting remain separate",
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

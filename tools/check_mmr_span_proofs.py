#!/usr/bin/env python3
"""Check exact conditional MMR span pixels, no-ops and transition-cache updates."""

import argparse
import json
import subprocess
import sys
from pathlib import Path

from proof_sensitivity import check_semantic_cases, replace_once
from rust_source import source_functions


ROOT = Path(__file__).resolve().parents[1]
BASELINE_GROUPS = (("mmr", ("fill_recorded_span", "fill_span")),)
NO_OP = """(start > end || end > old(bitmap).width || row >= old(bitmap).height
    || !black || start == end)
    ==> final(bitmap).data@ == old(bitmap).data@,"""
FLIP = """if let Some(byte) = bitmap.data.get_mut(0) { *byte ^= 1; }"""
CALL = """bitmap .fill_black_span(row, start, end)
    .map_err(|_error| MmrError::InvalidTransition)?;"""
CANONICAL_SUCCESS = """(crate::spec::bitmap::storage_valid(old(bitmap))
    && start <= end <= old(bitmap).width && row < old(bitmap).height)
    ==> out.is_ok(),"""
CANONICAL_RESULT = """(out.is_ok() && crate::spec::bitmap::storage_valid(old(bitmap))
    && start <= end <= old(bitmap).width && row < old(bitmap).height)
    ==> mmr_spec::span_result(old(bitmap), final(bitmap), row, start, end, black),"""
TRANSITION_RESULT = """mmr_spec::transition_span_result(
    old(transitions)@, final(transitions)@,
    *old(emitted_black), *final(emitted_black), start, end, black,
)"""
ROW_PREFIX_RESULT = """mmr_spec::row_cache_prefix(
    old(bitmap), row, old(transitions)@, *old(emitted_black), start,
) ==> mmr_spec::row_cache_prefix(
    final(bitmap), row, final(transitions)@, *final(emitted_black), end,
),"""
RECORDED = next(
    row["body_text"] for row in source_functions(ROOT)
    if row["id"] == "mmr::fill_recorded_span"
)
RECORD_CALL = "record_transition_span(transitions, emitted_black, start, end, width, black);"
ROLLED_BACK = replace_once(
    replace_once(
        RECORDED, RECORD_CALL,
        "let saved_length = transitions.len();\n"
        "let saved_emitted = *emitted_black;\n" + RECORD_CALL,
    ),
    "Ok(())\n}",
    """if transitions.len() > saved_length { let _ = transitions.pop(); }
    *emitted_black = saved_emitted;
    Ok(())
}""",
)
CASES = (
    {
        "id": "invalid_span_mutates_storage", "file": "src/mmr.rs",
        "module": "mmr", "function": "fill_span",
        "site": """if start > end || end > bitmap.width || row >= bitmap.height {
            return Err(MmrError::InvalidTransition);
        }""",
        "replacement": f"""if start > end || end > bitmap.width || row >= bitmap.height {{
            {FLIP}
            return Err(MmrError::InvalidTransition);
        }}""",
        "controls": ((NO_OP, ""),),
    },
    {
        "id": "white_span_mutates_storage", "file": "src/mmr.rs",
        "module": "mmr", "function": "fill_span",
        "site": "if !black { return Ok(()); }",
        "replacement": f"if !black {{ {FLIP} return Ok(()); }}",
        "controls": ((NO_OP, ""), (CANONICAL_RESULT, "")),
    },
    {
        "id": "empty_black_span_mutates_storage", "file": "src/mmr.rs",
        "module": "mmr", "function": "fill_span",
        "site": CALL,
        "replacement": f"if start == end {{ {FLIP} return Ok(()); }}\n{CALL}",
        "controls": ((NO_OP, ""), (CANONICAL_RESULT, "")),
    },
    {
        "id": "black_span_is_skipped", "file": "src/mmr.rs",
        "module": "mmr", "function": "fill_span",
        "site": CALL, "replacement": "return Ok(());",
        "controls": ((CANONICAL_RESULT, ""),),
    },
    {
        "id": "canonical_valid_span_is_falsely_rejected", "file": "src/mmr.rs",
        "module": "mmr", "function": "fill_span",
        "site": CALL,
        "replacement": "if start < end { return Err(MmrError::InvalidTransition); }\n" + CALL,
        "controls": ((CANONICAL_SUCCESS, ""),),
    },
    {
        "id": "recorded_span_discards_cache_update", "file": "src/mmr.rs",
        "module": "mmr", "function": "fill_recorded_span",
        "site": RECORDED,
        "replacement": ROLLED_BACK,
        "controls": (("&& " + TRANSITION_RESULT, ""), (ROW_PREFIX_RESULT, "")),
    },
    {
        "id": "transition_cache_stores_wrong_emitted_colour", "file": "src/mmr.rs",
        "module": "mmr", "function": "record_transition_span",
        "site": "*emitted_black = black;",
        "replacement": "*emitted_black = !black;",
        "controls": ((TRANSITION_RESULT + ",", ""),),
    },
    {
        "id": "empty_span_corrupts_emitted_colour", "file": "src/mmr.rs",
        "module": "mmr", "function": "record_transition_span",
        "site": "if start == end { return; }",
        "replacement": "if start == end { *emitted_black = !*emitted_black; return; }",
        "controls": ((TRANSITION_RESULT + ",", ""),),
    },
)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--verus-executable", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    try:
        result = check_semantic_cases(
            ROOT, arguments.verus_executable.resolve(), CASES,
            ("src/mmr.rs", "src/bitmap.rs", "specs/mmr.rs", "specs/bitmap.rs",
             "proofs/model_support/mmr.rs", "proofs/production_impl/mmr.rs",
             "proofs/spec_properties/mmr.rs", "proofs/shared/bitmap.rs"),
            "jbig2-mmr-span-proof-",
            "Exact canonical-storage-conditional black-span/white-no-op effects and exact internal transition/emitted-state updates; original invalid/white/empty no-op controls retained; no strengthened preconditions or complete raster-loop/polarity claim",
            baseline_groups=BASELINE_GROUPS,
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"MMR span proof sensitivity failed: {error}", file=sys.stderr)
        result = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2))
    return 0 if result["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

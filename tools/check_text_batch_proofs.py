#!/usr/bin/env python3
"""Check exact text batch clearing/no-op proofs, not general pixel semantics."""

import argparse
import json
import subprocess
import sys
from pathlib import Path

from proof_sensitivity import check_semantic_cases, replace_once
from rust_source import source_functions


ROOT = Path(__file__).resolve().parents[1]
FLIP = """if let Some(byte) = destination.data.get_mut(0) { *byte ^= 1; }
#[cfg(verus_keep_ghost)]
proof! {
    crate::views::bitmap_fields(destination);
    crate::views::bitmap_fields(old(destination));
}"""
NO_OP = """(old(batch).blits@.len() == 0 || old(batch).height == 0) ==>
    crate::views::bitmap_data(final(destination))
        == crate::views::bitmap_data(old(destination)),"""
CLEAR = "&& final(batch).blits@.len() == 0"
MULTI_CALL = """destination.compose_row_batch_chunked(
    &batch.blits, batch.source_y, batch.destination_y, batch.height, operator,
)?;"""
WORK_EFFECT = """ensures symbol_text_spec::work_charge_effect(
    old(self), final(self), units, limits, out.is_ok(),
),"""
BITMAP_EFFECT = """ensures symbol_text_spec::bitmap_charge_effect(
    old(self), final(self), width, height, limits, out.is_ok(),
),"""
DIMENSION_ACCEPTANCE = """ensures out.is_ok() == (
    width <= limits.max_width
    && height <= limits.max_height
    && width as nat * height as nat <= limits.max_pixels_per_page
),"""
PREFLIGHT_RESULT = """ensures symbol_text_spec::placement_preflight_result(
    destination, source, placement, old(resources), final(resources), limits, out,
),"""
INITIALIZATION_ACCEPTANCE = """ensures out.is_ok()
    == symbol_text_spec::text_initialization_allowed(
        config, declared_instances, initial_strip_value, limits,
    ),"""
CURRENT_ACCEPTANCE = """out.is_ok() == symbol_text_spec::text_placement_allowed(
    old(self), within_strip_t, width, height, old(resources), limits,
),"""
WORK_LIMIT_CALL = "enforce_limit(LimitKind::Work, limits.max_work_units, self.work_units)?;"
BITMAP_LIMIT_CALL = """enforce_limit(
    LimitKind::DecodedBitmapBytes,
    limits.max_cumulative_decoded_bitmap_bytes,
    self.decoded_bitmap_bytes,
)?;"""
BODIES = {entry["id"]: entry["body_text"] for entry in source_functions(ROOT)}
DIMENSION_BODY = BODIES["symbol_text::check_dimensions"]
CURRENT_BODY = BODIES["symbol_text::TextRegionState::place_current"]
CASES = (
    {
        "id": "empty_batch_mutates_storage", "file": "src/symbol_text.rs",
        "module": "symbol_text", "function": "flush_placement_batch",
        "site": "if batch.blits.is_empty() {",
        "replacement": "if batch.blits.is_empty() {" + FLIP,
        "controls": ((NO_OP, ""),),
    },
    {
        "id": "zero_height_multi_batch_mutates_storage", "file": "src/symbol_text.rs",
        "module": "symbol_text", "function": "flush_placement_batch",
        "site": MULTI_CALL,
        "replacement": f"if batch.height == 0 {{ {FLIP} }}\n{MULTI_CALL}",
        "controls": ((NO_OP, ""),),
    },
    {
        "id": "single_batch_keeps_pending_entries", "file": "src/symbol_text.rs",
        "module": "symbol_text", "function": "flush_placement_batch",
        "site": "if batch.blits.len() == 1 {",
        "replacement": "if batch.blits.len() == 1 { return Ok(());",
        "controls": ((CLEAR, ""),),
    },
    {
        "id": "multi_batch_keeps_pending_entries", "file": "src/symbol_text.rs",
        "module": "symbol_text", "function": "flush_placement_batch",
        "site": MULTI_CALL + "\nbatch.blits.clear();",
        "replacement": MULTI_CALL,
        "controls": ((CLEAR, ""),),
    },
    {
        "id": "rejected_work_charge_is_silently_rolled_back", "file": "src/symbol_text.rs",
        "module": "symbol_text", "function": "ResourceTracker::charge_work",
        "site": WORK_LIMIT_CALL,
        "replacement": """
if self.work_units > limits.max_work_units {
    let charged = self.work_units;
    self.work_units -= units;
    return Err(SymbolTextError::Decode(DecodeError::LimitExceeded(LimitExceeded {
        kind: LimitKind::Work, limit: limits.max_work_units, actual: charged,
    })));
}
""" + WORK_LIMIT_CALL,
        "controls": ((WORK_EFFECT, "ensures"),),
    },
    {
        "id": "rejected_bitmap_charge_is_silently_rolled_back", "file": "src/symbol_text.rs",
        "module": "symbol_text", "function": "ResourceTracker::charge_bitmap",
        "site": BITMAP_LIMIT_CALL,
        "replacement": """
if self.decoded_bitmap_bytes > limits.max_cumulative_decoded_bitmap_bytes {
    let charged = self.decoded_bitmap_bytes;
    self.decoded_bitmap_bytes -= bytes;
    return Err(SymbolTextError::Decode(DecodeError::LimitExceeded(LimitExceeded {
        kind: LimitKind::DecodedBitmapBytes,
        limit: limits.max_cumulative_decoded_bitmap_bytes, actual: charged,
    })));
}
""" + BITMAP_LIMIT_CALL,
        "controls": ((BITMAP_EFFECT, "ensures"),),
    },
    {
        "id": "valid_empty_dimensions_are_rejected", "file": "src/symbol_text.rs",
        "module": "symbol_text", "function": "check_dimensions",
        "site": DIMENSION_BODY,
        "replacement": replace_once(
            DIMENSION_BODY, ") -> Result<(), SymbolTextError> {",
            """) -> Result<(), SymbolTextError> {
if width == 0 && height == 0 { return Err(SymbolTextError::InvalidDimensions); }
""",
        ),
        "controls": ((DIMENSION_ACCEPTANCE, "ensures"),),
    },
    {
        "id": "preflight_selects_wrong_in_bounds_source_column", "file": "src/symbol_text.rs",
        "module": "symbol_text", "function": "prepare_placement",
        "site": "Ok(Some(prepared))\n}",
        "replacement": """
let mut wrong = prepared;
if wrong.blit.source_x + wrong.blit.width < source.width() {
    wrong.blit.source_x += 1;
}
Ok(Some(wrong))
}""",
        "controls": ((PREFLIGHT_RESULT, "ensures"),),
    },
    {
        "id": "preflight_rejects_valid_origin", "file": "src/symbol_text.rs",
        "module": "symbol_text", "function": "prepare_placement",
        "site": "let source_width = i64::from(source.width());",
        "replacement": """
if placement.x == 0 && placement.y == 0 {
    return Err(SymbolTextError::InvalidState("mutant false preflight rejection"));
}
let source_width = i64::from(source.width());""",
        "controls": ((PREFLIGHT_RESULT, "ensures"),),
    },
    {
        "id": "initialization_rejects_valid_zero_instances", "file": "src/symbol_text.rs",
        "module": "symbol_text", "function": "TextRegionState::new",
        "site": "let scaled = initial_strip_value",
        "replacement": """
if declared_instances == 0 {
    return Err(SymbolTextError::InvalidCount("mutant false initialization rejection"));
}
let scaled = initial_strip_value""",
        "controls": ((INITIALIZATION_ACCEPTANCE, "ensures"),),
    },
    {
        "id": "current_placement_rejects_valid_single_pixel", "file": "src/symbol_text.rs",
        "module": "symbol_text", "function": "TextRegionState::place_current",
        "site": CURRENT_BODY,
        "replacement": replace_once(
            CURRENT_BODY, "if width == 0 || height == 0 {",
            """if width == 1 && height == 1 {
    return Err(SymbolTextError::InvalidDimensions);
}
if width == 0 || height == 0 {""",
        ),
        "controls": ((CURRENT_ACCEPTANCE, ""),),
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
            ("src/bitmap.rs", "src/symbol_text.rs", "specs/bitmap.rs", "specs/symbol_text.rs",
             "proofs/production_impl/symbol_text.rs"),
            "jbig2-text-batch-",
            "Executing text resource/state/preflight and batch results: exact counter/acceptance effects, interval clipping geometry and retained source contents, nonvacuous initialization/current-placement acceptance, retained clearing/no-op contracts; typed error payloads and general composition pixels/order remain separately tested, not fully derived",
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"text batch proof sensitivity failed: {error}", file=sys.stderr)
        result = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2))
    return 0 if result["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

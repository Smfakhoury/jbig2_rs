#!/usr/bin/env python3
"""Check exact text batch clearing/no-op proofs, not general pixel semantics."""

import argparse
import json
import subprocess
import sys
from pathlib import Path

from proof_sensitivity import check_semantic_cases


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
)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--verus-executable", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    try:
        result = check_semantic_cases(
            ROOT, arguments.verus_executable.resolve(), CASES,
            ("src/bitmap.rs", "src/symbol_text.rs", "specs/bitmap.rs", "specs/symbol_text.rs"),
            "jbig2-text-batch-",
            "Executing text batch dispatcher: retained storage/frame, successful clearing and empty/zero-height no-op contracts; general composition pixels/order remain scalar-oracle tested, not formally derived",
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

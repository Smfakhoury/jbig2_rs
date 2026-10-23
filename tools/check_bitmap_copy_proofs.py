#!/usr/bin/env python3
"""Check bitmap copying, zero initialization and padding semantic contracts."""

import argparse
import json
import subprocess
import sys
from pathlib import Path

from proof_sensitivity import check_semantic_cases, replace_once, weakened_contract
from rust_source import source_functions


ROOT = Path(__file__).resolve().parents[1]
ROW_COPY = """if source < destination {
            let (before, after) = self.data.split_at_mut(destination_start);
            copy_row_bytes(&mut after[..self.stride], &before[source_start..source_end]);
        } else {
            let (before, after) = self.data.split_at_mut(source_start);
            copy_row_bytes(
                &mut before[destination_start..destination_end],
                &after[..self.stride],
            );
        }"""
ROW_RESULT = "crate::spec::bitmap::row_copy_result(old(self), final(self), source, destination),"
PREFIX_COPY = """if byte_count == 8 {
            self.data.as_mut_slice()[..8] .copy_from_slice(&source.data.as_slice()[..8]);
        } else {
            copy_row_bytes(
                &mut self.data.as_mut_slice()[..byte_count],
                &source.data.as_slice()[..byte_count],
            );
        }"""
PREFIX_RESULT = """&& crate::spec::bitmap::prefix_copy_result(
                        old(self), final(self), source, byte_count,
                    )"""
PAD_RESULT = """&& forall|index: int| 0 <= index < crate::views::bitmap_data(&bitmap).len() ==>
                crate::views::bitmap_data(&bitmap)[index]
                    == if index < crate::views::bitmap_data(&decoded).len() {
                        crate::views::bitmap_data(&decoded)[index]
                    } else { 0u8 }"""
FILLED = next(
    entry["body_text"] for entry in source_functions(ROOT)
    if entry["id"] == "bitmap::Bitmap::filled"
)
FILLED_WITHOUT_RESULT_SUPPORT = replace_once(
    FILLED,
    """#[cfg(verus_keep_ghost)]
    production_filled_storage_result!(bitmap, value);""",
    "",
)
FILLED_RESULT = "&& crate::spec::bitmap::filled_storage(&bitmap, value)"
CASES = (
    {
        "id": "skip_row_copy", "file": "src/bitmap.rs", "module": "bitmap",
        "function": "Bitmap::copy_row", "site": ROW_COPY, "replacement": ";",
        "control": ROW_RESULT,
    },
    {
        "id": "corrupt_other_row", "file": "src/bitmap.rs", "module": "bitmap",
        "function": "Bitmap::copy_row", "site": ROW_COPY,
        "replacement": ROW_COPY + "\n        if destination_start != 0 { self.data[0] ^= 1; }",
        "control": ROW_RESULT,
    },
    {
        "id": "skip_prefix_copy", "file": "src/bitmap.rs", "module": "bitmap",
        "function": "Bitmap::copy_prefix_from", "site": PREFIX_COPY, "replacement": ";",
        "control": PREFIX_RESULT,
    },
    {
        "id": "corrupt_uncopied_suffix", "file": "src/bitmap.rs", "module": "bitmap",
        "function": "Bitmap::copy_prefix_from", "site": PREFIX_COPY,
        "replacement": PREFIX_COPY + "\n        if byte_count < self.data.len() { self.data[byte_count] ^= 1; }",
        "control": PREFIX_RESULT,
    },
    {
        "id": "nonzero_blank_bitmap", "file": "src/bitmap.rs", "module": "bitmap",
        "function": "Bitmap::blank", "site": "Self::filled(width, height, false, limits)",
        "replacement": "Self::filled(width, height, true, limits)",
        "control": "&& crate::spec::bitmap::zeroed_storage(&bitmap)",
    },
    {
        "id": "discard_decoded_rows", "file": "src/generic_region.rs", "module": "generic_region",
        "function": "pad_bitmap_height", "site": "padded.copy_prefix_from(&decoded, byte_count)?;",
        "replacement": ";", "control": PAD_RESULT,
    },
    {
        "id": "filled_black_pixel_lost_after_masking", "file": "src/bitmap.rs", "module": "bitmap",
        "function": "Bitmap::filled", "site": FILLED,
        "replacement": replace_once(
            FILLED_WITHOUT_RESULT_SUPPORT, "let bitmap = Self {",
            """if value && !data.is_empty() { data[0] ^= 0x80; }
            let bitmap = Self {""",
        ),
        "control": FILLED_RESULT,
    },
    {
        "id": "filled_padding_reintroduced_after_masking", "file": "src/bitmap.rs", "module": "bitmap",
        "function": "Bitmap::filled", "site": FILLED,
        "replacement": replace_once(
            FILLED_WITHOUT_RESULT_SUPPORT, "let bitmap = Self {",
            """if value && width % 8 != 0 && !data.is_empty() {
                let last = data.len() - 1;
                data[last] |= 1;
            }
            let bitmap = Self {""",
        ),
        "control": FILLED_RESULT,
    },
)


def mutated_copy(source: str, case: dict) -> str:
    return replace_once(source, case["site"], case["replacement"])


def weaker_contract(source: str, case: dict) -> str:
    return weakened_contract(source, case)


def check_bitmap_copy_proofs(executable: Path) -> dict:
    files = (
        "src/bitmap.rs", "src/generic_region.rs", "specs/bitmap.rs",
        "proofs/shared/bitmap.rs", "proofs/production_impl/bitmap.rs",
        "proofs/production_impl/generic_region.rs",
    )
    return check_semantic_cases(
        ROOT, executable, CASES, files, "jbig2-bitmap-proof-",
        "Exact copy/blank/padding and complete filled packed-byte results; filled mutants retain all row/math proof support and remove only the new result-specific proof call so actual postconditions, not a helper precondition, detect corrupted output. Whole rendering/caller and normative review remain separate",
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--verus-executable", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    try:
        result = check_bitmap_copy_proofs(arguments.verus_executable.resolve())
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"bitmap proof sensitivity failed: {error}", file=sys.stderr)
        result = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2))
    return 0 if result["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

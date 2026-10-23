#!/usr/bin/env python3
"""Check executing clipped-composition success and byte-frame sensitivity."""

import argparse
import json
import subprocess
import sys
from pathlib import Path

from proof_sensitivity import check_semantic_cases


ROOT = Path(__file__).resolve().parents[1]
CALL = "let result = self.compose_chunked(source, x, y, operator);"
FLIP = "if let Some(byte) = self.data.get_mut(0) { *byte ^= 1; }"
FRAME = """forall|index: int|
    0 <= index < crate::views::bitmap_data(old(self)).len()
    && !crate::spec::bitmap::compose_byte_may_change(
        index, crate::views::bitmap_stride(old(self)),
        crate::views::bitmap_width(old(self)), crate::views::bitmap_height(old(self)),
        crate::views::bitmap_width(source), crate::views::bitmap_height(source), x, y,
    )
    ==> crate::views::bitmap_data(final(self))[index]
        == crate::views::bitmap_data(old(self))[index],"""
SUCCESS = "ensures out.is_ok(), crate::spec::bitmap::storage_valid(final(self)),"
ZERO_BLIT_RESULT = """width == 0 ==> out.is_ok(),
    width == 0 ==> final(self).data@ == old(self).data@,"""
ZERO_BLIT_GUARD = """if width == 0 {
        return Ok(());
    }
    let mut source_bit = source_x;"""
CASES = tuple(
    {
        "id": identity, "file": "src/bitmap.rs",
        "module": "bitmap", "function": "Bitmap::compose",
        "site": CALL, "replacement": CALL + f"\nif {condition} {{ {FLIP} }}",
        "controls": ((FRAME, ""),),
    }
    for identity, condition in (
        ("offscreen_composition_mutates_storage", "x == u32::MAX"),
        ("zero_width_source_mutates_storage", "source.width == 0"),
        ("zero_height_source_mutates_storage", "source.height == 0"),
        ("clipped_composition_changes_prefix", "x >= 8"),
    )
) + (
    {
        "id": "valid_composition_returns_error", "file": "src/bitmap.rs",
        "module": "bitmap", "function": "Bitmap::compose", "site": CALL,
        "replacement": "let result: Result<(), MalformedError> = Err(MalformedError::IntegerOverflow);",
        "controls": ((SUCCESS, "ensures crate::spec::bitmap::storage_valid(final(self)),"),),
    },
    {
        "id": "zero_width_unaligned_blit_returns_error", "file": "src/bitmap.rs",
        "module": "bitmap", "function": "Bitmap::compose_row_blit_chunked",
        "site": ZERO_BLIT_GUARD, "replacement": "let mut source_bit = source_x;",
        "controls": ((ZERO_BLIT_RESULT, ""),),
    },
    {
        "id": "zero_width_blit_mutates_storage", "file": "src/bitmap.rs",
        "module": "bitmap", "function": "Bitmap::compose_row_blit_chunked",
        "site": ZERO_BLIT_GUARD,
        "replacement": (
            f"if width == 0 {{ {FLIP} return Ok(()); }}\n"
            "    let mut source_bit = source_x;"
        ),
        "controls": ((ZERO_BLIT_RESULT, ""),),
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
            ("src/bitmap.rs", "specs/bitmap.rs", "proofs/shared/bitmap.rs",
             "proofs/production_impl/bitmap.rs"),
            "jbig2-bitmap-compose-",
            "Executing optimized composition and empty row blits: preserved success/storage/metadata and original outside-clipped-byte frame, plus the complete new zero-width success/storage result; general pixel/operator equivalence remains scalar-oracle tested",
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"bitmap composition proof sensitivity failed: {error}", file=sys.stderr)
        result = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2))
    return 0 if result["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

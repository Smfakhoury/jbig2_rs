#!/usr/bin/env python3
"""Check optimized black-span success, requested bits, and complete storage frames."""

import argparse
import json
import subprocess
import sys
from pathlib import Path

from proof_sensitivity import check_semantic_cases


ROOT = Path(__file__).resolve().parents[1]
RESULT = """crate::spec::bitmap::storage_valid(old(self)) ==>
    crate::spec::bitmap::black_span_result(old(self), final(self), row, start, end),"""
SUCCESS = "crate::spec::bitmap::storage_valid(old(self)) ==> out.is_ok(),"
TAIL = """production_black_span_result!(self, valid, row, start, end);
Ok(())
}"""
SITE = TAIL.removesuffix("Ok(())\n}")
CASES = tuple(
    {
        "id": identity, "file": "src/bitmap.rs", "module": "bitmap",
        "function": "Bitmap::fill_black_span", "site": TAIL,
        "replacement": SITE + mutation + "\nOk(())\n}",
        "controls": ((RESULT, ""),),
    }
    for identity, mutation in (
        ("requested_head_pixels_cleared",
         "if let Some(byte) = self.data.get_mut(first_index) { *byte &= !first_mask; }"),
        ("untouched_head_bits_cleared",
         "if let Some(byte) = self.data.get_mut(first_index) { *byte &= first_mask; }"),
        ("outside_span_prefix_changed",
         "if first_index > 0 { if let Some(byte) = self.data.get_mut(0) { *byte ^= 1; } }"),
        ("padding_bit_changed",
         """if x < end && end == self.width && end % 8 != 0 {
             if let Some(byte) = self.data.get_mut(index) { *byte |= 1; }
         }"""),
        ("requested_word_byte_cleared",
         """if end - start >= 72 {
             if let Some(chunk_start) = first_index.checked_add(1) {
                 if let Some(byte) = self.data.get_mut(chunk_start) { *byte = 0; }
             }
         }"""),
    )
) + (
    {
        "id": "valid_nonempty_span_returns_error",
        "file": "src/bitmap.rs", "module": "bitmap", "function": "Bitmap::fill_black_span",
        "site": TAIL, "replacement": SITE + "Err(MalformedError::IntegerOverflow)\n}",
        "controls": ((SUCCESS, ""),),
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
            "jbig2-black-span-",
            "Executing optimized black-span filler: valid-storage success, exact requested "
            "black bits, all other stored bits including dirty padding preserved, "
            "visible-pixel relation and outside-byte frame; complete MMR decoding remains open",
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"black-span proof sensitivity failed: {error}", file=sys.stderr)
        result = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2))
    return 0 if result["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

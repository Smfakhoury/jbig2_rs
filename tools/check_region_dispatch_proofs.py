#!/usr/bin/env python3
"""Check source-offset dispatch no-op contracts, not general pixel composition."""

import argparse
import json
import subprocess
import sys
from pathlib import Path

from proof_sensitivity import check_semantic_cases


ROOT = Path(__file__).resolve().parents[1]
SIGNATURE = """pub(crate) fn compose_region(
    &mut self, source: &Self, source_x: u32, source_y: u32,
    destination_x: u32, destination_y: u32, width: u32, height: u32,
    operator: CombinationOperator,
) -> Result<(), MalformedError> {"""
FLIP = """if let Some(byte) = self.data.get_mut(0) { *byte ^= 1; }
#[cfg(verus_keep_ghost)]
proof! {
    crate::views::bitmap_fields(self);
    crate::views::bitmap_fields(old(self));
}"""
NO_OP = """(!crate::spec::bitmap::region_request_valid(
    old(self), source, source_x, source_y, destination_x, destination_y, width, height,
) || width == 0 || height == 0) ==> final(self).data@ == old(self).data@,"""
CALL = "let result = self .compose_region_chunked("
CASES = (
    {
        "id": "overflowing_region_mutates_storage", "file": "src/bitmap.rs",
        "module": "bitmap", "function": "Bitmap::compose_region",
        "site": SIGNATURE,
        "replacement": SIGNATURE + f"\nif source_x == u32::MAX && width > 0 {{ {FLIP} }}",
        "controls": ((NO_OP, ""),),
    },
    {
        "id": "zero_width_region_mutates_storage", "file": "src/bitmap.rs",
        "module": "bitmap", "function": "Bitmap::compose_region",
        "site": CALL,
        "replacement": f"if width == 0 {{ {FLIP} return Ok(()); }}\n{CALL}",
        "controls": ((NO_OP, ""),),
    },
    {
        "id": "zero_height_region_mutates_storage", "file": "src/bitmap.rs",
        "module": "bitmap", "function": "Bitmap::compose_region",
        "site": CALL,
        "replacement": f"if height == 0 {{ {FLIP} return Ok(()); }}\n{CALL}",
        "controls": ((NO_OP, ""),),
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
            ("src/bitmap.rs", "specs/bitmap.rs"), "jbig2-region-dispatch-",
            "Exact source-offset dispatch: storage/frame, typed invalid requests and valid zero-area no-op properties; general composed pixels remain scalar-oracle tested, not formally derived",
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"region dispatch proof sensitivity failed: {error}", file=sys.stderr)
        result = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2))
    return 0 if result["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

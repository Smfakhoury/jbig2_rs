#!/usr/bin/env python3
"""Check exact chunk values and byte frames, not complete bitmap composition."""

import argparse
import json
import subprocess
import sys
from pathlib import Path

from proof_sensitivity import check_semantic_cases


ROOT = Path(__file__).resolve().parents[1]
READ_VALUE = "out.is_some() ==> out == Some(crate::spec::bitmap::chunk_value(bytes@, start as int)),"
WRITE_VALUE = """out ==> forall|offset: int| 0 <= offset < 8 ==>
    final(bytes)@[start as int + offset]
        == crate::spec::bitmap::chunk_byte(value, offset),"""
FRAME = """forall|index: int| 0 <= index < old(bytes)@.len()
    && (!out || index < start as int || start as int + 8 <= index)
    ==> final(bytes)@[index] == old(bytes)@[index],"""
SIGNATURE = "fn write_chunk(bytes: &mut Vec<u8>, start: usize, value: u64) -> bool {"
FLIP = "if let Some(byte) = bytes.get_mut(0) { *byte ^= 1; }"
CASES = (
    {
        "id": "read_high_byte_uses_wrong_shift", "file": "src/bitmap.rs",
        "module": "bitmap", "function": "read_chunk",
        "site": "u64::from(array[0]) << 56",
        "replacement": "u64::from(array[0]) << 48",
        "controls": ((READ_VALUE, ""),),
    },
    {
        "id": "write_high_byte_uses_wrong_shift", "file": "src/bitmap.rs",
        "module": "bitmap", "function": "write_chunk",
        "site": "(value >> 56) as u8",
        "replacement": "(value >> 48) as u8",
        "controls": ((WRITE_VALUE, ""),),
    },
    {
        "id": "successful_chunk_write_changes_prefix", "file": "src/bitmap.rs",
        "module": "bitmap", "function": "write_chunk",
        "site": SIGNATURE,
        "replacement": SIGNATURE + f"""
if bytes.len() >= 8 && start > 0 && start <= bytes.len() - 8 {{ {FLIP} }}
""",
        "controls": ((FRAME, ""),),
    },
    {
        "id": "overflowing_chunk_write_changes_storage", "file": "src/bitmap.rs",
        "module": "bitmap", "function": "write_chunk",
        "site": SIGNATURE,
        "replacement": SIGNATURE + f"\nif start == usize::MAX {{ {FLIP} }}",
        "controls": ((FRAME, ""),),
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
            ("src/bitmap.rs", "specs/bitmap.rs"), "jbig2-bitmap-chunks-",
            "Packed-byte kernels: exact big-endian values, complete range acceptance, failed-write and outside-span frame; complete composition remains unproved",
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"bitmap chunk proof sensitivity failed: {error}", file=sys.stderr)
        result = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2))
    return 0 if result["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

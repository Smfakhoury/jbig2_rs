#!/usr/bin/env python3
"""Check exact MMR span no-op contracts, not black-pixel functional correctness."""

import argparse
import json
import subprocess
import sys
from pathlib import Path

from proof_sensitivity import check_semantic_cases


ROOT = Path(__file__).resolve().parents[1]
NO_OP = """(start > end || end > old(bitmap).width || row >= old(bitmap).height
    || !black || start == end)
    ==> final(bitmap).data@ == old(bitmap).data@,"""
FLIP = """if let Some(byte) = bitmap.data.get_mut(0) { *byte ^= 1; }"""
CALL = """bitmap .fill_black_span(row, start, end)
    .map_err(|_error| MmrError::InvalidTransition)?;"""
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
        "controls": ((NO_OP, ""),),
    },
    {
        "id": "empty_black_span_mutates_storage", "file": "src/mmr.rs",
        "module": "mmr", "function": "fill_span",
        "site": CALL,
        "replacement": f"if start == end {{ {FLIP} return Ok(()); }}\n{CALL}",
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
            ("src/mmr.rs", "src/bitmap.rs"), "jbig2-mmr-span-proof-",
            "Executing optimized span dispatch: storage/metadata preservation, typed invalid bounds and invalid/white/empty no-op guarantees only; separate primitive black-span theorem is not a complete MMR caller/polarity proof",
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

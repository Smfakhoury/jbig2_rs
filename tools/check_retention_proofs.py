#!/usr/bin/env python3
"""Check retention wire-order, padding and materialization semantic sensitivity."""

import argparse
import json
import subprocess
import sys
from pathlib import Path

from proof_sensitivity import check_semantic_cases


ROOT = Path(__file__).resolve().parents[1]
CASES = (
    {
        "id": "reverse_wire_flag_bytes", "file": "src/segment_header.rs",
        "module": "segment_header", "function": "read_retention_flag",
        "site": "let byte = *bytes .get(byte_index) .ok_or(MalformedError::IntegerOverflow)?;",
        "replacement": """let byte = *bytes.get(byte_index).ok_or(MalformedError::IntegerOverflow)?;
        let byte = if bytes.len() > 1 { bytes[bytes.len() - 1 - byte_index] } else { byte };""",
        "control": "&& value == crate::spec::segments::retention_bit(bytes@, index as nat)",
    },
    {
        "id": "validate_first_not_last_flag_byte", "file": "src/segment_header.rs",
        "module": "segment_header", "function": "validate_unused_retention_bits",
        "site": "let last = bytes.last().ok_or(MalformedError::IntegerOverflow)?;",
        "replacement": "let last = bytes.first().ok_or(MalformedError::IntegerOverflow)?;",
        "controls": ((
            """match out {
        Ok(()) => crate::spec::segments::unused_retention_bits_zero(bit_count, bytes@),
        Err(_) => !crate::spec::segments::unused_retention_bits_zero(bit_count, bytes@),
    }""", "true",
        ),),
    },
    {
        "id": "invert_materialized_retention_flags", "file": "src/segment_header.rs",
        "module": "segment_header", "function": "fill_retention_values",
        "site": "values.push(read_retention_flag(bytes, index)?);",
        "replacement": "let value = read_retention_flag(bytes, index)?;\n        values.push(value ^ 1);",
        "controls": (
            ("out.is_ok() ==> crate::spec::segments::retention_values_match(final(values)@, bytes@),", ""),
            ("crate::spec::segments::retention_values_match(values@, bytes@),", ""),
        ),
    },
)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--verus-executable", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    try:
        result = check_semantic_cases(
            ROOT, arguments.verus_executable.resolve(), CASES,
            ("src/segment_header.rs", "specs/segments.rs"),
            "jbig2-retention-proof-",
            "Selected T.88 7.2.4 field-rule helpers only; worked-example conflict and normative review remain open",
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"retention proof sensitivity failed: {error}", file=sys.stderr)
        result = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2))
    return 0 if result["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

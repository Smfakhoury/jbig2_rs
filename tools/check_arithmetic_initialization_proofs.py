#!/usr/bin/env python3
"""Check exact initializer acceptance and composition with the project refill step."""

import argparse
import json
from pathlib import Path
import subprocess
import sys

from proof_sensitivity import check_semantic_cases


ROOT = Path(__file__).resolve().parents[1]
ACCEPTANCE = "out.is_ok() == (bytes@.len() >= 2),"
FIELDS = {
    "interval": "&& decoder.interval == MIN_INTERVAL",
    "position": "&& decoder.byte_position == arithmetic_spec::initialized_state(bytes).unwrap().0",
    "current": "&& decoder.current_byte == arithmetic_spec::initialized_state(bytes).unwrap().1",
    "code": "&& decoder.code == arithmetic_spec::initialized_state(bytes).unwrap().2",
    "bits": "&& decoder.bit_count == arithmetic_spec::initialized_state(bytes).unwrap().3",
    "marker": "&& decoder.marker == arithmetic_spec::initialized_state(bytes).unwrap().4",
}
SIGNATURE = "pub(crate) fn new(bytes: &'a [u8]) -> Result<Self, ArithmeticError> {"
RETURN = "Ok(decoder)\n    }"


def case(identity, site, replacement, *controls):
    return {
        "id": identity, "file": "src/arithmetic.rs", "module": "arithmetic",
        "function": "ArithmeticDecoder::new", "site": site,
        "replacement": replacement, "controls": controls,
    }


CASES = (
    case(
        "initializer_rejects_every_valid_prefix",
        SIGNATURE,
        SIGNATURE + " return Err(ArithmeticError::Malformed(MalformedError::IntegerOverflow));",
        (ACCEPTANCE, ""),
    ),
    case(
        "initializer_scales_code_by_six_bits",
        ".checked_mul(1 << 7)", ".checked_mul(64)",
        (FIELDS["code"], ""),
    ),
    case(
        "initializer_subtracts_six_available_bits",
        ".checked_sub(7)", ".checked_sub(6)",
        (FIELDS["bits"], ""),
    ),
    case(
        "initializer_discards_detected_marker",
        RETURN, "decoder.marker = None;\n        " + RETURN,
        (FIELDS["marker"], ""),
    ),
    case(
        "initializer_rewinds_consumed_prefix",
        RETURN,
        "decoder.byte_position = 0;\n        decoder.current_byte = first;\n        " + RETURN,
        (FIELDS["position"], ""), (FIELDS["current"], ""),
    ),
    case(
        "initializer_returns_wrong_interval",
        RETURN, "decoder.interval = MIN_INTERVAL + 1;\n        " + RETURN,
        (FIELDS["interval"], ""),
    ),
)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--verus-executable", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    try:
        report = check_semantic_cases(
            ROOT, arguments.verus_executable.resolve(), CASES,
            ("src/arithmetic.rs", "specs/arithmetic.rs"),
            "jbig2-arithmetic-initialization-",
            "Exact project two-byte acceptance and initialized input/code/count/marker/interval state composed with refill; normative translation and later stream decoding remain separate",
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"arithmetic initialization proof sensitivity failed: {error}", file=sys.stderr)
        report = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

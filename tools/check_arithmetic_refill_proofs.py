#!/usr/bin/env python3
"""Check exact project refill outputs, including partial error-path consumption."""

import argparse
import json
from pathlib import Path
import subprocess
import sys

from proof_sensitivity import check_semantic_cases


ROOT = Path(__file__).resolve().parents[1]
FIELDS = {
    "position": "final(self).byte_position == arithmetic_spec::refill_step(old(self)).1,",
    "current": "final(self).current_byte == arithmetic_spec::refill_step(old(self)).2,",
    "code": "final(self).code == arithmetic_spec::refill_step(old(self)).3,",
    "bits": "final(self).bit_count == arithmetic_spec::refill_step(old(self)).4,",
    "marker": "final(self).marker == arithmetic_spec::refill_step(old(self)).5,",
}
SIGNAL_STATUS = "out == arithmetic_spec::refill_step(old(self)).0,"
CHECKED_STATUS = "out.is_ok() == (arithmetic_spec::refill_step(old(self)).0 == 0),"
SIGNAL_POST = "\n".join((SIGNAL_STATUS, *FIELDS.values()))
CHECKED_POST = "\n".join((CHECKED_STATUS, *FIELDS.values()))
MARKER_ASSIGNMENT = """self.marker = Some(ArithmeticMarker {
    offset: self.byte_position,
    code: next_byte,
});"""
MARKER_SITE = f"""self.code = code;
self.bit_count = 8;
{MARKER_ASSIGNMENT}
return 0;"""
OVERFLOW_SITE = """let Some(code) = self.code.checked_add(u32::from(next_byte) << 8) else {
    return RASTER_SIGNAL_OVERFLOW;
};"""
CHECKED_SITE = """let signal = self.byte_in_signal();
if signal == 0 {
    Ok(())"""


def signal_controls(*removed):
    clauses = {"signal": SIGNAL_STATUS, **FIELDS}
    return ((SIGNAL_POST, "\n".join(
        value for name, value in clauses.items() if name not in removed
    )),)


def signal_case(identity, site, replacement, removed):
    return {
        "id": identity, "file": "src/arithmetic.rs", "module": "arithmetic",
        "function": "ArithmeticDecoder::byte_in_signal",
        "site": site, "replacement": replacement,
        "controls": signal_controls(*removed),
    }


CASES = (
    signal_case(
        "stuffed_refill_returns_wrong_signal",
        "self.bit_count = 7;\n            return 0;",
        "self.bit_count = 7;\n            return RASTER_SIGNAL_EOF;",
        ("signal",),
    ),
    signal_case(
        "marker_refill_loses_code_increment",
        MARKER_SITE, MARKER_SITE.replace("self.code = code;", "let _ = code;"),
        ("code",),
    ),
    signal_case(
        "marker_refill_uses_wrong_bit_count",
        MARKER_SITE, MARKER_SITE.replace("self.bit_count = 8;", "self.bit_count = 7;"),
        ("bits",),
    ),
    signal_case(
        "marker_refill_loses_marker",
        MARKER_SITE, MARKER_SITE.replace(MARKER_ASSIGNMENT, "self.marker = None;"),
        ("marker",),
    ),
    signal_case(
        "overflow_refill_rolls_back_consumption",
        OVERFLOW_SITE,
        """let Some(code) = self.code.checked_add(u32::from(next_byte) << 8) else {
    self.byte_position -= 1;
    self.current_byte = self.bytes[self.byte_position];
    return RASTER_SIGNAL_OVERFLOW;
};""",
        ("position", "current"),
    ),
    {
        "id": "checked_refill_rejects_success",
        "file": "src/arithmetic.rs", "module": "arithmetic",
        "function": "ArithmeticDecoder::byte_in",
        "site": CHECKED_SITE,
        "replacement": CHECKED_SITE.replace(
            "Ok(())", "Err(ArithmeticError::Malformed(MalformedError::IntegerOverflow))",
        ),
        "controls": ((CHECKED_POST, "\n".join(FIELDS.values())),),
    },
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
            "jbig2-arithmetic-refill-",
            "Exact project byte-input signal, consumption, code, counter and marker outputs; initialization/renormalization composition and normative equivalence remain separate",
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"arithmetic refill proof sensitivity failed: {error}", file=sys.stderr)
        report = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

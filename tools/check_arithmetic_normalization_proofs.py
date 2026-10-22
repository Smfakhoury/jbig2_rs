#!/usr/bin/env python3
"""Check exact normalization outputs against compiling relation-specific controls."""

import argparse
import json
from pathlib import Path
import subprocess
import sys

from proof_sensitivity import check_semantic_cases


ROOT = Path(__file__).resolve().parents[1]
STATUS = "out == arithmetic_spec::normalized_state(old(self)).0,"
FIELDS = {
    "interval": "final(self).interval == arithmetic_spec::normalized_state(old(self)).1,",
    "position": "final(self).byte_position == arithmetic_spec::normalized_state(old(self)).2,",
    "current": "final(self).current_byte == arithmetic_spec::normalized_state(old(self)).3,",
    "code": "final(self).code == arithmetic_spec::normalized_state(old(self)).4,",
    "bits": "final(self).bit_count == arithmetic_spec::normalized_state(old(self)).5,",
    "marker": "final(self).marker == arithmetic_spec::normalized_state(old(self)).6,",
}
POST = "\n".join((STATUS, *FIELDS.values()))
RETURN = "        }\n        0\n    }\n\n    #[cfg_attr"


def case(identity, change, *removed):
    clauses = {"signal": STATUS, **FIELDS}
    return {
        "id": identity, "file": "src/arithmetic.rs", "module": "arithmetic",
        "function": "ArithmeticDecoder::renormalize_signal",
        "site": RETURN,
        "replacement": RETURN.replace("        0\n", change + "\n        0\n"),
        "controls": ((POST, "\n".join(
            value for name, value in clauses.items() if name not in removed
        )),),
    }


CASES = (
    case("normalization_returns_wrong_interval", "        self.interval = MIN_INTERVAL + 1;", "interval"),
    case("normalization_discards_code", "        self.code = 0;", "code"),
    case("normalization_discards_bit_count", "        self.bit_count = 0;", "bits"),
    case("normalization_discards_marker", "        self.marker = None;", "marker"),
    case(
        "normalization_rewinds_consumed_history",
        "        self.byte_position = 0;\n        self.current_byte = self.bytes[0];\n        self.marker = None;",
        "position", "current", "marker",
    ),
    {
        **case("normalization_rejects_success", "", "signal"),
        "replacement": RETURN.replace("        0\n", "        RASTER_SIGNAL_EOF\n"),
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
            "jbig2-arithmetic-normalization-",
            "Exact project normalization signal/interval/input-position/current-byte/code/count/marker, including partial errors; normative register interpretation and caller composition remain separate",
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"arithmetic normalization proof sensitivity failed: {error}", file=sys.stderr)
        report = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

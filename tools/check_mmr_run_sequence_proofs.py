#!/usr/bin/env python3
"""Check input-bound MMR make-up/terminal sequence and exact run sums."""

import argparse
import json
from pathlib import Path
import subprocess
import sys

from proof_sensitivity import check_semantic_cases


ROOT = Path(__file__).resolve().parents[1]
RELATION = """&& exists|codes: Seq<RunCode>| mmr_spec::complete_run(
    black, old(reader).bytes@, old(reader).bit_position as nat,
    final(reader).bit_position as nat, total, codes,
)"""
COMPLETE_ASSERT = """assert(mmr_spec::complete_run(
    black, old(reader).bytes@, old(reader).bit_position as nat,
    reader.bit_position as nat, total, codes,
));"""
RETURN_SITE = """if code.terminal {
    #[cfg(verus_keep_ghost)]
    proof! {
""" + COMPLETE_ASSERT + """
    }
    return Ok(total);
}"""
HEAD = """fn decode_run(reader: &mut BitReader<'_>, black: bool, remaining: u32) -> Result<u32, MmrError> {"""


def case(identity, site, replacement, extra=()):
    return {
        "id": identity, "file": "src/mmr.rs", "module": "mmr",
        "function": "decode_run", "site": site, "replacement": replacement,
        "controls": ((RELATION, ""), *extra),
    }


CASES = (
    case("mmr_run_discards_total", RETURN_SITE, RETURN_SITE.replace("return Ok(total)", "return Ok(0)")),
    case(
        "mmr_run_discards_makeup_values", RETURN_SITE,
        RETURN_SITE.replace("return Ok(total)", "return Ok(u32::from(code.value))"),
    ),
    case(
        "mmr_run_accepts_makeup_as_terminating", RETURN_SITE,
        RETURN_SITE.replace("if code.terminal", "if true").replace(COMPLETE_ASSERT, ""),
    ),
    case(
        "mmr_run_accepts_empty_zero_without_terminating_code", HEAD,
        HEAD + "\nif remaining == 0 { return Ok(0); }",
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
            ("src/mmr.rs", "specs/mmr.rs", "specs/readers.rs"),
            "jbig2-mmr-run-sequence-",
            "Successful project MMR run equals an input-bound make-up/terminal sequence sum with exact consumed extent; independent table fidelity, complete acceptance/errors and raster remain separate",
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"MMR run-sequence proof sensitivity failed: {error}", file=sys.stderr)
        report = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

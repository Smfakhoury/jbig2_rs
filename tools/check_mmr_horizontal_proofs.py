#!/usr/bin/env python3
"""Check exact opposite-colour horizontal MMR run composition."""

import argparse
import json
from pathlib import Path
import subprocess
import sys

from proof_sensitivity import check_semantic_cases, replace_once
from source_group import runtime_function


ROOT = Path(__file__).resolve().parents[1]
BODY = runtime_function(ROOT, "mmr", "decode_horizontal_runs")["body_text"]
RELATION = """&& mmr_spec::horizontal_pair(
    black, old(reader).bytes@, old(reader).bit_position as nat,
    final(reader).bit_position as nat, x, width, after_first, end,
)"""
PROOF_START = "    #[cfg(verus_keep_ghost)]\n    proof! {"
if BODY.count(PROOF_START) != 1:
    raise ValueError("horizontal result proof support must have one exact site")
SUPPORT = BODY[BODY.index(PROOF_START):BODY.rindex("\n    Ok((after_first, end))")]
WITHOUT_SUPPORT = replace_once(BODY, SUPPORT, "")
SECOND_STEP = """let second = decode_run(reader, !black, second_remaining)?;
    #[cfg(verus_keep_ghost)]
    proof_decl! {
        let ghost second_codes = choose|codes: Seq<RunCode>|
            #[trigger] codes.len() > 0
            && mmr_spec::complete_run(
                !black, old(reader).bytes@, split, reader.bit_position as nat,
                second, codes,
            );
    }"""


def case(identity, replacement, extra=()):
    return {
        "id": identity, "file": "src/mmr.rs", "module": "mmr",
        "function": "decode_horizontal_runs", "site": BODY, "replacement": replacement,
        "controls": ((RELATION, ""), *extra),
    }


CASES = (
    case(
        "horizontal_loses_first_endpoint",
        replace_once(WITHOUT_SUPPORT, "Ok((after_first, end))", "Ok((x, end))"),
    ),
    case(
        "horizontal_shortens_second_endpoint",
        replace_once(
            WITHOUT_SUPPORT, "Ok((after_first, end))",
            """if end - x > 1 && end > after_first {
                Ok((after_first, end - 1))
            } else {
                Ok((after_first, end))
            }""",
        ),
    ),
    case(
        "horizontal_uses_opposite_first_colour",
        replace_once(
            replace_once(
                WITHOUT_SUPPORT,
                "let first = decode_run(reader, black, remaining)?;",
                "let first = decode_run(reader, !black, remaining)?;",
            ),
            "black, old(reader).bytes@, old(reader).bit_position as nat,",
            "!black, old(reader).bytes@, old(reader).bit_position as nat,",
        ),
    ),
    case(
        "horizontal_reuses_first_colour_for_second_run",
        replace_once(
            replace_once(
                WITHOUT_SUPPORT,
                "let second = decode_run(reader, !black, second_remaining)?;",
                "let second = decode_run(reader, black, second_remaining)?;",
            ),
            "!black, old(reader).bytes@, split, reader.bit_position as nat,",
            "black, old(reader).bytes@, split, reader.bit_position as nat,",
        ),
    ),
    case(
        "horizontal_discards_second_run",
        replace_once(WITHOUT_SUPPORT, SECOND_STEP, "let second = 0_u32;"),
    ),
    case(
        "horizontal_accepts_zero_pair",
        replace_once(
            replace_once(
                WITHOUT_SUPPORT,
                """#[cfg(verus_keep_ghost)]
                production_mmr_horizontal_progress!(x, width, first, second);""", "",
            ),
            "if end == x", "if false",
        ),
        (("&& x < end", ""),),
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
            ("src/mmr.rs", "specs/mmr.rs", "specs/readers.rs",
             "proofs/production_impl/mmr.rs"),
            "jbig2-mmr-horizontal-",
            "Exact project opposite-colour input-bound horizontal run pairs and endpoints; independent table fidelity, horizontal mode dispatch and full raster/caller semantics remain separate",
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"MMR horizontal proof sensitivity failed: {error}", file=sys.stderr)
        report = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

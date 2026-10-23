#!/usr/bin/env python3
"""Check first-match group and complete collected/codebook cardinality contracts."""

import argparse
import json
from pathlib import Path
import subprocess
import sys

from proof_sensitivity import check_semantic_cases


ROOT = Path(__file__).resolve().parents[1]
CODEBOOK_COUNT = """crate::symbol_text::symbol_text_spec::codebook_len(out)
    == decode_spec::collected_symbol_count(self),"""
GROUP_COUNT = """&& resource.exported@.len()
    == decode_spec::symbol_group_count(symbols@, number)"""
COLLECTED_COUNT = """&& decode_spec::collected_symbol_count(&bitmaps)
    == decode_spec::referred_symbol_count(
        symbols@, segment.header.referred_segments@,
        segment.header.referred_segments@.len(),
    )"""
IMPORT_OUTER = """imported@.len() == decode_spec::referred_symbol_count(
    symbols@, segment.header.referred_segments@, reference_index as nat,
),"""
IMPORT_INNER = """imported@.len() == decode_spec::referred_symbol_count(
    symbols@, segment.header.referred_segments@, reference_index as nat,
) + bitmap_index as nat,"""


def case(identity, function, site, replacement, controls):
    return {
        "id": identity, "file": "src/decode.rs", "module": "decode",
        "function": function, "site": site, "replacement": replacement,
        "controls": controls,
    }


CASES = (
    case(
        "contiguous_codebook_loses_count", "CollectedSymbols::codebook",
        "Self::Contiguous(symbols) => SymbolCodebook::Contiguous(symbols),",
        "Self::Contiguous(symbols) => SymbolCodebook::Contiguous(&symbols[..0]),",
        ((CODEBOOK_COUNT, ""),),
    ),
    case(
        "borrowed_codebook_loses_count", "CollectedSymbols::codebook",
        "Self::Borrowed(symbols) => SymbolCodebook::Borrowed(symbols),",
        "Self::Borrowed(symbols) => SymbolCodebook::Borrowed(&symbols[..0]),",
        ((CODEBOOK_COUNT, ""),),
    ),
    case(
        "symbol_index_skips_first_duplicate", "symbol_index",
        "if symbols[index].0 == number { return Some(index); }",
        """if symbols[index].0 == number {
            if index + 1 < symbols.len() && symbols[index + 1].0 == number {
                return Some(index + 1);
            }
            return Some(index);
        }""",
        (("&& decode_spec::first_symbol_index(symbols@, number, index as int)", ""),),
    ),
    case(
        "resource_projection_uses_wrong_group_count", "get_symbol_resource",
        "Some(&symbols[index].1)", "Some(&symbols[0].1)",
        ((GROUP_COUNT, ""),),
    ),
    case(
        "borrowed_collection_drops_later_group_bitmaps", "collect_referred_symbols",
        "imported.push(&resource.exported[bitmap_index]);",
        "if bitmap_index == 0 { imported.push(&resource.exported[bitmap_index]); }",
        ((COLLECTED_COUNT, ""), (IMPORT_OUTER, ""), (IMPORT_INNER, "")),
    ),
    case(
        "contiguous_collection_discards_nonempty_group", "collect_referred_symbols",
        """let direct = match contiguous {
            Some(bitmaps) => bitmaps,
            None => &[],
        };""",
        "let direct: &[Bitmap] = &[];",
        ((COLLECTED_COUNT, ""),),
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
            ("src/decode.rs", "specs/decode.rs", "specs/symbol_text.rs"),
            "jbig2-symbol-cardinality-",
            "Exact project first-match group counts, complete referenced-group cardinality and codebook projection; no formal ordering, observed-capacity enforcement or normative/caller-stream claim",
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"Symbol cardinality proof sensitivity failed: {error}", file=sys.stderr)
        report = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

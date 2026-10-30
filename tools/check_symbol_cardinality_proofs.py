#!/usr/bin/env python3
"""Check exact collected cardinality, symbol content selection and export order."""

import argparse
import json
from pathlib import Path
import subprocess
import sys

from proof_sensitivity import check_semantic_cases, replace_once
from source_group import runtime_function


ROOT = Path(__file__).resolve().parents[1]
BASELINE_GROUPS = (
    ("decode", (
        "CollectedSymbols::codebook", "collect_referred_symbols",
        "get_symbol_resource", "symbol_index",
    )),
    ("symbol_text", (
        "SymbolCodebook::get", "assemble_exported_symbols", "symbol_by_id",
    )),
)
GET = runtime_function(ROOT, "symbol_text", "SymbolCodebook::get")["body_text"]
LOOKUP = runtime_function(ROOT, "symbol_text", "symbol_by_id")["body_text"]
ASSEMBLE = runtime_function(ROOT, "symbol_text::dictionary", "assemble_exported_symbols")["body_text"]
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
GET_CONTENT = "&& symbol_text_spec::codebook_selects(self, index as nat, bitmap)"
LOOKUP_CONTENT = """&& symbol_text_spec::symbol_id_selects(
    imported, new_symbols@, id, bitmap
)"""
EXPORT_CONTENT = """&& symbol_text_spec::exported_contents_match(
    imported, new_symbols@, exported_indices@, exported@,
)"""
EXPORT_PREFIX = """forall|offset: int| 0 <= offset < index ==>
    #[trigger] symbol_text_spec::symbol_id_selects(
        imported, new_symbols@, exported_indices@[offset], &exported_symbols@[offset],
    ),"""
LOOKUP_ACCEPTS = """(symbol_text_spec::codebook_len(imported) + new_symbols@.len() <= usize::MAX
    && symbol_text_spec::codebook_len(imported) + new_symbols@.len() <= u32::MAX
    && (id as nat) < symbol_text_spec::codebook_len(imported) + new_symbols@.len())
    ==> out.is_ok(),"""


def case(identity, function, site, replacement, controls, *,
         file="src/decode.rs", module="decode"):
    return {
        "id": identity, "file": file, "module": module,
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
    case(
        "contiguous_lookup_returns_first_instead_of_requested_bitmap",
        "SymbolCodebook::get", GET,
        replace_once(
            GET,
            "Self::Contiguous(symbols) => symbols.get(index),",
            """Self::Contiguous(symbols) =>
                symbols.get(if index < symbols.len() { 0 } else { index }),""",
        ),
        ((GET_CONTENT, ""),),
        file="src/symbol_text.rs", module="symbol_text",
    ),
    case(
        "borrowed_lookup_returns_first_instead_of_requested_bitmap",
        "SymbolCodebook::get", GET,
        replace_once(
            GET,
            "Self::Borrowed(symbols) => match symbols.get(index) {",
            """Self::Borrowed(symbols) => match symbols.get(
                if index < symbols.len() { 0 } else { index }
            ) {""",
        ),
        ((GET_CONTENT, ""),),
        file="src/symbol_text.rs", module="symbol_text",
    ),
    case(
        "new_symbol_lookup_returns_first_instead_of_selected_bitmap",
        "symbol_by_id", LOOKUP,
        replace_once(LOOKUP, "Ok(&new_symbols[new_index])", "Ok(&new_symbols[0])"),
        ((LOOKUP_CONTENT, ""),),
        file="src/symbol_text.rs", module="symbol_text",
    ),
    case(
        "export_assembly_repeats_first_selection_instead_of_requested_order",
        "assemble_exported_symbols", ASSEMBLE,
        replace_once(
            ASSEMBLE, "let symbol_id = exported_indices[index];",
            "let symbol_id = exported_indices[0];",
        ),
        ((EXPORT_CONTENT, ""), (EXPORT_PREFIX, "")),
        file="src/symbol_text.rs", module="symbol_text",
    ),
    case(
        "symbol_lookup_falsely_rejects_valid_representable_id",
        "symbol_by_id", LOOKUP,
        replace_once(
            LOOKUP, "if id_usize < imported.len() {",
            """if id_usize < available {
                return Err(SymbolTextError::InvalidState("mutant false valid lookup rejection"));
            }
            if id_usize < imported.len() {""",
        ),
        ((LOOKUP_ACCEPTS, ""),),
        file="src/symbol_text.rs", module="symbol_text",
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
            ("src/decode.rs", "specs/decode.rs", "src/symbol_text.rs",
             "specs/symbol_text.rs", "src/bitmap.rs", "specs/bitmap.rs"),
            "jbig2-symbol-cardinality-",
            "Exact project group/cardinality plus contiguous/borrowed bitmap content selection, imported/new ID mapping and all ordered/repeated owned export bytes/dimensions. Original shape controls remain; no encoded export-run selection, observed-capacity, independent normative or caller-stream claim",
            baseline_groups=BASELINE_GROUPS,
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

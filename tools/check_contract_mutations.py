#!/usr/bin/env python3
"""Check that critical API and diagnostic regressions detect isolated mutations."""

import argparse
import json
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from check_distribution import isolated_environment, run


ROOT = Path(__file__).resolve().parents[1]
MUTATIONS = (
    {
        "id": "generic-probability-state-validation-skipped",
        "file": "src/generic_region.rs",
        "before": "if let Err(error) = ArithmeticContext::from_state(context.state_index(), context.mps()) {",
        "after": "if let Err(error) = ArithmeticContext::from_state(0, 0) {",
        "target": "lib",
        "test": "generic_region::tests::probability_state_validation_matches_all_raw_fields_and_first_error",
    },
    {
        "id": "generic-packed-eight-bit-window-rejected",
        "file": "src/generic_region.rs",
        "before": "if count == 0 || count > 8 || row < 0 || row >= i64::from(bitmap.height) {",
        "after": "if count == 0 || count > 7 || row < 0 || row >= i64::from(bitmap.height) {",
        "target": "lib",
        "test": "generic_region::tests::packed_rows_match_scalar_msb_windows_padding_and_numeric_edges",
    },
    {
        "id": "generic-probability-context-error-fields-discarded",
        "file": "src/generic_region.rs",
        "before": (
            "if signal == RASTER_SIGNAL_CONTEXT {\n"
            "        GenericRegionError::ContextIndex {\n"
            "            index,\n"
            "            count: context_count,\n"
            "        }"
        ),
        "after": (
            "if signal == RASTER_SIGNAL_CONTEXT {\n"
            "        GenericRegionError::ContextIndex {\n"
            "            index: 0,\n"
            "            count: 0,\n"
            "        }"
        ),
        "target": "lib",
        "test": "generic_region::tests::probability_failure_projection_preserves_context_and_decoder_errors",
    },
    {
        "id": "mmr-reference-cursor-update-discarded",
        "file": "src/mmr.rs",
        "before": "*search_start = index;",
        "after": "*search_start = 0;",
        "target": "lib",
        "test": "mmr::tests::reference_search_matches_scalar_suffix_and_cursor_for_all_small_sorted_vectors",
    },
    {
        "id": "borrowed-symbol-peak-limit-off-by-one",
        "file": "src/decode.rs",
        "before": (
            "let (requested_bytes, peak) = borrowed_symbol_memory(count, active_retained_bytes)?;\n"
            "    if peak > limits.max_decoder_managed_memory_bytes {"
        ),
        "after": (
            "let (requested_bytes, peak) = borrowed_symbol_memory(count, active_retained_bytes)?;\n"
            "    if peak >= limits.max_decoder_managed_memory_bytes {"
        ),
        "target": "lib",
        "test": "decode::tests::referred_symbol_collection_preserves_order_identity_and_exact_borrow_budget",
    },
    {
        "id": "borrowed-symbol-group-loses-later-bitmaps",
        "file": "src/decode.rs",
        "before": "imported.push(&resource.exported[bitmap_index]);",
        "after": "if bitmap_index == 0 { imported.push(&resource.exported[bitmap_index]); }",
        "target": "lib",
        "test": "decode::tests::referred_symbol_collection_preserves_order_identity_and_exact_borrow_budget",
    },
    {
        "id": "total-reference-limit-off-by-one",
        "file": "src/document.rs",
        "before": "if total_references > limits.max_total_references {",
        "after": "if total_references >= limits.max_total_references {",
        "target": "lib",
        "test": "document::tests::reference_graph_retention_and_budget_precedence_match_scalar_policy",
    },
    {
        "id": "halftone-gray-state-trailing-bytes-ignored",
        "file": "src/halftone.rs",
        "before": "if *offset != coded.len() {",
        "after": "if *offset > coded.len() {",
        "target": "lib",
        "test": "halftone::tests::gray_mmr_consumes_exact_prefixes_and_requires_all_declared_bytes",
    },
    {
        "id": "pattern-mmr-single-use-check-disabled",
        "file": "src/halftone.rs",
        "before": "if *used {",
        "after": "if *used && 0_u8 == 1_u8 {",
        "target": "lib",
        "test": "halftone::tests::pattern_mmr_is_single_use_and_mode_errors_do_not_consume_it",
    },
    {
        "id": "symbol-code-length-power-boundary-misclassified",
        "file": "src/symbol_text.rs",
        "before": "else if symbol_count <= 65_536 {",
        "after": "else if symbol_count <= 65_535 {",
        "target": "lib",
        "test": "symbol_text::tests::symbol_code_length_is_minimal_at_every_u32_power_boundary",
    },
    {
        "id": "packed-symbol-padding-check-weakened",
        "file": "src/symbol_text.rs",
        "before": "if bytes[final_byte] & padding_mask != 0 {",
        "after": "if bytes[final_byte] & padding_mask > 0x80 {",
        "target": "lib",
        "test": "symbol_text::tests::packed_symbol_storage_matches_pixels_and_rejects_each_dirty_row",
    },
    {
        "id": "generic-formatter-failure-swallowed",
        "file": "src/generic_region.rs",
        "before": 'formatter.write_str("arithmetic region does not end in FF AC")',
        "after": '{ let _ = formatter.write_str("arithmetic region does not end in FF AC"); Ok(()) }',
        "target": "lib",
        "test": "generic_region::tests::diagnostic_variants_preserve_fields_sources_and_formatter_failures",
    },
    {
        "id": "io-kind-misclassification",
        "file": "src/error.rs",
        "before": "io::ErrorKind::PermissionDenied => Self::PermissionDenied,",
        "after": "io::ErrorKind::PermissionDenied => Self::Other,",
        "target": "error_contract",
        "test": "io_error_conversion_preserves_public_kind_without_retaining_private_message",
    },
    {
        "id": "reader-inclusive-limit-off-by-one",
        "file": "src/document.rs",
        "before": "if new_length_u64 > limits.max_stream_bytes {",
        "after": "if new_length_u64 >= limits.max_stream_bytes {",
        "target": "public_api",
        "test": "stream_byte_limit_is_inclusive_for_slice_and_reader_apis",
    },
    {
        "id": "swallowed-page-sink-error",
        "file": "src/decode.rs",
        "before": "    sink.write_page(page)\n",
        "after": "    let _ = sink.write_page(page);\n    Ok(())\n",
        "target": "public_api",
        "test": "sink_failure_is_preserved_and_does_not_poison_the_decoder",
    },
    {
        "id": "generic-marker-offset-discarded",
        "file": "src/decode.rs",
        "before": "GenericRegionError::UnexpectedArithmeticMarker { offset, code } => {\n            GenericRegionErrorKind::UnexpectedArithmeticMarker { offset, code }\n        }",
        "after": "GenericRegionError::UnexpectedArithmeticMarker { offset: _offset, code } => {\n            GenericRegionErrorKind::UnexpectedArithmeticMarker { offset: 0, code }\n        }",
        "target": "lib",
        "test": "decode::tests::generic_error_conversion_preserves_every_classification_and_field",
    },
    {
        "id": "symbol-availability-discarded",
        "file": "src/decode.rs",
        "before": "SymbolTextError::SymbolIdOutOfRange { id, available } => {\n            DecodeError::Malformed(MalformedError::SymbolIdOutOfRange { id, available })\n        }",
        "after": "SymbolTextError::SymbolIdOutOfRange { id, available: _available } => {\n            DecodeError::Malformed(MalformedError::SymbolIdOutOfRange { id, available: 0 })\n        }",
        "target": "lib",
        "test": "decode::tests::symbol_error_conversion_preserves_reasons_and_structured_fields",
    },
    {
        "id": "huffman-kind-misclassification",
        "file": "src/decode.rs",
        "before": "HuffmanError::InvalidTable(kind) => {\n            DecodeError::Malformed(MalformedError::InvalidHuffman(kind))\n        }",
        "after": "HuffmanError::InvalidTable(_kind) => {\n            DecodeError::Malformed(MalformedError::IntegerOverflow)\n        }",
        "target": "lib",
        "test": "decode::tests::huffman_error_conversion_preserves_every_table_kind_and_field",
    },
    {
        "id": "generic-decode-error-flattened",
        "file": "src/decode.rs",
        "before": "GenericRegionError::Decode(error) => return error,",
        "after": "GenericRegionError::Decode(_error) => return DecodeError::Malformed(MalformedError::IntegerOverflow),",
        "target": "lib",
        "test": "decode::tests::error_adapters_preserve_existing_decode_errors",
    },
    {
        "id": "reference-table-limit-off-by-one",
        "file": "src/document.rs",
        "before": "if table_count > maximum_tables {",
        "after": "if table_count > maximum_tables + 1 {",
        "target": "public_envelope",
        "test": "reference_table_limits_count_tables_not_dictionaries_in_both_header_forms",
    },
    {
        "id": "immediate-region-accepted-as-refinement-reference",
        "file": "src/document.rs",
        "before": "Some(kind) => is_intermediate_region(kind),",
        "after": "Some(_kind) => true,",
        "target": "public_envelope",
        "test": "only_intermediate_region_roles_can_supply_explicit_refinement_references",
    },
    {
        "id": "halftone-reference-cardinality-weakened",
        "file": "src/document.rs",
        "before": "reference_count == 1\n                && matches!(",
        "after": "reference_count >= 1\n                && matches!(",
        "target": "public_envelope",
        "test": "halftone_and_refinement_reference_cardinality_is_exact",
    },
    {
        "id": "global-reference-type-misclassification",
        "file": "src/document.rs",
        "before": "let kind = keyed_get(&state.types, number)?;\n    Some(*kind)",
        "after": (
            "let _kind = keyed_get(&state.types, number)?;\n"
            "    Some(SegmentType::SymbolDictionary)"
        ),
        "target": "public_envelope",
        "test": "global_reference_roles_are_rechecked_without_mutating_shared_state",
    },
    {
        "id": "table-body-error-misclassification",
        "file": "src/document.rs",
        "before": "HuffmanError::Decode(error) => error,",
        "after": (
            "HuffmanError::Decode(_error) => "
            "DecodeError::Malformed(MalformedError::IntegerOverflow),"
        ),
        "target": "public_envelope",
        "test": "table_body_errors_precede_reference_role_checks_without_losing_error_fields",
    },
)


def mutate(source: str, mutation: dict) -> str:
    if source.count(mutation["before"]) != 1:
        raise ValueError(f"mutation site is missing or ambiguous: {mutation['id']}")
    return source.replace(mutation["before"], mutation["after"], 1)


def check_mutations(toolchain: str) -> dict:
    if not MUTATIONS or len({case["id"] for case in MUTATIONS}) != len(MUTATIONS):
        raise ValueError("missing or duplicate contract mutations")
    results = []
    cargo = ["cargo", f"+{toolchain}"]
    with tempfile.TemporaryDirectory(prefix="jbig2-contract-mutations-") as temporary:
        directory = Path(temporary).resolve()
        if directory.is_relative_to(ROOT):
            raise ValueError("mutation checks must run outside the checkout")
        for name in ("src", "tests"):
            shutil.copytree(ROOT / name, directory / name)
        (directory / "tools").mkdir()
        shutil.copyfile(ROOT / "tools/distribution_consumer.rs",
                        directory / "tools/distribution_consumer.rs")
        for name in ("Cargo.toml", "Cargo.lock", "README.md", "SECURITY.md", "LICENSE"):
            shutil.copyfile(ROOT / name, directory / name)
        environment = isolated_environment(directory)
        for mutation in MUTATIONS:
            selector = ["--lib"] if mutation["target"] == "lib" else ["--test", mutation["target"]]
            command = cargo + [
                "test", "--offline", "--locked", *selector,
                mutation["test"], "--", "--exact",
            ]
            baseline = run(command, directory, environment, capture=True)
            if not re.search(r"test result: ok\. 1 passed; 0 failed;", baseline):
                raise ValueError(f"baseline did not run exactly one passing test: {mutation['id']}")
            path = directory / mutation["file"]
            original = path.read_bytes()
            try:
                path.write_text(mutate(original.decode("utf-8"), mutation), encoding="utf-8")
                run(cargo + [
                    "test", "--offline", "--locked", *selector, "--no-run",
                ], directory, environment, capture=True)
                process = subprocess.run(
                    command, cwd=directory, env=environment,
                    capture_output=True, text=True, check=False,
                )
                if process.returncode == 0 or not re.search(
                    r"test result: FAILED\. 0 passed; 1 failed;", process.stdout
                ):
                    raise ValueError(
                        f"mutation survived or failed for a non-test reason: {mutation['id']}\n"
                        + process.stdout + process.stderr
                    )
                results.append({
                    "id": mutation["id"], "test_target": mutation["target"],
                    "test_name": mutation["test"], "baseline": "passed",
                    "mutant_compiles": True, "mutant": "detected_by_failing_test",
                })
            finally:
                path.write_bytes(original)
    return {
        "schema_version": 1, "status": "passed", "toolchain": toolchain,
        "scope": "Targeted public API, reference-role, typed-error, formatter-failure, symbol storage and halftone state contracts, not exhaustive codec mutation coverage",
        "runtime_source_modified": False, "mutations": results,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--toolchain", default="1.89.0")
    parser.add_argument("--output", type=Path)
    arguments = parser.parse_args()
    try:
        result = check_mutations(arguments.toolchain)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"contract mutation validation failed: {error}", file=sys.stderr)
        result = {"schema_version": 1, "status": "failed", "error": str(error)}
    text = json.dumps(result, indent=2) + "\n"
    if arguments.output:
        arguments.output.parent.mkdir(parents=True, exist_ok=True)
        arguments.output.write_text(text, encoding="utf-8")
    print(text, end="")
    return 0 if result["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

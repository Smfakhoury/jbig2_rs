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
        "id": "empty-unaligned-row-blit-rejected",
        "file": "src/bitmap.rs",
        "before": (
            "if width == 0 {\n"
            "            return Ok(());\n"
            "        }\n"
            "        let mut source_bit = source_x;"
        ),
        "after": (
            "if width == 0 {\n"
            "            return Err(MalformedError::IntegerOverflow);\n"
            "        }\n"
            "        let mut source_bit = source_x;"
        ),
        "target": "lib",
        "test": "bitmap::tests::zero_width_unaligned_row_blit_preserves_all_storage",
    },
    {
        "id": "row-blit-source-bit-alignment-discarded",
        "file": "src/bitmap.rs",
        "before": (
            "let source_chunk = read_shifted_chunk(&source.data, source_byte, source_bit % 8)\n"
            "                .ok_or(MalformedError::IntegerOverflow)?;\n"
            "            let destination_chunk =\n"
            "                read_chunk(&self.data, destination_index).ok_or(MalformedError::IntegerOverflow)?;"
        ),
        "after": (
            "let source_chunk = read_shifted_chunk(&source.data, source_byte, 0)\n"
            "                .ok_or(MalformedError::IntegerOverflow)?;\n"
            "            let destination_chunk =\n"
            "                read_chunk(&self.data, destination_index).ok_or(MalformedError::IntegerOverflow)?;"
        ),
        "target": "lib",
        "test": "bitmap::tests::row_batch_chunking_matches_independent_ordered_bit_updates",
    },
    {
        "id": "generic-snapshot-rejects-inclusive-retained-limit",
        "file": "src/generic_region.rs",
        "before": "if storage_bytes > limits.max_retained_state_bytes {",
        "after": "if storage_bytes >= limits.max_retained_state_bytes {",
        "target": "lib",
        "test": "generic_region::tests::generic_snapshots_copy_all_states_and_preserve_rejected_targets",
    },
    {
        "id": "generic-typical-work-row-charge-lost",
        "file": "src/generic_region.rs",
        "before": "work = next_work;\n    }\n    Ok(work)\n}",
        "after": "work = next_work - u64::from(rows);\n    }\n    Ok(work)\n}",
        "target": "lib",
        "test": "generic_region::tests::raw_raster_work_guards_cover_exact_addition_overflow_boundaries",
    },
    {
        "id": "reference-role-rejects-inclusive-table-count",
        "file": "src/document.rs",
        "before": "if table_count > maximum_tables {",
        "after": "if table_count >= maximum_tables {",
        "target": "lib",
        "test": "document::tests::raw_reference_role_matrix_matches_numeric_tag_policy_locally_and_globally",
    },
    {
        "id": "failed-stripe-seal-clobbers-last-end",
        "file": "src/decode.rs",
        "before": "let next_open = checked_seal_stripe_state(page.open_stripe_max_row, row)?;",
        "after": (
            "page.last_stripe_end = Some(row);\n"
            "    let next_open = checked_seal_stripe_state(page.open_stripe_max_row, row)?;"
        ),
        "target": "lib",
        "test": "decode::tests::raw_stripe_sealing_preserves_pixels_and_rejected_page_state",
    },
    {
        "id": "owned-generic-normalization-appends-wrong-marker",
        "file": "src/generic_region.rs",
        "before": (
            "normalized.push(ARITHMETIC_TERMINATOR[1]);\n"
            "    decode_arithmetic_generic(&normalized, header, rows, None, limits)"
        ),
        "after": (
            "normalized.push(0xab);\n"
            "    decode_arithmetic_generic(&normalized, header, rows, None, limits)"
        ),
        "target": "lib",
        "test": "generic_region::tests::known_length_normalization_preserves_zero_row_bitmap_geometry",
    },
    {
        "id": "direct-generic-normalization-appends-wrong-marker",
        "file": "src/generic_region.rs",
        "before": (
            "normalized.push(ARITHMETIC_TERMINATOR[1]);\n"
            "    decode_arithmetic_generic_into(&normalized, header, rows, limits, bitmap)"
        ),
        "after": (
            "normalized.push(0xab);\n"
            "    decode_arithmetic_generic_into(&normalized, header, rows, limits, bitmap)"
        ),
        "target": "lib",
        "test": "generic_region::tests::known_length_normalization_preserves_zero_row_bitmap_geometry",
    },
    {
        "id": "unknown-generic-extent-includes-following-byte",
        "file": "src/generic_region.rs",
        "before": "consumed: count_end,",
        "after": "consumed: count_end + 1,",
        "target": "lib",
        "test": "generic_region::tests::unknown_length_framing_preserves_count_extent_and_exact_eof_fields",
    },
    {
        "id": "runtime-work-pair-rejects-inclusive-limit",
        "file": "src/decode.rs",
        "before": "Some(used) => used <= limits.max_work_units,",
        "after": "Some(used) => used < limits.max_work_units,",
        "target": "lib",
        "test": "decode::tests::runtime_budgets_match_wide_arithmetic_and_preserve_rejected_state",
    },
    {
        "id": "referred-buffer-retention-polarity-reversed",
        "file": "src/decode.rs",
        "before": (
            "== Some(false)\n"
            "        {\n"
            "            if let Some(buffer_index) = region_buffer_index(buffers, number) {"
        ),
        "after": (
            "== Some(true)\n"
            "        {\n"
            "            if let Some(buffer_index) = region_buffer_index(buffers, number) {"
        ),
        "target": "lib",
        "test": "decode::tests::referred_bitmap_resources_release_only_selected_existing_keys_once",
    },
    {
        "id": "released-symbol-bitmap-bytes-not-deducted",
        "file": "src/decode.rs",
        "before": ".checked_add(bitmap_byte_len(&resource.exported[bitmap_index])?)",
        "after": ".checked_add(0)",
        "target": "lib",
        "test": "decode::tests::referred_bitmap_resources_release_only_selected_existing_keys_once",
    },
    {
        "id": "raw-bitmap-snapshot-discards-source-x",
        "file": "src/decode.rs",
        "before": "snapshot.compose_region(\n            page,\n            region.x,",
        "after": "snapshot.compose_region(\n            page,\n            0,",
        "target": "lib",
        "test": "decode::tests::raw_bitmap_snapshot_matches_literal_scalar_clipping_and_padding",
    },
    {
        "id": "reference-depth-fails-to-count-own-node",
        "file": "src/document.rs",
        "before": "let Some(candidate) = referred_depth.checked_add(1) else {",
        "after": "let Some(candidate) = referred_depth.checked_add(0) else {",
        "target": "lib",
        "test": "document::tests::reference_depth_orders_raw_metadata_and_reports_missing_or_overflowed_ancestors",
    },
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
    {
        "id": "document-stream-quota-rejects-inclusive-boundary",
        "file": "src/document.rs",
        "before": (
            "fn enforce_stream_limit(length: usize, limits: &Limits) -> Result<(), DecodeError> {\n"
            "    let Ok(actual) = u64::try_from(length) else {\n"
            "        return Err(MalformedError::IntegerOverflow.into());\n"
            "    };\n"
            "    if actual > limits.max_stream_bytes {"
        ),
        "after": (
            "fn enforce_stream_limit(length: usize, limits: &Limits) -> Result<(), DecodeError> {\n"
            "    let Ok(actual) = u64::try_from(length) else {\n"
            "        return Err(MalformedError::IntegerOverflow.into());\n"
            "    };\n"
            "    if actual >= limits.max_stream_bytes {"
        ),
        "target": "lib",
        "test": "document::tests::numeric_preflight_quotas_match_wide_results_and_priority",
    },
    {
        "id": "document-page-quota-discards-next-count",
        "file": "src/document.rs",
        "before": (
            "fn enforce_page_limit(count: usize, limits: &Limits) -> Result<(), DecodeError> {\n"
            "    let Some(next_count) = count.checked_add(1) else {"
        ),
        "after": (
            "fn enforce_page_limit(count: usize, limits: &Limits) -> Result<(), DecodeError> {\n"
            "    let Some(next_count) = count.checked_add(0) else {"
        ),
        "target": "lib",
        "test": "document::tests::numeric_preflight_quotas_match_wide_results_and_priority",
    },
    {
        "id": "filled-bitmap-loses-black-storage-bit",
        "file": "src/bitmap.rs",
        "before": "data.resize(length, if value { 0xFF } else { 0 });",
        "after": "data.resize(length, if value { 0xFE } else { 0 });",
        "target": "lib",
        "test": "bitmap::tests::filled_layout_and_padding_are_deterministic",
    },
    {
        "id": "filled-bitmap-keeps-black-padding",
        "file": "src/bitmap.rs",
        "before": "data[final_byte] &= trailing_mask;",
        "after": "data[final_byte] |= trailing_mask;",
        "target": "lib",
        "test": "bitmap::tests::filled_layout_and_padding_are_deterministic",
    },
    {
        "id": "mmr-raster-reference-pixels-corrupted-after-row-finish",
        "file": "src/mmr.rs",
        "before": "production_mmr_row_termination!(row, height);",
        "after": (
            "production_mmr_row_termination!(row, height);\n"
            "        if row == 0 && height == 1 && width > 0 && !bitmap.data.is_empty() {\n"
            "            bitmap.data[0] ^= 0x80;\n"
            "        }"
        ),
        "target": "lib",
        "test": "mmr::tests::decoded_reference_cache_matches_the_last_physical_row",
    },
    {
        "id": "mmr-raster-reference-cache-corrupted-after-row-swap",
        "file": "src/mmr.rs",
        "before": "production_mmr_row_termination!(row, height);",
        "after": (
            "production_mmr_row_termination!(row, height);\n"
            "        if row == 0 && transitions.len() > 2 { transitions[0] = 0; }"
        ),
        "target": "lib",
        "test": "mmr::tests::decoded_reference_cache_matches_the_last_physical_row",
    },
    {
        "id": "mmr-raster-output-discards-input-extent",
        "file": "src/mmr.rs",
        "before": "production_mmr_trace_output!(data, width, height, bitmap, reader, traced_rows);",
        "after": (
            "production_mmr_trace_output!(data, width, height, bitmap, reader, traced_rows);\n"
            "    reader.bit_position = 0;"
        ),
        "target": "lib",
        "test": "mmr::tests::decoded_reference_cache_matches_the_last_physical_row",
    },
    {
        "id": "mmr-raster-output-blackens-a-white-pixel",
        "file": "src/mmr.rs",
        "before": "production_mmr_trace_output!(data, width, height, bitmap, reader, traced_rows);",
        "after": (
            "production_mmr_trace_output!(data, width, height, bitmap, reader, traced_rows);\n"
            "    if bitmap.width > 0 && bitmap.height > 0 {\n"
            "        bitmap.fill_black_span(0, 0, 1)\n"
            "            .map_err(|_error| MmrError::InvalidTransition)?;\n"
            "    }"
        ),
        "target": "lib",
        "test": "mmr::tests::decoded_reference_cache_matches_the_last_physical_row",
    },
    {
        "id": "mmr-required-prefix-ignores-nonzero-padding",
        "file": "src/mmr.rs",
        "before": (
            "if reader.read_bits(padding_count)? != 0 {\n"
            "            return Err(MmrError::NonZeroEofbPadding);\n"
            "        }"
        ),
        "after": "let _padding_value = reader.read_bits(padding_count)?;",
        "target": "lib",
        "test": "mmr::tests::prefix_decoder_rejects_nonzero_alignment_padding",
    },
    {
        "id": "contiguous-symbol-lookup-uses-first-bitmap",
        "file": "src/symbol_text.rs",
        "before": "Self::Contiguous(symbols) => symbols.get(index),",
        "after": (
            "Self::Contiguous(symbols) => "
            "symbols.get(if index < symbols.len() { 0 } else { index }),"
        ),
        "target": "lib",
        "test": "symbol_text::tests::exported_symbols_match_exact_clone_oracle",
    },
    {
        "id": "borrowed-symbol-lookup-uses-first-bitmap",
        "file": "src/symbol_text.rs",
        "before": "Self::Borrowed(symbols) => match symbols.get(index) {",
        "after": (
            "Self::Borrowed(symbols) => match symbols.get("
            "if index < symbols.len() { 0 } else { index }) {"
        ),
        "target": "lib",
        "test": "symbol_text::tests::exported_symbols_match_exact_clone_oracle",
    },
    {
        "id": "new-symbol-lookup-uses-first-bitmap",
        "file": "src/symbol_text.rs",
        "before": "Ok(&new_symbols[new_index])",
        "after": "Ok(&new_symbols[0])",
        "target": "lib",
        "test": "symbol_text::tests::exported_symbols_match_exact_clone_oracle",
    },
    {
        "id": "exported-symbols-repeat-first-selection",
        "file": "src/symbol_text.rs",
        "before": "let symbol_id = exported_indices[index];",
        "after": "let symbol_id = exported_indices[0];",
        "target": "lib",
        "test": "symbol_text::tests::exported_symbols_match_exact_clone_oracle",
    },
    {
        "id": "symbol-lookup-rejects-valid-representable-id",
        "file": "src/symbol_text.rs",
        "before": "if id_usize < imported.len() {",
        "after": (
            "if id_usize < available {\n"
            "        return Err(SymbolTextError::InvalidState(\"mutant false valid lookup rejection\"));\n"
            "    }\n"
            "    if id_usize < imported.len() {"
        ),
        "target": "lib",
        "test": "symbol_text::tests::exported_symbols_match_exact_clone_oracle",
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

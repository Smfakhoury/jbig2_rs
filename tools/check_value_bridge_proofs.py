#!/usr/bin/env python3
"""Check exact value/error bridges and checked-ID caller frames against compiling controls."""

import argparse
import json
import subprocess
import sys
from pathlib import Path

from proof_sensitivity import check_semantic_cases, replace_once
from source_group import runtime_function


ROOT = Path(__file__).resolve().parents[1]
BASELINE_GROUPS = (("symbol_text", (
    "widen_bool_i64", "widen_u32_i64", "widen_u8_i32", "widen_u8_i64",
)), ("symbol_text", ("checked_symbol_id", "decode_checked_symbol_id")))
ID_RESULT_POST = "out == symbol_text_spec::checked_symbol_id_result(id, available_symbols),"
ID_TRANSITION_POST = "symbol_text_spec::checked_symbol_id_transition(old(codec), final(codec), request, out),"
ZERO_COUNT = """if request.available_symbols == 0 {
    return Err(SymbolTextError::InvalidCount(
        "symbol ID with no available symbols",
    ));
}"""
DECODE_ID = "let id = codec.decode_symbol_id(request)?;"
CHECK_ID = "checked_symbol_id(id, request.available_symbols)"
NATIVE_PREFIX = "symbol_text::tests::arithmetic_composition::"
TABLE_NATIVE_PREFIX = "symbol_text::tests::huffman_symbol_ids::"


def checked_case(identity, function, site, replacement, control, test, *, native_prefix=NATIVE_PREFIX):
    return {
        "id": identity, "file": "src/symbol_text.rs", "module": "symbol_text",
        "function": function, "site": site, "replacement": replacement,
        "controls": ((control, ""),), "native_test": native_prefix + test,
    }


def checked_table_case(identity, site, replacement):
    row = checked_case(
        identity, "decode_checked_symbol_id", site, replacement, ID_TRANSITION_POST,
        "checked_table_ids_preserve_values_rejections_errors_and_consumption",
        native_prefix=TABLE_NATIVE_PREFIX,
    )
    guarded = """(request.available_symbols == 0
        || match *old(codec) {
            SymbolCodec::Huffman(state) => state.symbol_id_table.is_none(),
            _ => true,
        }) ==> symbol_text_spec::checked_symbol_id_transition(old(codec), final(codec), request, out),"""
    declaration = runtime_function(ROOT, "symbol_text", "decode_checked_symbol_id")
    attributes = [attribute for attribute in declaration["attributes"] if "verus_spec(" in attribute]
    if len(attributes) != 1:
        raise ValueError("checked table caller needs one exact attached contract")
    row["controls"] = ((attributes[0], replace_once(attributes[0], ID_TRANSITION_POST, guarded)),)
    return row


def numeric_case(name, argument, result):
    contract = f"""#[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out as int == value as int,
    ))]
    pub(super) fn {name}"""
    return {
        "id": f"{name}_changes_value", "file": "src/symbol_text.rs",
        "module": "symbol_text", "function": name,
        "site": f"fn {name}(value: {argument}) -> {result} {{ {result}::from(value) }}",
        "replacement": f"fn {name}(value: {argument}) -> {result} {{ {result}::from(value) + 1 }}",
        "controls": ((contract, contract.replace("out as int == value as int", "true")),),
    }

def width_case(identity, site, replacement, retained):
    row = runtime_function(ROOT, "symbol_text", "symbol_id_code_length")
    contracts = [attribute for attribute in row["attributes"] if "verus_spec(" in attribute]
    if len(contracts) != 1:
        raise ValueError("symbol width needs one exact production contract")
    weaker = f"""#[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures match out {{
            Ok(length) => {retained},
            Err(_) => false,
        }},
    ))]"""
    return {
        "id": identity, "file": row["file"], "module": "symbol_text",
        "function": "symbol_id_code_length", "site": site, "replacement": replacement,
        "controls": ((contracts[0], weaker),),
        "native_test": "symbol_text::tests::resource_accounting::symbol_width_thresholds_zero_max_and_every_class_are_exact",
    }


WIDTH_VALID = "symbol_text_spec::symbol_code_length_valid(symbol_count, length)"
WIDTH_CAPACITY = "symbol_count as nat <= symbol_text_spec::pow2(length as nat)"
WIDTH_PREVIOUS = """(length > 0 ==> symbol_text_spec::pow2(
    (length as nat - 1) as nat,
) < symbol_count as nat)"""
WIDTH_LEAST = """(forall|smaller: nat| smaller < length ==>
    #[trigger] symbol_text_spec::pow2(smaller) < symbol_count as nat)"""


CASES = (
    width_case(
        "symbol_width_drops_covering_capacity", "} else if symbol_count <= 4 { 2 }",
        "} else if symbol_count <= 4 { 1 }",
        f"{WIDTH_VALID} && {WIDTH_PREVIOUS} && {WIDTH_LEAST}",
    ),
    width_case(
        "symbol_width_is_not_least", "} else if symbol_count <= 4 { 2 }",
        "} else if symbol_count <= 4 { 3 }",
        f"{WIDTH_VALID} && {WIDTH_CAPACITY}",
    ),
    width_case(
        "symbol_width_truncates_high_class", """} else if symbol_count <= 2_147_483_648 { 31 } else { 32 };""",
        """} else if symbol_count <= 2_147_483_648 { 31 } else { 31 };""",
        f"{WIDTH_VALID} && {WIDTH_PREVIOUS} && {WIDTH_LEAST}",
    ),
    width_case(
        "symbol_width_spuriously_rejects_count",
        """production_symbol_code_length_bounds!(symbol_count); Ok(length)""",
        """production_symbol_code_length_bounds!(symbol_count);
        if symbol_count == 3 {
            return Err(SymbolTextError::InvalidCount("mutant symbol width"));
        }
        Ok(length)""",
        "length == symbol_text_spec::symbol_code_length_expected(symbol_count)"
        " && symbol_text_spec::symbol_code_length_bounds(symbol_count, length)"
        f" && {WIDTH_LEAST}",
    ),
    checked_table_case(
            "checked_table_id_filters_substituted_zero",
            CHECK_ID, """let table_backed = match &*codec {
                SymbolCodec::Huffman(state) => state.symbol_id_table.is_some(),
                _ => false,
            };
            if table_backed {
                checked_symbol_id(0, request.available_symbols)
            } else {
                checked_symbol_id(id, request.available_symbols)
            }""",
    ),
    checked_table_case(
            "checked_table_id_rewinds_rejected_code",
            CHECK_ID, """{
                let result = checked_symbol_id(id, request.available_symbols);
                if result.is_err() {
                    if let SymbolCodec::Huffman(state) = codec {
                        if state.symbol_id_table.is_some() { state.reader.bit_position = 0; }
                    }
                }
                result
            }""",
    ),
    checked_case(
        "checked_id_error_discards_id", "checked_symbol_id",
        """return Err(SymbolTextError::SymbolIdOutOfRange {
            id,
            available: available_symbols,
        });""",
        """return Err(SymbolTextError::SymbolIdOutOfRange {
            id: 0,
            available: available_symbols,
        });""",
        ID_RESULT_POST, "checked_symbol_ids_preserve_u32_values_and_exact_rejection_payloads",
    ),
    checked_case(
        "checked_id_error_discards_available", "checked_symbol_id",
        """return Err(SymbolTextError::SymbolIdOutOfRange {
            id,
            available: available_symbols,
        });""",
        """return Err(SymbolTextError::SymbolIdOutOfRange {
            id,
            available: 0,
        });""",
        ID_RESULT_POST, "checked_symbol_ids_preserve_u32_values_and_exact_rejection_payloads",
    ),
    checked_case(
        "checked_id_empty_book_decodes_before_rejection", "decode_checked_symbol_id",
        ZERO_COUNT, ZERO_COUNT.replace(
            "return Err", "let _ignored = codec.decode_symbol_id(request)?;\nreturn Err",
        ),
        ID_TRANSITION_POST, "empty_codebook_rejects_before_arithmetic_or_literal_bit_decoding",
    ),
    checked_case(
        "checked_id_substitutes_zero_after_validation", "decode_checked_symbol_id",
        CHECK_ID, """match checked_symbol_id(id, request.available_symbols) {
            Ok(_id) => Ok(0),
            Err(error) => Err(error),
        }""",
        ID_TRANSITION_POST, "checked_literal_ids_preserve_scalar_bits_errors_and_consumed_positions",
    ),
    checked_case(
        "checked_id_masks_entropy_error", "decode_checked_symbol_id",
        DECODE_ID, """let id = match codec.decode_symbol_id(request) {
            Ok(id) => id,
            Err(_error) => return Err(SymbolTextError::InvalidCount("masked symbol ID error")),
        };""",
        ID_TRANSITION_POST, "checked_literal_ids_preserve_scalar_bits_errors_and_consumed_positions",
    ),
    checked_case(
        "checked_id_rewinds_rejected_literal", "decode_checked_symbol_id",
        CHECK_ID, """{
            let result = checked_symbol_id(id, request.available_symbols);
            if result.is_err() {
                if let SymbolCodec::Huffman(state) = codec { state.reader.bit_position = 0; }
            }
            result
        }""",
        ID_TRANSITION_POST, "checked_literal_ids_preserve_scalar_bits_errors_and_consumed_positions",
    ),
    checked_case(
        "checked_id_discards_rejected_iaid_bank", "decode_checked_symbol_id",
        CHECK_ID, """{
            let result = checked_symbol_id(id, request.available_symbols);
            if result.is_err() {
                if let SymbolCodec::Arithmetic(state) = codec { state.iaid.clear(); }
            }
            result
        }""",
        ID_TRANSITION_POST, "checked_arithmetic_ids_preserve_raw_results_errors_and_consumed_state",
    ),
) + (
    numeric_case("widen_u32_i64", "u32", "i64"),
    numeric_case("widen_u8_i64", "u8", "i64"),
    numeric_case("widen_u8_i32", "u8", "i32"),
    {
        "id": "boolean_conversion_inverted", "file": "src/symbol_text.rs",
        "module": "symbol_text", "function": "widen_bool_i64",
        "site": "fn widen_bool_i64(value: bool) -> i64 { if value { 1 } else { 0 } }",
        "replacement": "fn widen_bool_i64(value: bool) -> i64 { if value { 0 } else { 1 } }",
        "controls": (("out == if value { 1i64 } else { 0i64 }", "true"),),
    },
    {
        "id": "operator_equality_changes_or_to_and", "file": "src/decode.rs",
        "module": "decode", "function": "same_combination_operator",
        "site": """crate::bitmap::CombinationOperator::Or => {
            matches!(right, crate::bitmap::CombinationOperator::Or)
        }""",
        "replacement": """crate::bitmap::CombinationOperator::Or => {
            matches!(right, crate::bitmap::CombinationOperator::And)
        }""",
        "controls": (("out == (left == right)", "true"),),
    },
)
CASES = tuple(
    {**case, "controls": tuple((strong, weak.replace("Err(_) => false", "Err(_) => true"))
                              for strong, weak in case["controls"])}
    if case["id"] == "symbol_width_spuriously_rejects_count" else case
    for case in CASES
)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--verus-executable", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    try:
        result = check_semantic_cases(
            ROOT, arguments.verus_executable.resolve(), CASES,
            ("src/symbol_text.rs", "src/decode.rs", "specs/symbol_text.rs",
             "specs/symbol_text/symbol_ids.rs", "specs/symbol_text/arithmetic_codec.rs",
             "specs/symbol_text/huffman_codec.rs", "specs/arithmetic.rs"), "jbig2-value-bridges-",
            "Exact widening/operator bridges, checked-ID error payloads, zero-book preflight and arithmetic/table/literal-Huffman result/state composition. Table-only controls retain arithmetic, literal and empty-book guarantees; normative table/extent choice and full rendered streams remain separate",
            baseline_groups=BASELINE_GROUPS,
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"value bridge proof sensitivity failed: {error}", file=sys.stderr)
        result = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2))
    return 0 if result["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

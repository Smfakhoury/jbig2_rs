#!/usr/bin/env python3
"""Check exact value/error bridges and checked-ID caller frames against compiling controls."""

import argparse
import json
import subprocess
import sys
from pathlib import Path

from proof_sensitivity import check_semantic_cases


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


def checked_case(identity, function, site, replacement, control, test):
    return {
        "id": identity, "file": "src/symbol_text.rs", "module": "symbol_text",
        "function": function, "site": site, "replacement": replacement,
        "controls": ((control, ""),), "native_test": NATIVE_PREFIX + test,
    }


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


CASES = (
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
             "specs/arithmetic.rs"), "jbig2-value-bridges-",
            "Exact widening/operator bridges plus checked-ID error payloads, zero-book preflight and arithmetic/literal-Huffman result/state composition; table-backed Huffman and full normative streams remain separate",
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

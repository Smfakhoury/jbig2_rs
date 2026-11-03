#!/usr/bin/env python3
"""Check entropy-adapter results, exact literal bits and arithmetic bank isolation."""

import argparse
import json
from pathlib import Path
import subprocess
import sys

from proof_sensitivity import check_semantic_cases, replace_once
from rust_source import source_functions
from source_group import runtime_function


ROOT = Path(__file__).resolve().parents[1]
BASELINE_GROUPS = (
    ("arithmetic", ("ArithmeticContext::from_state", "ArithmeticError::into_decode")),
    ("generic_region", (
        "integer_overflow", "probability_signal_error", "unexpected_eof", "validate_probability_states",
    )),
    ("symbol_text", (
        "ArithmeticCodec::decode_integer", "ArithmeticCodec::decode_symbol_id", "new_arithmetic_codec",
    )),
    ("symbol_text", (
        "HuffmanCodec::read_raw_bits", "HuffmanCodec::decode_integer",
        "HuffmanCodec::decode_symbol_id",
    )),
    ("symbol_text", ("SymbolCodec::decode_integer", "SymbolCodec::decode_symbol_id")),
)
ERROR_POST = "out == arithmetic_spec::decode_error(self),"
SLOT_POST = "out == symbol_text_spec::integer_field_slot(self),"
INTEGER_CALL = """crate::arithmetic::arithmetic_spec::integer_state(
    old(self).decoder,
    old(self).integers.slots[symbol_text_spec::integer_field_slot(field) as int]@,
)"""
INTEGER_RESULT = f"""out == symbol_text_spec::arithmetic_integer_result(
    {INTEGER_CALL}.0,
),"""
INTEGER_DECODER = f"final(self).decoder == {INTEGER_CALL}.1,"
INTEGER_FRAME = """forall|slot: int| 0 <= slot < INTEGER_FIELD_COUNT
    && slot != symbol_text_spec::integer_field_slot(field) ==>
        final(self).integers.slots[slot]@ == old(self).integers.slots[slot]@,"""
INTEGER_IAID_SITE = """final(self).iaid@ == old(self).iaid@,
final(self).refinement == old(self).refinement,
final(self).refinement_work == old(self).refinement_work,
out == symbol_text_spec::arithmetic_integer_result("""
IAID_FRAME_SITE = """final(self).integers.slots@ == old(self).integers.slots@,
final(self).refinement == old(self).refinement,
final(self).refinement_work == old(self).refinement_work,
out == symbol_text_spec::arithmetic_symbol_id_result("""
IAID_RESULT = """out == symbol_text_spec::arithmetic_symbol_id_result(
    crate::arithmetic::arithmetic_spec::iaid_state(
        old(self).decoder, old(self).iaid@, request.code_length,
    ).0,
),"""
DECODED_TAIL = """Err(error) => return Err(SymbolTextError::Decode(error.into_decode())),
};
match decoded {"""
IAID_HEAD = """match self
        .decoder
        .decode_iaid(request.code_length, &mut self.iaid)"""
IAID_RETURN = """match self
    .decoder
    .decode_iaid(request.code_length, &mut self.iaid) {
    Ok(value) => Ok(value),
    Err(error) => Err(SymbolTextError::Decode(error.into_decode())),
}"""
FUNCTIONS = {row["id"]: row for row in source_functions(ROOT)}
VALIDATION_BODY = FUNCTIONS["generic_region::contexts::validate_probability_states"]["body_text"]
SIGNAL_BODY = FUNCTIONS["generic_region::generic::probability_signal_error"]["body_text"]
EOF_BODY = FUNCTIONS["generic_region::support::unexpected_eof"]["body_text"]
OVERFLOW_BODY = FUNCTIONS["generic_region::support::integer_overflow"]["body_text"]
STATE_ERROR_POST = "&& error == ArithmeticError::InvalidContextState { state_index, mps }"
VALIDATION_POST = "generic_region_spec::probability_validation_result(contexts@, out),"
SIGNAL_POST = """ensures out == generic_region_spec::probability_error_for_signal(
    decoder, index, context_count, signal,
),"""
EOF_POST = """ensures out == GenericRegionError::Decode(
    crate::arithmetic::arithmetic_spec::decode_error(
        crate::arithmetic::arithmetic_spec::eof_error(offset, needed, remaining),
    ),
),"""
OVERFLOW_POST = """ensures out == GenericRegionError::Decode(
    DecodeError::Malformed(MalformedError::IntegerOverflow),
),"""
INVALID_STATE_RETURN = "return Err(ArithmeticError::InvalidContextState { state_index, mps });"
INVALID_STATE_VALUE = """DecodeError::Malformed(MalformedError::InvalidArithmeticContextState {
    state_index,
    mps,
})"""
ENUM_INTEGER_POST = "symbol_text_spec::arithmetic_integer_transition(old(self), final(self), field, out),"
ENUM_ID_POST = "symbol_text_spec::arithmetic_symbol_id_transition(old(self), final(self), request, out),"
BANK_INITIAL_POST = "&& symbol_text_spec::integer_contexts_initial(&codec.integers)"
IAID_LENGTH_POST = "&& codec.iaid@.len() == (1usize << iaid_code_length)"
IAID_INITIAL_POST = """&& (forall|index: int| 0 <= index < codec.iaid@.len() ==>
    #[trigger] codec.iaid@[index]
        == ArithmeticContext { state_index: 0, mps: 0 })"""
INPUT_INITIAL_POST = """&& codec.decoder.bytes@ == bytes@
&& codec.decoder.interval == crate::arithmetic::MIN_INTERVAL
&& codec.decoder.byte_position
    == crate::arithmetic::arithmetic_spec::initialized_state(bytes).unwrap().0
&& codec.decoder.current_byte
    == crate::arithmetic::arithmetic_spec::initialized_state(bytes).unwrap().1
&& codec.decoder.code
    == crate::arithmetic::arithmetic_spec::initialized_state(bytes).unwrap().2
&& codec.decoder.bit_count
    == crate::arithmetic::arithmetic_spec::initialized_state(bytes).unwrap().3
&& codec.decoder.marker
    == crate::arithmetic::arithmetic_spec::initialized_state(bytes).unwrap().4"""
CONSTRUCTOR_TAIL = """iaid,
refinement: match refinement_template {
    Some(template) => Some(RefinementContexts::new(template, limits)?),
    None => None,
},
refinement_work: 0,"""
CONSTRUCTOR_TEST = (
    "symbol_text::tests::arithmetic_composition::"
    "fresh_arithmetic_codec_has_zero_banks_exact_size_and_literal_initial_state"
)
ENUM_INTEGER_TEST = (
    "symbol_text::tests::arithmetic_composition::"
    "arithmetic_enum_integer_preserves_direct_result_state_and_other_banks"
)
ENUM_ID_TEST = (
    "symbol_text::tests::arithmetic_composition::"
    "arithmetic_enum_iaid_preserves_direct_result_range_and_full_frames"
)


def case(identity, module, function, site, replacement, control, weaker="", *, native_test=None):
    return {
        "id": identity, "file": f"src/{module}.rs", "module": module,
        "function": function, "site": site, "replacement": replacement,
        "controls": ((control, weaker),),
        **({"native_test": native_test} if native_test is not None else {}),
    }


def literal_case(identity, function, site, replacement, test):
    row = runtime_function(ROOT, "symbol_text", function)
    attributes = [attribute for attribute in row["attributes"] if "verus_spec(" in attribute]
    if len(attributes) != 1:
        raise ValueError("literal adapter needs one exact attached contract")
    if function.startswith("SymbolCodec::"):
        weaker = """#[cfg_attr(verus_keep_ghost, verus_spec(out =>
            requires symbol_text_spec::codec_valid(old(self)),
            ensures match out {
                Ok(_) => symbol_text_spec::codec_valid(final(self)),
                Err(_) => true,
            },
        ))]"""
        post = ENUM_INTEGER_POST if function.endswith("decode_integer") else ENUM_ID_POST
        weaker = weaker.replace("ensures match out {", f"ensures {post}\nmatch out {{")
    else:
        weaker = """#[cfg_attr(verus_keep_ghost, verus_spec(out =>
            requires crate::huffman::huffman_spec::reader_valid(&old(self).reader),
            ensures crate::huffman::huffman_spec::reader_valid(&final(self).reader),
        ))]"""
    return {
        "id": identity, "file": row["file"], "module": "symbol_text",
        "function": function, "site": site, "replacement": replacement,
        "controls": ((attributes[0], weaker),),
        "native_test": "symbol_text::tests::huffman_fields::" + test,
    }


CASES = (
    case(
        "fresh_codec_poisoned_integer_banks", "symbol_text", "new_arithmetic_codec",
        "integers: IntegerContexts { slots: [[ArithmeticContext::new(); 512]; INTEGER_FIELD_COUNT], },",
        "integers: IntegerContexts { slots: [[ArithmeticContext { state_index: 0, mps: 1 }; 512]; INTEGER_FIELD_COUNT], },",
        BANK_INITIAL_POST, native_test=CONSTRUCTOR_TEST,
    ),
    case(
        "fresh_codec_empty_iaid_bank", "symbol_text", "new_arithmetic_codec",
        "iaid.resize(count, ArithmeticContext::new());",
        "iaid.resize(0, ArithmeticContext::new());",
        IAID_LENGTH_POST, native_test=CONSTRUCTOR_TEST,
    ),
    case(
        "fresh_codec_poisoned_iaid_bank", "symbol_text", "new_arithmetic_codec",
        "iaid.resize(count, ArithmeticContext::new());",
        "iaid.resize(count, ArithmeticContext { state_index: 1, mps: 1 });",
        IAID_INITIAL_POST, native_test=CONSTRUCTOR_TEST,
    ),
    case(
        "fresh_codec_nonzero_refinement_work", "symbol_text", "new_arithmetic_codec",
        CONSTRUCTOR_TAIL, CONSTRUCTOR_TAIL.replace("refinement_work: 0,", "refinement_work: 1,"),
        "&& codec.refinement_work == 0", native_test=CONSTRUCTOR_TEST,
    ),
    case(
        "fresh_codec_drops_first_input_byte", "symbol_text", "new_arithmetic_codec",
        "decoder: ArithmeticDecoder::new(bytes)?,",
        "decoder: ArithmeticDecoder::new(if bytes.is_empty() { bytes } else { &bytes[1..] })?,",
        INPUT_INITIAL_POST, native_test=CONSTRUCTOR_TEST,
    ),
    case(
        "arithmetic_enum_integer_discards_value", "symbol_text", "SymbolCodec::decode_integer",
        "Self::Arithmetic(codec) => codec.decode_integer(field),",
        """Self::Arithmetic(codec) => match codec.decode_integer(field) {
            Ok(_) => Ok(EntropyInteger::OutOfBand), Err(error) => Err(error),
        },""",
        ENUM_INTEGER_POST, native_test=ENUM_INTEGER_TEST,
    ),
    case(
        "arithmetic_enum_iaid_discards_value", "symbol_text", "SymbolCodec::decode_symbol_id",
        "Self::Arithmetic(codec) => codec.decode_symbol_id(request),",
        "Self::Arithmetic(codec) => codec.decode_symbol_id(request).map(|_value| 0),",
        ENUM_ID_POST, native_test=ENUM_ID_TEST,
    ),
    case(
        "arithmetic_enum_discards_refinement_work", "symbol_text", "SymbolCodec::decode_integer",
        "Self::Arithmetic(codec) => codec.decode_integer(field),",
        """Self::Arithmetic(codec) => {
            codec.refinement_work = 0; codec.decode_integer(field)
        },""",
        ENUM_INTEGER_POST, native_test=ENUM_INTEGER_TEST,
    ),
) + (
    case(
        "arithmetic_error_forwarding_loses_iaid_length", "arithmetic", "ArithmeticError::into_decode",
        """Self::InvalidIaidCodeLength(length) => {
            DecodeError::Malformed(MalformedError::InvalidIaidCodeLength(length))
        }""",
        """Self::InvalidIaidCodeLength(length) => {
            DecodeError::Malformed(MalformedError::InvalidIaidCodeLength(0))
        }""",
        ERROR_POST,
    ),
    case(
        "integer_field_uses_the_next_bank", "symbol_text", "IntegerField::slot",
        "const fn slot(self) -> usize { self as usize }",
        "const fn slot(self) -> usize { (self as usize + 1) % INTEGER_FIELD_COUNT }",
        SLOT_POST,
    ),
    case(
        "arithmetic_integer_caller_discards_value", "symbol_text", "ArithmeticCodec::decode_integer",
        "ArithmeticInteger::Value(value) => Ok(EntropyInteger::Value(value)),",
        "ArithmeticInteger::Value(value) => Ok(EntropyInteger::Value(0)),",
        INTEGER_RESULT,
    ),
    case(
        "arithmetic_integer_caller_replaces_decoder", "symbol_text", "ArithmeticCodec::decode_integer",
        DECODED_TAIL,
        DECODED_TAIL.replace("match decoded {", """if let Ok(replacement) = ArithmeticDecoder::new(&[0xff, 0x90]) {
            self.decoder = replacement;
        }
        match decoded {"""),
        INTEGER_DECODER,
    ),
    case(
        "arithmetic_integer_caller_changes_another_bank", "symbol_text", "ArithmeticCodec::decode_integer",
        DECODED_TAIL,
        DECODED_TAIL.replace(
            "match decoded {",
            "self.integers.slots[(slot + 1) % INTEGER_FIELD_COUNT] = [ArithmeticContext::new(); 512];\nmatch decoded {",
        ),
        INTEGER_FRAME,
    ),
    case(
        "arithmetic_integer_caller_clears_iaid_bank", "symbol_text", "ArithmeticCodec::decode_integer",
        DECODED_TAIL, DECODED_TAIL.replace("match decoded {", "self.iaid.clear();\nmatch decoded {"),
        INTEGER_IAID_SITE, INTEGER_IAID_SITE.replace("final(self).iaid@ == old(self).iaid@,", ""),
    ),
    case(
        "arithmetic_iaid_caller_discards_value", "symbol_text", "ArithmeticCodec::decode_symbol_id",
        IAID_RETURN, IAID_RETURN.replace("Ok(value) => Ok(value)", "Ok(value) => Ok(0)"), IAID_RESULT,
    ),
    case(
        "arithmetic_iaid_caller_changes_integer_bank", "symbol_text", "ArithmeticCodec::decode_symbol_id",
        IAID_HEAD, IAID_HEAD.replace(
            "match self",
            "self.integers.slots[0] = [ArithmeticContext::new(); 512];\nmatch self",
        ),
        IAID_FRAME_SITE, IAID_FRAME_SITE.replace("final(self).integers.slots@ == old(self).integers.slots@,", ""),
    ),
    case(
        "context_constructor_loses_invalid_state", "arithmetic", "ArithmeticContext::from_state",
        INVALID_STATE_RETURN,
        "return Err(ArithmeticError::InvalidContextState { state_index: 0, mps });",
        STATE_ERROR_POST,
    ),
    case(
        "context_constructor_loses_invalid_mps", "arithmetic", "ArithmeticContext::from_state",
        INVALID_STATE_RETURN,
        "return Err(ArithmeticError::InvalidContextState { state_index, mps: 0 });",
        STATE_ERROR_POST,
    ),
    case(
        "probability_validation_rejects_valid_bank", "generic_region", "validate_probability_states",
        VALIDATION_BODY,
        replace_once(
            VALIDATION_BODY, "let mut index = 0_usize;",
            "return Err(integer_overflow());\nlet mut index = 0_usize;",
        ),
        VALIDATION_POST,
    ),
    case(
        "probability_validation_loses_first_invalid_state", "generic_region", "validate_probability_states",
        VALIDATION_BODY,
        replace_once(
            VALIDATION_BODY, INVALID_STATE_VALUE,
            INVALID_STATE_VALUE.replace("state_index,", "state_index: 0,"),
        ),
        VALIDATION_POST,
    ),
    case(
        "probability_validation_loses_first_invalid_mps", "generic_region", "validate_probability_states",
        VALIDATION_BODY,
        replace_once(
            VALIDATION_BODY, INVALID_STATE_VALUE,
            INVALID_STATE_VALUE.replace("mps,", "mps: 0,"),
        ),
        VALIDATION_POST,
    ),
    case(
        "probability_validation_reclassifies_invalid_bank", "generic_region", "validate_probability_states",
        VALIDATION_BODY,
        replace_once(
            VALIDATION_BODY, "return Err(GenericRegionError::Decode(error));",
            "return Err(integer_overflow());",
        ),
        VALIDATION_POST,
    ),
    case(
        "probability_failure_loses_context_index", "generic_region", "probability_signal_error",
        SIGNAL_BODY,
        replace_once(SIGNAL_BODY, "index,\ncount: context_count,", "index: 0,\ncount: context_count,"),
        SIGNAL_POST, "ensures true,",
    ),
    case(
        "probability_failure_loses_context_count", "generic_region", "probability_signal_error",
        SIGNAL_BODY,
        replace_once(SIGNAL_BODY, "count: context_count,", "count: 0,"),
        SIGNAL_POST, "ensures true,",
    ),
    case(
        "probability_failure_reclassifies_eof", "generic_region", "probability_signal_error",
        SIGNAL_BODY,
        replace_once(
            SIGNAL_BODY, "decoder.signal_error(signal)",
            "decoder.signal_error(RASTER_SIGNAL_CONTEXT)",
        ),
        SIGNAL_POST, "ensures true,",
    ),
    case(
        "generic_eof_swaps_needed_and_remaining", "generic_region", "unexpected_eof",
        EOF_BODY,
        replace_once(
            EOF_BODY, "offset,\nneeded,\nremaining,",
            "offset,\nneeded: remaining,\nremaining: needed,",
        ),
        EOF_POST, "ensures true,",
    ),
    case(
        "generic_overflow_reclassified_as_eof", "generic_region", "integer_overflow",
        OVERFLOW_BODY,
        replace_once(
            OVERFLOW_BODY,
            "GenericRegionError::Decode(DecodeError::Malformed(MalformedError::IntegerOverflow))",
            "GenericRegionError::Decode(DecodeError::Malformed(MalformedError::UnexpectedEof { offset: 0, needed: 1, remaining: 0 }))",
        ),
        OVERFLOW_POST, "ensures true,",
    ),
    literal_case(
        "huffman_raw_bits_discard_value", "HuffmanCodec::read_raw_bits",
        "Ok(self.reader.read_bits(count)?)",
        "let _discarded = self.reader.read_bits(count)?; Ok(0)",
        "raw_huffman_bits_preserve_exact_value_cursor_error_frame_and_next_field",
    ),
    literal_case(
        "huffman_raw_bits_reclassify_error", "HuffmanCodec::read_raw_bits",
        "Ok(self.reader.read_bits(count)?)",
        """match self.reader.read_bits(count) {
            Ok(value) => Ok(value),
            Err(_) => Err(SymbolTextError::Decode(crate::DecodeError::Malformed(
                MalformedError::IntegerOverflow,
            ))),
        }""",
        "raw_huffman_bits_preserve_exact_value_cursor_error_frame_and_next_field",
    ),
    literal_case(
        "huffman_strip_field_reads_one_bit", "HuffmanCodec::decode_integer",
        "self.read_raw_bits(self.log_strip_size)?", "self.read_raw_bits(1_u8)?",
        "fixed_huffman_integer_fields_match_literal_bits_through_both_adapters",
    ),
    literal_case(
        "huffman_literal_id_reads_one_bit", "HuffmanCodec::decode_symbol_id",
        "self.read_raw_bits(request.code_length)", "self.read_raw_bits(1_u8)",
        "literal_huffman_symbol_ids_match_all_widths_through_both_adapters",
    ),
    literal_case(
        "huffman_integer_caller_selects_indicator", "SymbolCodec::decode_integer",
        "Self::Huffman(codec) => codec.decode_integer(field),",
        "Self::Huffman(codec) => codec.decode_integer(IntegerField::RefinementIndicator),",
        "fixed_huffman_integer_fields_match_literal_bits_through_both_adapters",
    ),
    literal_case(
        "huffman_id_caller_discards_code_length", "SymbolCodec::decode_symbol_id",
        "Self::Huffman(codec) => codec.decode_symbol_id(request),",
        """Self::Huffman(codec) => codec.decode_symbol_id(
            SymbolIdRequest { code_length: 1, ..request }
        ),""",
        "literal_huffman_symbol_ids_match_all_widths_through_both_adapters",
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
            ("src/arithmetic.rs", "src/symbol_text.rs", "src/generic_region.rs",
             "specs/arithmetic.rs", "specs/symbol_text.rs",
             "specs/symbol_text/arithmetic_codec.rs", "specs/generic_region.rs"),
            "jbig2-arithmetic-codec-",
            "Exact fresh arithmetic banks/input/registers and enum integer/IAID result/state composition, plus prior arithmetic projections, generic diagnostics and Huffman literal contracts. Normative field/reset timing, table selection and full entropy/raster streams remain separate",
            baseline_groups=BASELINE_GROUPS,
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"arithmetic codec proof sensitivity failed: {error}", file=sys.stderr)
        report = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Check raw Huffman assignments and total selected-line outcome contracts."""

import argparse
import json
from pathlib import Path
import subprocess
import sys

from proof_sensitivity import check_semantic_cases, replace_once
from source_group import runtime_function


ROOT = Path(__file__).resolve().parents[1]
LINE = runtime_function(ROOT, "huffman", "decode_line")
LINE_BODY = LINE["body_text"]
PEEK = runtime_function(ROOT, "huffman", "peek_prefix")
PEEK_BODY = PEEK["body_text"]
DECODE = runtime_function(ROOT, "huffman", "HuffmanTable::decode")
DECODE_BODY = DECODE["body_text"]
FALLBACK = runtime_function(ROOT, "huffman", "HuffmanTable::decode_fallback")
FALLBACK_BODY = FALLBACK["body_text"]
CACHED_POST = "huffman_spec::cached_decode_outcome(self, old(reader), final(reader), out),"
DECODE_VALUE = """out == huffman_spec::decode_state(
    self, old(reader).bytes@, old(reader).bit_position,
).0,"""
DECODE_CURSOR = """final(reader).bit_position == huffman_spec::decode_state(
    self, old(reader).bytes@, old(reader).bit_position,
).1,"""
FALLBACK_VALUE = """out == huffman_spec::fallback_run(
    self, old(reader).bytes@, old(reader).bit_position,
    1, WideCode { limbs: [0u64, 0, 0, 0] },
).0,"""
FALLBACK_CURSOR = """final(reader).bit_position == huffman_spec::fallback_run(
    self, old(reader).bytes@, old(reader).bit_position,
    1, WideCode { limbs: [0u64, 0, 0, 0] },
).1,"""
FALLBACK_INVARIANT = """huffman_spec::fallback_run(
    self, old(reader).bytes@, old(reader).bit_position,
    1, WideCode { limbs: [0u64, 0, 0, 0] },
) == huffman_spec::fallback_run(
    self, reader.bytes@, reader.bit_position, length, code,
),"""
LINE_VALUE = """out == huffman_spec::line_state(
    line, old(reader).bytes@, old(reader).bit_position,
).0,"""
LINE_CURSOR = """final(reader).bit_position == huffman_spec::line_state(
    line, old(reader).bytes@, old(reader).bit_position,
).1,"""
PEEK_POST = """out == if reader.bit_position + PREFIX_LOOKUP_BITS as nat
    > reader.bytes@.len() * 8 {
    None
} else {
    let index = reader.bit_position / 8;
    let offset = reader.bit_position % 8;
    if offset == 0 {
        Some(reader.bytes@[index as int])
    } else {
        let first = reader.bytes@[index as int] as u16;
        let second = reader.bytes@[(index + 1) as int] as u16;
        Some(((((first << 8) | second) >> ((8 - offset) as usize)) & 0xff) as u8)
    }
},"""
TOTAL_LINE_POST = """match line.kind {
    LineKind::OutOfBand =>
        out == Ok(HuffmanValue::OutOfBand)
            && final(reader).bit_position == old(reader).bit_position,
    _ => match crate::spec::readers::bit_read_error(
        old(reader).bytes@, old(reader).bit_position, line.range_len,
    ) {
        Some(error) =>
            out == Err(HuffmanError::Decode(DecodeError::Malformed(error)))
                && final(reader).bit_position == old(reader).bit_position,
        None => {
            let offset = crate::spec::readers::bits_value(
                old(reader).bytes@, old(reader).bit_position as nat,
                line.range_len as nat,
            ) as int;
            let candidate = if line.kind == LineKind::Lower {
                line.range_low as int - offset
            } else {
                line.range_low as int + offset
            };
            &&& final(reader).bit_position
                == old(reader).bit_position + line.range_len as nat
            &&& out == if i32::MIN as int <= candidate <= i32::MAX as int {
                Ok(HuffmanValue::Integer(candidate as i32))
            } else {
                Err(HuffmanError::Decode(DecodeError::Malformed(
                    MalformedError::IntegerOverflow,
                )))
            }
        },
    },
},"""
RETURN = """Ok(Self {
    lines: owned_lines,
    ordered_lines,
    length_count,
    first_code,
    first_line,
    prefix_lookup,
    prefer_prefix_lookup,
    max_prefix_len,
})"""


def output_case(identity, field, replacement, predicate):
    return {
        "id": identity, "file": "src/huffman.rs", "module": "huffman",
        "function": "HuffmanTable::from_line_source",
        "site": RETURN,
        "replacement": RETURN.replace(f"\n    {field},", f"\n    {field}: {replacement},"),
        "controls": ((f"&& huffman_spec::{predicate}(huffman_spec::raw_lines(lines), &table)", ""),),
    }


def line_case(identity, site, replacement):
    return {
        "id": identity, "file": LINE["file"], "module": "huffman",
        "function": "decode_line", "site": LINE_BODY,
        "replacement": replace_once(LINE_BODY, site, replacement),
        "controls": ((TOTAL_LINE_POST, ""), (LINE_VALUE, ""), (LINE_CURSOR, "")),
        "native_test": "huffman::tests::canonical_decoding::raw_line_outcomes_match_wide_oracle_with_exact_errors_frames_and_acceptance",
    }


def peek_case(identity, site, replacement):
    return {
        "id": identity, "file": PEEK["file"], "module": "huffman",
        "function": "peek_prefix", "site": PEEK_BODY,
        "replacement": replace_once(PEEK_BODY, site, replacement),
        "controls": ((PEEK_POST, ""),),
        "native_test": "huffman::tests::canonical_decoding::prefix_peek_matches_every_byte_window_with_exact_availability_and_no_consumption",
    }


def cached_case(identity, site, replacement):
    return {
        "id": identity, "file": DECODE["file"], "module": "huffman",
        "function": "HuffmanTable::decode", "site": DECODE_BODY,
        "replacement": replace_once(DECODE_BODY, site, replacement),
        "controls": ((CACHED_POST, ""), (DECODE_VALUE, ""), (DECODE_CURSOR, "")),
        "native_test": "huffman::tests::canonical_decoding::fixed_width_cached_table_selects_literal_values_with_exact_consumption",
    }


def fallback_case(identity, site, replacement, test):
    return {
        "id": identity, "file": FALLBACK["file"], "module": "huffman",
        "function": "HuffmanTable::decode_fallback", "site": FALLBACK_BODY,
        "replacement": replace_once(FALLBACK_BODY, site, replacement),
        "controls": ((FALLBACK_VALUE, ""), (FALLBACK_CURSOR, ""), (FALLBACK_INVARIANT, "")),
        "native_test": "huffman::tests::fallback_state::" + test,
    }


CASES = (
    {
        "id": "zero_constructor_sets_low_bit", "file": "src/huffman.rs",
        "module": "huffman", "function": "WideCode::zero",
        "site": "const fn zero() -> Self { Self { limbs: [0; 4] } }",
        "replacement": "const fn zero() -> Self { Self { limbs: [1, 0, 0, 0] } }",
        "controls": ((
            "out.limbs@ == seq![0u64, 0u64, 0u64, 0u64]",
            "out.limbs@[0] <= 1 && out.limbs@[1] == 0 && out.limbs@[2] == 0 && out.limbs@[3] == 0",
        ),),
    },
    output_case(
        "returned_first_code_corrupted", "first_code",
        "{ first_code[usize::from(max_prefix_len)] = WideCode { limbs: [1, 0, 0, 0] }; first_code }",
        "assignment_codes",
    ),
    output_case(
        "returned_bucket_start_corrupted", "first_line",
        "{ first_line[usize::from(max_prefix_len)] = 1; first_line }",
        "assignment_starts",
    ),
    output_case(
        "returned_original_line_order_corrupted", "ordered_lines",
        "{ if !ordered_lines.is_empty() { ordered_lines[0] = 0; } ordered_lines }",
        "assignment_order",
    ),
    line_case(
        "line_falsely_rejects_valid_integer",
        "Ok(HuffmanValue::Integer(value as i32))",
        """{
            let _valid_but_rejected = value;
            Err(HuffmanError::Decode(DecodeError::Malformed(
                MalformedError::IntegerOverflow,
            )))
        }""",
    ),
    line_case(
        "line_falsely_rejects_oob",
        "LineKind::OutOfBand => return Ok(HuffmanValue::OutOfBand),",
        """LineKind::OutOfBand => return Err(HuffmanError::InvalidTable(
            HuffmanTableError::InvalidCode,
        )),""",
    ),
    line_case(
        "line_reclassifies_bit_reader_error",
        "let offset = reader.read_bits(line.range_len)? as i64;",
        """let offset = match reader.read_bits(line.range_len) {
            Ok(value) => value as i64,
            Err(_) => return Err(HuffmanError::InvalidTable(HuffmanTableError::InvalidCode)),
        };""",
    ),
    line_case(
        "line_rolls_back_consumed_overflow_suffix",
        "if value < i32::MIN as i64 || value > i32::MAX as i64 {",
        """if value < i32::MIN as i64 || value > i32::MAX as i64 {
            reader.bit_position = reader.bit_position.saturating_sub(usize::from(line.range_len));""",
    ),
    peek_case(
        "prefix_aligned_value_discarded",
        "return Some(first as u8);",
        "return Some(0_u8);",
    ),
    peek_case(
        "prefix_unaligned_next_byte_discarded",
        "let combined = (first << 8) | second;",
        "let combined = (first << 8) | 0_u16;",
    ),
    peek_case(
        "prefix_exact_eight_bits_falsely_unavailable",
        "if remaining < usize::from(PREFIX_LOOKUP_BITS) {",
        "if remaining <= usize::from(PREFIX_LOOKUP_BITS) {",
    ),
    cached_case(
        "cached_prefix_uses_first_lookup_slot",
        "self.prefix_lookup.get(usize::from(prefix))",
        "self.prefix_lookup.get(0_usize)",
    ),
    cached_case(
        "cached_prefix_selects_first_line",
        ".get(line_index as usize)",
        ".get(0_usize)",
    ),
    cached_case(
        "cached_prefix_discards_consumed_code_length",
        ".checked_add(usize::from(length))",
        ".checked_add(0_usize)",
    ),
    cached_case(
        "cached_prefix_falsely_rejects_selected_line",
        "return decode_line(*line, reader);",
        """return Err(HuffmanError::InvalidTable(HuffmanTableError::InvalidCode));""",
    ),
    fallback_case(
        "fallback_discards_prefix_bits",
        ".append_bit(bit)", ".append_bit(0_u32)",
        "standard_prefix_selection_matches_independent_numeric_assignment_and_frames",
    ),
    fallback_case(
        "fallback_uses_first_ordered_line",
        ".get(ordered_index)", ".get(0_usize)",
        "standard_prefix_selection_matches_independent_numeric_assignment_and_frames",
    ),
    fallback_case(
        "fallback_uses_first_length_bucket",
        "let count = self.length_count[length];",
        "let count = self.length_count[1];",
        "standard_prefix_selection_matches_independent_numeric_assignment_and_frames",
    ),
    fallback_case(
        "fallback_reclassifies_prefix_eof",
        "return Err(HuffmanError::Decode(DecodeError::Malformed(error)));",
        """let _discarded_prefix_error = error;
            return Err(HuffmanError::InvalidTable(HuffmanTableError::InvalidCode));""",
        "longest_literal_prefixes_keep_complete_success_truncation_and_miss_state",
    ),
    fallback_case(
        "fallback_discards_high_prefix_limbs",
        "let count = self.length_count[length];",
        """code.limbs[1] = 0;
        code.limbs[2] = 0;
        code.limbs[3] = 0;
        let count = self.length_count[length];""",
        "nonzero_long_prefixes_select_the_given_wide_code_and_ordered_line",
    ),
    {
        "id": "table_dispatch_falsely_rejects_fallback",
        "file": DECODE["file"], "module": "huffman", "function": "HuffmanTable::decode",
        "site": DECODE_BODY,
        "replacement": replace_once(
            DECODE_BODY, "self.decode_fallback(reader)",
            "Err(HuffmanError::InvalidTable(HuffmanTableError::InvalidCode))",
        ),
        "controls": ((DECODE_VALUE, ""), (DECODE_CURSOR, "")),
        "native_test": "huffman::tests::fallback_state::standard_prefix_selection_matches_independent_numeric_assignment_and_frames",
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
            ("src/huffman.rs", "specs/huffman.rs", "proofs/model_support/huffman.rs",
             "proofs/shared/huffman.rs", "proofs/spec_properties/huffman.rs",
             "proofs/production_impl/huffman.rs"),
            "jbig2-huffman-assignment-",
            "Raw assignment, total selected-line/prefix/cache results and complete fallback/public input-table result/cursor state. Existing weaker controls remove equivalent newly coupled result clauses but retain original reader/success/frame boundaries. Operational matching does not establish canonical cache construction, table-parameter fidelity or independent normative/full-stream approval.",
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"Huffman assignment proof sensitivity failed: {error}", file=sys.stderr)
        report = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

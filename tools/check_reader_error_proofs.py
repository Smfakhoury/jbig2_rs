#!/usr/bin/env python3
"""Check exact bounded reader error fields and exclusion of false failures."""

import argparse
import json
from pathlib import Path
import subprocess
import sys

from proof_sensitivity import check_semantic_cases


ROOT = Path(__file__).resolve().parents[1]
BASELINE_GROUPS = (("reader", (
    "ByteReader::read_bytes", "ByteReader::read_u16",
    "ByteReader::read_u32", "ByteReader::read_u8",
)),)
BYTE_EOF = """MalformedError::UnexpectedEof {
    offset: self.position as u64,
    needed: length as u64,
    remaining: remaining as u64,
}"""
BIT_EOF = """if count_usize > remaining {
    return Err(MalformedError::UnexpectedEof {
        offset: self.bit_position as u64,
        needed: u64::from(count),
        remaining: remaining as u64,
    });
}"""
BIT_HEAD = """pub(crate) fn read_bits(&mut self, count: u8) -> Result<u32, MalformedError> {
    if count > 32 {"""
BIT_POST = """match out {
    Err(error) => Some(error) == crate::spec::readers::bit_read_error(
        old(self).bytes@, old(self).bit_position, count,
    ),
    Ok(_) => true,
},"""


def byte_post(count):
    return f"""match out {{
        Err(error) => error == crate::spec::readers::eof_error(
            old(self).position, {count}, (old(self).bytes@.len() - old(self).position) as usize,
        ),
        Ok(_) => true,
    }},"""


def case(identity, function, site, replacement, control):
    return {
        "id": identity, "file": "src/reader.rs", "module": "reader",
        "function": function, "site": site, "replacement": replacement,
        "controls": ((control, ""),),
    }


U8_CALL = """let bytes = match self.read_bytes(1) {
    Ok(value) => value,
    Err(error) => return Err(error),
};"""
U16_CALL = """let octets = match self.read_bytes(2) {
    Ok(value) => value,
    Err(error) => return Err(error),
};"""
U32_ERROR = """Err(error) => {
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(self.bytes@.len() - self.position < 4);
    }
    return Err(error);
}"""

CASES = (
    case("byte_eof_discards_offset", "ByteReader::read_bytes", BYTE_EOF,
         BYTE_EOF.replace("self.position as u64", "0"), byte_post("length")),
    case("byte_eof_discards_needed_count", "ByteReader::read_bytes", BYTE_EOF,
         BYTE_EOF.replace("length as u64", "0"), byte_post("length")),
    case("byte_eof_discards_remaining_count", "ByteReader::read_bytes", BYTE_EOF,
         BYTE_EOF.replace("remaining as u64", "0"), byte_post("length")),
    case("byte_u8_error_becomes_overflow", "ByteReader::read_u8", U8_CALL,
         U8_CALL.replace("return Err(error)", "return Err(MalformedError::IntegerOverflow)"), byte_post("1")),
    case("byte_u16_error_becomes_overflow", "ByteReader::read_u16", U16_CALL,
         U16_CALL.replace("return Err(error)", "return Err(MalformedError::IntegerOverflow)"), byte_post("2")),
    case("byte_u32_error_becomes_overflow", "ByteReader::read_u32", U32_ERROR,
         U32_ERROR.replace("return Err(error)", "return Err(MalformedError::IntegerOverflow)"), byte_post("4")),
    case("bit_invalid_count_discards_value", "BitReader::read_bits",
         BIT_HEAD + "\nreturn Err(MalformedError::InvalidBitCount(count));\n}",
         BIT_HEAD + "\nreturn Err(MalformedError::InvalidBitCount(0));\n}", BIT_POST),
    case("bit_eof_becomes_overflow", "BitReader::read_bits", BIT_EOF,
         "if count_usize > remaining { return Err(MalformedError::IntegerOverflow); }", BIT_POST),
    case("bit_reader_rejects_valid_32_bit_read", "BitReader::read_bits", BIT_HEAD,
         BIT_HEAD.replace("count > 32", "count >= 32"), BIT_POST),
)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--verus-executable", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    try:
        report = check_semantic_cases(
            ROOT, arguments.verus_executable.resolve(), CASES,
            ("src/reader.rs", "specs/readers.rs", "proofs/spec_properties/readers.rs",
             "proofs/production_impl/readers.rs"),
            "jbig2-reader-errors-",
            "Exact project byte/bit reader failure fields, count priority, no false overflow or false valid-width rejection; original input/value/cursor bounds remain",
            baseline_groups=BASELINE_GROUPS,
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"reader error proof sensitivity failed: {error}", file=sys.stderr)
        report = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

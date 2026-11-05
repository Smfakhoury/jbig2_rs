#!/usr/bin/env python3
"""Check exact stored session queries and existing strict arithmetic framing policy."""

import argparse
import json
from pathlib import Path
import subprocess
import sys

from proof_sensitivity import check_semantic_cases, replace_once
from source_group import runtime_function


ROOT = Path(__file__).resolve().parents[1]
FILES = (
    "src/generic_region.rs", "src/arithmetic.rs", "specs/generic_region.rs",
    "specs/generic_region/framing.rs", "specs/arithmetic.rs", "proofs/shared/generic_region.rs",
)
BASELINE_GROUPS = (("generic_region", (
    "GenericArithmeticSession::new", "GenericArithmeticSession::consumed_byte_offset",
    "GenericArithmeticSession::observed_marker_offset",
    "GenericArithmeticSession::validate_final_marker",
    "validate_arithmetic_terminator", "validate_observed_marker",
)),)
TEST_PREFIX = "generic_region::tests::segment_framing::"
SESSION_TEST = "session_queries_and_marker_results_match_stored_metadata_without_mutation"
TAIL_TEST = "strict_terminator_matches_every_byte_pair_and_first_error_priority"
PREFIX_INVARIANT = """forall|before: int| 0 <= before < offset ==>
    !generic_region_spec::early_arithmetic_marker(data@, before),"""


def case(function, identity, site, replacement, lost, weaker="true", *,
         test=SESSION_TEST, drop=()):
    row = runtime_function(ROOT, "generic_region", function)
    attributes = [attribute for attribute in row["attributes"] if "verus_spec(" in attribute]
    if len(attributes) != 1:
        raise ValueError("session/framing target needs one exact attached contract")
    body = row["body_text"]
    mutant = replace_once(body, site, replacement)
    controls = ((attributes[0], replace_once(attributes[0], lost, weaker)),)
    if drop:
        weak_body = mutant
        for invariant in drop:
            weak_body = replace_once(weak_body, invariant, "")
        controls += ((mutant, weak_body),)
    return {
        "id": identity, "file": row["file"], "module": "generic_region",
        "function": function, "site": body, "replacement": mutant,
        "controls": controls, "native_test": TEST_PREFIX + test,
    }


FINAL = """out == generic_region_spec::final_marker_result(
    self.decoder.marker, self.coded_length,
)"""
OBSERVED = "out == generic_region_spec::observed_marker_result(decoder.marker, coded_length)"
TERMINATOR = "generic_region_spec::arithmetic_terminator_validation(data@, out)"
TAIL_ONLY = "out.is_ok() ==> generic_region_spec::bounded_arithmetic_tail(data@)"
CASES = (
    case("GenericArithmeticSession::consumed_byte_offset", "consumed_offset_discarded",
         "self.decoder.byte_position()", "0", "out == self.decoder.byte_position"),
    case("GenericArithmeticSession::observed_marker_offset", "observed_offset_discarded",
         "self.decoder.marker().map", "None::<crate::arithmetic::ArithmeticMarker>.map",
         """out == match self.decoder.marker {
             Some(marker) => Some(marker.offset),
             None => None,
         }"""),
    case("GenericArithmeticSession::validate_final_marker", "final_offset_wrong",
         ".checked_sub(2)", ".checked_sub(1)", FINAL),
    case("GenericArithmeticSession::validate_final_marker", "final_observed_error_ignored",
         "validate_observed_marker(&self.decoder, self.coded_length)?;",
         "let _unchecked = &self.decoder;", FINAL),
    case("validate_observed_marker", "observed_code_ignored",
         "marker.code != ARITHMETIC_TERMINATOR[1]", "false", OBSERVED),
    case("validate_observed_marker", "observed_offset_ignored",
         "marker.offset != expected_offset", "false", OBSERVED),
    case("validate_arithmetic_terminator", "interior_90_accepted",
         "data[offset + 1] > 0x8f", "data[offset + 1] > 0x90",
         TERMINATOR, TAIL_ONLY, test=TAIL_TEST, drop=(PREFIX_INVARIANT,)),
    case("validate_arithmetic_terminator", "stuffed_8f_rejected",
         "data[offset + 1] > 0x8f", "data[offset + 1] > 0x8e",
         TERMINATOR, TAIL_ONLY, test=TAIL_TEST),
    case("validate_arithmetic_terminator", "bad_tail_error_substituted",
         "return Err(GenericRegionError::InvalidArithmeticTerminator);",
         "return Err(GenericRegionError::UnexpectedArithmeticMarker { offset: 0, code: 0 });",
         TERMINATOR, TAIL_ONLY, test=TAIL_TEST),
    case("validate_arithmetic_terminator", "interior_error_offset_shifted",
         """offset,
             code: data[offset + 1],""",
         """offset: offset + 1,
             code: data[offset + 1],""",
         TERMINATOR, TAIL_ONLY, test=TAIL_TEST),
    case("GenericArithmeticSession::new", "fresh_coded_length_discarded",
         "coded_length: coded.len(),", "coded_length: 0,",
         "&& session.coded_length == coded@.len()", "",
         test="fresh_sessions_preserve_supplied_stream_configuration_and_limits"),
    case("GenericArithmeticSession::new", "fresh_work_counter_charged",
         "work_used: 0,", "work_used: 1,", "&& session.work_used == 0", "",
         test="fresh_sessions_preserve_supplied_stream_configuration_and_limits"),
)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--verus-executable", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    try:
        report = check_semantic_cases(
            ROOT, arguments.verus_executable.resolve(), CASES,
            FILES,
            "jbig2-session-framing-",
            "Exact session stored offsets, supplied constructor metadata and existing strict "
            "tail/first-marker/error-priority policy. Internal arbitrary metadata is not a public "
            "compressed-stream witness; no full raster, normative framing fidelity or recovery claim.",
            baseline_groups=BASELINE_GROUPS,
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"session framing sensitivity failed: {error}", file=sys.stderr)
        report = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

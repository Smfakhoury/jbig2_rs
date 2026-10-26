#!/usr/bin/env python3
"""Check exact PageState metadata transitions with individual compiling controls."""

import argparse
import json
from pathlib import Path
import subprocess
import sys

from proof_sensitivity import check_semantic_cases, replace_once
from rust_source import source_functions


ROOT = Path(__file__).resolve().parents[1]
FUNCTIONS = {entry["id"]: entry for entry in source_functions(ROOT)}
NEW = "ensures out.info == info, out.last_stripe == None, !out.ended,"
STRIPE_RESULT = "out == document_spec::stripe_result(old(self).info, old(self).last_stripe, row)"
STRIPE = f"""ensures
    {STRIPE_RESULT},
    final(self).info == old(self).info,
    final(self).ended == old(self).ended,
    final(self).last_stripe == if out is Ok {{ Some(row) }} else {{ old(self).last_stripe }},"""
FINISH_RESULT = """out == document_spec::page_finish_result(
    old(self).info.height, old(self).last_stripe, old(self).ended,
)"""
FINISH = f"""ensures
    {FINISH_RESULT},
    final(self).info == old(self).info,
    final(self).last_stripe == old(self).last_stripe,
    final(self).ended == (old(self).ended || out is Ok),"""
HEIGHT = """match out {
    Ok(page) => document_spec::public_page_height(self.info.height, self.last_stripe)
        == Ok(page.height),
    Err(error) => document_spec::public_page_height(self.info.height, self.last_stripe)
        == Err(error),
}"""
FIELDS = "document_spec::public_page_fields(self.info, number, page)"
TESTS = {
    "new": "raw_page_state_new_preserves_all_information_and_initial_boundaries",
    "add_stripe": "raw_page_state_stripes_match_wide_boundaries_and_complete_failure_frames",
    "finish": "raw_page_state_finish_preserves_frames_and_duplicate_error_priority",
    "into_public": "raw_page_state_public_metadata_matches_known_and_wide_inferred_heights",
}


def case(identity, method, before, after, contract, clause):
    original = FUNCTIONS[f"document::PageState::{method}"]["body_text"]
    return {
        "id": identity, "file": "src/document.rs", "module": "document",
        "function": f"PageState::{method}", "site": original,
        "replacement": replace_once(original, before, after),
        "controls": ((contract, replace_once(contract, clause, "true")),),
        "test": "document::tests::" + TESTS[method],
    }


CASES = (
    case("new_changes_information", "new", "Self { info,",
         "let mut info = info; info.width ^= 1; Self { info,",
         NEW, "out.info == info"),
    case("new_invents_stripe", "new", "last_stripe: None", "last_stripe: Some(0)",
         NEW, "out.last_stripe == None"),
    case("new_already_ended", "new", "ended: false", "ended: true", NEW, "!out.ended"),
    case("stripe_accepts_unstriped", "add_stripe", "if !self.info.striped {", "if false {",
         STRIPE, STRIPE_RESULT),
    case("stripe_rejects_first_distance_equality", "add_stripe",
         "row > u32::from(self.info.max_stripe_size)", "row >= u32::from(self.info.max_stripe_size)",
         STRIPE, STRIPE_RESULT),
    case("stripe_accepts_equal_row", "add_stripe", "row <= previous", "row < previous",
         STRIPE, STRIPE_RESULT),
    case("stripe_rejects_later_distance_equality", "add_stripe",
         "row - previous > u32::from(self.info.max_stripe_size)",
         "row - previous >= u32::from(self.info.max_stripe_size)", STRIPE, STRIPE_RESULT),
    case("stripe_accepts_declared_height", "add_stripe", "row >= height", "row > height",
         STRIPE, STRIPE_RESULT),
    case("stripe_discards_error_row", "add_stripe",
         """if !self.info.striped {
             return Err(MalformedError::InvalidStripeEndRow(row).into());""",
         """if !self.info.striped {
             return Err(MalformedError::InvalidStripeEndRow(0).into());""", STRIPE, STRIPE_RESULT),
    case("stripe_drops_successful_update", "add_stripe", "self.last_stripe = Some(row);", "",
         STRIPE, "final(self).last_stripe == if out is Ok { Some(row) } else { old(self).last_stripe }"),
    case("stripe_changes_state_on_error", "add_stripe", "if !self.info.striped {",
         "if !self.info.striped { self.last_stripe = Some(row);", STRIPE,
         "final(self).last_stripe == if out is Ok { Some(row) } else { old(self).last_stripe }"),
    case("stripe_changes_information", "add_stripe", "if !self.info.striped {",
         "self.info.width ^= 1; if !self.info.striped {", STRIPE, "final(self).info == old(self).info"),
    case("stripe_changes_ended", "add_stripe", "if !self.info.striped {",
         "self.ended = !self.ended; if !self.info.striped {", STRIPE,
         "final(self).ended == old(self).ended"),
    case("finish_accepts_duplicate", "finish", "if self.ended {", "if false {", FINISH, FINISH_RESULT),
    case("finish_changes_duplicate_priority", "finish",
         """return Err(MalformedError::DuplicatePageBoundary {
             page: 0, segment_type: 49,
         }
         .into());""",
         "return Err(MalformedError::InvalidStripeEndRow(0).into());", FINISH, FINISH_RESULT),
    case("finish_accepts_unknown_without_stripe", "finish",
         "if self.info.height.is_none() && self.last_stripe.is_none() {", "if false {",
         FINISH, FINISH_RESULT),
    case("finish_drops_successful_update", "finish", "self.ended = true;", "",
         FINISH, "final(self).ended == (old(self).ended || out is Ok)"),
    case("finish_changes_ended_on_error", "finish", "if self.ended {",
         "if self.ended { self.ended = false;", FINISH,
         "final(self).ended == (old(self).ended || out is Ok)"),
    case("finish_changes_information", "finish", "if self.ended {",
         "self.info.width ^= 1; if self.ended {", FINISH, "final(self).info == old(self).info"),
    case("finish_changes_last_stripe", "finish", "self.ended = true;",
         "self.last_stripe = None; self.ended = true;", FINISH,
         "final(self).last_stripe == old(self).last_stripe"),
    case("public_discards_known_height", "into_public", "(Some(height), _) => height,",
         "(Some(_height), _) => 0,", HEIGHT, HEIGHT),
    case("public_omits_inclusive_row_increment", "into_public", "last.checked_add(1)",
         "last.checked_add(0)", HEIGHT, HEIGHT),
    case("public_changes_overflow_error", "into_public", ".ok_or(MalformedError::IntegerOverflow)?",
         ".ok_or(MalformedError::InvalidStripeEndRow(0))?", HEIGHT, HEIGHT),
    case("public_changes_missing_stripe_error", "into_public",
         "(None, None) => return Err(MalformedError::InvalidStripeEndRow(0).into()),",
         "(None, None) => return Err(MalformedError::IntegerOverflow.into()),", HEIGHT, HEIGHT),
)

for field, replacement in (
    ("number", "number: 0"), ("width", "width: 0"),
    ("x_resolution", "x_resolution: self.info.y_resolution"),
    ("y_resolution", "y_resolution: self.info.x_resolution"),
    ("striped", "striped: !self.info.striped"),
):
    before = "number," if field == "number" else f"{field}: self.info.{field},"
    entry = case(f"public_changes_{field}", "into_public", before, replacement + ",", FIELDS, FIELDS)
    remaining = [
        "page.number == number" if name == "number" else f"page.{name} == self.info.{name}"
        for name in ("number", "width", "x_resolution", "y_resolution", "striped") if name != field
    ]
    entry["controls"] = ((FIELDS, " && ".join(remaining)),)
    CASES += (entry,)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--verus-executable", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    try:
        report = check_semantic_cases(
            ROOT, arguments.verus_executable.resolve(), CASES,
            ("src/document.rs", "specs/document.rs", "proofs/model_support/document.rs"),
            "jbig2-page-state-",
            "Exact existing PageState transitions, typed errors, failure frames and public metadata; no raster, caller lifecycle or independent normative approval",
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"page state proof sensitivity failed: {error}", file=sys.stderr)
        report = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

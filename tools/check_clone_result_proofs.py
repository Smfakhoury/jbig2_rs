#!/usr/bin/env python3
"""Check actual public/configuration/header clone values and complete vector contents."""

import argparse
import json
from pathlib import Path
import subprocess
import sys

from proof_sensitivity import check_semantic_cases, replace_once
from rust_source import direct_contract_sections
from source_group import runtime_function


ROOT = Path(__file__).resolve().parents[1]
BASELINE_GROUPS = (
    ("generic_region", ("GenericRegionHeader::clone", "RefinementRegionHeader::clone")),
    ("segment_header", ("RetentionFlags::clone", "SegmentHeader::clone")),
)
FILES = (
    "src/document.rs", "src/generic_region.rs", "src/segment_header.rs",
    "specs/document.rs", "specs/generic_region.rs", "specs/segments.rs",
    "proofs/shared/document.rs", "proofs/shared/generic_region.rs",
)


def case(component, owner, identity, edits, test):
    row = runtime_function(ROOT, component, owner + "::clone")
    clauses = direct_contract_sections(row, "ensures")
    if len(clauses) != 1:
        raise ValueError("clone needs one actual direct owning result clause")
    body = row["body_text"]
    mutant = body
    for before, after in edits:
        mutant = replace_once(mutant, before, after)
    weaker = replace_once(mutant, "ensures" + clauses[0], "ensures(|_out: Self| true)")
    return {
        "id": identity, "file": row["file"], "module": component,
        "function": owner + "::clone", "site": body, "replacement": mutant,
        "controls": ((mutant, weaker),),
        "native_test": test,
    }


GENERIC_TEST = (
    "generic_region::tests::headers_and_prediction::"
    "generic_header_clone_copies_every_flag_region_and_adaptive_slot"
)
REFINEMENT_TEST = (
    "generic_region::tests::headers_and_prediction::"
    "refinement_header_clone_copies_every_field_and_both_adaptive_slots"
)
RETENTION_TEST = (
    "segment_header::tests::retention_clone_keeps_arbitrary_bytes_and_independent_owned_storage"
)
SEGMENT_TEST = (
    "segment_header::tests::segment_header_clone_keeps_all_scalars_vectors_and_nested_retention"
)
CASES = (
    case("document", "Decoder", "decoder_clone_discards_limits",
         (("limits: self.limits,", "limits: Limits::default(),"),),
         "document::tests::container_inspection::"
         "decoder_clone_preserves_all_supplied_limits_and_independent_configuration"),
    case("generic_region", "GenericRegionHeader", "generic_clone_flips_mmr",
         (("mmr: self.mmr,", "mmr: !self.mmr,"),), GENERIC_TEST),
    case("generic_region", "GenericRegionHeader", "generic_clone_discards_adaptive_slots",
         (("adaptive: self.adaptive,", "adaptive: [TemplateOffset::new(0, 0); 12],"),), GENERIC_TEST),
    case("generic_region", "RefinementRegionHeader", "refinement_clone_discards_template",
         (("template: self.template,", "template: RefinementTemplate::Template0,"),), REFINEMENT_TEST),
    case("generic_region", "RefinementRegionHeader", "refinement_clone_discards_adaptive_slots",
         (("adaptive: self.adaptive,", "adaptive: [TemplateOffset::new(0, 0); 2],"),), REFINEMENT_TEST),
    case("segment_header", "RetentionFlags", "retention_clone_discards_values",
         (("values: self.values.clone(),", "values: Vec::new(),"),), RETENTION_TEST),
    case("segment_header", "RetentionFlags", "retention_clone_corrupts_first_value",
         (
             ("let out = Self {", "let mut out = Self {"),
             ("#[cfg(verus_keep_ghost)]\n        proof!",
              """if !out.values.is_empty() { out.values[0] = 0; }
              #[cfg(verus_keep_ghost)]
              proof!"""),
         ), RETENTION_TEST),
    case("segment_header", "SegmentHeader", "segment_clone_discards_number",
         (("number: self.number,", "number: 0,"),), SEGMENT_TEST),
    case("segment_header", "SegmentHeader", "segment_clone_discards_references",
         (("referred_segments: self.referred_segments.clone(),",
           "referred_segments: Vec::new(),"),), SEGMENT_TEST),
    case("segment_header", "SegmentHeader", "segment_clone_discards_nested_retention",
         (("retention_flags: self.retention_flags.clone(),",
           "retention_flags: RetentionFlags { values: Vec::new() },"),), SEGMENT_TEST),
)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--verus-executable", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    try:
        report = check_semantic_cases(
            ROOT, arguments.verus_executable.resolve(), CASES, FILES, "jbig2-clone-results-",
            "Actual five non-Copy Clone result values/metadata and complete vector contents."
            " No new input preconditions, allocator promises, private-field visibility or"
            " assumptions. Closed logical views preserve runtime privacy; native internal"
            " header states are not public compressed-stream witnesses.",
            baseline_groups=BASELINE_GROUPS,
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"clone result sensitivity failed: {error}", file=sys.stderr)
        report = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

import copy
from pathlib import Path
import sys
import tempfile
import unittest


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from check_traceability import check_contract, runtime_inventory
from readiness_assessment import assess, direct_postconditions
from rust_source import contract_sections


class ReadinessAssessmentTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        (self.root / "src").mkdir()
        self.path = self.root / "src/lib.rs"
        self.path.write_text("""
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires input < 10,
    ensures out == input,
))]
fn mapped(input: u8) -> u8 { input }
#[cfg_attr(verus_keep_ghost, verus_verify)]
fn body_only() {}
fn plain() {}
""")
        self.runtime = runtime_inventory(self.root)
        self.ledger = {
            "requirements": [{
                "id": "RULE", "runtime": ["lib::mapped"],
                "functional_evidence": [{"runtime": "lib::mapped"}],
                "invariant_evidence": [], "strength": "functional_contract",
                "gaps": ["Statement denominator and source fidelity remain unqualified."],
            }],
            "features": [{
                "feature": "fixture", "decomposition": "partial",
                "requirement_ids": ["RULE"], "remaining_scope": ["Other statements."],
            }],
        }

    def test_explicit_denominator_and_links_do_not_grant_readiness(self):
        result = assess(self.ledger, self.runtime, {"jbig2_rs_production::mapped"})
        self.assertEqual(result["code"]["explicit_declarations"], 3)
        self.assertEqual(result["code"]["with_requirement_links"], 1)
        self.assertEqual(result["code"]["with_direct_functional_evidence_links"], 1)
        mapped = next(row for row in result["code"]["declarations"] if row["id"] == "lib::mapped")
        self.assertEqual(mapped["contract_surface"], "local_postcondition_surface")
        self.assertEqual(mapped["proof_identity_status"], "matched_verifier_name")
        self.assertFalse(result["ready_for_external_validation"])
        self.assertIsNone(result["readiness_percentage"])
        self.assertFalse(result["code"]["complete_production_inventory_qualified"])
        self.assertFalse(result["code"]["semantic_contract_quality_qualified"])
        self.assertFalse(result["code"]["public_entry_reachability_qualified"])

    def test_body_verification_and_plain_functions_do_not_gain_postconditions(self):
        result = assess(self.ledger, self.runtime, {"jbig2_rs_production::body_only"})
        rows = {row["id"]: row for row in result["code"]["declarations"]}
        self.assertEqual(rows["lib::body_only"]["contract_surface"], "no_local_postcondition_surface")
        self.assertEqual(rows["lib::plain"]["contract_surface"], "no_local_postcondition_surface")
        self.assertFalse(rows["lib::body_only"]["direct_functional_evidence_links"])
        self.assertEqual(rows["lib::plain"]["proof_identity_status"], "no_verified_name_match")

    def test_missing_verifier_input_is_not_a_proof_match(self):
        result = assess(self.ledger, self.runtime, None)
        self.assertEqual(result["code"]["proof_identity_statuses"], {"not_checked": 3})

    def test_accounting_boundaries_are_not_silently_counted_as_missing_proofs(self):
        accounting = {
            identity: {
                "file": record["file"], "file_sha256": record["file_sha256"],
                "status": "reviewed-verifier-limitation", "disposition_reason": "Callback boundary.",
            }
            for identity, record in self.runtime.items()
        }
        result = assess(self.ledger, self.runtime, None, accounting)
        self.assertEqual(result["code"]["accounting_statuses"], {"reviewed-verifier-limitation": 3})
        self.assertEqual(result["code"]["no_local_postcondition_by_accounting_status"], {
            "reviewed-verifier-limitation": 2,
        })
        self.assertTrue(all(
            row["reviewed_boundary_reason"] == "Callback boundary."
            for row in result["code"]["declarations"]
        ))

    def test_stale_or_missing_accounting_never_inherits_a_validated_status(self):
        record = self.runtime["lib::mapped"]
        accounting = {"lib::mapped": {
            "file": record["file"], "file_sha256": "0" * 64,
            "status": "verified", "disposition_reason": None,
        }}
        result = assess(self.ledger, self.runtime, None, accounting)
        self.assertEqual(result["code"]["accounting_statuses"], {
            "stale_or_missing_accounting_identity": 3,
        })
        self.assertFalse(result["ready_for_external_validation"])

    def test_feature_completion_does_not_invent_source_statement_count(self):
        ledger = copy.deepcopy(self.ledger)
        ledger["features"][0].update(decomposition="complete", remaining_scope=[])
        result = assess(ledger, self.runtime, None)
        self.assertFalse(result["statements"]["statement_inventory_qualified"])
        self.assertIsNone(result["statements"]["exhaustive_statement_count"])
        self.assertIsNone(result["statements"]["statement_coverage_percentage"])
        self.assertFalse(result["ready_for_external_validation"])

    def test_unmapped_functions_and_remaining_worklists_are_visible(self):
        result = assess(self.ledger, self.runtime, None)
        rows = {row["id"]: row for row in result["code"]["declarations"]}
        self.assertEqual(rows["lib::plain"]["requirement_links"], [])
        self.assertEqual(result["statements"]["with_explicit_gap_notes"], 1)
        self.assertEqual(result["statements"]["remaining_feature_worklists"][0]["feature"], "fixture")
        self.assertEqual(sum(m["explicit_declarations"] for m in result["code"]["modules"]), 3)

    def test_unknown_links_and_escaping_evidence_fail_closed(self):
        for field in ("runtime", "functional_evidence"):
            ledger = copy.deepcopy(self.ledger)
            if field == "runtime":
                ledger["requirements"][0][field] = ["lib::absent"]
            else:
                ledger["requirements"][0][field][0]["runtime"] = "lib::plain"
            with self.subTest(field=field), self.assertRaises(ValueError):
                assess(ledger, self.runtime, None)
        with self.assertRaisesRegex(ValueError, "actual production"):
            assess(self.ledger, {}, None)

    def test_duplicate_links_do_not_inflate_function_counts(self):
        self.ledger["requirements"].append(copy.deepcopy(self.ledger["requirements"][0]))
        result = assess(self.ledger, self.runtime, None)
        self.assertEqual(result["code"]["with_direct_functional_evidence_links"], 1)
        self.assertEqual(result["code"]["declarations"][0]["requirement_links"], ["RULE"])

    def test_trait_ownership_and_shared_verifier_names_remain_explicit(self):
        self.path.write_text("""
struct Failure;
#[cfg_attr(verus_keep_ghost, verus_verify)]
impl From<u8> for Failure { fn from(_: u8) -> Self { Self } }
#[cfg_attr(verus_keep_ghost, verus_verify)]
impl From<u16> for Failure { fn from(_: u16) -> Self { Self } }
""")
        ledger = {"requirements": [], "features": []}
        result = assess(ledger, runtime_inventory(self.root), {"jbig2_rs_production::Failure::from"})
        code = result["code"]
        self.assertEqual(code["explicit_declarations"], 2)
        self.assertEqual(code["contract_surfaces"], {
            "trait_implementation_without_local_postcondition": 2,
        })
        self.assertEqual(code["proof_identity_statuses"], {
            "shared_verifier_name_requires_disambiguation": 2,
        })
        self.assertEqual(len(code["shared_verifier_names"]["jbig2_rs_production::Failure::from"]), 2)

    def test_nested_closure_ensures_is_not_an_owning_function_postcondition(self):
        self.path.write_text("""
fn outer() {
    let callback = |value: u8| {
        #[cfg(verus_keep_ghost)]
        ensures(|out: u8| equal(out, value));
        value
    };
}
""")
        record = next(iter(runtime_inventory(self.root).values()))
        self.assertEqual(direct_postconditions(record), 0)

    def test_direct_legacy_ensures_is_measured_but_not_qualified(self):
        self.path.write_text("""
fn legacy() -> u8 {
    #[cfg(verus_keep_ghost)]
    ensures(|out: u8| equal(out, 0));
    0
}
""")
        record = next(iter(runtime_inventory(self.root).values()))
        self.assertEqual(direct_postconditions(record), 1)
        self.assertEqual(contract_sections(record, "ensures"), [])

    def test_comment_and_literal_section_markers_cannot_create_guarantees(self):
        for fake in ('/* ensures out == 0 */', '"ensures out == 0"'):
            record = {
                "id": "fixture",
                "attributes": [f"#[cfg_attr(verus_keep_ghost, verus_spec(out => requires {fake},))]"],
            }
            with self.subTest(fake=fake):
                self.assertEqual(contract_sections(record, "ensures"), [])
                with self.assertRaisesRegex(ValueError, "not present"):
                    check_contract(record, "ensures", ["out == 0"])

    def test_keywords_in_result_literals_do_not_truncate_real_sections(self):
        record = {
            "id": "fixture",
            "attributes": [
                '#[cfg_attr(verus_keep_ghost, verus_spec(out => '
                'ensures out == "requires ensures decreases", decreases 1,))]',
            ],
        }
        check_contract(record, "ensures", ['out == "requires ensures decreases"'])
        self.assertNotIn("decreases 1", contract_sections(record, "ensures")[0])
        with self.assertRaisesRegex(ValueError, "unknown"):
            contract_sections(record, "missing")

    def test_literal_true_is_only_a_syntactic_review_candidate(self):
        self.path.write_text("""
#[cfg_attr(verus_keep_ghost, verus_spec(out => ensures true,))]
fn vacuous() {}
""")
        result = assess({"requirements": [], "features": []}, runtime_inventory(self.root), None)
        self.assertTrue(result["code"]["declarations"][0]["literal_true_postcondition_candidate"])
        self.assertFalse(result["code"]["semantic_contract_quality_qualified"])
        self.assertFalse(result["ready_for_external_validation"])


if __name__ == "__main__":
    unittest.main()

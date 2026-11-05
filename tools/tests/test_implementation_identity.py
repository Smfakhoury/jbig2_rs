import copy
from pathlib import Path
import sys
import tempfile
import unittest


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from check_traceability import runtime_inventory
from implementation_identity import qualify
from readiness_assessment import assess


class ImplementationIdentityTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        (self.root / "src").mkdir()
        (self.root / "src/lib.rs").write_text("""
struct Failure;
#[cfg_attr(verus_keep_ghost, verus_verify)]
impl From<u8> for Failure { fn from(_: u8) -> Self { Self } }
#[cfg_attr(verus_keep_ghost, verus_verify)]
impl From<u16> for Failure { fn from(_: u16) -> Self { Self } }
""")
        self.runtime = runtime_inventory(self.root)
        self.assessment = assess(
            {"requirements": [], "features": []}, self.runtime,
            {"jbig2_rs_production::Failure::from"},
        )
        digest = next(iter(self.runtime.values()))["file_sha256"]
        self.reports = {"fixture": {
            "status": "passed", "production_sources_modified": False,
            "source_bindings": {"src/lib.rs": digest},
            "counterexamples": [
                {
                    "id": trait, "function": "Failure::from", "source_file": "src/lib.rs",
                    "verification_module": "", "impl_trait": trait,
                    "selected_impl_traits": ["From<u16>", "From<u8>"],
                    "normal_rust_compiles": True, "strong_contract": "rejected",
                    "weaker_control": "accepted", "strong_verifier_errors": 1,
                    "control_verifier_errors": 0, "strong_verified_functions": 1,
                    "control_verified_functions": 2,
                    "baseline_selected_functions": ["Failure::from"],
                    "baseline_selected_verification": {
                        "verified": 2, "errors": 0, "encountered-error": False,
                        "encountered-vir-error": False, "is-verifying-entire-crate": False,
                    },
                } for trait in ["From<u8>", "From<u16>"]
            ],
        }}

    def result(self, reports=None):
        return qualify(self.runtime, self.assessment, self.reports if reports is None else reports)

    def test_exact_fresh_peer_controls_qualify_each_source_impl_not_readiness(self):
        result = self.result()
        self.assertEqual(result["shared_declarations"], 2)
        self.assertEqual(result["qualified_shared_implementations"], 2)
        self.assertEqual(result["remaining_unqualified_shared_implementations"], [])
        self.assertFalse(result["ready_for_external_validation"])
        self.assertEqual({row["impl_trait"] for row in result["implementations"]}, {
            "From<u8>", "From<u16>",
        })
        self.assertTrue(all(row["file_sha256"] and row["semantic_controls"]
                            for row in result["implementations"]))

    def test_missing_implementation_or_receipts_fail_closed(self):
        for reports in ({}, {"fixture": {**self.reports["fixture"], "counterexamples": []}}):
            with self.subTest(reports=reports), self.assertRaisesRegex(ValueError, "lack exact"):
                self.result(reports)
        self.reports["fixture"]["counterexamples"].pop()
        with self.assertRaisesRegex(ValueError, "lack exact"):
            self.result()

    def test_file_scope_trait_or_function_substitution_does_not_inherit_name_credit(self):
        for key, value in (
            ("source_file", "src/other.rs"), ("verification_module", "other"),
            ("impl_trait", "From<u32>"), ("function", "Other::from"),
        ):
            reports = copy.deepcopy(self.reports)
            reports["fixture"]["counterexamples"][0][key] = value
            with self.subTest(key=key), self.assertRaisesRegex(ValueError, "lack exact"):
                self.result(reports)

    def test_partial_or_misordered_peer_lists_cannot_qualify(self):
        for traits in (["From<u8>"], ["From<u8>", "From<u16>"], ["From<u16>", "From<u8>", "From<u32>"]):
            reports = copy.deepcopy(self.reports)
            reports["fixture"]["counterexamples"][0]["selected_impl_traits"] = traits
            with self.subTest(traits=traits), self.assertRaisesRegex(ValueError, "incomplete"):
                self.result(reports)

    def test_stale_source_hash_is_not_a_valid_identity(self):
        reports = copy.deepcopy(self.reports)
        reports["fixture"]["source_bindings"]["src/lib.rs"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "incomplete"):
            self.result(reports)

    def test_compiler_failure_or_nonsemantic_or_missing_peer_controls_fail(self):
        for key, value in (
            ("normal_rust_compiles", False), ("strong_contract", "compiler_error"),
            ("weaker_control", "rejected"), ("strong_verifier_errors", 0),
            ("strong_verified_functions", 0), ("control_verified_functions", 1),
            ("control_verifier_errors", 1), ("baseline_selected_functions", ["other"]),
            ("strong_verifier_errors", True), ("control_verified_functions", True),
        ):
            reports = copy.deepcopy(self.reports)
            reports["fixture"]["counterexamples"][0][key] = value
            with self.subTest(key=key, value=value), self.assertRaisesRegex(ValueError, "incomplete"):
                self.result(reports)

    def test_whole_name_match_cannot_replace_exact_selected_positive(self):
        reports = copy.deepcopy(self.reports)
        reports["fixture"]["counterexamples"][0]["baseline_selected_verification"]["whole_positive"] = {}
        with self.assertRaisesRegex(ValueError, "fresh exact"):
            self.result(reports)

    def test_failed_partial_or_missing_positive_metadata_fails(self):
        for key, value in (
            ("verified", 1), ("verified", True), ("errors", 1),
            ("encountered-error", True), ("encountered-vir-error", True),
            ("is-verifying-entire-crate", True),
        ):
            reports = copy.deepcopy(self.reports)
            reports["fixture"]["counterexamples"][0]["baseline_selected_verification"][key] = value
            with self.subTest(key=key), self.assertRaisesRegex(ValueError, "fresh exact"):
                self.result(reports)
        reports = copy.deepcopy(self.reports)
        del reports["fixture"]["counterexamples"][0]["baseline_selected_verification"]["errors"]
        with self.assertRaisesRegex(ValueError, "fresh exact"):
            self.result(reports)

    def test_failed_or_source_modifying_report_is_not_reused(self):
        for key, value in (("status", "failed"), ("production_sources_modified", True)):
            reports = copy.deepcopy(self.reports)
            reports["fixture"][key] = value
            with self.subTest(key=key), self.assertRaisesRegex(ValueError, "passed unmodified"):
                self.result(reports)

    def test_bad_second_control_is_not_hidden_by_a_good_first_control(self):
        reports = copy.deepcopy(self.reports)
        second = copy.deepcopy(reports["fixture"]["counterexamples"][0])
        second["control_verified_functions"] = 0
        reports["fixture"]["counterexamples"].append(second)
        with self.assertRaisesRegex(ValueError, "incomplete"):
            self.result(reports)

    def test_truncated_source_assessment_and_duplicated_source_keys_fail(self):
        partial = copy.deepcopy(self.assessment)
        partial["code"]["declarations"].pop()
        with self.assertRaisesRegex(ValueError, "exact production"):
            qualify(self.runtime, partial, self.reports)
        runtime = copy.deepcopy(self.runtime)
        values = list(runtime.values())
        values[1]["impl_trait"] = values[0]["impl_trait"]
        with self.assertRaisesRegex(ValueError, "one exact"):
            qualify(runtime, self.assessment, self.reports)

    def test_no_shared_names_does_not_claim_general_identity_coverage(self):
        assessment = copy.deepcopy(self.assessment)
        for row in assessment["code"]["declarations"]:
            row["proof_identity_status"] = "not_checked"
        result = qualify(self.runtime, assessment, {})
        self.assertEqual(result["qualified_shared_implementations"], 0)
        self.assertFalse(result["ready_for_external_validation"])


if __name__ == "__main__":
    unittest.main()

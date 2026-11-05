import copy
from pathlib import Path
import sys
import tempfile
import unittest


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from check_traceability import runtime_inventory
from readiness_assessment import assess
from run_semantic_proofs import baseline_from_whole
from trait_ownership import (
    FROM_SOURCE_SHA256, VSTD_CHECKSUM, VSTD_VERSION, model_inventory, qualify,
    standard_from_contract,
)


PREFIX = """impl FromSpecImpl<u8> for Failure {
    open spec fn obeys_from_spec() -> bool { true }"""
MODEL = PREFIX + """
    closed spec fn from_spec(error: u8) -> Failure { Failure::Value(error) }
}"""
SOURCE = """
#[cfg(verus_keep_ghost)]
use vstd::std_specs::convert::{FromSpec, FromSpecImpl};
enum Failure { Value(u8) }
#[cfg_attr(verus_keep_ghost, verus_verify)]
impl From<u8> for Failure {
    fn from(error: u8) -> Self { Self::Value(error) }
}
#[cfg(verus_keep_ghost)]
verus! {
MODEL
}
"""


class TraitOwnershipTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        (self.root / "src").mkdir()
        (self.root / "verification").mkdir()
        self.path = self.root / "src/lib.rs"
        (self.root / "verification/Cargo.toml").write_text(
            '[dependencies]\nvstd = "=' + VSTD_VERSION + '"\n'
        )
        self.lock = {
            "version": VSTD_VERSION, "checksum": VSTD_CHECKSUM,
            "source": "registry+https://github.com/rust-lang/crates.io-index",
        }
        self.write_lock()
        self.write_source(SOURCE.replace("MODEL", MODEL))

    def write_lock(self):
        (self.root / "verification/Cargo.lock").write_text(
            '[[package]]\nname = "vstd"\n'
            + "".join(f'{key} = "{value}"\n' for key, value in self.lock.items())
        )

    def write_source(self, source, prefix=PREFIX):
        self.path.write_text(source)
        self.runtime = runtime_inventory(self.root)
        accounting = {
            identity: {
                "file": record["file"], "file_sha256": record["file_sha256"],
                "status": "verified", "disposition_reason": None,
            } for identity, record in self.runtime.items()
        }
        self.assessment = assess(
            {"requirements": [], "features": []}, self.runtime,
            {"jbig2_rs_production::Failure::from"}, accounting,
        )
        self.reports = {"fixture": {
            "status": "passed", "production_sources_modified": False,
            "source_bindings": {"src/lib.rs": next(iter(self.runtime.values()))["file_sha256"]},
            "counterexamples": [{
                "id": "conversion", "function": "Failure::from",
                "source_file": "src/lib.rs", "verification_module": "", "impl_trait": "From<u8>",
                "selected_impl_traits": ["From<u8>"],
                "normal_rust_compiles": True, "strong_contract": "rejected",
                "weaker_control": "accepted", "strong_verifier_errors": 1,
                "control_verifier_errors": 0, "strong_verified_functions": 0,
                "control_verified_functions": 1, "baseline_selected_functions": ["Failure::from"],
                "baseline_selected_verification": {
                    "verified": 1, "errors": 0, "encountered-error": False,
                    "encountered-vir-error": False, "is-verifying-entire-crate": False,
                },
            }],
        }}
        self.recipes = {"fixture": {"conversion": {
            "controls": ((prefix, prefix.replace("true", "false")),),
        }}}

    def result(self, whole=None):
        return qualify(self.root, self.runtime, self.assessment, self.reports, self.recipes, whole)

    def test_existing_enabled_result_owner_is_qualified_without_new_contract(self):
        result = self.result()
        self.assertEqual(result["qualified_existing_owners"], 1)
        self.assertEqual(result["remaining_verified_ownership_review"], 0)
        self.assertFalse(result["ready_for_external_validation"])
        owner = result["owners"][0]
        self.assertEqual(owner["impl_trait"], "From<u8>")
        self.assertTrue(owner["model"]["model_sha256"])
        self.assertTrue(owner["semantic_controls"])
        self.assertEqual(result["standard_contract"]["source_file_sha256"], FROM_SOURCE_SHA256)
        self.assertIn("No fresh", result["standard_contract"]["binding_scope"])

    def test_existing_standard_pin_cannot_be_silently_upgraded_or_replaced(self):
        for field in ("version", "checksum", "source"):
            original = self.lock[field]
            self.lock[field] = "changed"
            self.write_lock()
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, "package pin"):
                self.result()
            self.lock[field] = original
        self.write_lock()
        (self.root / "verification/Cargo.toml").write_text('[dependencies]\nvstd = "*"\n')
        with self.assertRaisesRegex(ValueError, "package pin"):
            standard_from_contract(self.root)

    def test_false_nonliteral_or_wrong_signature_enable_does_not_own_result(self):
        for replacement in ("false", "1 == 1"):
            self.write_source(SOURCE.replace("MODEL", MODEL.replace("{ true }", "{ " + replacement + " }")))
            with self.subTest(replacement=replacement), self.assertRaisesRegex(ValueError, "literal enabled"):
                self.result()
        self.write_source(SOURCE.replace("MODEL", MODEL.replace("-> bool", "-> u8")))
        with self.assertRaisesRegex(ValueError, "literal enabled"):
            self.result()

    def test_missing_input_dependency_or_opaque_result_needs_review(self):
        for body in ("Failure::Value(0)", "arbitrary()"):
            self.write_source(SOURCE.replace("MODEL", MODEL.replace("Failure::Value(error)", body)))
            with self.subTest(body=body), self.assertRaisesRegex(ValueError, "opaque or input-free"):
                self.result()

    def test_wrong_input_or_output_type_cannot_bind(self):
        for changed in (
            MODEL.replace("from_spec(error: u8)", "from_spec(error: u16)"),
            MODEL.replace("-> Failure", "-> u8"),
        ):
            self.write_source(SOURCE.replace("MODEL", changed))
            with self.subTest(changed=changed), self.assertRaisesRegex(ValueError, "signature"):
                self.result()

    def test_external_model_body_cannot_be_credited_as_concrete(self):
        model = MODEL.replace(
            "closed spec fn from_spec", "#[verifier::external_body]\nclosed spec fn from_spec",
        )
        self.write_source(SOURCE.replace("MODEL", model))
        with self.assertRaisesRegex(ValueError, "opaque or input-free"):
            self.result()

    def test_duplicate_models_are_not_arbitrarily_selected(self):
        self.write_source(SOURCE.replace("MODEL", MODEL + "\n" + MODEL))
        with self.assertRaisesRegex(ValueError, "duplicate From"):
            self.result()

    def test_nested_or_test_only_models_do_not_own_outer_native_impl(self):
        self.write_source(SOURCE.replace("MODEL", "").replace(
            "#[cfg(verus_keep_ghost)]\nverus!", "#[cfg(test)]\nverus!",
        ))
        with self.assertRaisesRegex(ValueError, "missing exact"):
            self.result()
        self.write_source(SOURCE.replace("MODEL", "") + """
mod other {
    use vstd::std_specs::convert::FromSpecImpl;
    #[cfg(verus_keep_ghost)]
    verus! { MODEL }
}
""".replace("MODEL", MODEL))
        with self.assertRaisesRegex(ValueError, "missing exact"):
            self.result()

    def test_unrelated_trait_import_cannot_be_credited_as_standard(self):
        self.write_source(SOURCE.replace("MODEL", MODEL).replace(
            "vstd::std_specs::convert::{FromSpec, FromSpecImpl}", "other::FromSpecImpl",
        ))
        with self.assertRaisesRegex(ValueError, "unresolved"):
            self.result()

    def test_fully_qualified_standard_model_needs_no_short_import(self):
        model = MODEL.replace("impl FromSpecImpl", "impl vstd::std_specs::convert::FromSpecImpl")
        source = SOURCE.replace("MODEL", model).replace(
            "use vstd::std_specs::convert::{FromSpec, FromSpecImpl};", "",
        )
        prefix = PREFIX.replace("impl FromSpecImpl", "impl vstd::std_specs::convert::FromSpecImpl")
        self.write_source(source, prefix)
        self.assertEqual(self.result()["qualified_existing_owners"], 1)

    def test_model_like_comments_or_strings_do_not_create_owners(self):
        self.write_source(SOURCE.replace("MODEL", "") + "\n// " + MODEL.replace("\n", " ") + "\n")
        self.assertEqual(model_inventory(self.root, self.runtime), {})
        with self.assertRaisesRegex(ValueError, "missing exact"):
            self.result()

    def test_missing_receipt_or_recipe_is_not_inherited_contract_evidence(self):
        reports = self.reports
        self.reports = {}
        with self.assertRaisesRegex(ValueError, "lacks exact"):
            self.result()
        self.reports = reports
        self.recipes = {}
        with self.assertRaisesRegex(ValueError, "no source-bound"):
            self.result()

    def test_weak_control_must_disable_this_exact_model(self):
        for replacement in (
            PREFIX.replace("true", "true"), PREFIX.replace("true", "false").replace("u8", "u16"),
        ):
            self.recipes["fixture"]["conversion"]["controls"] = ((PREFIX, replacement),)
            with self.subTest(replacement=replacement), self.assertRaisesRegex(ValueError, "exact owning"):
                self.result()

    def test_bad_selected_proof_or_stale_source_cannot_qualify_owner(self):
        original = copy.deepcopy(self.reports)
        self.reports["fixture"]["source_bindings"]["src/lib.rs"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "incomplete"):
            self.result()
        self.reports = original
        self.reports["fixture"]["counterexamples"][0]["baseline_selected_verification"]["success"] = False
        with self.assertRaisesRegex(ValueError, "fresh exact"):
            self.result()

    def test_unique_source_impl_can_use_exact_bound_whole_positive(self):
        name = "jbig2_rs_production::Failure::from"
        whole = {
            "receipt_sha256": "a" * 64, "report_sha256": "b" * 64,
            "report": {"func-details": {name: {}}, "verification-results": {
                "success": True, "verified": 20, "errors": 0,
                "encountered-error": False, "encountered-vir-error": False,
                "is-verifying-entire-crate": True,
            }},
        }
        self.reports["fixture"]["counterexamples"][0]["baseline_selected_verification"] = (
            baseline_from_whole(whole, (name,))
        )
        self.assertEqual(self.result(whole)["qualified_existing_owners"], 1)
        with self.assertRaisesRegex(ValueError, "whole positive"):
            self.result()

    def test_unchanged_reviewed_boundary_or_other_trait_is_not_credited(self):
        assessment = copy.deepcopy(self.assessment)
        assessment["code"]["declarations"][0]["impl_trait"] = "Default"
        result = qualify(self.root, self.runtime, assessment, {}, {})
        self.assertEqual(result["qualified_existing_owners"], 0)
        self.assertEqual(result["remaining_verified_ownership_review"], 1)
        self.assertFalse(result["ready_for_external_validation"])

    def test_partial_assessment_cannot_hide_owners(self):
        self.assessment["code"]["declarations"].pop()
        with self.assertRaisesRegex(ValueError, "exact production"):
            self.result()


if __name__ == "__main__":
    unittest.main()

import copy
import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from check_traceability import check, runtime_inventory
from evidence import load_json, validate
from rust_tests import test_inventory as inventory_tests


class TraceabilityTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.write("src/lib.rs", """
            mod leaf;
            #[cfg(verus_keep_ghost)]
            #[path = "../specs/model.rs"]
            mod spec;
        """)
        self.write("src/leaf.rs", """
            #[cfg_attr(verus_keep_ghost, verus_spec(out =>
                ensures out == crate::spec::identity(x),
            ))]
            pub fn identity(x: u8) -> u8 { x }
            #[cfg(test)]
            mod tests {
                #[test] fn identity_returns_input() {}
                #[test] #[ignore] fn ignored_probe() {}
                #[cfg(verus_keep_ghost)]
                #[test] fn ghost_only() {}
            }
            #[cfg_attr(verus_keep_ghost, verus_verify)]
            fn after_tests() {}
        """)
        self.write("specs/model.rs", """
            verus! {
                pub open spec fn identity(x: u8) -> u8 { x }
            }
        """)
        self.write_json("support-matrix.json", {"rows": [{"feature": "decoder", "status": "required"}]})
        self.standard = {"id": "ITU-T T.88 (08/2018)", "edition": "2.0", "document_sha256": "a" * 64}
        self.write_json("source-provenance.json", {"normative_documents": [self.standard]})
        runtime = runtime_inventory(self.root)
        coverage = {
            "schema_version": 1, "source": "src/*.rs", "verus_verified_obligations": 2,
            "native_assumptions": [],
            "counts": {
                "verified": 2, "incomplete": 0, "native-assumption": 0,
                "audited-forwarder": 0, "reviewed-verifier-limitation": 0,
            },
            "functions": [{
                "id": name, "file": entry["file"], "file_sha256": entry["file_sha256"],
                "visibility": entry["visibility"], "status": "verified",
                "disposition_reason": None,
                "verus_functions": [f"jbig2_rs_production::leaf::{entry['name']}"],
            } for name, entry in runtime.items()],
        }
        self.write_json("production-proof-coverage.json", coverage)
        self.ledger = {
            "schema_version": 1, "review_policy": "provisional fixture",
            "standards": [self.standard],
            "features": [{"feature": "decoder", "support_status": "required", "decomposition": "partial"}],
            "definitions": {
                "identity-model": {
                    "kind": "spec", "file": "specs/model.rs", "module_path": ["spec"],
                    "name": "identity", "role": "output model",
                },
            },
            "bindings": {
                path: hashlib.sha256((self.root / path).read_bytes()).hexdigest()
                for path in ("src/lib.rs", "src/leaf.rs", "specs/model.rs")
            },
            "requirements": [{
                "id": "T88-FIXTURE", "standard": self.standard["id"], "clause": "5.4",
                "title": "synthetic identity fixture", "feature": "decoder",
                "runtime": ["leaf::identity"], "models": ["identity-model"], "proof_macros": [],
                "tests": [{"target": "lib", "name": "leaf::tests::identity_returns_input"}],
                "strength": "functional_contract", "review": "provisional",
                "scope_of_formal_result": "fixture only", "assumptions": ["fixture translation"],
                "gaps": [], "invariant_evidence": [],
                "functional_evidence": [{
                    "runtime": "leaf::identity", "section": "ensures",
                    "tokens": ["out == crate::spec::identity(x)"],
                }],
            }],
        }
        self.names = {
            "jbig2_rs_production::spec::identity",
            "jbig2_rs_production::leaf::identity",
        }
        self.discovered = {("lib", "leaf::tests::identity_returns_input")}

    def write(self, relative, text):
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")

    def write_json(self, relative, document):
        self.write(relative, json.dumps(document))

    def run_check(self, ledger=None, names=None, discovered=None):
        return check(
            ledger or self.ledger, self.root,
            self.names if names is None else names,
            self.discovered if discovered is None else discovered,
        )

    def test_links_are_valid_without_claiming_scope_or_owner_approval(self):
        validate(self.ledger, "traceability")
        report = self.run_check()
        self.assertEqual(report["requirement_count"], 1)
        self.assertFalse(report["normative_decomposition_complete"])
        self.assertEqual(report["status"], "references_valid_scope_incomplete")
        self.assertTrue(report["verifier_report_checked"])
        self.assertTrue(report["cargo_test_discovery_checked"])

    def test_test_inventory_respects_test_and_ghost_scopes(self):
        self.write("tests/hidden.rs", "#![cfg(verus_keep_ghost)]\n#[test] fn hidden() {}")
        tests = inventory_tests(self.root)
        self.assertIn(("lib", "leaf::tests::identity_returns_input"), tests)
        self.assertTrue(tests[("lib", "leaf::tests::ignored_probe")]["ignored"])
        self.assertNotIn(("lib", "leaf::tests::ghost_only"), tests)
        self.assertNotIn(("hidden", "hidden"), tests)
        self.assertIn("leaf::after_tests", runtime_inventory(self.root))

    def test_stale_runtime_and_model_hashes_fail(self):
        for path in ("src/leaf.rs", "specs/model.rs"):
            original = (self.root / path).read_bytes()
            (self.root / path).write_bytes(original + b"\n")
            with self.subTest(path=path), self.assertRaisesRegex(ValueError, "stale"):
                self.run_check()
            (self.root / path).write_bytes(original)

    def test_same_named_wrong_namespace_does_not_satisfy_model(self):
        wrong = {"jbig2_rs_production::other::identity", "jbig2_rs_production::leaf::identity"}
        with self.assertRaisesRegex(ValueError, "definition absent"):
            self.run_check(names=wrong)

    def test_commented_declaration_is_not_a_model(self):
        self.write("specs/model.rs", "verus! {\n// pub open spec fn identity(x:u8)->u8 {x}\n}")
        with self.assertRaisesRegex(ValueError, "missing or ambiguous Verus"):
            self.run_check()

    def test_module_alias_must_resolve_to_the_recorded_file(self):
        ledger = copy.deepcopy(self.ledger)
        ledger["definitions"]["identity-model"]["module_path"] = ["leaf"]
        with self.assertRaisesRegex(ValueError, "another file"):
            self.run_check(ledger)

    def test_ignored_missing_and_undiscovered_tests_fail(self):
        for name in ("leaf::tests::ignored_probe", "leaf::tests::nonexistent"):
            ledger = copy.deepcopy(self.ledger)
            ledger["requirements"][0]["tests"][0]["name"] = name
            with self.subTest(name=name), self.assertRaises((ValueError, KeyError)):
                self.run_check(ledger)
        with self.assertRaisesRegex(ValueError, "Cargo discovery"):
            self.run_check(discovered=set())

    def test_weak_evidence_cannot_be_promoted_without_postcondition(self):
        ledger = copy.deepcopy(self.ledger)
        ledger["requirements"][0]["functional_evidence"] = []
        with self.assertRaisesRegex(ValueError, "explicit postcondition"):
            self.run_check(ledger)
        ledger = copy.deepcopy(self.ledger)
        ledger["requirements"][0]["functional_evidence"][0]["tokens"] = ["out == 0"]
        with self.assertRaisesRegex(ValueError, "not present"):
            self.run_check(ledger)

    def test_preconditions_are_not_functional_output_evidence(self):
        ledger = copy.deepcopy(self.ledger)
        ledger["requirements"][0]["functional_evidence"][0]["section"] = "requires"
        with self.assertRaisesRegex(ValueError, "preconditions cannot"):
            self.run_check(ledger)

    def test_unverified_body_cannot_back_a_functional_claim(self):
        path = self.root / "production-proof-coverage.json"
        coverage = load_json(path)
        entry = next(item for item in coverage["functions"] if item["id"] == "leaf::identity")
        entry.update(
            status="runtime-proof-divergence", verus_functions=[],
            disposition_reason="synthetic divergent-body fixture",
        )
        coverage["counts"]["verified"] -= 1
        coverage["counts"]["runtime-proof-divergence"] = 1
        self.write_json("production-proof-coverage.json", coverage)
        with self.assertRaisesRegex(ValueError, "unverified or divergent"):
            self.run_check()

    def test_unrelated_existing_proof_macro_cannot_back_a_runtime_claim(self):
        original = (self.root / "src/lib.rs").read_text()
        self.write("src/lib.rs", original + """
            #[cfg(verus_keep_ghost)]
            #[path = "../proofs/helper.rs"] mod helper;
        """)
        self.write("proofs/helper.rs", "macro_rules! unrelated { () => {}; }")
        ledger = copy.deepcopy(self.ledger)
        ledger["definitions"]["unrelated"] = {
            "kind": "macro", "file": "proofs/helper.rs", "module_path": ["helper"],
            "name": "unrelated", "role": "synthetic proof support",
        }
        ledger["requirements"][0]["proof_macros"] = ["unrelated"]
        with self.assertRaisesRegex(ValueError, "not wired"):
            self.run_check(ledger)

    def test_required_feature_cannot_disappear_or_be_excluded(self):
        for features in ([], [{"feature": "decoder", "support_status": "required", "decomposition": "excluded"}]):
            ledger = copy.deepcopy(self.ledger)
            ledger["features"] = features
            with self.subTest(features=features), self.assertRaises(ValueError):
                self.run_check(ledger)

    def test_complete_feature_cannot_be_an_empty_declaration(self):
        self.write_json("support-matrix.json", {"rows": [
            {"feature": "decoder", "status": "required"},
            {"feature": "other", "status": "required"},
        ]})
        ledger = copy.deepcopy(self.ledger)
        ledger["features"].append({
            "feature": "other", "support_status": "required", "decomposition": "complete",
        })
        with self.assertRaisesRegex(ValueError, "complete feature has no requirement"):
            self.run_check(ledger)
    def test_duplicate_requirements_and_unused_bindings_fail(self):
        ledger = copy.deepcopy(self.ledger)
        ledger["requirements"].append(copy.deepcopy(ledger["requirements"][0]))
        with self.assertRaisesRegex(ValueError, "duplicate requirement"):
            self.run_check(ledger)
        ledger = copy.deepcopy(self.ledger)
        ledger["bindings"]["README.md"] = "a" * 64
        with self.assertRaisesRegex(ValueError, "unused or missing"):
            self.run_check(ledger)

    def test_normative_edition_or_hash_change_invalidates_review(self):
        for change in ({"edition": "old"}, {"document_sha256": "b" * 64}):
            ledger = copy.deepcopy(self.ledger)
            ledger["standards"][0] = self.standard | change
            with self.subTest(change=change), self.assertRaisesRegex(ValueError, "normative"):
                self.run_check(ledger)

    def test_checked_repository_ledger_is_schema_valid_and_provisional(self):
        ledger = load_json(TOOLS.parent / "validation/traceability/requirements.json")
        validate(ledger, "traceability")
        self.assertEqual(len(ledger["requirements"]), 16)
        self.assertTrue(all(entry["review"] == "provisional" for entry in ledger["requirements"]))

    def test_typical_row_ledger_records_exact_bytes_and_visible_pixels(self):
        ledger = load_json(TOOLS.parent / "validation/traceability/requirements.json")
        requirement = next(entry for entry in ledger["requirements"]
                           if entry["id"] == "T88-GENERIC-TYPICAL-ROW")
        self.assertEqual(requirement["strength"], "functional_contract")
        self.assertEqual(requirement["review"], "provisional")
        self.assertEqual(set(requirement["runtime"]), {
            "generic_region::copy_bitmap_row", "bitmap::Bitmap::copy_row", "bitmap::copy_row_bytes",
        })
        tokens = " ".join(
            token for claim in requirement["functional_evidence"] for token in claim["tokens"]
        )
        self.assertIn("row_copy_result(old(bitmap), final(bitmap)", tokens)
        self.assertIn("stored_pixel(final(bitmap), x, destination)", tokens)
        self.assertIn("copied-row-pixels", requirement["proof_macros"])

    def test_batch_reset_ledger_records_the_proved_zero_result(self):
        ledger = load_json(TOOLS.parent / "validation/traceability/requirements.json")
        requirement = next(entry for entry in ledger["requirements"]
                           if entry["id"] == "T88-ARITHMETIC-RESET-SET")
        self.assertEqual(requirement["strength"], "functional_contract")
        self.assertEqual(requirement["review"], "provisional")
        tokens = " ".join(
            token for claim in requirement["functional_evidence"] for token in claim["tokens"]
        )
        self.assertIn("state_index == 0", tokens)
        self.assertIn("mps == 0", tokens)


if __name__ == "__main__":
    unittest.main()

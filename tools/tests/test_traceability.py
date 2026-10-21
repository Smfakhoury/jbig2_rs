import copy
import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path

from jsonschema import ValidationError


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from check_traceability import check, runtime_inventory
from evidence import load_json, validate
from rust_tests import test_inventory as inventory_tests


class TraceabilityTests(unittest.TestCase):
    def setUp(self):
        fixture_directory = TOOLS.parent / "target" / "traceability-test-fixtures"
        fixture_directory.mkdir(parents=True, exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(dir=fixture_directory)
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
            "features": [{
                "feature": "decoder", "support_status": "required", "decomposition": "partial",
                "requirement_ids": ["T88-FIXTURE"],
                "remaining_scope": ["Synthetic fixture is not a full normative decomposition."],
            }],
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
        feature = report["feature_decomposition"][0]
        self.assertEqual(feature["requirement_ids"], ["T88-FIXTURE"])
        self.assertEqual(feature["requirement_count"], 1)
        self.assertEqual(feature["evidence_strength_counts"]["functional_contract"], 1)
        self.assertTrue(feature["remaining_scope"])

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

    def test_feature_accounting_fields_are_required_by_schema(self):
        for field in ("requirement_ids", "remaining_scope"):
            ledger = copy.deepcopy(self.ledger)
            del ledger["features"][0][field]
            with self.subTest(field=field), self.assertRaises(ValidationError):
                validate(ledger, "traceability")

    def test_missing_stale_and_duplicate_feature_requirement_ids_fail(self):
        for references in ([], ["T88-STALE"], ["T88-FIXTURE", "T88-FIXTURE"]):
            ledger = copy.deepcopy(self.ledger)
            ledger["features"][0]["requirement_ids"] = references
            with self.subTest(references=references), self.assertRaisesRegex(ValueError, "accounting"):
                self.run_check(ledger)

    def test_feature_cannot_claim_another_features_requirement(self):
        self.write_json("support-matrix.json", {"rows": [
            {"feature": "decoder", "status": "required"},
            {"feature": "other", "status": "required"},
        ]})
        ledger = copy.deepcopy(self.ledger)
        ledger["features"].append({
            "feature": "other", "support_status": "required", "decomposition": "partial",
            "requirement_ids": ["T88-FIXTURE"], "remaining_scope": ["Still unenumerated."],
        })
        with self.assertRaisesRegex(ValueError, "accounting"):
            self.run_check(ledger)

    def test_recorded_feature_cannot_stay_not_started(self):
        ledger = copy.deepcopy(self.ledger)
        ledger["features"][0]["decomposition"] = "not_started"
        with self.assertRaisesRegex(ValueError, "state disagrees"):
            self.run_check(ledger)

    def test_partial_feature_cannot_have_no_requirement_records(self):
        ledger = copy.deepcopy(self.ledger)
        ledger["requirements"] = []
        ledger["definitions"] = {}
        ledger["bindings"] = {}
        ledger["features"][0]["requirement_ids"] = []
        with self.assertRaisesRegex(ValueError, "state disagrees"):
            self.run_check(ledger)

    def test_unfinished_or_excluded_feature_needs_remaining_scope(self):
        for status, state, selected in (
            ("required", "partial", True),
            ("required", "not_started", False),
            ("deferred", "excluded", False),
        ):
            self.write_json("support-matrix.json", {"rows": [{"feature": "decoder", "status": status}]})
            for notes in ([], ["   "]):
                ledger = copy.deepcopy(self.ledger)
                ledger["features"][0].update(
                    support_status=status, decomposition=state, remaining_scope=notes,
                    requirement_ids=["T88-FIXTURE"] if selected else [],
                )
                if not selected:
                    ledger.update(requirements=[], definitions={}, bindings={})
                with self.subTest(state=state, notes=notes), self.assertRaisesRegex(ValueError, "remaining scope"):
                    self.run_check(ledger)

    def test_complete_feature_rejects_provisional_gapped_or_weak_records(self):
        for change in (
            {"review": "provisional"},
            {"gaps": ["Unproved output semantics."]},
            {"strength": "safety_only", "functional_evidence": []},
        ):
            ledger = copy.deepcopy(self.ledger)
            ledger["features"][0].update(decomposition="complete", remaining_scope=[])
            ledger["requirements"][0]["review"] = "approved"
            ledger["requirements"][0].update(change)
            with self.subTest(change=change), self.assertRaisesRegex(ValueError, "unresolved scope or evidence"):
                self.run_check(ledger)

    def test_complete_feature_cannot_hide_remaining_scope(self):
        ledger = copy.deepcopy(self.ledger)
        ledger["features"][0]["decomposition"] = "complete"
        ledger["requirements"][0]["review"] = "approved"
        with self.assertRaisesRegex(ValueError, "unresolved scope or evidence"):
            self.run_check(ledger)

    def test_duplicate_runtime_model_test_and_contract_evidence_fails(self):
        for field in ("runtime", "models", "tests", "functional_evidence"):
            ledger = copy.deepcopy(self.ledger)
            ledger["requirements"][0][field].append(copy.deepcopy(ledger["requirements"][0][field][0]))
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, f"duplicate {field}"):
                self.run_check(ledger)
            with self.subTest(schema=field), self.assertRaises(ValidationError):
                validate(ledger, "traceability")

    def test_duplicate_contract_tokens_fail_even_when_whitespace_differs(self):
        for token in ("out == crate::spec::identity(x)", "out==crate::spec::identity(x)"):
            ledger = copy.deepcopy(self.ledger)
            ledger["requirements"][0]["functional_evidence"][0]["tokens"].append(token)
            with self.subTest(token=token), self.assertRaisesRegex(ValueError, "duplicate contract token"):
                self.run_check(ledger)

    def test_unused_and_aliased_duplicate_model_definitions_fail(self):
        ledger = copy.deepcopy(self.ledger)
        ledger["definitions"]["alias"] = copy.deepcopy(ledger["definitions"]["identity-model"])
        with self.assertRaisesRegex(ValueError, "unused or missing"):
            self.run_check(ledger)
        ledger["requirements"][0]["models"].append("alias")
        with self.assertRaisesRegex(ValueError, "duplicate model/proof definition"):
            self.run_check(ledger)

    def test_weakened_contract_fails_after_source_hashes_are_refreshed(self):
        path = self.root / "src/leaf.rs"
        path.write_text(path.read_text().replace(
            "ensures out == crate::spec::identity(x)", "ensures out <= 255",
        ))
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        ledger = copy.deepcopy(self.ledger)
        ledger["bindings"]["src/leaf.rs"] = digest
        coverage = load_json(self.root / "production-proof-coverage.json")
        for function in coverage["functions"]:
            if function["file"] == "src/leaf.rs":
                function["file_sha256"] = digest
        self.write_json("production-proof-coverage.json", coverage)
        with self.assertRaisesRegex(ValueError, "not present"):
            self.run_check(ledger)

    def test_state_preservation_cannot_use_a_precondition(self):
        ledger = copy.deepcopy(self.ledger)
        requirement = ledger["requirements"][0]
        requirement["strength"] = "state_preservation"
        requirement["invariant_evidence"] = requirement["functional_evidence"]
        requirement["functional_evidence"] = []
        requirement["invariant_evidence"][0]["section"] = "requires"
        with self.assertRaisesRegex(ValueError, "preconditions cannot satisfy a state"):
            self.run_check(ledger)

    def test_functional_evidence_cannot_hide_under_a_weaker_strength(self):
        for strength in ("state_preservation", "safety_only"):
            ledger = copy.deepcopy(self.ledger)
            ledger["requirements"][0]["strength"] = strength
            ledger["requirements"][0]["invariant_evidence"] = copy.deepcopy(
                ledger["requirements"][0]["functional_evidence"],
            )
            with self.subTest(strength=strength), self.assertRaisesRegex(ValueError, "weaker strength classification"):
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
        self.assertEqual(len(ledger["requirements"]), 48)
        self.assertTrue(all(entry["review"] == "provisional" for entry in ledger["requirements"]))

    def test_repository_feature_accounting_is_exact_but_still_incomplete(self):
        ledger = load_json(TOOLS.parent / "validation/traceability/requirements.json")
        features = {feature["feature"]: feature for feature in ledger["features"]}
        for name, feature in features.items():
            expected = {row["id"] for row in ledger["requirements"] if row["feature"] == name}
            with self.subTest(feature=name):
                self.assertEqual(set(feature["requirement_ids"]), expected)
                self.assertTrue(feature["remaining_scope"])
                self.assertNotEqual(feature["decomposition"], "complete")
        for name in (
            "sequential_organization", "random_access_organization", "embedded_organization",
            "standard_huffman_tables", "custom_huffman_tables", "mmr_coding",
        ):
            self.assertEqual(features[name]["decomposition"], "partial")

    def test_new_family_records_do_not_upgrade_parser_or_pixel_evidence(self):
        ledger = load_json(TOOLS.parent / "validation/traceability/requirements.json")
        rows = {row["id"]: row for row in ledger["requirements"]}
        for name in ("T88-SEQUENTIAL-SEGMENT-LAYOUT", "T88-RANDOM-HEADER-PAYLOAD-LAYOUT",
                     "T88-HUFFMAN-CANONICAL-ASSIGNMENT", "T88-HUFFMAN-STANDARD-TABLES"):
            self.assertEqual(rows[name]["strength"], "safety_only")
            self.assertEqual(rows[name]["functional_evidence"], [])
            self.assertTrue(rows[name]["gaps"])
        self.assertEqual(rows["T88-HUFFMAN-PREFIX-MATCH"]["strength"], "state_preservation")
        self.assertEqual(rows["T88-MMR-BINARY-POLARITY"]["strength"], "state_preservation")
        self.assertIn("T.6", " ".join(rows["T88-MMR-BINARY-POLARITY"]["assumptions"]))

    def test_retention_records_disclose_the_worked_example_conflict(self):
        ledger = load_json(TOOLS.parent / "validation/traceability/requirements.json")
        for identity in ("T88-SEGMENT-RETENTION-WIRE-ORDER", "T88-SEGMENT-RETENTION-UNUSED-BITS"):
            requirement = next(entry for entry in ledger["requirements"] if entry["id"] == identity)
            with self.subTest(requirement=identity):
                self.assertEqual(requirement["strength"], "functional_contract")
                self.assertEqual(requirement["review"], "provisional")
                self.assertIn("7.2.8", " ".join(requirement["assumptions"]))
                self.assertTrue(requirement["gaps"])
        definition = ledger["definitions"]["retention-wire-bit"]
        self.assertEqual(definition["module_path"], ["spec", "segments"])
        self.assertEqual(definition["name"], "retention_bit")

    def test_typical_row_ledger_records_exact_bytes_and_visible_pixels(self):
        ledger = load_json(TOOLS.parent / "validation/traceability/requirements.json")
        requirement = next(entry for entry in ledger["requirements"]
                           if entry["id"] == "T88-GENERIC-TYPICAL-ROW")
        self.assertEqual(requirement["strength"], "functional_contract")
        self.assertEqual(requirement["review"], "provisional")
        self.assertEqual(set(requirement["runtime"]), {
            "generic_region::copy_bitmap_row", "bitmap::Bitmap::copy_row", "bitmap::copy_row_bytes",
            "bitmap::Bitmap::blank", "bitmap::Bitmap::filled",
        })
        tokens = " ".join(
            token for claim in requirement["functional_evidence"] for token in claim["tokens"]
        )
        self.assertIn("row_copy_result(old(bitmap), final(bitmap)", tokens)
        self.assertIn("stored_pixel(final(bitmap), x, destination)", tokens)
        self.assertIn("copied-row-pixels", requirement["proof_macros"])
        self.assertIn("zeroed-storage", requirement["models"])
        self.assertIn("zeroed_storage(&bitmap)", tokens)

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

import copy
from contextlib import redirect_stderr, redirect_stdout
import hashlib
from io import StringIO
import json
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch
from pathlib import Path

from jsonschema import ValidationError


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from check_traceability import check, enforce_completion, local_document_checks, policy_row_hash, runtime_inventory
from evidence import load_json, validate
from rust_tests import test_inventory as inventory_tests
import check_traceability as checker


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
        self.consultation = {
            "standard": self.standard["id"],
            "acquisition_kind": "official_package",
            "acquisition_url": "https://www.itu.int/rec/dologin_pub.asp?lang=e&id=T-REC-T.88-201808-I!!SOFT-ZST-E&type=items",
            "package_sha256": "b" * 64, "document_sha256": self.standard["document_sha256"],
            "clause_families": ["5.4 (synthetic fixture)"],
        }
        self.write_json("source-provenance.json", {"normative_documents": [self.standard | {
            "package_sha256": "b" * 64, "source_url": "https://www.itu.int/rec/T-REC-T.88-201808-I/en",
        }]})
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
            "source_consultations": [self.consultation],
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

    def with_reference_standard(self):
        ledger = copy.deepcopy(self.ledger)
        reference = {
            "id": "ITU-T T.6 (1988)", "document_sha256": "c" * 64,
            "source_url": "https://www.itu.int/rec/dologin_pub.asp?lang=e&id=T-REC-T.6-198811-I!!PDF-E&type=items",
        }
        provenance = load_json(self.root / "source-provenance.json")
        provenance["normative_references"] = [reference]
        self.write_json("source-provenance.json", provenance)
        ledger["standards"].append({
            "id": reference["id"], "edition": None,
            "provenance_collection": "normative_references",
            "document_sha256": reference["document_sha256"],
        })
        ledger["source_consultations"].append({
            "standard": reference["id"], "acquisition_kind": "official_pdf",
            "acquisition_url": reference["source_url"],
            "document_sha256": reference["document_sha256"], "clause_families": ["2.2.1"],
        })
        requirement = copy.deepcopy(ledger["requirements"][0])
        requirement.update(id="T6-FIXTURE", standard=reference["id"], clause="2.2.1")
        ledger["requirements"].append(requirement)
        ledger["features"][0]["requirement_ids"].append(requirement["id"])
        return ledger

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

    def test_complete_decomposition_still_rejects_unreviewed_records(self):
        ledger = copy.deepcopy(self.ledger)
        ledger["features"][0].update(decomposition="complete", remaining_scope=[])
        with self.assertRaisesRegex(ValueError, "unresolved scope or evidence"):
            self.run_check(ledger)

    def test_reviewed_decomposition_can_account_for_explicit_semantic_gaps(self):
        for change in (
            {"gaps": ["Unproved output semantics."]},
            {"strength": "safety_only", "functional_evidence": []},
            {"strength": "safety_only", "functional_evidence": [], "gaps": ["Unproved output semantics."]},
        ):
            ledger = copy.deepcopy(self.ledger)
            ledger["features"][0].update(decomposition="complete", remaining_scope=[])
            ledger["requirements"][0]["review"] = "approved"
            ledger["requirements"][0].update(change)
            with self.subTest(change=change):
                report = self.run_check(ledger)
                self.assertTrue(report["decomposition_review_complete"])
                self.assertFalse(report["semantic_claims_complete"])
                self.assertFalse(report["normative_decomposition_complete"])
                self.assertEqual(report["status"], "references_valid_scope_incomplete")
                self.assertEqual(report["release_gate"], "semantic_claims_review_required")
                enforce_completion(report, require_decomposition=True)
                with self.assertRaisesRegex(ValueError, "semantic claims"):
                    enforce_completion(report, require_complete=True)

    def test_both_gates_pass_only_when_reviewed_scope_and_functional_claims_are_closed(self):
        ledger = copy.deepcopy(self.ledger)
        ledger["features"][0].update(decomposition="complete", remaining_scope=[])
        ledger["requirements"][0]["review"] = "approved"
        report = self.run_check(ledger)
        self.assertTrue(report["decomposition_review_complete"])
        self.assertTrue(report["semantic_claims_complete"])
        self.assertTrue(report["normative_decomposition_complete"])
        enforce_completion(report, require_decomposition=True, require_complete=True)

    def test_reviewed_functional_rows_cannot_hide_unenumerated_feature_scope(self):
        ledger = copy.deepcopy(self.ledger)
        ledger["requirements"][0]["review"] = "approved"
        report = self.run_check(ledger)
        self.assertFalse(report["decomposition_review_complete"])
        self.assertFalse(report["semantic_claims_complete"])
        for flags in ({"require_decomposition": True}, {"require_complete": True}):
            with self.subTest(flags=flags), self.assertRaises(ValueError):
                enforce_completion(report, **flags)

    def test_cli_decomposition_gate_does_not_make_the_full_semantic_gate_pass(self):
        ledger = copy.deepcopy(self.ledger)
        ledger["features"][0].update(decomposition="complete", remaining_scope=[])
        ledger["requirements"][0].update(
            review="approved", strength="safety_only", functional_evidence=[],
            gaps=["The output transformation is unproved."],
        )
        self.write_json("validation/traceability/requirements.json", ledger)
        original_read = checker.read_evidence

        def read(path, kind):
            if kind == "verus":
                return {"func-details": {name: {} for name in self.names}}
            return original_read(path, kind)

        for flag, expected in (("--require-reviewed-decomposition", 0), ("--require-complete", 1)):
            with self.subTest(flag=flag):
                output = self.root / ("result-" + flag[2:] + ".json")
                arguments = [
                    "check_traceability.py", flag, "--verus-json", "fixture.json",
                    "--discover-tests", "--output", str(output),
                ]
                with patch.object(checker, "ROOT", self.root), patch.object(sys, "argv", arguments), \
                        patch.object(checker, "read_evidence", side_effect=read), \
                        patch.object(checker, "discover_tests", return_value=self.discovered), \
                        patch.object(checker.subprocess, "run", return_value=SimpleNamespace(stdout="fixture\n")), \
                        redirect_stdout(StringIO()), redirect_stderr(StringIO()):
                    self.assertEqual(checker.main(), expected)
                result = load_json(output)
                self.assertTrue(result["decomposition_review_complete"])
                self.assertFalse(result["semantic_claims_complete"])
                self.assertFalse(result["normative_decomposition_complete"])

    def test_both_cli_gates_require_verifier_and_real_discovery_inputs(self):
        for flag in ("--require-reviewed-decomposition", "--require-complete"):
            with self.subTest(flag=flag):
                with patch.object(sys, "argv", [
                    "check_traceability.py", flag, "--output", str(self.root / "unused.json"),
                ]), redirect_stderr(StringIO()), self.assertRaises(SystemExit) as error:
                    checker.main()
                self.assertEqual(error.exception.code, 2)

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

    def test_commented_out_postcondition_cannot_satisfy_a_refreshed_binding(self):
        path = self.root / "src/leaf.rs"
        path.write_text(path.read_text().replace(
            "ensures out == crate::spec::identity(x),",
            "ensures /* out == crate::spec::identity(x) */ true,",
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

    def test_documentation_cannot_replace_a_removed_ghost_contract(self):
        path = self.root / "src/leaf.rs"
        path.write_text(path.read_text().replace(
            """#[cfg_attr(verus_keep_ghost, verus_spec(out =>
                ensures out == crate::spec::identity(x),
            ))]""",
            """#[doc = "verus_spec(ensures out == crate::spec::identity(x))"]
            #[cfg_attr(verus_keep_ghost, verus_verify)]""",
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

    def test_true_cannot_be_promoted_into_functional_or_state_evidence(self):
        for strength in ("functional_contract", "state_preservation"):
            ledger = copy.deepcopy(self.ledger)
            requirement = ledger["requirements"][0]
            requirement["strength"] = strength
            claim = {"runtime": "leaf::identity", "section": "ensures", "tokens": ["( true )"]}
            requirement["functional_evidence"] = [claim] if strength == "functional_contract" else []
            requirement["invariant_evidence"] = [claim] if strength == "state_preservation" else []
            with self.subTest(strength=strength), self.assertRaisesRegex(ValueError, "tautological"):
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

    def test_source_consultations_are_required_and_cannot_disappear_or_duplicate(self):
        ledger = copy.deepcopy(self.ledger)
        del ledger["source_consultations"]
        with self.assertRaises(ValidationError):
            validate(ledger, "traceability")
        for consultations in ([], [self.consultation, self.consultation]):
            ledger = copy.deepcopy(self.ledger)
            ledger["source_consultations"] = consultations
            with self.subTest(consultations=consultations), self.assertRaisesRegex(ValueError, "exact normative standards once"):
                self.run_check(ledger)

    def test_stale_document_or_package_consultation_hash_fails(self):
        for field in ("document_sha256", "package_sha256"):
            ledger = copy.deepcopy(self.ledger)
            ledger["source_consultations"][0][field] = "c" * 64
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, "consultation .* differs"):
                self.run_check(ledger)

    def test_consultation_cannot_point_to_a_mirror_or_credentialed_url(self):
        for url in (
            "https://mirror.invalid/T88.zip",
            "http://www.itu.int/rec/dologin_pub.asp?id=fixture",
            "https://user@www.itu.int/rec/dologin_pub.asp?id=fixture",
            "https://www.itu.int/unrecorded/path",
        ):
            ledger = copy.deepcopy(self.ledger)
            ledger["source_consultations"][0]["acquisition_url"] = url
            with self.subTest(url=url), self.assertRaisesRegex(ValueError, "official publication host"):
                self.run_check(ledger)

    def test_consultation_cannot_name_another_official_publication(self):
        ledger = copy.deepcopy(self.ledger)
        ledger["source_consultations"][0]["acquisition_url"] = (
            "https://www.itu.int/rec/dologin_pub.asp?lang=e&id=T-REC-T.6-198811-I!!SOFT-ZST-E&type=items"
        )
        with self.assertRaisesRegex(ValueError, "different publication/package"):
            self.run_check(ledger)

    def test_consultation_clause_families_cannot_be_empty_or_repeated(self):
        for families in ([], [" "], ["6.3", "6.3 "], ["6.3", "6.3"]):
            ledger = copy.deepcopy(self.ledger)
            ledger["source_consultations"][0]["clause_families"] = families
            with self.subTest(families=families), self.assertRaisesRegex(ValueError, "nonempty distinct clause families"):
                self.run_check(ledger)

    def test_explicit_project_policy_is_counted_separately_from_normative_rules(self):
        policy = {"feature": "decoder", "status": "required", "note": "Synthetic API policy."}
        self.write_json("support-matrix.json", {"rows": [policy]})
        ledger = copy.deepcopy(self.ledger)
        ledger["requirements"][0].update(
            obligation_kind="project_profile_policy", policy_row_sha256=policy_row_hash(policy),
        )
        validate(ledger, "traceability")
        report = self.run_check(ledger)
        self.assertEqual(report["obligation_kind_counts"], {"normative_rule": 0, "project_profile_policy": 1})
        self.assertEqual(report["requirements"][0]["obligation_kind"], "project_profile_policy")
        self.assertEqual(report["requirements"][0]["policy_row_sha256"], policy_row_hash(policy))

    def test_stale_missing_or_unstated_policy_basis_fails(self):
        policy = {"feature": "decoder", "status": "required", "note": "Synthetic API policy."}
        self.write_json("support-matrix.json", {"rows": [policy]})
        ledger = copy.deepcopy(self.ledger)
        ledger["requirements"][0]["obligation_kind"] = "project_profile_policy"
        with self.assertRaisesRegex(ValueError, "stale or missing project policy"):
            self.run_check(ledger)
        ledger["requirements"][0]["policy_row_sha256"] = policy_row_hash(policy)
        self.write_json("support-matrix.json", {"rows": [policy | {"note": "Weakened API policy."}]})
        with self.assertRaisesRegex(ValueError, "stale or missing project policy"):
            self.run_check(ledger)
        self.write_json("support-matrix.json", {"rows": [{"feature": "decoder", "status": "required"}]})
        with self.assertRaisesRegex(ValueError, "no explicit support-matrix basis"):
            self.run_check(ledger)

    def test_project_policy_hash_cannot_be_mislabeled_normative(self):
        ledger = copy.deepcopy(self.ledger)
        ledger["requirements"][0]["policy_row_sha256"] = "c" * 64
        with self.assertRaises(ValidationError):
            validate(ledger, "traceability")
        with self.assertRaisesRegex(ValueError, "cannot hide a project policy"):
            self.run_check(ledger)

    def test_direct_reference_pdf_preserves_missing_edition_and_source_namespace(self):
        ledger = self.with_reference_standard()
        validate(ledger, "traceability")
        report = self.run_check(ledger)
        self.assertIsNone(report["standards"][1]["edition"])
        self.assertEqual(report["standards"][1]["provenance_collection"], "normative_references")
        self.assertEqual(report["requirements"][1]["standard"], "ITU-T T.6 (1988)")
        self.assertNotIn("package_sha256", report["source_consultations"][1])

    def test_reference_cannot_silently_resolve_from_another_collection(self):
        ledger = self.with_reference_standard()
        del ledger["standards"][1]["provenance_collection"]
        with self.assertRaisesRegex(ValueError, "provenance collection differs"):
            self.run_check(ledger)

    def test_reference_edition_cannot_be_invented_from_the_year(self):
        for edition in ("1988", "1.0", "2.0"):
            ledger = self.with_reference_standard()
            ledger["standards"][1]["edition"] = edition
            with self.subTest(edition=edition), self.assertRaisesRegex(ValueError, "normative edition differs"):
                self.run_check(ledger)

    def test_ambiguous_cross_collection_identity_is_rejected(self):
        ledger = self.with_reference_standard()
        provenance = load_json(self.root / "source-provenance.json")
        provenance["normative_documents"].append(copy.deepcopy(provenance["normative_references"][0]))
        self.write_json("source-provenance.json", provenance)
        with self.assertRaisesRegex(ValueError, "ambiguous normative provenance"):
            self.run_check(ledger)

    def test_reference_requires_an_approved_document_hash_not_only_metadata(self):
        ledger = self.with_reference_standard()
        provenance = load_json(self.root / "source-provenance.json")
        del provenance["normative_references"][0]["document_sha256"]
        self.write_json("source-provenance.json", provenance)
        with self.assertRaisesRegex(ValueError, "no approved document identity"):
            self.run_check(ledger)

    def test_reference_cannot_reuse_another_standards_hash_or_prefix(self):
        ledger = self.with_reference_standard()
        ledger["standards"][1]["document_sha256"] = self.standard["document_sha256"]
        with self.assertRaisesRegex(ValueError, "normative document identity differs"):
            self.run_check(ledger)
        ledger = self.with_reference_standard()
        ledger["requirements"][1]["standard"] = self.standard["id"]
        with self.assertRaisesRegex(ValueError, "prefix differs from normative source"):
            self.run_check(ledger)

    def test_direct_pdf_cannot_invent_a_package_or_use_an_unrecorded_url(self):
        ledger = self.with_reference_standard()
        ledger["source_consultations"][1]["package_sha256"] = "b" * 64
        with self.assertRaises(ValidationError):
            validate(ledger, "traceability")
        with self.assertRaisesRegex(ValueError, "cannot invent a package"):
            self.run_check(ledger)
        ledger = self.with_reference_standard()
        ledger["source_consultations"][1]["acquisition_url"] += "&unrecorded=1"
        with self.assertRaisesRegex(ValueError, "URL differs from recorded provenance"):
            self.run_check(ledger)

    def test_direct_reference_cannot_masquerade_as_the_other_standards_package(self):
        ledger = self.with_reference_standard()
        ledger["source_consultations"][1].update(
            acquisition_kind="official_package", package_sha256=self.consultation["package_sha256"],
            acquisition_url=self.consultation["acquisition_url"],
        )
        with self.assertRaisesRegex(ValueError, "package_sha256 differs from provenance"):
            self.run_check(ledger)

    def test_missing_reference_edition_or_acquisition_kind_is_not_guessed(self):
        ledger = self.with_reference_standard()
        del ledger["standards"][1]["edition"]
        with self.assertRaises(ValidationError):
            validate(ledger, "traceability")
        ledger = self.with_reference_standard()
        del ledger["source_consultations"][1]["acquisition_kind"]
        with self.assertRaises(ValidationError):
            validate(ledger, "traceability")

    def test_local_document_hashes_are_named_and_not_inferred_from_the_first_standard(self):
        ledger = self.with_reference_standard()
        self.write("fixture-t88.pdf", "synthetic first document")
        self.write("fixture-t6.pdf", "synthetic reference document")
        for standard, path in zip(ledger["standards"], ("fixture-t88.pdf", "fixture-t6.pdf")):
            standard["document_sha256"] = hashlib.sha256((self.root / path).read_bytes()).hexdigest()
        t6 = f"ITU-T T.6 (1988)={self.root / 'fixture-t6.pdf'}"
        checks = local_document_checks(ledger, standard_documents=[t6])
        self.assertEqual(checks, {self.standard["id"]: False, "ITU-T T.6 (1988)": True})
        self.assertFalse(all(checks.values()))
        checks = local_document_checks(ledger, self.root / "fixture-t88.pdf", [t6])
        self.assertTrue(all(checks.values()))
        with self.assertRaisesRegex(ValueError, "hash mismatch: ITU-T T.6"):
            local_document_checks(ledger, standard_documents=[
                f"ITU-T T.6 (1988)={self.root / 'fixture-t88.pdf'}",
            ])

    def test_named_local_documents_reject_duplicates_unknown_ids_and_missing_names(self):
        for selection in ("missing separator", "=fixture.pdf", "ITU-T T.6 (1988)="):
            with self.subTest(selection=selection), self.assertRaisesRegex(ValueError, "needs STANDARD_ID=PATH"):
                local_document_checks(self.ledger, standard_documents=[selection])
        with self.assertRaisesRegex(ValueError, "unknown local normative document"):
            local_document_checks(self.ledger, standard_documents=["unknown=fixture.pdf"])
        ledger = copy.deepcopy(self.ledger)
        self.write("fixture.pdf", "synthetic document")
        path = self.root / "fixture.pdf"
        ledger["standards"][0]["document_sha256"] = hashlib.sha256(path.read_bytes()).hexdigest()
        selection = f"{self.standard['id']}={path}"
        with self.assertRaisesRegex(ValueError, "duplicate local normative document"):
            local_document_checks(ledger, path, [selection])

    def test_typed_error_policy_has_a_separate_approved_public_api_binding(self):
        matrix = load_json(self.root / "support-matrix.json")
        matrix["public_api"] = {
            "status": "approved",
            "requirements": ["Typed malformed, unsupported, resource-limit, input/output, and allocation errors."],
        }
        self.write_json("support-matrix.json", matrix)
        ledger = copy.deepcopy(self.ledger)
        ledger["requirements"][0].update(
            obligation_kind="project_profile_policy", policy_source="public_api",
            policy_row_sha256=policy_row_hash(matrix["public_api"]),
        )
        validate(ledger, "traceability")
        report = self.run_check(ledger)
        self.assertEqual(report["requirements"][0]["policy_source"], "public_api")
        matrix["public_api"]["requirements"].append("Changed policy scope.")
        self.write_json("support-matrix.json", matrix)
        with self.assertRaisesRegex(ValueError, "stale or missing project policy"):
            self.run_check(ledger)
        matrix["public_api"]["status"] = "pending"
        self.write_json("support-matrix.json", matrix)
        with self.assertRaisesRegex(ValueError, "no approved public API basis"):
            self.run_check(ledger)

    def test_public_api_policy_cannot_omit_the_typed_error_basis_or_change_selector(self):
        ledger = copy.deepcopy(self.ledger)
        ledger["requirements"][0].update(
            obligation_kind="project_profile_policy", policy_source="public_api", policy_row_sha256="c" * 64,
        )
        self.write_json("support-matrix.json", {
            "rows": [{"feature": "decoder", "status": "required"}],
            "public_api": {"status": "approved", "requirements": ["Unrelated API policy."]},
        })
        with self.assertRaisesRegex(ValueError, "no approved public API basis"):
            self.run_check(ledger)
        ledger["requirements"][0]["policy_source"] = "unrecorded"
        with self.assertRaisesRegex(ValueError, "unknown project policy source"):
            self.run_check(ledger)

    def test_policy_selector_cannot_be_hidden_on_a_normative_record(self):
        ledger = copy.deepcopy(self.ledger)
        ledger["requirements"][0]["policy_source"] = "public_api"
        with self.assertRaises(ValidationError):
            validate(ledger, "traceability")
        with self.assertRaisesRegex(ValueError, "cannot hide a project policy"):
            self.run_check(ledger)

    def test_checked_repository_ledger_is_schema_valid_and_provisional(self):
        ledger = load_json(TOOLS.parent / "validation/traceability/requirements.json")
        validate(ledger, "traceability")
        self.assertEqual(len(ledger["requirements"]), 249)
        self.assertTrue(all(entry["review"] == "provisional" for entry in ledger["requirements"]))

    def test_selected_arithmetic_algorithm_membership_is_exact_and_unapproved(self):
        ledger = load_json(TOOLS.parent / "validation/traceability/requirements.json")
        rows = {row["id"].removeprefix("T88-ARITHMETIC-"): row
                for row in ledger["requirements"] if row["feature"] == "arithmetic_coding"}
        groups = {
            "INTEGER": [
                "PROCEDURE-BANKS", "HISTORY-SEED", "CONTEXT-SELECT", "HISTORY-GROW",
                "HISTORY-ROLL", "CONTEXT-STORAGE", "SIGN-DECISION", "PAYLOAD-ORDER",
                "RESULT-NONNEGATIVE", "RESULT-NEGATIVE", "RESULT-OOB",
            ] + [f"MAGNITUDE-LEAF-{i}" for i in range(1, 7)]
              + [f"TABLE-ROW-{i:02}" for i in range(1, 15)],
            "IAID": [
                "LENGTH", "SEED", "CONTEXT", "HISTORY", "ITERATIONS",
                "CLEAR-SENTINEL", "RESULT", "STORAGE",
            ],
            "DRIVER": ["INIT", "CONTEXT-SEQUENCE"],
            "REGISTER": ["HIGH-LOW", "LOW-INPUT", "UNSIGNED"],
            "DECODE": [
                "QE-LOOKUP", "INTERVAL-SUBTRACT", "CODE-PARTITION",
                "UPPER-CODE-SUBTRACT", "FAST-MPS", "UPPER-DISPATCH",
                "LOWER-DISPATCH", "RETURN-DECISION",
            ],
            "EXCHANGE-MPS": ["COMPARE", "MPS-RESULT", "LPS-RESULT", "SWITCH", "NLPS"],
            "EXCHANGE-LPS": ["COMPARE", "MPS-RESULT", "LPS-RESULT", "SWITCH", "NLPS", "INTERVAL"],
            "RENORM": ["REFILL", "INTERVAL-SHIFT", "CODE-SHIFT", "COUNTER", "STOP"],
            "BYTEIN": [
                "BRANCH", "MARKER-THRESHOLD", "MARKER-POINTER", "MARKER-FILL",
                "MARKER-COUNTER", "STUFFED-POINTER", "STUFFED-ALIGN",
                "STUFFED-COUNTER", "ORDINARY-POINTER", "ORDINARY-ALIGN", "ORDINARY-COUNTER",
            ],
            "INITDEC": ["POINTER", "CODE-SEED", "BYTEIN", "CODE-ALIGN", "COUNTER", "INTERVAL"],
            "QE": ["ESTIMATE-SCALE", "UPDATE-TIMING", "NMPS", "NLPS-SWITCH", "FIXED-STATE"]
                  + [f"TABLE-INDEX-{i:02}" for i in range(47)],
        }
        original = {"RESET-ONE", "RESET-SET", "DECISION"}
        added = {"INTERFACE"} | {f"{group}-{suffix}"
                                 for group, suffixes in groups.items() for suffix in suffixes}
        self.assertEqual(set(rows), original | added)
        self.assertEqual(len(added), 138)
        for name in added:
            with self.subTest(requirement=name):
                row = rows[name]
                self.assertEqual(row["review"], "provisional")
                self.assertEqual(row["strength"], "state_preservation")
                self.assertEqual(row["functional_evidence"], [])
                self.assertTrue(row["invariant_evidence"])
                self.assertTrue(row["gaps"])
                self.assertEqual(row["standard"], "ITU-T T.88 (08/2018)")
                self.assertTrue(all(name.startswith("arithmetic::") for name in row["runtime"]))
        feature = next(f for f in ledger["features"] if f["feature"] == "arithmetic_coding")
        self.assertEqual(feature["decomposition"], "partial")
        self.assertTrue(feature["remaining_scope"])
        self.assertEqual(rows["RESET-ONE"]["strength"], "functional_contract")
        self.assertEqual(rows["RESET-SET"]["strength"], "functional_contract")
        self.assertEqual(rows["DECISION"]["strength"], "state_preservation")

    def test_selected_arithmetic_table_locators_and_checked_links_are_not_shifted(self):
        ledger = load_json(TOOLS.parent / "validation/traceability/requirements.json")
        rows = {row["id"]: row for row in ledger["requirements"]}
        for index in range(47):
            row = rows[f"T88-ARITHMETIC-QE-TABLE-INDEX-{index:02}"]
            with self.subTest(index=index):
                self.assertEqual(
                    row["clause"],
                    f"E.2.5, Table E.1, index {index}, Qe/NMPS/NLPS/SWITCH columns (p. 116)",
                )
                self.assertEqual(row["proof_macros"], ["arithmetic-qe-transition-bounds"])
                self.assertIn({
                    "target": "lib",
                    "name": "arithmetic::tests::qe_table_matches_annex_e_table_e_1",
                }, row["tests"])
                self.assertEqual(row["runtime"], [
                    "arithmetic::ArithmeticDecoder::decode_signal",
                    "arithmetic::ArithmeticDecoder::decode_lower_signal",
                    "arithmetic::ArithmeticDecoder::decode_upper_signal",
                ])
        for index in range(1, 15):
            row = rows[f"T88-ARITHMETIC-INTEGER-TABLE-ROW-{index:02}"]
            with self.subTest(integer_table_row=index):
                self.assertTrue(row["clause"].startswith(f"A.2, Table A.1, row {index} ("))
                self.assertTrue(row["clause"].endswith("(p. 91)"))
                self.assertEqual(row["runtime"], [
                    "arithmetic::ArithmeticDecoder::decode_integer",
                    "arithmetic::ArithmeticDecoder::decode_integer_bits",
                ])
        for name in ("LENGTH", "SEED", "CONTEXT", "HISTORY", "ITERATIONS",
                     "CLEAR-SENTINEL", "RESULT", "STORAGE"):
            row = rows[f"T88-ARITHMETIC-IAID-{name}"]
            self.assertTrue({"arithmetic-binary-value", "arithmetic-pow2"} <= set(row["models"]))
            self.assertTrue(any("assumed decision sequence" in text for text in row["assumptions"]))

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

    def test_all_feature_worklists_have_started_without_claiming_full_decomposition(self):
        ledger = load_json(TOOLS.parent / "validation/traceability/requirements.json")
        self.assertEqual(sum(feature["decomposition"] == "partial" for feature in ledger["features"]), 29)
        self.assertEqual(sum(feature["decomposition"] == "excluded" for feature in ledger["features"]), 1)
        self.assertTrue(all(feature["decomposition"] != "complete" for feature in ledger["features"]))
        self.assertTrue(all(feature["remaining_scope"] for feature in ledger["features"]))
        policy = [row for row in ledger["requirements"] if row.get("obligation_kind") == "project_profile_policy"]
        self.assertEqual(len(policy), 9)
        self.assertEqual({row["feature"] for row in policy}, {
            "pdf_global_and_page_segments", "colour_palette", "lossy_symbol_substitution_by_decoder", "mmr_coding",
        })
        for row in policy:
            self.assertTrue(row["policy_row_sha256"])
            self.assertTrue(row["gaps"])

    def test_repository_t6_source_and_records_are_distinct_and_provisional(self):
        ledger = load_json(TOOLS.parent / "validation/traceability/requirements.json")
        standard = next(entry for entry in ledger["standards"] if entry["id"] == "ITU-T T.6 (1988)")
        self.assertIsNone(standard["edition"])
        self.assertEqual(standard["provenance_collection"], "normative_references")
        self.assertEqual(standard["document_sha256"], "8ccbe2d203e03ee2ac37478d50d2d946bab49dfb45376eb0f773009d183a7767")
        consultation = next(entry for entry in ledger["source_consultations"] if entry["standard"] == standard["id"])
        self.assertEqual(consultation["acquisition_kind"], "official_pdf")
        self.assertNotIn("package_sha256", consultation)
        rows = [row for row in ledger["requirements"] if row["id"].startswith("T6-")]
        self.assertEqual(len(rows), 17)
        self.assertTrue(all(row["standard"] == standard["id"] and row["review"] == "provisional" and row["gaps"] for row in rows))
        self.assertEqual(sum(row.get("obligation_kind") == "project_profile_policy" for row in rows), 3)

    def test_new_page_records_do_not_claim_transitive_kernel_or_pixel_equivalence(self):
        ledger = load_json(TOOLS.parent / "validation/traceability/requirements.json")
        rows = {row["id"]: row for row in ledger["requirements"]}
        for name in ("T88-IMMEDIATE-DIRECT-PAGE", "T88-IMMEDIATE-CLIPPED-PAGE"):
            self.assertTrue(all(runtime.startswith("decode::compose_immediate") for runtime in rows[name]["runtime"]))
            self.assertEqual(rows[name]["strength"], "state_preservation")
            self.assertEqual(rows[name]["functional_evidence"], [])
            self.assertTrue(rows[name]["invariant_evidence"])
            self.assertIn("audited inventory", " ".join(rows[name]["assumptions"]))
            self.assertTrue(rows[name]["gaps"])

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

import copy
import contextlib
import io
import json
import hashlib
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from evidence import load_json, validate
from jsonschema import ValidationError
from rust_source import production_code, source_functions
from audit_production_coverage import verified_matches
import audit_production_coverage as coverage_audit


class ConformanceExpectationSchemaTests(unittest.TestCase):
    def expectation(self):
        return {
            "schema_version": 1,
            "cases": [{
                "id": "original-fixture", "classification": "pending_normative_review",
                "rationale": "Source-bound observation, not normative approval",
                "expected_error_exact": "Error: original fixture",
                "stream_sha256": "1" * 64,
                "source_files_sha256": {"src/reader.rs": "2" * 64},
            }],
        }

    def test_source_bound_pending_diagnosis_schema(self):
        validate(self.expectation(), "conformance_differences")
        document = self.expectation()
        document["cases"][0]["source_files_sha256"] = {"src/bitmap/compose.rs": "2" * 64}
        validate(document, "conformance_differences")

    def test_reviewed_rejections_keep_exact_errors_and_source_bindings(self):
        document = self.expectation()
        document["cases"][0]["classification"] = "reviewed_strict_rejection"
        validate(document, "conformance_differences")
        for field in ("expected_error_exact", "stream_sha256", "source_files_sha256"):
            changed = copy.deepcopy(document)
            del changed["cases"][0][field]
            with self.subTest(field=field), self.assertRaises(ValidationError):
                validate(changed, "conformance_differences")
        document["cases"][0]["expected_error_contains"] = "original fixture"
        with self.assertRaises(ValidationError):
            validate(document, "conformance_differences")

    def test_pending_review_requires_exact_error_and_safe_hash_identities(self):
        for overrides in (
            {"expected_error_exact": ""}, {"expected_error_contains": "fixture"},
            {"stream_sha256": "not-a-hash"}, {"source_files_sha256": {}},
            {"source_files_sha256": {"../src/reader.rs": "2" * 64}},
            {"source_files_sha256": {"src/../reader.rs": "2" * 64}},
            {"source_files_sha256": {"src/bitmap//compose.rs": "2" * 64}},
            {"source_files_sha256": {"src/bitmap/compose.txt": "2" * 64}},
            {"source_files_sha256": {"src/reader.rs": True}},
            {"classification": "passed"}, {"extra": "not accepted"},
        ):
            with self.subTest(overrides=overrides):
                document = self.expectation()
                document["cases"][0].update(overrides)
                with self.assertRaises(ValidationError):
                    validate(document, "conformance_differences")
        for key in ("expected_error_exact", "stream_sha256", "source_files_sha256"):
            document = self.expectation()
            del document["cases"][0][key]
            with self.assertRaises(ValidationError):
                validate(document, "conformance_differences")

    def test_duplicate_case_identity_fails(self):
        document = self.expectation()
        document["cases"].append(copy.deepcopy(document["cases"][0]))
        with self.assertRaisesRegex(ValueError, "duplicate conformance"):
            validate(document, "conformance_differences")

    def test_review_source_byte_identities_have_lf_checkout_policy(self):
        source = TOOLS.parent
        review = load_json(source / "validation/conformance/seven-rejection-review.json")
        paths = sorted(
            {".gitattributes", "verification/stdlib/vec-capacity-interface/Cargo.toml",
             "verification/stdlib/vec-capacity-interface/lib.rs"}
            | set(review["source_files_sha256"]) | set(review["validation_files_sha256"])
        )
        process = subprocess.run(
            ["git", "check-attr", "eol", "--", *paths],
            cwd=source, capture_output=True, text=True, check=True,
        )
        self.assertEqual(len(process.stdout.splitlines()), len(paths))
        for line in process.stdout.splitlines():
            self.assertTrue(line.endswith(": eol: lf"), line)
        for path in paths:
            self.assertNotIn(b"\r\n", (source / path).read_bytes(), path)

    def test_provenance_rejects_stale_hash_count_and_rehashed_source_claim(self):
        source = TOOLS.parent
        review_path = "validation/conformance/seven-rejection-review.json"
        review = load_json(source / review_path)
        paths = {
            ".gitattributes",
            "tools/verify_provenance.py", "tools/evidence.py", "tools/evidence.schema.json",
            "config/support-matrix.json", "validation/source-provenance.json", "verification/proof-attestation.json",
            "validation/conformance/attestation.json",
            "validation/schemas/release-attestation.schema.json", "config/toolchain.json",
            "verification/production-proof-coverage.json", "verification/proof-dispositions.json",
            "verification/Cargo.toml", "verification/stdlib/review.json",
            "verification/stdlib/vec-capacity-interface/Cargo.toml",
            "verification/stdlib/vec-capacity-interface/lib.rs",
            "validation/conformance/expected-differences.json",
            "validation/conformance/itu-t88-2018-manifest.json",
            "validation/conformance/itu-t88-2018-results.json", review_path,
            *review["source_files_sha256"], *review["validation_files_sha256"],
        }
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for relative in paths:
                destination = root / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(source / relative, destination)
            attestation_path = root / "validation/conformance/attestation.json"
            original_attestation = attestation_path.read_text()
            original_review_bytes = (root / review_path).read_bytes()
            original_review = original_review_bytes.decode("utf-8")

            def audit(*arguments):
                return subprocess.run(
                    [sys.executable, "-B", str(root / "tools/verify_provenance.py"), *arguments],
                    cwd=root, capture_output=True, text=True, check=False,
                )

            baseline = audit()
            self.assertEqual(baseline.returncode, 0, baseline.stderr)
            strict = audit("--require-exact-conformance")
            self.assertEqual(strict.returncode, 1)
            self.assertIn("exact conformance gate blocked", strict.stderr)
            failures = {
                "hash": "stale conformance attestation hash",
                "count": "stale conformance attestation count",
                "source": "stale conformance review identity: src/mmr.rs",
                "source_inventory": "complete runtime source inventory",
                "crlf": "stale conformance review identity: tools/evidence.py",
                "policy": "stale conformance checkout policy identity",
                "live_source": "review source hash mismatch: codeStreamTest1_TT1: src/reader.rs",
                "root_artifact": "repository-root JSON belongs in",
            }
            if review.get("owner_disposition"):
                failures.update({
                    "decision_absent": "explicit bounded owner disposition",
                    "decision_normative": "explicit bounded owner disposition",
                    "decision_full_spec": "explicit bounded owner disposition",
                    "decision_subset": "explicit bounded owner disposition",
                    "decision_naive_time": "timestamp with timezone",
                    "decision_evidence": "explicit bounded owner disposition",
                    "decision_reference": "explicit bounded owner disposition",
                    "decision_release": "explicit bounded owner disposition",
                    "decision_status": "preserve the separate release gate",
                })
            for mutation, expected_error in failures.items():
                with self.subTest(mutation=mutation):
                    document = json.loads(original_attestation)
                    (root / review_path).write_bytes(original_review_bytes)
                    tool = root / "tools/evidence.py"
                    tool.write_bytes((source / "tools/evidence.py").read_bytes())
                    attributes = root / ".gitattributes"
                    attributes.write_bytes((source / ".gitattributes").read_bytes())
                    reader = root / "src/reader.rs"
                    reader.write_bytes((source / "src/reader.rs").read_bytes())
                    tracking = root / "tracking.json"
                    tracking.unlink(missing_ok=True)
                    if mutation == "hash":
                        document["normative_vectors"]["review_sha256"] = "0" * 64
                    elif mutation == "count":
                        document["normative_vectors"]["pending_normative_reviews"] += 1
                    elif mutation == "crlf":
                        tool.write_bytes(tool.read_bytes().replace(b"\n", b"\r\n"))
                    elif mutation == "policy":
                        attributes.write_bytes(attributes.read_bytes() + b"# fixture mutation\n")
                    elif mutation == "live_source":
                        reader.write_bytes(reader.read_bytes() + b"// fixture mutation\n")
                    elif mutation == "root_artifact":
                        tracking.write_text("{}")
                    elif mutation.startswith("decision_"):
                        changed = json.loads(original_review)
                        decision = changed["owner_disposition"]
                        if mutation == "decision_absent":
                            del changed["owner_disposition"]
                        elif mutation == "decision_normative":
                            decision["normative_approval"] = True
                        elif mutation == "decision_full_spec":
                            decision["full_spec_conformance"] = True
                        elif mutation == "decision_subset":
                            decision["case_ids"].pop()
                        elif mutation == "decision_naive_time":
                            decision["authorized_at"] = "2026-11-02T00:13:38"
                        elif mutation == "decision_evidence":
                            decision["evidence_revision"] = ""
                        elif mutation == "decision_reference":
                            decision["decision_reference"] = "https://example.com/approval"
                        elif mutation == "decision_release":
                            decision["release_gate"] = "ready"
                        elif mutation == "decision_status":
                            document["release_gate"] = "ready"
                        (root / review_path).write_text(json.dumps(changed))
                        document["normative_vectors"]["review_sha256"] = hashlib.sha256(
                            (root / review_path).read_bytes()
                        ).hexdigest()
                    else:
                        changed = json.loads(original_review)
                        if mutation == "source_inventory":
                            del changed["source_files_sha256"]["src/mmr.rs"]
                        else:
                            changed["source_files_sha256"]["src/mmr.rs"] = "0" * 64
                        (root / review_path).write_bytes(json.dumps(changed).encode("utf-8"))
                        document["normative_vectors"]["review_sha256"] = hashlib.sha256(
                            (root / review_path).read_bytes()
                        ).hexdigest()
                    attestation_path.write_text(json.dumps(document))
                    result = audit()
                    tracking.unlink(missing_ok=True)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn("provenance audit failed", result.stderr)
                    self.assertIn(expected_error, result.stderr)


class RustAuditTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        (self.root / "src").mkdir()

    def write(self, name, source):
        path = self.root / "src" / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(source, encoding="utf-8")

    def inventory(self, source):
        self.write("lib.rs", source)
        return source_functions(self.root)

    def test_production_after_test_module_and_test_item(self):
        functions = self.inventory("""
            #[cfg(test)] mod tests { fn skipped() {} }
            #[cfg(test)] fn also_skipped() {}
            #[cfg(not(test))]
            #[cfg_attr(verus_keep_ghost, verus_verify)]
            pub async unsafe extern "C" fn retained() {}
        """)
        self.assertEqual([f["id"] for f in functions], ["lib::retained"])
        self.assertTrue(functions[0]["verification_requested"])

    def test_nested_cfg_scopes_and_unknown_platforms(self):
        functions = self.inventory("""
            #[cfg(all(test, unix))] mod tests { fn skipped() {} }
            #[cfg(any(test, verus_keep_ghost))] fn oracle() {}
            #[cfg(unix)] mod platform { pub fn retained() {} }
            #[cfg(windows)] fn other_platform() {}
            #[cfg_attr(verus_keep_ghost, verus_verify)]
            impl Example { fn method() {} }
        """)
        self.assertEqual([f["id"] for f in functions], [
            "lib::platform::retained", "lib::other_platform", "lib::Example::method",
        ])
        self.assertTrue(functions[-1]["verification_requested"])

    def test_path_included_and_nested_directory_modules(self):
        self.write("custom.rs", '#[cfg_attr(verus_keep_ghost, verus_verify)] fn path_fn() {}')
        self.write("outer.rs", "mod inner { mod leaf; }")
        self.write("outer/inner/leaf.rs", "fn deep() {}")
        functions = self.inventory('#[path="custom.rs"] mod alias; mod outer;')
        by_name = {f["name"]: f for f in functions}
        self.assertEqual(by_name["path_fn"]["verifier_module"], "alias")
        self.assertEqual(by_name["deep"]["verifier_module"], "outer::inner::leaf")

    def test_test_only_modules_do_not_require_files(self):
        self.assertEqual(self.inventory("#[cfg(test)] mod absent;"), [])

    def test_file_wide_cfg(self):
        self.assertEqual(self.inventory("#![cfg(test)] fn skipped() {}"), [])

    def test_inventory_normalizes_root_aliases(self):
        expected = self.inventory("fn retained() {}")
        self.assertEqual(source_functions(self.root / "src" / ".."), expected)

    def test_unknown_and_ambiguous_modules_fail(self):
        with self.assertRaisesRegex(ValueError, "missing or ambiguous"):
            self.inventory("mod missing;")
        self.write("both.rs", "")
        self.write("both/mod.rs", "")
        with self.assertRaisesRegex(ValueError, "missing or ambiguous"):
            self.inventory("mod both;")

    def test_runtime_and_proof_algorithms_are_not_conflated(self):
        functions = self.inventory("""
            #[cfg_attr(verus_keep_ghost, verus_verify)]
            fn decode() {
                #[cfg(verus_keep_ghost)] { oracle(); }
                #[cfg(not(verus_keep_ghost))] { executing_path(); }
            }
        """)
        self.assertTrue(functions[0]["runtime_proof_divergence"])
        self.assertEqual(verified_matches(functions[0], {"jbig2_rs_production::decode"}), [])

    def test_exact_owner_and_verification_request_are_required(self):
        functions = self.inventory("""
            struct First; struct Second;
            #[cfg_attr(verus_keep_ghost, verus_verify)]
            impl First { fn new() {} }
            impl Second { fn new() {} }
        """)
        names = {"jbig2_rs_production::First::new"}
        self.assertEqual(verified_matches(functions[0], names), list(names))
        self.assertEqual(verified_matches(functions[1], names), [])
        unrequested = dict(functions[0], verification_requested=False)
        self.assertEqual(verified_matches(unrequested, names), [])

    def test_unsupported_syntax_cannot_silently_disappear(self):
        for source in (
            "fn broken( {",
            "#[cfg_attr(test, cfg(unix))] fn conditional() {}",
            "fn outer() { fn local() {} }",
            "macro_rules! make { () => { fn generated() {} }; }",
        ):
            with self.subTest(source=source), self.assertRaises(ValueError):
                self.inventory(source)

    def test_code_mask_preserves_lines_and_actual_trust_tokens(self):
        source = b'''
            // panic!("not code");
            #[cfg(test)] fn ignored() { panic!("test"); }
            fn after() { let text = r###"panic!( /* raw */"###; assume(false); }
        '''
        code = production_code(source, exclude_proofs=False)
        self.assertEqual(code.count("\n"), source.count(b"\n"))
        self.assertNotIn("panic!", code)
        self.assertIn("assume(false)", code)

    def test_ghost_macro_does_not_swallow_following_runtime_statement(self):
        source = b"""
            fn f(output: &mut i32) -> (i32, i32) {
                #[cfg(verus_keep_ghost)] proof! { assert(true); }
                *output = 1;
                #[cfg(verus_keep_ghost)] proof! { assert(true); }
                (1, 2)
            }
        """
        code = production_code(source, mask_noncode=False)
        self.assertIn("*output = 1", code)
        self.assertIn("(1, 2)", code)
        self.assertNotIn("proof!", code)
        self.assertNotIn("assert(true)", code)
        self.assertEqual(len(code.encode()), len(source))

    def run_gate(self, details, dispositions=(), *options):
        report = self.root / "verus.json"
        report.write_text(json.dumps({
            "verification-results": {
                "success": True, "errors": 0, "verified": 1,
                "is-verifying-entire-crate": True,
            },
            "func-details": {name: {"failed_proof_notes": []} for name in details},
        }), encoding="utf-8")
        reviews = self.root / "reviews.json"
        reviews.write_text(json.dumps({
            "schema_version": 1, "policy": "explicit review", "functions": list(dispositions),
        }), encoding="utf-8")
        argv = [
            "audit", "--verus-json", str(report), "--dispositions", str(reviews),
            "--output", str(self.root / "coverage.json"), *options,
        ]
        with (
            patch.object(coverage_audit, "ROOT", self.root),
            patch.object(sys, "argv", argv),
            contextlib.redirect_stdout(io.StringIO()),
        ):
            coverage_audit.main()

    def test_incomplete_cannot_be_satisfied_by_another_constructor(self):
        self.inventory("""
            struct Missing;
            #[cfg_attr(verus_keep_ghost, verus_verify)]
            impl Missing { fn new() {} }
        """)
        with self.assertRaisesRegex(ValueError, "remain incomplete"):
            self.run_gate(["jbig2_rs_production::Other::new"], (), "--require-complete")

    def test_stale_unused_and_duplicate_reviews_fail(self):
        functions = self.inventory("fn unverified() {}")
        review = {
            key: functions[0][key] for key in ("id", "file", "file_sha256")
        } | {"status": "reviewed-verifier-limitation", "reason": "explicit test review"}
        self.run_gate(["jbig2_rs_production::other"], [review], "--require-complete")
        for changes in ({"file_sha256": "a" * 64}, {"id": "unused::function"}):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                self.run_gate(["jbig2_rs_production::other"], [review | changes])
        with self.assertRaisesRegex(ValueError, "duplicate"):
            self.run_gate(["jbig2_rs_production::other"], [review, review])

    def test_reviewed_divergence_does_not_pass_exact_release_gate(self):
        functions = self.inventory("""
            #[cfg_attr(verus_keep_ghost, verus_verify)]
            fn f() { #[cfg(not(verus_keep_ghost))] executing_path(); }
        """)
        review = {
            key: functions[0][key] for key in ("id", "file", "file_sha256")
        } | {"status": "runtime-proof-divergence", "reason": "explicit test divergence"}
        self.run_gate(["jbig2_rs_production::f"], [review], "--require-complete")
        with self.assertRaisesRegex(ValueError, "exact runtime proof gate blocked"):
            self.run_gate(["jbig2_rs_production::f"], [review], "--require-exact")


class EvidenceTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def test_duplicate_keys_and_nonfinite_numbers_fail(self):
        path = self.root / "report.json"
        for text in ('{"x":1,"x":2}', '{"x":NaN}', '{"x":Infinity}', '{"x":{"a":0,"a":1}}'):
            path.write_text(text, encoding="utf-8")
            with self.subTest(text=text), self.assertRaises(ValueError):
                load_json(path)

    def test_dispositions_require_review_and_source_identity(self):
        disposition = {
            "id": "module::function", "file": "src/module.rs", "file_sha256": "a" * 64,
            "status": "audited-forwarder", "reason": "explicit source review",
        }
        document = {"schema_version": 1, "policy": "explicit review", "functions": [disposition]}
        validate(document, "dispositions")
        for change in ({"status": "unreviewed"}, {"status": "native-assumption"},
                       {"reason": " "}, {"file_sha256": "stale"}):
            broken = copy.deepcopy(document)
            broken["functions"][0].update(change)
            with self.subTest(change=change), self.assertRaises(ValidationError):
                validate(broken, "dispositions")
        document["functions"].append(disposition)
        with self.assertRaisesRegex(ValueError, "duplicate"):
            validate(document, "dispositions")

    def test_only_successful_whole_crate_reports_are_accepted(self):
        report = {
            "verification-results": {
                "success": True, "errors": 0, "verified": 1,
                "is-verifying-entire-crate": True,
            },
            "func-details": {"crate::f": {"failed_proof_notes": []}},
        }
        validate(report, "verus")
        for change in ({"success": False}, {"errors": 1}, {"verified": 0},
                       {"is-verifying-entire-crate": False}):
            broken = copy.deepcopy(report)
            broken["verification-results"].update(change)
            with self.subTest(change=change), self.assertRaises(ValidationError):
                validate(broken, "verus")
        report["func-details"]["crate::f"]["failed_proof_notes"] = ["failure"]
        with self.assertRaises(ValidationError):
            validate(report, "verus")

    def test_disposition_templates_cannot_overwrite_reviewed_file(self):
        process = subprocess.run(
            [sys.executable, str(TOOLS / "update_proof_dispositions.py"),
             "--output", str(TOOLS.parent / "verification/proof-dispositions.json")],
            capture_output=True, text=True, check=False,
        )
        self.assertNotEqual(process.returncode, 0)
        self.assertIn("must not overwrite", process.stderr)

    def test_coverage_counts_and_verified_matches_are_not_invented(self):
        document = load_json(TOOLS.parent / "verification/production-proof-coverage.json")
        validate(document, "coverage")
        broken = copy.deepcopy(document)
        broken["counts"]["verified"] += 1
        with self.assertRaisesRegex(ValueError, "inconsistent"):
            validate(broken, "coverage")
        broken = copy.deepcopy(document)
        verified = next(entry for entry in broken["functions"] if entry["status"] == "verified")
        verified["verus_functions"] = []
        with self.assertRaisesRegex(ValueError, "invalid verified"):
            validate(broken, "coverage")

    def test_disposition_generator_emits_only_unreviewed_templates(self):
        document = load_json(TOOLS.parent / "verification/production-proof-coverage.json")
        entry = next(entry for entry in document["functions"] if entry["status"] == "verified")
        entry["status"] = "incomplete"
        entry["verus_functions"] = []
        document["counts"]["verified"] -= 1
        document["counts"]["incomplete"] += 1
        coverage = self.root / "coverage.json"
        output = self.root / "proposals.json"
        coverage.write_text(json.dumps(document), encoding="utf-8")
        command = [
            sys.executable, str(TOOLS / "update_proof_dispositions.py"),
            "--coverage", str(coverage), "--output", str(output),
        ]
        process = subprocess.run(command, capture_output=True, text=True, check=False)
        self.assertEqual(process.returncode, 0, process.stderr)
        proposal = load_json(output)
        self.assertEqual(proposal["functions"][0]["status"], "unreviewed")
        with self.assertRaises(ValidationError):
            validate(proposal, "dispositions")
        process = subprocess.run(command, capture_output=True, text=True, check=False)
        self.assertNotEqual(process.returncode, 0)


if __name__ == "__main__":
    unittest.main()

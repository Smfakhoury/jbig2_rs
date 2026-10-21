import copy
import hashlib
import io
import json
import subprocess
import sys
import tempfile
import unittest
from contextlib import redirect_stderr
from pathlib import Path
from unittest.mock import patch


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from check_test_catalogs import check, main
from evidence import validate
from jsonschema import ValidationError
from rust_tests import discover_tests


class TestCatalogTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        (self.root / "src").mkdir()
        (self.root / "src/lib.rs").write_text("mod leaf;\n", encoding="utf-8")
        self.source = self.root / "src/leaf.rs"
        self.source.write_text(
            "#[cfg(test)] mod tests {\n"
            "  #[test] fn negative() {}\n"
            "  #[test] fn metamorphic() {}\n"
            "  #[test] #[ignore] fn ignored() {}\n"
            "  fn helper() {}\n"
            "}\n", encoding="utf-8",
        )
        self.documents = {
            "negative": {"schema_version": 1, "cases": [{
                "id": "NEG-EXAMPLE", "surface": "generated fixture", "expected": "typed error",
                "tests": [{"target": "lib", "name": "leaf::tests::negative"}],
            }]},
            "metamorphic": {"schema_version": 1, "cases": [{
                "id": "META-EXAMPLE", "transformation": "repeat", "invariant": "same result",
                "tests": [{"target": "lib", "name": "leaf::tests::metamorphic"}],
            }]},
        }
        self.discovered = {("lib", "leaf::tests::negative"), ("lib", "leaf::tests::metamorphic")}

    def test_exact_links_locations_and_source_hashes_are_reported_without_execution_claim(self):
        report = check(self.documents, self.root, self.discovered)
        self.assertEqual(report["case_count"], 2)
        self.assertEqual(report["reference_count"], 2)
        self.assertEqual(report["unique_test_count"], 2)
        self.assertTrue(report["cargo_test_discovery_checked"])
        self.assertFalse(report["tests_executed_by_this_command"])
        self.assertEqual(report["cases"][0]["test_locations"][0]["line"], 2)
        self.assertEqual(
            report["source_bindings"]["src/leaf.rs"], hashlib.sha256(self.source.read_bytes()).hexdigest(),
        )
        self.assertFalse(check(self.documents, self.root)["cargo_test_discovery_checked"])

    def test_empty_missing_and_legacy_references_fail_schema_validation(self):
        for change in ("empty_catalog", "missing_tests", "empty_tests", "legacy_coverage"):
            document = copy.deepcopy(self.documents["negative"])
            if change == "empty_catalog":
                document["cases"] = []
            elif change == "missing_tests":
                del document["cases"][0]["tests"]
            elif change == "empty_tests":
                document["cases"][0]["tests"] = []
            else:
                document["cases"][0]["coverage"] = "src/leaf.rs"
            with self.subTest(change=change), self.assertRaises(ValidationError):
                validate(document, "test_catalog")

    def test_duplicate_test_references_and_extra_reference_fields_fail(self):
        for change in ("duplicate", "extra"):
            documents = copy.deepcopy(self.documents)
            case = documents["negative"]["cases"][0]
            if change == "duplicate":
                case["tests"].append(copy.deepcopy(case["tests"][0]))
            else:
                case["tests"][0]["coverage"] = "not a test"
            with self.subTest(change=change), self.assertRaises(ValidationError):
                check(documents, self.root)

    def test_duplicate_case_identifiers_fail(self):
        documents = copy.deepcopy(self.documents)
        documents["negative"]["cases"].append(copy.deepcopy(documents["negative"]["cases"][0]))
        with self.assertRaisesRegex(ValueError, "duplicate case"):
            check(documents, self.root)

    def test_case_kinds_cannot_mix_negative_and_metamorphic_fields(self):
        documents = copy.deepcopy(self.documents)
        documents["negative"]["cases"][0]["transformation"] = "wrong case kind"
        with self.assertRaises(ValidationError):
            check(documents, self.root)

    def test_both_catalogs_and_correct_case_kind_are_required(self):
        with self.assertRaisesRegex(ValueError, "both negative and metamorphic"):
            check({"negative": self.documents["negative"]}, self.root)
        documents = copy.deepcopy(self.documents)
        documents["negative"] = copy.deepcopy(documents["metamorphic"])
        with self.assertRaisesRegex(ValueError, "wrong catalog kind"):
            check(documents, self.root)

    def test_missing_ignored_non_test_and_wrong_target_references_fail(self):
        for target, name in (
            ("lib", "leaf::tests::missing"), ("lib", "leaf::tests::ignored"),
            ("lib", "leaf::tests::helper"), ("other_target", "leaf::tests::negative"),
        ):
            documents = copy.deepcopy(self.documents)
            documents["negative"]["cases"][0]["tests"] = [{"target": target, "name": name}]
            with self.subTest(target=target, name=name), self.assertRaisesRegex(ValueError, "missing or ignored"):
                check(documents, self.root)

    def test_source_presence_does_not_replace_actual_cargo_discovery(self):
        with self.assertRaisesRegex(ValueError, "absent from Cargo"):
            check(self.documents, self.root, set())

    def test_report_source_bindings_detect_changed_test_bytes(self):
        before = check(self.documents, self.root)["source_bindings"]
        self.source.write_text(self.source.read_text() + "// changed fixture\n", encoding="utf-8")
        self.assertNotEqual(before, check(self.documents, self.root)["source_bindings"])

    def test_cli_invalid_input_exits_nonzero_without_a_success_report(self):
        for kind, document in self.documents.items():
            directory = self.root / "validation" / kind
            directory.mkdir(parents=True)
            (directory / "cases.json").write_text(json.dumps(document), encoding="utf-8")
        (self.root / "validation/negative/cases.json").write_text(
            '{"schema_version":1,"cases":[]}', encoding="utf-8",
        )
        output = self.root / "report.json"
        with patch("check_test_catalogs.ROOT", self.root), patch.object(
            sys, "argv", ["check_test_catalogs", "--output", str(output)],
        ), redirect_stderr(io.StringIO()) as errors:
            self.assertEqual(main(), 1)
        self.assertIn("test catalog validation failed", errors.getvalue())
        self.assertFalse(output.exists())


class CargoDiscoveryTests(unittest.TestCase):
    def test_exact_targets_are_discovered_once_with_locked_all_feature_builds(self):
        process = subprocess.CompletedProcess([], 0, "named: test\nignored_bench: benchmark\n", "")
        with patch("rust_tests.subprocess.run", return_value=process) as run:
            discovered = discover_tests(Path("/fixture"), ["lib", "catalog_contracts", "lib"])
        self.assertEqual(discovered, {("lib", "named"), ("catalog_contracts", "named")})
        self.assertEqual(run.call_count, 2)
        self.assertEqual(run.call_args_list[0].args[0], [
            "cargo", "test", "--locked", "--all-features", "--test", "catalog_contracts", "--", "--list",
        ])
        self.assertEqual(run.call_args_list[1].args[0], [
            "cargo", "test", "--locked", "--all-features", "--lib", "--", "--list",
        ])

    def test_empty_selection_and_failed_cargo_are_not_discovery_success(self):
        with self.assertRaisesRegex(ValueError, "at least one"):
            discover_tests(Path("/fixture"), [])
        process = subprocess.CompletedProcess([], 1, "", "controlled build failure")
        with patch("rust_tests.subprocess.run", return_value=process), redirect_stderr(io.StringIO()) as errors:
            with self.assertRaises(subprocess.CalledProcessError):
                discover_tests(Path("/fixture"), ["lib"])
        self.assertIn("controlled build failure", errors.getvalue())


if __name__ == "__main__":
    unittest.main()

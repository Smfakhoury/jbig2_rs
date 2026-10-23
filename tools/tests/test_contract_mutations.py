import sys
import unittest
from pathlib import Path
from unittest.mock import patch


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from check_contract_mutations import MUTATIONS, mutate
import check_contract_mutations
from evidence import load_json, validate
from rust_tests import test_inventory as inventory_tests


class ContractMutationTests(unittest.TestCase):
    def test_mutation_sites_are_unique_and_not_noops(self):
        identifiers = [mutation["id"] for mutation in MUTATIONS]
        self.assertEqual(len(identifiers), 26)
        self.assertEqual(len(identifiers), len(set(identifiers)))
        for mutation in MUTATIONS:
            with self.subTest(mutation=mutation["id"]):
                source = (TOOLS.parent / mutation["file"]).read_text(encoding="utf-8")
                changed = mutate(source, mutation)
                self.assertNotEqual(changed, source)
                self.assertEqual(changed.count(mutation["after"]), 1)

    def test_empty_or_duplicate_mutation_sets_fail_closed(self):
        for cases in ((), (MUTATIONS[0], MUTATIONS[0])):
            with (
                self.subTest(cases=len(cases)),
                patch.object(check_contract_mutations, "MUTATIONS", cases),
                self.assertRaisesRegex(ValueError, "missing or duplicate"),
            ):
                check_contract_mutations.check_mutations("unused-toolchain")

    def test_missing_or_ambiguous_sites_fail_closed(self):
        mutation = {"id": "example", "before": "before", "after": "after"}
        for source in ("", "before before"):
            with self.subTest(source=source), self.assertRaisesRegex(ValueError, "missing or ambiguous"):
                mutate(source, mutation)

    def test_mutation_filters_name_real_nonignored_tests(self):
        tests = inventory_tests(TOOLS.parent)
        for mutation in MUTATIONS:
            identity = (mutation["target"], mutation["test"])
            self.assertIn(identity, tests)
            self.assertFalse(tests[identity]["ignored"])

    def test_all_catalog_cases_require_exact_executable_test_references(self):
        tests = inventory_tests(TOOLS.parent)
        structured = 0
        for kind in ("negative", "metamorphic"):
            document = load_json(TOOLS.parent / "validation" / kind / "cases.json")
            validate(document, "test_catalog")
            for case in document["cases"]:
                self.assertTrue(case["tests"], case["id"])
                structured += 1
                for reference in case["tests"]:
                    identity = (reference["target"], reference["name"])
                    self.assertIn(identity, tests, case["id"])
                    self.assertFalse(tests[identity]["ignored"], case["id"])
        self.assertEqual(structured, 45)


if __name__ == "__main__":
    unittest.main()

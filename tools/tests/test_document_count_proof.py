import sys
import unittest
from pathlib import Path


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from check_document_count_proof import CASES, mutated_document, weaker_contract


class DocumentCountProofFixtureTests(unittest.TestCase):
    def setUp(self):
        self.source = (TOOLS.parent / "src/document.rs").read_text(encoding="utf-8")

    def test_each_mutant_changes_one_semantic_site_and_retains_its_contract(self):
        for case in CASES:
            with self.subTest(case=case["id"]):
                changed = mutated_document(self.source, case)
                self.assertNotEqual(changed, self.source)
                self.assertIn(case["replacement"], changed)
                self.assertEqual(changed.count("fn " + case["function"] + "("), 1)
                self.assertIn("document_spec::page_information_count(segments@", changed)

    def test_weaker_controls_retain_unrelated_metadata_and_length_contracts(self):
        for case in CASES:
            with self.subTest(case=case["id"]):
                control = weaker_contract(mutated_document(self.source, case), case)
                self.assertIn("document_spec::document_organization(&info) == organization", control)
                self.assertIn("document_spec::document_declared_page_count(&info) == declared_page_count", control)
                self.assertIn("final(page_states)@.len() == final(page_order)@.len()", control)
                self.assertIn("page_states@.len() == page_order@.len()", control)

    def test_missing_or_duplicate_sites_fail_closed(self):
        for case in CASES:
            with self.subTest(case=case["id"]):
                for source in ("", self.source + "\n" + case["site"]):
                    with self.assertRaises(ValueError):
                        mutated_document(source, case)
                control = weaker_contract(self.source, case)
                with self.assertRaises(ValueError):
                    weaker_contract(control, case)


if __name__ == "__main__":
    unittest.main()

import sys
import unittest
from pathlib import Path


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from check_document_count_proof import CASES, mutated_document, weaker_contract
from rust_source import parse


class DocumentCountProofFixtureTests(unittest.TestCase):
    def setUp(self):
        self.source = (TOOLS.parent / "src/document.rs").read_text(encoding="utf-8")
        self.models = (TOOLS.parent / "specs/document.rs").read_text(encoding="utf-8")

    def control_source(self, case):
        return self.models if case.get("control_file") == "specs/document.rs" else self.source

    def test_each_mutant_changes_one_semantic_site_and_retains_its_contract(self):
        self.assertEqual(len(CASES), 20)
        self.assertEqual(len({case["id"] for case in CASES}), 20)
        for case in CASES:
            with self.subTest(case=case["id"]):
                changed = mutated_document(self.source, case)
                self.assertNotEqual(changed, self.source)
                self.assertIn(case["replacement"], changed)
                self.assertEqual(changed.count("fn " + case["function"]), 1)
                self.assertIn("document_spec::page_information_count(segments@", changed)
                self.assertFalse(parse(changed.encode()).has_error)

    def test_weaker_controls_retain_unrelated_metadata_and_length_contracts(self):
        for case in CASES:
            with self.subTest(case=case["id"]):
                source = self.control_source(case)
                control = weaker_contract(source, case)
                if source == self.models:
                    self.assertIn("declared_count_matches_pages(document)", control)
                    self.assertIn("file_header_spec::valid(input)", control)
                    self.assertIn("document_organization(document) == file_header_spec::organization(input)", control)
                    self.assertIn("document_organization(document) == crate::file_header::Organization::Sequential", control)
                else:
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
                control = weaker_contract(self.control_source(case), case)
                with self.assertRaises(ValueError):
                    weaker_contract(control, case)

    def test_parser_controls_remove_only_the_selected_declaration_relation(self):
        standalone, embedded = (
            next(case for case in CASES if case["id"] == identity)
            for identity in ("discard_standalone_header_count", "invent_embedded_header_count")
        )
        control = weaker_contract(self.models, standalone)
        self.assertNotIn(standalone["control_site"], control)
        self.assertIn(embedded["control_site"], control)
        control = weaker_contract(self.models, embedded)
        self.assertIn(standalone["control_site"], control)
        self.assertNotIn(embedded["control_site"], control)

    def test_quota_controls_keep_original_success_bounds(self):
        for case in CASES:
            if not case["function"].startswith("enforce_"):
                continue
            with self.subTest(case=case["id"]):
                control = weaker_contract(self.source, case)
                self.assertIn("Err(_) => true,", control)
                self.assertIn("Ok(()) =>", control)
                self.assertNotIn(case["control_site"], control)

    def test_layout_controls_preserve_independent_payload_or_caller_guarantees(self):
        for case in CASES:
            if case["id"] in {
                "known_payload_returns_wrong_bytes", "unknown_payload_returns_wrong_bytes",
            }:
                control = weaker_contract(self.source, case)
                self.assertIn("document_spec::sequential_container_fields(", control)
                self.assertIn("final(reader).position >= old(reader).position", control)
            if case["function"] == "parse_sequential":
                control = weaker_contract(self.source, case)
                self.assertIn("document_spec::payload_window(", control)
                self.assertIn("document_spec::sequential_windows(", control)
                self.assertIn("production_sequential_complete!", control)
            if case["id"] in {
                "standalone_disables_sequential_number_order", "embedded_discards_first_input_byte",
            }:
                control = weaker_contract(self.source, case)
                self.assertIn("document_spec::standalone_metadata(input@, &result.0)", control)
                self.assertIn("document_spec::embedded_metadata(&result.0)", control)

    def test_loop_history_stays_in_proof_support_and_completion_uses_proved_result(self):
        support = (TOOLS.parent / "proofs/model_support/document.rs").read_text(encoding="utf-8")
        bridge = (TOOLS.parent / "proofs/production_impl/document.rs").read_text(encoding="utf-8")
        self.assertNotIn("spec fn sequential_windows(", self.models)
        self.assertIn("spec fn sequential_windows(", support)
        self.assertIn("proof fn sequential_complete_fields(", support)
        self.assertIn("reveal(sequential_container_fields);", support)
        self.assertIn("document_spec::sequential_complete_fields(", bridge)
        self.assertNotIn("reveal(document_spec::sequential_container_fields);", bridge)


if __name__ == "__main__":
    unittest.main()

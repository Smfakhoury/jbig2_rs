from pathlib import Path
import sys
import unittest


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from check_contract_mutations import MUTATIONS
from check_public_context_proofs import CASES, CASE_GROUPS, FILES, ROOT, baseline_groups
from proof_sensitivity import replace_once, weakened_contract
from source_group import SourceGroup, bound_source_groups, relocate_cases


class PublicContextProofTests(unittest.TestCase):
    def test_all_seventeen_actual_context_and_caller_sites_have_scoped_controls(self):
        self.assertEqual({name: len(cases) for name, cases in CASE_GROUPS.items()}, {
            "public-context": 10, "context-callers": 7,
        })
        self.assertEqual(len(CASES), 17)
        self.assertEqual(len({case["id"] for case in CASES}), 17)
        for case in CASES:
            with self.subTest(case=case["id"]):
                source = SourceGroup(ROOT, case["file"])
                mutant = replace_once(source.text, case["site"], case["replacement"])
                weaker = weakened_contract(mutant, case)
                self.assertNotEqual(mutant, source.text)
                self.assertNotEqual(weaker, mutant)
                self.assertEqual(
                    sum(row["id"] == "public-context-" + case["id"] for row in MUTATIONS), 1,
                )
                self.assertTrue(case["native_test"].startswith(
                    "document::tests::container_inspection::public_context_",
                ))

    def test_both_groups_bind_real_native_scopes_and_the_new_logical_model(self):
        _, sources, _ = bound_source_groups(ROOT, FILES)
        self.assertEqual(sources["specs/document/context.rs"],
                         (ROOT / "specs/document/context.rs").read_bytes())
        for name, selected in CASE_GROUPS.items():
            with self.subTest(group=name):
                cases, _ = relocate_cases(ROOT, selected, baseline_groups(selected), sources)
                self.assertEqual(len(cases), len(selected))
                self.assertTrue(all(case["verification_module"] for case in cases))

    def test_caller_controls_keep_existing_metadata_guarantees(self):
        for case in CASE_GROUPS["context-callers"]:
            source = SourceGroup(ROOT, case["file"])
            mutant = replace_once(source.text, case["site"], case["replacement"])
            weaker = weakened_contract(mutant, case)
            if case["function"].endswith("::inspect"):
                self.assertIn("document_spec::standalone_metadata(input@, &info)", weaker)
            elif "::inspect_embedded" in case["function"]:
                self.assertIn("document_spec::embedded_metadata(&info)", weaker)
            self.assertIn("self.decoder", weaker)


if __name__ == "__main__":
    unittest.main()

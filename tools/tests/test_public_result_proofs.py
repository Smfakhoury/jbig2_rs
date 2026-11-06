from pathlib import Path
import sys
import unittest


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from check_contract_mutations import MUTATIONS
from check_public_result_proofs import CASES, CASE_GROUPS, FILES, ROOT, baseline_groups
from proof_sensitivity import replace_once, weakened_contract
from source_group import SourceGroup, bound_source_groups, relocate_cases


class PublicResultProofTests(unittest.TestCase):
    def test_all_twelve_actual_result_sites_have_scoped_compiling_recipes(self):
        self.assertEqual({name: len(cases) for name, cases in CASE_GROUPS.items()}, {
            "public-decode-results": 8, "public-global-results": 4,
        })
        self.assertEqual(len(CASES), 12)
        self.assertEqual(len({case["id"] for case in CASES}), 12)
        for case in CASES:
            with self.subTest(case=case["id"]):
                source = SourceGroup(ROOT, case["file"])
                mutant = replace_once(source.text, case["site"], case["replacement"])
                weaker = weakened_contract(mutant, case)
                self.assertNotEqual(mutant, source.text)
                self.assertNotEqual(weaker, mutant)
                self.assertEqual(
                    sum(row["id"] == "public-results-" + case["id"] for row in MUTATIONS), 1,
                )

    def test_both_groups_bind_native_scopes_and_actual_global_result_definition(self):
        _, sources, _ = bound_source_groups(ROOT, FILES)
        self.assertEqual(sources["specs/document/results.rs"],
                         (ROOT / "specs/document/results.rs").read_bytes())
        for name, cases in CASE_GROUPS.items():
            with self.subTest(group=name):
                relocated, _ = relocate_cases(ROOT, cases, baseline_groups(cases), sources)
                self.assertEqual(len(relocated), len(cases))
                self.assertTrue(all(case["verification_module"] for case in relocated))

    def test_result_controls_preserve_existing_cleanup_and_configuration_frames(self):
        for case in CASES:
            mutant = case["replacement"]
            weaker = weakened_contract(mutant, case)
            if case["function"].startswith("DecoderContext::"):
                if case["function"].endswith("::decode_internal"):
                    self.assertIn("decoder_context_state(final(self)).0", weaker)
                    self.assertIn("decoder_context_state(old(self)).0", weaker)
                else:
                    self.assertIn("decoder_context_finished(old(self), final(self))", weaker)
            if case["id"] == "global_input_bytes_discarded":
                self.assertIn(
                    "vstd::assert_seqs_equal!(input@.subrange(0, input@.len() as int), input@)",
                    weaker,
                )


if __name__ == "__main__":
    unittest.main()

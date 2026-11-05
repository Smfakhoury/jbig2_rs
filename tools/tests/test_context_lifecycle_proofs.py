from pathlib import Path
import sys
import unittest


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from check_context_lifecycle_proofs import BASELINE_GROUPS, CASES, ROOT
from check_contract_mutations import MUTATIONS
from proof_sensitivity import replace_once, weakened_contract
from source_group import SourceGroup, relocate_cases


class ContextLifecycleProofTests(unittest.TestCase):
    def test_all_twelve_actual_sites_have_precise_weaker_controls_and_native_oracles(self):
        source = SourceGroup(ROOT, "src/generic_region.rs")
        self.assertEqual(len(CASES), 12)
        self.assertEqual(len({case["id"] for case in CASES}), 12)
        for case in CASES:
            with self.subTest(case=case["id"]):
                mutant = replace_once(source.text, case["site"], case["replacement"])
                weak = weakened_contract(mutant, case)
                self.assertNotEqual(mutant, source.text)
                self.assertNotEqual(weak, mutant)
                self.assertEqual(
                    sum(row["id"] == "context-lifecycle-" + case["id"] for row in MUTATIONS), 1,
                )
                self.assertTrue(case["native_test"].startswith(
                    "generic_region::tests::sessions_and_snapshots::",
                ))

    def test_eight_pristine_targets_relocate_to_the_actual_context_module(self):
        source = SourceGroup(ROOT, "src/generic_region.rs")
        cases, groups = relocate_cases(ROOT, CASES, BASELINE_GROUPS, source.names)
        self.assertEqual(len(groups), 1)
        self.assertEqual(len(groups[0][1]), 8)
        self.assertTrue(all(case["verification_module"] == "generic_region::contexts" for case in cases))
        self.assertEqual(len({case["function"] for case in cases}), 8)

    def test_copy_controls_remove_only_dependent_bank_obligations(self):
        source = SourceGroup(ROOT, "src/generic_region.rs")
        for case in CASES:
            if "repeats_first" not in case["id"]:
                continue
            with self.subTest(case=case["id"]):
                mutant = replace_once(source.text, case["site"], case["replacement"])
                weak = weakened_contract(mutant, case)
                self.assertIn("final(self).storage_bytes == old(self).storage_bytes", weak)
                self.assertIn("Err(_) => final(self).contexts@ == old(self).contexts@", weak)
                self.assertIn("generic_region_spec::contexts_valid(snapshot.contexts@)", weak)


if __name__ == "__main__":
    unittest.main()

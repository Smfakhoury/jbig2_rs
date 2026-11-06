from pathlib import Path
import sys
import unittest
from unittest.mock import patch


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from check_contract_mutations import MUTATIONS
from check_reference_type_proofs import CASES, CASE_GROUPS, FILES, ROOT, baseline_groups
from proof_sensitivity import replace_once, weakened_contract
from source_group import SourceGroup, bound_source_groups, relocate_cases
import run_semantic_proofs as runner


class ReferenceTypeProofTests(unittest.TestCase):
    def test_all_twelve_exact_sites_have_actual_native_and_selective_weaker_recipes(self):
        self.assertEqual({name: len(cases) for name, cases in CASE_GROUPS.items()}, {
            "reference-type-lookup": 6, "reference-type-classes": 6,
        })
        self.assertEqual(len({case["id"] for case in CASES}), 12)
        for case in CASES:
            with self.subTest(case=case["id"]):
                source = SourceGroup(ROOT, case["file"])
                mutant = replace_once(source.text, case["site"], case["replacement"])
                weaker = weakened_contract(mutant, case)
                self.assertNotEqual(mutant, source.text)
                self.assertNotEqual(mutant, weaker)
                self.assertIn("requires", weaker)
                self.assertIn("indexes@", weaker)
                self.assertEqual(sum(
                    mutation["id"] == "reference-type-" + case["id"] for mutation in MUTATIONS
                ), 1)

    def test_real_scope_and_private_global_projection_are_source_bound(self):
        _, sources, _ = bound_source_groups(ROOT, FILES)
        self.assertEqual(sources["specs/document/reference_types.rs"],
                         (ROOT / "specs/document/reference_types.rs").read_bytes())
        for group, cases in CASE_GROUPS.items():
            with self.subTest(group=group):
                relocated, _ = relocate_cases(ROOT, cases, baseline_groups(cases), sources)
                self.assertEqual(len(relocated), 6)

    def test_new_groups_do_not_redistribute_any_old_case_or_add_roles(self):
        added = set(CASE_GROUPS)
        with patch.object(runner, "CHECKS", tuple(row for row in runner.CHECKS if row[0] not in added)):
            before = (runner.configured_whole_checks(), runner.configured_collector_checks(),
                      *runner.configured_shards())
        after = (runner.configured_whole_checks(), runner.configured_collector_checks(),
                 *runner.configured_shards())
        self.assertEqual(len(after), 16)
        for old, current in zip(before, after):
            self.assertEqual(old, tuple(row for row in current if row[0] not in added))
        self.assertIn("reference-type-lookup", {row[0] for row in after[5]})
        self.assertIn("reference-type-classes", {row[0] for row in after[13]})


if __name__ == "__main__":
    unittest.main()

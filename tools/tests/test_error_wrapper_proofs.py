from pathlib import Path
import sys
import unittest


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))
from check_error_wrapper_proofs import CASE_GROUPS, CASES, FILES, ROOT
from proof_sensitivity import replace_once, weakened_contract
from source_group import bound_source_groups, relocate_cases


class ErrorWrapperProofTests(unittest.TestCase):
    def test_all_twenty_sites_and_selective_contracts_bind_exact_source_traits(self):
        groups, sources, _ = bound_source_groups(ROOT, FILES)
        self.assertEqual(len(CASES), 20)
        self.assertEqual([len(cases) for cases in CASE_GROUPS.values()], [6, 7, 7])
        for configured in CASE_GROUPS.values():
            cases, baselines = relocate_cases(ROOT, configured, (), sources)
            self.assertEqual(baselines, ())
            for case in cases:
                self.assertIn(case["impl_trait"], case["selected_impl_traits"])
                source = groups[case["file"]].text
                mutant = replace_once(source, case["site"], case["replacement"])
                weak = weakened_contract(mutant, case)
                self.assertNotEqual(mutant, source)
                self.assertNotEqual(weak, mutant)
                self.assertEqual(weak.count("false"), mutant.count("false") + 1)


if __name__ == "__main__":
    unittest.main()

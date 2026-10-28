from pathlib import Path
import sys
import unittest


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))
from check_error_conversion_proofs import CASES, FILES, ROOT
from proof_sensitivity import replace_once, weakened_contract
from source_group import bound_source_groups, relocate_cases


class ErrorConversionProofTests(unittest.TestCase):
    def test_every_case_binds_the_original_native_body_and_only_its_mapping_contract(self):
        groups, sources, _ = bound_source_groups(ROOT, FILES)
        cases, baselines = relocate_cases(ROOT, CASES, (), sources)
        self.assertEqual(len(cases), 8)
        self.assertEqual(baselines, ())
        self.assertEqual(len({case["verification_module"] for case in cases}), 4)
        for case in cases:
            original = groups[case["file"]].text
            mutated = replace_once(original, case["site"], case["replacement"])
            control = weakened_contract(mutated, case)
            self.assertNotEqual(mutated, original)
            self.assertNotEqual(control, mutated)
            self.assertIn("closed spec fn from_spec", control)
            self.assertEqual(case["module"], "error")
            self.assertEqual(case["function"], "DecodeError::from")


if __name__ == "__main__":
    unittest.main()

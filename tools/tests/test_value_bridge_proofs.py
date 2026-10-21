import sys
import unittest
from pathlib import Path


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from check_value_bridge_proofs import CASES
from proof_sensitivity import replace_once, weakened_contract


class ValueBridgeProofFixtureTests(unittest.TestCase):
    def test_all_five_output_mutations_have_unique_source_sites(self):
        self.assertEqual(len({case["id"] for case in CASES}), 5)
        self.assertEqual(
            {case["function"] for case in CASES},
            {"widen_u32_i64", "widen_u8_i64", "widen_u8_i32",
             "widen_bool_i64", "same_combination_operator"},
        )
        for case in CASES:
            with self.subTest(case=case["id"]):
                source = (TOOLS.parent / case["file"]).read_text(encoding="utf-8")
                changed = replace_once(source, case["site"], case["replacement"])
                self.assertNotEqual(source, changed)
                self.assertIn(case["replacement"], changed)
                self.assertNotEqual(changed, weakened_contract(changed, case))

    def test_missing_duplicate_and_already_weakened_sites_fail_closed(self):
        for case in CASES:
            source = (TOOLS.parent / case["file"]).read_text(encoding="utf-8")
            with self.subTest(case=case["id"]):
                for invalid in ("", source + "\n" + case["site"]):
                    with self.assertRaises(ValueError):
                        replace_once(invalid, case["site"], case["replacement"])
                with self.assertRaises(ValueError):
                    weakened_contract(weakened_contract(source, case), case)


if __name__ == "__main__":
    unittest.main()

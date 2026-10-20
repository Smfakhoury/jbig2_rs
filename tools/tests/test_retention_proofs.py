import sys
import unittest
from pathlib import Path


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from check_retention_proofs import CASES
from proof_sensitivity import replace_once, weakened_contract


class RetentionProofFixtureTests(unittest.TestCase):
    def setUp(self):
        self.source = (TOOLS.parent / "src/segment_header.rs").read_text(encoding="utf-8")

    def test_all_mutants_have_unique_semantic_sites(self):
        self.assertEqual(len({case["id"] for case in CASES}), 3)
        for case in CASES:
            with self.subTest(case=case["id"]):
                changed = replace_once(self.source, case["site"], case["replacement"])
                self.assertNotEqual(changed, self.source)
                self.assertIn(case["replacement"], changed)

    def test_controls_keep_materialized_length_and_other_contracts(self):
        for case in CASES:
            with self.subTest(case=case["id"]):
                control = weakened_contract(self.source, case)
                self.assertIn("out.is_ok() ==> final(values)@.len() == value_count as nat", control)
                self.assertIn("flags.values@.len() == count as nat + 1", control)
                self.assertIn("((index / 8) as nat) < bytes@.len()", control)

    def test_missing_or_duplicate_control_sites_fail_closed(self):
        for case in CASES:
            with self.subTest(case=case["id"]):
                for source in ("", self.source + "\n" + case["site"]):
                    with self.assertRaises(ValueError):
                        replace_once(source, case["site"], case["replacement"])
                with self.assertRaises(ValueError):
                    weakened_contract(weakened_contract(self.source, case), case)


if __name__ == "__main__":
    unittest.main()

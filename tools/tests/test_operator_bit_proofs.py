import sys
import unittest
from pathlib import Path


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from source_group import source_text
from check_operator_bit_proofs import BYTE_POST, CASES, WORD_POST
from proof_sensitivity import replace_once, weakened_contract


class OperatorBitProofFixtureTests(unittest.TestCase):
    def setUp(self):
        self.source = source_text(TOOLS.parent, "src/bitmap.rs")
        self.proofs = source_text(TOOLS.parent, "proofs/production_impl/bitmap.rs")

    def test_four_compiling_constant_mutations_use_unique_function_sites(self):
        self.assertEqual(len({case["id"] for case in CASES}), 4)
        for case in CASES:
            with self.subTest(case=case["id"]):
                changed = replace_once(self.source, case["site"], case["replacement"])
                self.assertNotEqual(changed, self.source)
                self.assertIn(case["replacement"], changed)

    def test_controls_remove_only_the_affected_output_relation(self):
        for case in CASES:
            with self.subTest(case=case["id"]):
                control = weakened_contract(self.source, case)
                retained = WORD_POST if case["controls"][0][0] == BYTE_POST else BYTE_POST
                replace_once(control, retained, "")
                self.assertIn("production_bitmap_byte_operator!", control)
                self.assertIn("production_bitmap_word_operator!", control)
                self.assertIn("bitmap_shared::byte_operator_semantics", self.proofs)
                self.assertIn("bitmap_shared::word_operator_semantics", self.proofs)

    def test_missing_ambiguous_or_already_weakened_sites_fail(self):
        for case in CASES:
            with self.subTest(case=case["id"]):
                for invalid in ("", self.source + "\n" + case["site"]):
                    with self.assertRaises(ValueError):
                        replace_once(invalid, case["site"], case["replacement"])
                with self.assertRaises(ValueError):
                    weakened_contract(weakened_contract(self.source, case), case)


if __name__ == "__main__":
    unittest.main()

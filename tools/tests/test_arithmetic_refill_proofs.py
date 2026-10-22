from pathlib import Path
import sys
import unittest


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))
from check_arithmetic_refill_proofs import CASES, CHECKED_POST, FIELDS, SIGNAL_POST
from proof_sensitivity import replace_once, weakened_contract


class ArithmeticRefillProofFixtureTests(unittest.TestCase):
    def setUp(self):
        self.source = (TOOLS.parent / "src/arithmetic.rs").read_text()

    def test_six_output_and_error_order_mutations_have_unique_sites(self):
        self.assertEqual(len({case["id"] for case in CASES}), 6)
        self.assertEqual(
            {case["function"] for case in CASES},
            {"ArithmeticDecoder::byte_in", "ArithmeticDecoder::byte_in_signal"},
        )
        for case in CASES:
            with self.subTest(case=case["id"]):
                changed = replace_once(self.source, case["site"], case["replacement"])
                self.assertNotEqual(changed, self.source)

    def test_controls_remove_only_selected_new_relations(self):
        for case in CASES:
            with self.subTest(case=case["id"]):
                original, replacement = case["controls"][0]
                self.assertIn(original, (SIGNAL_POST, CHECKED_POST))
                self.assertTrue(replacement.strip())
                for field in FIELDS.values():
                    self.assertIn(field, original)
                changed = weakened_contract(self.source, case)
                self.assertIn("arithmetic_spec::decoder_state_valid(final(self))", changed)
                self.assertIn("final(self).interval == old(self).interval", changed)
                self.assertIn("final(self).bytes@ == old(self).bytes@", changed)
                self.assertIn("final(self).bit_count >= 7", changed)

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

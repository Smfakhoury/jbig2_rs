from pathlib import Path
import sys
import unittest


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))
from check_arithmetic_normalization_proofs import CASES, POST
from proof_sensitivity import replace_once, weakened_contract


class ArithmeticNormalizationProofFixtureTests(unittest.TestCase):
    def setUp(self):
        self.source = (TOOLS.parent / "src/arithmetic.rs").read_text()

    def test_six_mutations_have_unique_success_path_sites(self):
        self.assertEqual(len({case["id"] for case in CASES}), 6)
        for case in CASES:
            with self.subTest(case=case["id"]):
                self.assertNotEqual(
                    replace_once(self.source, case["site"], case["replacement"]), self.source,
                )

    def test_weaker_controls_preserve_validity_input_frame_and_loop_recurrence(self):
        for case in CASES:
            with self.subTest(case=case["id"]):
                changed = weakened_contract(self.source, case)
                self.assertIn("arithmetic_spec::decoder_state_valid(final(self))", changed)
                self.assertIn("final(self).bytes@ == old(self).bytes@", changed)
                self.assertIn("== arithmetic_spec::normalized_state(self)", changed)
                self.assertIn("reveal_with_fuel(arithmetic_spec::normalized_state, 1)", changed)
                self.assertNotEqual(case["controls"][0][1], POST)

    def test_missing_ambiguous_or_repeated_controls_fail(self):
        for case in CASES:
            with self.subTest(case=case["id"]):
                for source in ("", self.source + "\n" + case["site"]):
                    with self.assertRaises(ValueError):
                        replace_once(source, case["site"], case["replacement"])
                with self.assertRaises(ValueError):
                    weakened_contract(weakened_contract(self.source, case), case)

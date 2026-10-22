from pathlib import Path
import sys
import unittest


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))
from check_arithmetic_decision_proofs import CASES
from proof_sensitivity import replace_once, weakened_contract


class ArithmeticDecisionProofFixtureTests(unittest.TestCase):
    def setUp(self):
        self.source = (TOOLS.parent / "src/arithmetic.rs").read_text()

    def test_seven_compiling_output_context_mutations_have_unique_sites(self):
        self.assertEqual(len({case["id"] for case in CASES}), 7)
        for case in CASES:
            with self.subTest(case=case["id"]):
                changed = replace_once(self.source, case["site"], case["replacement"])
                self.assertNotEqual(changed, self.source)
                self.assertIn(case["replacement"], changed)

    def test_weaker_controls_retain_original_state_signal_and_context_validity(self):
        for case in CASES:
            with self.subTest(case=case["id"]):
                changed = weakened_contract(self.source, case)
                self.assertIn("arithmetic_spec::decoder_state_valid(final(self))", changed)
                self.assertIn("arithmetic_spec::context_valid(*final(context))", changed)
                self.assertIn("out <= RASTER_SIGNAL_OVERFLOW", changed)
                self.assertIn("out <= 1 ==> arithmetic_spec::decoder_valid(final(self))", changed)

    def test_missing_ambiguous_or_already_weakened_evidence_fails(self):
        for case in CASES:
            with self.subTest(case=case["id"]):
                for invalid in ("", self.source + "\n" + case["site"]):
                    with self.assertRaises(ValueError):
                        replace_once(invalid, case["site"], case["replacement"])
                with self.assertRaises(ValueError):
                    weakened_contract(weakened_contract(self.source, case), case)


if __name__ == "__main__":
    unittest.main()

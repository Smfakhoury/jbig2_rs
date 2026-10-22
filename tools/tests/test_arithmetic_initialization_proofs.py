from pathlib import Path
import sys
import unittest


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))
from check_arithmetic_initialization_proofs import CASES
from proof_sensitivity import replace_once, weakened_contract


class ArithmeticInitializationProofFixtureTests(unittest.TestCase):
    def setUp(self):
        self.source = (TOOLS.parent / "src/arithmetic.rs").read_text()

    def test_six_initializer_mutations_have_unique_sites(self):
        self.assertEqual(len({case["id"] for case in CASES}), 6)
        self.assertEqual({case["function"] for case in CASES}, {"ArithmeticDecoder::new"})
        for case in CASES:
            with self.subTest(case=case["id"]):
                self.assertNotEqual(
                    replace_once(self.source, case["site"], case["replacement"]), self.source,
                )

    def test_controls_retain_validity_and_unaffected_outputs(self):
        for case in CASES:
            with self.subTest(case=case["id"]):
                changed = weakened_contract(self.source, case)
                self.assertIn("arithmetic_spec::decoder_valid(&decoder)", changed)
                self.assertIn("&& decoder.bytes@ == bytes@", changed)
                self.assertIn("arithmetic_spec::initialized_state(bytes)", changed)

    def test_missing_ambiguous_or_repeated_controls_fail(self):
        for case in CASES:
            with self.subTest(case=case["id"]):
                for invalid in ("", self.source + "\n" + case["site"]):
                    with self.assertRaises(ValueError):
                        replace_once(invalid, case["site"], case["replacement"])
                with self.assertRaises(ValueError):
                    weakened_contract(weakened_contract(self.source, case), case)


if __name__ == "__main__":
    unittest.main()

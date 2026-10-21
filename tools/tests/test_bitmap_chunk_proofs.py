import sys
import unittest
from pathlib import Path


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from check_bitmap_chunk_proofs import CASES, FRAME, READ_VALUE, WRITE_VALUE
from proof_sensitivity import replace_once, weakened_contract


class BitmapChunkProofFixtureTests(unittest.TestCase):
    def setUp(self):
        self.source = (TOOLS.parent / "src/bitmap.rs").read_text(encoding="utf-8")

    def test_four_value_and_frame_mutations_have_unique_sites(self):
        self.assertEqual(len({case["id"] for case in CASES}), 4)
        for case in CASES:
            with self.subTest(case=case["id"]):
                changed = replace_once(self.source, case["site"], case["replacement"])
                self.assertNotEqual(changed, self.source)
                self.assertIn(case["replacement"], changed)

    def test_weaker_controls_keep_range_acceptance_length_and_unaffected_relations(self):
        for case in CASES:
            with self.subTest(case=case["id"]):
                control = weakened_contract(self.source, case)
                self.assertIn("out.is_some() == (start as nat + 8 <= bytes@.len())", control)
                self.assertIn("out == (start as nat + 8 <= old(bytes)@.len())", control)
                self.assertIn("final(bytes)@.len() == old(bytes)@.len()", control)
                removed = case["controls"][0][0]
                for relation in (READ_VALUE, WRITE_VALUE, FRAME):
                    if relation != removed:
                        replace_once(control, relation, "")

    def test_missing_ambiguous_and_already_weakened_sites_fail_closed(self):
        for case in CASES:
            with self.subTest(case=case["id"]):
                for invalid in ("", self.source + "\n" + case["site"]):
                    with self.assertRaises(ValueError):
                        replace_once(invalid, case["site"], case["replacement"])
                with self.assertRaises(ValueError):
                    weakened_contract(weakened_contract(self.source, case), case)


if __name__ == "__main__":
    unittest.main()

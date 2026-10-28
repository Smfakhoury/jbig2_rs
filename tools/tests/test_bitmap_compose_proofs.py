import sys
import unittest
from pathlib import Path


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from source_group import source_text
from check_bitmap_compose_proofs import CASES, FRAME, ZERO_BLIT_RESULT
from proof_sensitivity import replace_once, weakened_contract


class BitmapCompositionProofFixtureTests(unittest.TestCase):
    def setUp(self):
        self.source = source_text(TOOLS.parent, "src/bitmap.rs")

    def test_seven_distinct_mutations_have_unique_sites(self):
        self.assertEqual(len({case["id"] for case in CASES}), 7)
        for case in CASES:
            with self.subTest(case=case["id"]):
                changed = replace_once(self.source, case["site"], case["replacement"])
                self.assertNotEqual(changed, self.source)
                self.assertIn(case["replacement"], changed)

    def test_controls_retain_storage_metadata_and_unaffected_semantic_guarantee(self):
        for case in CASES:
            with self.subTest(case=case["id"]):
                control = weakened_contract(self.source, case)
                self.assertIn("crate::spec::bitmap::storage_valid(final(self))", control)
                self.assertIn("crate::views::bitmap_width(final(self))", control)
                self.assertIn("crate::views::bitmap_data(final(self)).len()", control)
                if case["controls"][0][0] == FRAME:
                    self.assertIn("ensures\n            out.is_ok(),", control)
                else:
                    replace_once(control, FRAME, "")
                if case["controls"][0][0] != ZERO_BLIT_RESULT:
                    replace_once(control, ZERO_BLIT_RESULT, "")
                else:
                    self.assertIn("destination_shift < 8", control)
                    self.assertIn("final(self).stride == old(self).stride", control)
                    self.assertIn("final(self).data@.len() == old(self).data@.len()", control)

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

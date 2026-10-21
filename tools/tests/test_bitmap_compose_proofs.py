import sys
import unittest
from pathlib import Path


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from check_bitmap_compose_proofs import CASES, FRAME
from proof_sensitivity import replace_once, weakened_contract


class BitmapCompositionProofFixtureTests(unittest.TestCase):
    def setUp(self):
        self.source = (TOOLS.parent / "src/bitmap.rs").read_text(encoding="utf-8")

    def test_five_distinct_mutations_have_unique_sites(self):
        self.assertEqual(len({case["id"] for case in CASES}), 5)
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

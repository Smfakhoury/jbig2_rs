import sys
import unittest
from pathlib import Path


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from check_bitmap_copy_proofs import CASES, mutated_copy, weaker_contract


class BitmapCopyProofFixtureTests(unittest.TestCase):
    def setUp(self):
        self.sources = {
            path: (TOOLS.parent / path).read_text(encoding="utf-8")
            for path in ("src/bitmap.rs", "src/generic_region.rs")
        }

    def test_each_mutant_preserves_its_strong_contract(self):
        for case in CASES:
            with self.subTest(case=case["id"]):
                source = self.sources[case["file"]]
                changed = mutated_copy(source, case)
                self.assertNotEqual(changed, source)
                self.assertIn(case["replacement"], changed)
                self.assertEqual(weaker_contract(source, case), source.replace(case["control"], "", 1))

    def test_storage_control_only_removes_byte_result(self):
        for case in CASES[:4]:
            control = weaker_contract(self.sources[case["file"]], case)
            self.assertIn("crate::spec::bitmap::storage_valid(final(self))", control)
            self.assertIn("final(self).data@.len() == old(self).data@.len()", control)
            self.assertIn("out.is_ok()", control)
            if case["function"] == "Bitmap::copy_prefix_from":
                self.assertIn("final(self).data@ == old(self).data@", control)
                self.assertIn("error == MalformedError::IntegerOverflow", control)

    def test_missing_and_duplicate_sites_fail_closed(self):
        for case in CASES:
            source = self.sources[case["file"]]
            for changed in ("", source + case["site"]):
                with self.assertRaises(ValueError):
                    mutated_copy(changed, case)
            with self.assertRaises(ValueError):
                weaker_contract(weaker_contract(source, case), case)

    def test_catalog_covers_distinct_copy_initialization_and_padding_results(self):
        self.assertEqual(len({case["id"] for case in CASES}), 6)
        self.assertEqual({case["function"] for case in CASES}, {
            "Bitmap::copy_row", "Bitmap::copy_prefix_from", "Bitmap::blank", "pad_bitmap_height",
        })

    def test_zero_and_padding_controls_keep_storage_and_dimension_guarantees(self):
        blank = weaker_contract(self.sources["src/bitmap.rs"], CASES[4])
        self.assertIn("crate::spec::bitmap::storage_valid(&bitmap)", blank)
        self.assertIn("crate::spec::bitmap::limits_allow_bitmap(width, height, limits)", blank)
        padded = weaker_contract(self.sources["src/generic_region.rs"], CASES[5])
        self.assertIn("bitmap.width == decoded.width", padded)
        self.assertIn("bitmap.height == nominal_height", padded)
        self.assertIn("crate::spec::bitmap::storage_valid(&bitmap)", padded)


if __name__ == "__main__":
    unittest.main()

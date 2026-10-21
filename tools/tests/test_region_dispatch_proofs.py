import sys
import unittest
from pathlib import Path


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from check_region_dispatch_proofs import CASES
from proof_sensitivity import replace_once, weakened_contract


class RegionDispatchProofFixtureTests(unittest.TestCase):
    def setUp(self):
        self.source = (TOOLS.parent / "src/bitmap.rs").read_text(encoding="utf-8")

    def test_distinct_overflow_and_zero_area_mutations_have_unique_sites(self):
        self.assertEqual(len({case["id"] for case in CASES}), 3)
        for case in CASES:
            with self.subTest(case=case["id"]):
                changed = replace_once(self.source, case["site"], case["replacement"])
                self.assertNotEqual(changed, self.source)
                self.assertIn(case["replacement"], changed)

    def test_weaker_controls_keep_existing_storage_and_typed_request_guarantees(self):
        for case in CASES:
            with self.subTest(case=case["id"]):
                control = weakened_contract(self.source, case)
                self.assertIn("crate::spec::bitmap::storage_valid(final(self))", control)
                self.assertIn("matches!(out, Err(MalformedError::IntegerOverflow))", control)
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

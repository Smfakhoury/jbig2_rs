import sys
import unittest
from pathlib import Path


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from check_text_batch_proofs import CASES, CLEAR, NO_OP
from proof_sensitivity import replace_once, weakened_contract


class TextBatchProofFixtureTests(unittest.TestCase):
    def setUp(self):
        self.source = (TOOLS.parent / "src/symbol_text.rs").read_text(encoding="utf-8")

    def test_four_distinct_no_op_and_clearing_mutations_have_unique_sites(self):
        self.assertEqual(len({case["id"] for case in CASES}), 4)
        for case in CASES:
            with self.subTest(case=case["id"]):
                changed = replace_once(self.source, case["site"], case["replacement"])
                self.assertNotEqual(changed, self.source)
                self.assertIn(case["replacement"], changed)

    def test_controls_keep_storage_layout_success_and_the_other_semantic_guarantee(self):
        for case in CASES:
            with self.subTest(case=case["id"]):
                control = weakened_contract(self.source, case)
                self.assertIn("crate::spec::bitmap::storage_valid(final(destination))", control)
                self.assertIn("crate::views::bitmap_stride(final(destination))", control)
                self.assertIn("crate::views::bitmap_data(final(destination)).len()", control)
                self.assertIn("old(batch).height == 0) ==> out.is_ok()", control)
                retained = CLEAR if case["controls"][0][0] == NO_OP else NO_OP
                replace_once(control, retained, "")

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

import sys
import unittest
from pathlib import Path


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from check_text_batch_proofs import (
    BITMAP_EFFECT, CASES, CLEAR, CURRENT_ACCEPTANCE, DIMENSION_ACCEPTANCE,
    INITIALIZATION_ACCEPTANCE, NO_OP, PREFLIGHT_RESULT, WORK_EFFECT,
)
from proof_sensitivity import replace_once, weakened_contract


class TextBatchProofFixtureTests(unittest.TestCase):
    def setUp(self):
        self.source = (TOOLS.parent / "src/symbol_text.rs").read_text(encoding="utf-8")

    def test_eleven_distinct_mutations_have_unique_sites(self):
        self.assertEqual(len({case["id"] for case in CASES}), 11)
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
                for relation in (
                    WORK_EFFECT, BITMAP_EFFECT, DIMENSION_ACCEPTANCE, PREFLIGHT_RESULT,
                    INITIALIZATION_ACCEPTANCE, CURRENT_ACCEPTANCE,
                ):
                    if relation != case["controls"][0][0]:
                        replace_once(control, relation, "")
                self.assertIn("symbol_text_spec::text_state_valid(old(self))", control)
                self.assertIn("symbol_text_spec::placement_for_current(", control)

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

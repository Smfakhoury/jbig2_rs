import sys
import unittest
from pathlib import Path


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from check_black_span_proofs import CASES, RESULT, SUCCESS
from proof_sensitivity import replace_once, weakened_contract


class BlackSpanProofFixtureTests(unittest.TestCase):
    def setUp(self):
        self.source = (TOOLS.parent / "src/bitmap.rs").read_text(encoding="utf-8")

    def test_six_output_contrasts_have_unique_compilation_sites(self):
        self.assertEqual(len({case["id"] for case in CASES}), 6)
        for case in CASES:
            with self.subTest(case=case["id"]):
                changed = replace_once(self.source, case["site"], case["replacement"])
                self.assertNotEqual(changed, self.source)
                self.assertIn(case["replacement"], changed)

    def test_controls_keep_old_contract_and_unaffected_new_relation(self):
        for case in CASES:
            with self.subTest(case=case["id"]):
                control = weakened_contract(self.source, case)
                for relation in (
                    "row < old(self).height,",
                    "start <= end <= old(self).width,",
                    "final(self).data@.len() == old(self).data@.len(),",
                    "start == end ==> final(self).data@ == old(self).data@,",
                    "start == end ==> out.is_ok(),",
                ):
                    self.assertIn(relation, control)
                removed = case["controls"][0][0]
                replace_once(control, SUCCESS if removed == RESULT else RESULT, "")

    def test_missing_ambiguous_and_twice_weakened_sites_fail_closed(self):
        for case in CASES:
            with self.subTest(case=case["id"]):
                for invalid in ("", self.source + "\n" + case["site"]):
                    with self.assertRaises(ValueError):
                        replace_once(invalid, case["site"], case["replacement"])
                with self.assertRaises(ValueError):
                    weakened_contract(weakened_contract(self.source, case), case)


if __name__ == "__main__":
    unittest.main()

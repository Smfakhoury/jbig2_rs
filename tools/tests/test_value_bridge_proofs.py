import sys
import unittest
from pathlib import Path


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from source_group import source_text
from check_value_bridge_proofs import CASES
from proof_sensitivity import replace_once, weakened_contract


class ValueBridgeProofFixtureTests(unittest.TestCase):
    def test_all_fourteen_value_error_and_state_mutations_have_unique_source_sites(self):
        self.assertEqual(len({case["id"] for case in CASES}), 14)
        self.assertEqual(
            {case["function"] for case in CASES},
            {"widen_u32_i64", "widen_u8_i64", "widen_u8_i32",
             "widen_bool_i64", "same_combination_operator",
             "checked_symbol_id", "decode_checked_symbol_id"},
        )
        for case in CASES:
            with self.subTest(case=case["id"]):
                source = source_text(TOOLS.parent, case["file"])
                changed = replace_once(source, case["site"], case["replacement"])
                self.assertNotEqual(source, changed)
                self.assertIn(case["replacement"], changed)
                self.assertNotEqual(changed, weakened_contract(changed, case))

    def test_missing_duplicate_and_already_weakened_sites_fail_closed(self):
        for case in CASES:
            source = source_text(TOOLS.parent, case["file"])
            with self.subTest(case=case["id"]):
                for invalid in ("", source + "\n" + case["site"]):
                    with self.assertRaises(ValueError):
                        replace_once(invalid, case["site"], case["replacement"])
                with self.assertRaises(ValueError):
                    weakened_contract(weakened_contract(source, case), case)

    def test_checked_id_controls_retain_original_success_bounds_and_validity(self):
        cases = [case for case in CASES if case["function"] == "decode_checked_symbol_id"]
        self.assertEqual(len(cases), 7)
        for case in cases:
            source = source_text(TOOLS.parent, case["file"])
            weaker = weakened_contract(source, case)
            with self.subTest(case=case["id"]):
                self.assertIn("requires symbol_text_spec::codec_valid(old(codec))", weaker)
                self.assertIn("symbol_text_spec::codec_valid(final(codec))", weaker)
                self.assertIn("id < request.available_symbols", weaker)
                if case["id"].startswith("checked_table_id_"):
                    self.assertIn("request.available_symbols == 0", weaker)
                    self.assertIn("state.symbol_id_table.is_none()", weaker)
                    self.assertIn("==> symbol_text_spec::checked_symbol_id_transition", weaker)
                else:
                    self.assertNotIn(
                        "symbol_text_spec::checked_symbol_id_transition(old(codec), final(codec), request, out)",
                        weaker,
                    )


if __name__ == "__main__":
    unittest.main()

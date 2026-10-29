from pathlib import Path
import sys
import unittest


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))
import check_collector_sink_proof as checker
from proof_sensitivity import replace_once, weakened_contract
from rust_source import source_functions


class CollectorSinkProofTests(unittest.TestCase):
    def test_both_outcomes_have_exact_owned_implementation_controls(self):
        cases = checker.CASES
        self.assertEqual(len(cases), 2)
        self.assertEqual(len({case["id"] for case in cases}), 2)
        self.assertTrue(all(
            case["module"] == case["verification_module"] == "decode"
            and case["function"] == "PageCollector::write_page"
            and case["impl_trait"] == "PageSink"
            for case in cases
        ))
        self.assertIn("Ok(())", cases[0]["replacement"])
        self.assertNotIn("self.push_page(page)", cases[0]["replacement"])
        self.assertIn("self.push_page(page)?; Err(", cases[1]["replacement"])

    def test_weaker_control_changes_only_its_sink_contract(self):
        original = (checker.ROOT / "src/decode.rs").read_text()
        for case in checker.CASES:
            mutated = replace_once(original, case["site"], case["replacement"])
            weaker = weakened_contract(mutated, case)
            self.assertEqual(weaker.count("ensures " + checker.RELATION), 2)
            self.assertEqual(weaker.count("=> true"), original.count("=> true") + 1)
            self.assertIn("impl PageSink for PageCollector", weaker)
            self.assertIn("fn collect_page", weaker)
            self.assertIn("fn push_page", weaker)

    def test_target_is_one_verified_concrete_body_not_the_callback_trait(self):
        matches = [
            row for row in source_functions(checker.ROOT)
            if row["file"] == "src/decode.rs" and row["owner"] == "PageCollector"
            and row["name"] == "write_page" and row["impl_trait"] == "PageSink"
        ]
        self.assertEqual(len(matches), 1)
        self.assertTrue(matches[0]["verification_requested"])


if __name__ == "__main__":
    unittest.main()

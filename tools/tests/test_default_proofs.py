from pathlib import Path
import re
import sys
import unittest


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))
import check_default_proofs as checker
from proof_sensitivity import replace_once, weakened_contract
from rust_source import source_functions


class DefaultProofTests(unittest.TestCase):
    def test_every_field_has_one_case_for_each_public_default(self):
        names = {name for name, _ in checker.FIELDS}
        self.assertEqual(len(names), 15)
        self.assertTrue(all(int(value.replace("_", "")) > 0 for _, value in checker.FIELDS))
        for module, group in (("limits", "default-limits"), ("document", "default-decoder")):
            cases = checker.CASE_GROUPS[group]
            self.assertEqual(len(cases), 15)
            self.assertEqual({case["id"] for case in cases}, {
                f"{module}-default-{name}" for name in names
            })
            self.assertTrue(all(case["impl_trait"] == "Default" for case in cases))

    def test_each_mutant_changes_only_its_named_policy_field(self):
        for cases in checker.CASE_GROUPS.values():
            for case in cases:
                field = case["id"].partition("-default-")[2]
                self.assertEqual(
                    re.findall(r"limits\.(\w+)\s*=\s*(\d+)", case["replacement"]),
                    [(field, "0")],
                )
                self.assertIn("::project_default()", case["replacement"])

    def test_each_weaker_control_keeps_the_other_fourteen_fields(self):
        names = {name for name, _ in checker.FIELDS}
        for cases in checker.CASE_GROUPS.values():
            for case in cases:
                field = case["id"].partition("-default-")[2]
                original, weaker = case["controls"][0]
                self.assertIn("impl Default for", original)
                self.assertIn("impl Default for", weaker)
                self.assertEqual(set(re.findall(r"\.(\w+)\s*==", weaker)), names - {field})
                self.assertEqual(weaker.count("=="), 14)

    def test_controls_bind_only_the_real_default_not_its_helper(self):
        functions = source_functions(checker.ROOT)
        for cases in checker.CASE_GROUPS.values():
            for case in cases:
                original = (checker.ROOT / case["file"]).read_text()
                mutated = replace_once(original, case["site"], case["replacement"])
                weaker = weakened_contract(mutated, case)
                self.assertEqual(mutated.count("fn project_default()"), original.count("fn project_default()"))
                self.assertIn(case["replacement"], re.sub(r"\s+", " ", weaker))
                module, method = case["module"], case["function"]
                matches = [
                    row for row in functions
                    if row["verifier_module"] == module
                    and f"{row['owner']}::{row['name']}" == method
                    and row["impl_trait"] == "Default"
                ]
                self.assertEqual(len(matches), 1)
                self.assertTrue(matches[0]["verification_requested"])


if __name__ == "__main__":
    unittest.main()

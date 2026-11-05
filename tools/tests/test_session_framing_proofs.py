from pathlib import Path
import sys
import unittest


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from check_session_framing_proofs import BASELINE_GROUPS, CASES, FILES, ROOT
from check_contract_mutations import MUTATIONS
from proof_sensitivity import replace_once, weakened_contract
from source_group import SourceGroup, bound_source_groups, relocate_cases


class SessionFramingProofTests(unittest.TestCase):
    def test_sensitivity_explicitly_binds_the_new_framing_models(self):
        _, sources, _ = bound_source_groups(ROOT, FILES)
        path = "specs/generic_region/framing.rs"
        self.assertIn(path, sources)
        self.assertEqual(sources[path], (ROOT / path).read_bytes())

    def test_twelve_actual_mutations_keep_compiling_control_and_native_oracle_recipes(self):
        source = SourceGroup(ROOT, "src/generic_region.rs")
        self.assertEqual(len(CASES), 12)
        self.assertEqual(len({case["id"] for case in CASES}), 12)
        for case in CASES:
            with self.subTest(case=case["id"]):
                mutant = replace_once(source.text, case["site"], case["replacement"])
                weak = weakened_contract(mutant, case)
                self.assertNotEqual(mutant, source.text)
                self.assertNotEqual(weak, mutant)
                self.assertEqual(
                    sum(row["id"] == "session-framing-" + case["id"] for row in MUTATIONS), 1,
                )
                self.assertTrue(case["native_test"].startswith(
                    "generic_region::tests::segment_framing::",
                ))

    def test_six_pristine_targets_relocate_to_the_two_actual_modules(self):
        source = SourceGroup(ROOT, "src/generic_region.rs")
        cases, groups = relocate_cases(ROOT, CASES, BASELINE_GROUPS, source.names)
        self.assertEqual(sum(len(functions) for _, functions in groups), 6)
        self.assertEqual({module for module, _ in groups}, {
            "generic_region", "generic_region::segments",
        })
        self.assertEqual(len({case["function"] for case in cases}), 6)

    def test_controls_preserve_unrelated_constructor_and_safety_guarantees(self):
        source = SourceGroup(ROOT, "src/generic_region.rs")
        for case in CASES:
            mutant = replace_once(source.text, case["site"], case["replacement"])
            weak = weakened_contract(mutant, case)
            if case["function"] == "GenericArithmeticSession::new":
                self.assertIn("session.limits == *limits", weak)
                self.assertIn("session.contexts.template == template", weak)
            elif case["function"] == "validate_arithmetic_terminator":
                self.assertIn("offset <= data@.len() - 1", weak)
                self.assertIn("decreases data@.len() - 1 - offset", weak)
                self.assertIn("out.is_ok() ==> generic_region_spec::bounded_arithmetic_tail", weak)


if __name__ == "__main__":
    unittest.main()

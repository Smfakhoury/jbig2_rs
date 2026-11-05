from pathlib import Path
import sys
import unittest


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from check_clone_result_proofs import BASELINE_GROUPS, CASES, ROOT
from check_contract_mutations import MUTATIONS
from check_traceability import check_contract
from proof_sensitivity import replace_once, weakened_contract
from rust_source import contract_sections, direct_contract_sections
from source_group import SourceGroup, relocate_cases


class CloneResultProofTests(unittest.TestCase):
    def test_ten_real_faults_and_weaker_controls_have_independent_native_oracles(self):
        self.assertEqual(len(CASES), 10)
        self.assertEqual(len({case["id"] for case in CASES}), 10)
        for case in CASES:
            with self.subTest(case=case["id"]):
                source = SourceGroup(ROOT, case["file"])
                mutant = replace_once(source.text, case["site"], case["replacement"])
                weaker = weakened_contract(mutant, case)
                self.assertNotEqual(mutant, source.text)
                self.assertNotEqual(mutant, weaker)
                self.assertEqual(
                    sum(row["id"] == "clone-result-" + case["id"] for row in MUTATIONS), 1,
                )
                self.assertTrue(case["native_test"].endswith((
                    "independent_configuration", "adaptive_slot", "adaptive_slots",
                    "owned_storage", "nested_retention",
                )))

    def test_pristine_targets_relocate_to_the_three_actual_owner_scopes(self):
        cases, groups = relocate_cases(ROOT, CASES, BASELINE_GROUPS, {
            "src/document.rs", "src/generic_region.rs", "src/segment_header.rs",
        })
        self.assertEqual(sum(len(functions) for _, functions in groups), 4)
        self.assertEqual(len({case["function"] for case in cases}), 5)
        self.assertEqual({case["verification_module"] for case in cases}, {
            "document", "generic_region", "segment_header",
        })

    def test_direct_legacy_guarantee_is_not_manufactured_by_a_nested_closure_or_comment(self):
        record = {"id": "example", "attributes": [], "body_text": """
fn example(value: u8) -> u8 {
    let nested = |input: u8| {
        ensures(|ret: u8| equal(ret, input));
        input
    };
    // ensures(|ret: u8| equal(ret, value));
    nested(value)
}
"""}
        self.assertEqual(direct_contract_sections(record, "ensures"), [])
        self.assertEqual(contract_sections(record, "ensures", include_legacy=True), [])
        with self.assertRaisesRegex(ValueError, "not present"):
            check_contract(record, "ensures", ["equal(ret, input)"])
        record["body_text"] = """
fn example(value: u8) -> u8 {
    #[cfg(verus_keep_ghost)]
    ensures(|ret: u8| equal(ret, value) /* fake_ret == 93 */);
    value
}
"""
        self.assertEqual(contract_sections(record, "ensures"), [])
        self.assertEqual(len(direct_contract_sections(record, "ensures")), 1)
        check_contract(record, "ensures", ["equal(ret, value)"])
        with self.assertRaisesRegex(ValueError, "not present"):
            check_contract(record, "ensures", ["fake_ret == 93"])


if __name__ == "__main__":
    unittest.main()

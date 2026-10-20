import sys
import unittest
from pathlib import Path


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from check_contract_mutations import MUTATIONS, mutate
from evidence import load_json
from rust_source import parse


def integration_test_names(target):
    path = TOOLS.parent / "tests" / f"{target}.rs"
    names = set()
    attributes = []
    for node in parse(path.read_bytes()).named_children:
        if node.type in {"line_comment", "block_comment"}:
            continue
        if node.type == "attribute_item":
            attributes.append(b"".join(node.text.split()))
            continue
        if (
            node.type == "function_item" and b"#[test]" in attributes
            and not any(attribute.startswith(b"#[ignore") for attribute in attributes)
        ):
            names.add(node.child_by_field_name("name").text.decode("utf-8"))
        attributes = []
    return names


class ContractMutationTests(unittest.TestCase):
    def test_mutation_sites_are_unique_and_not_noops(self):
        identifiers = [mutation["id"] for mutation in MUTATIONS]
        self.assertEqual(len(identifiers), len(set(identifiers)))
        for mutation in MUTATIONS:
            with self.subTest(mutation=mutation["id"]):
                source = (TOOLS.parent / mutation["file"]).read_text(encoding="utf-8")
                changed = mutate(source, mutation)
                self.assertNotEqual(changed, source)
                self.assertEqual(changed.count(mutation["after"]), 1)

    def test_missing_or_ambiguous_sites_fail_closed(self):
        mutation = {"id": "example", "before": "before", "after": "after"}
        for source in ("", "before before"):
            with self.subTest(source=source), self.assertRaisesRegex(ValueError, "missing or ambiguous"):
                mutate(source, mutation)

    def test_mutation_filters_name_real_integration_tests(self):
        for mutation in MUTATIONS:
            self.assertIn(mutation["test"], integration_test_names(mutation["target"]))

    def test_new_catalog_cases_require_exact_executable_test_references(self):
        legacy = {
            "NEG-TRUNCATE-STANDALONE-EVERY-BYTE", "NEG-SEGMENT-RESERVED-TYPE",
            "NEG-SEGMENT-DUPLICATE-NUMBER", "NEG-REFERENCE-MISSING", "NEG-REFERENCE-DEPTH",
            "NEG-RETENTION-PREMATURE", "NEG-PAGE-DIMENSION-BOMB", "NEG-ARITHMETIC-TRUNCATED-BYTEIN",
            "NEG-HUFFMAN-OVERSUBSCRIBED", "NEG-MMR-EXTENSION", "NEG-MMR-RUN-OVERSHOOT",
            "NEG-GENERIC-BAD-AT", "NEG-HALFTONE-PATTERN-ID", "NEG-STRIPE-SEALED-WRITE",
            "META-ORGANIZATION-EQUIVALENCE", "META-REPEAT-DETERMINISM", "META-BYTE-READER",
            "META-COMBINATION-IDENTITIES", "META-STREAMING-COLLECTION",
        }
        structured = 0
        for kind in ("negative", "metamorphic"):
            document = load_json(TOOLS.parent / "validation" / kind / "cases.json")
            for case in document["cases"]:
                if "tests" not in case:
                    self.assertIn(case["id"], legacy, "new cases need explicit executable references")
                    continue
                self.assertTrue(case["tests"], case["id"])
                structured += 1
                for reference in case["tests"]:
                    self.assertIn(
                        reference["name"], integration_test_names(reference["target"]), case["id"],
                    )
        self.assertGreaterEqual(structured, 5)


if __name__ == "__main__":
    unittest.main()

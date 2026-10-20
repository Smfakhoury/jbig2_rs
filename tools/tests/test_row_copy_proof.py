import sys
import unittest
from pathlib import Path


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from check_row_copy_proof import CASES, COPY, RESULT, mutated_copy, storage_only_contract


class RowCopyProofFixtureTests(unittest.TestCase):
    def setUp(self):
        self.source = (TOOLS.parent / "src/bitmap.rs").read_text(encoding="utf-8")

    def test_each_mutant_preserves_its_strong_contract(self):
        for identity, replacement in CASES:
            with self.subTest(case=identity):
                changed = mutated_copy(self.source, replacement)
                self.assertNotEqual(changed, self.source)
                self.assertIn(RESULT, changed)
                self.assertIn(replacement, changed)

    def test_storage_control_only_removes_byte_result(self):
        control = storage_only_contract(self.source)
        self.assertEqual(control, self.source.replace(RESULT, "", 1))
        self.assertIn("crate::spec::bitmap::storage_valid(final(self))", control)
        self.assertIn("final(self).data@.len() == old(self).data@.len()", control)
        self.assertIn("out.is_ok()", control)

    def test_missing_and_duplicate_sites_fail_closed(self):
        for source in ("", self.source + COPY):
            with self.assertRaises(ValueError):
                mutated_copy(source, ";")
        with self.assertRaises(ValueError):
            storage_only_contract(storage_only_contract(self.source))


if __name__ == "__main__":
    unittest.main()

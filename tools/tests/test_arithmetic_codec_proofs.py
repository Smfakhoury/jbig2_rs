import contextlib
import io
import json
from pathlib import Path
import runpy
import sys
import tempfile
import unittest
from unittest.mock import patch


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from source_group import source_text
from check_arithmetic_codec_proofs import CASES
from proof_sensitivity import replace_once, weakened_contract
from rust_source import parse


class ArithmeticCodecProofTests(unittest.TestCase):
    def test_literal_controls_retain_each_original_validity_boundary(self):
        literals = [case for case in CASES if "native_test" in case]
        self.assertEqual(len(literals), 6)
        self.assertEqual(len({case["function"] for case in literals}), 5)
        for case in literals:
            with self.subTest(case=case["id"]):
                strong, weaker = case["controls"][0]
                self.assertIn("bit_read_error", strong)
                self.assertNotIn("bit_read_error", weaker)
                if case["function"].startswith("SymbolCodec::"):
                    self.assertIn("requires symbol_text_spec::codec_valid(old(self))", weaker)
                    self.assertIn("Ok(_) => symbol_text_spec::codec_valid(final(self))", weaker)
                else:
                    self.assertIn("reader_valid(&old(self).reader)", weaker)
                    self.assertIn("reader_valid(&final(self).reader)", weaker)

    def test_unique_sites_and_relation_specific_controls(self):
        self.assertEqual(len(CASES), 25)
        self.assertEqual(len({case["id"] for case in CASES}), 25)
        for case in CASES:
            source = source_text(TOOLS.parent, case["file"])
            with self.subTest(case=case["id"]):
                changed = replace_once(source, case["site"], case["replacement"])
                self.assertNotEqual(changed, source)
                self.assertNotEqual(weakened_contract(changed, case), changed)

    def test_context_validation_controls_preserve_original_success_guarantee(self):
        for case in CASES:
            if case["function"] != "validate_probability_states":
                continue
            source = source_text(TOOLS.parent, case["file"])
            with self.subTest(case=case["id"]):
                weaker = weakened_contract(source, case)
                self.assertIn("Ok(()) => forall|index: int|", weaker)
                self.assertIn("Err(_) => true,", weaker)
                self.assertIn("decreases contexts@.len() - index,", weaker)

    def test_mutants_preserve_ordinary_rust_syntax(self):
        for case in CASES:
            source = source_text(TOOLS.parent, case["file"])
            with self.subTest(case=case["id"]):
                changed = replace_once(source, case["site"], case["replacement"])
                self.assertFalse(parse(changed.encode()).has_error)

    def test_missing_or_duplicate_sites_fail_closed(self):
        for case in CASES:
            for source in ("", case["site"] + "\n" + case["site"]):
                with self.subTest(case=case["id"]), self.assertRaises(ValueError):
                    replace_once(source, case["site"], case["replacement"])

    def test_missing_or_repeated_controls_fail_closed(self):
        for case in CASES:
            with self.subTest(case=case["id"]), self.assertRaises(ValueError):
                weakened_contract("", case)
            source = source_text(TOOLS.parent, case["file"])
            with self.subTest(case=case["id"]), self.assertRaises(ValueError):
                weakened_contract(weakened_contract(source, case), case)

    def test_cli_propagates_rejected_verification_as_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "failed.json"
            arguments = [
                str(TOOLS / "check_arithmetic_codec_proofs.py"),
                "--verus-executable", str(Path(directory) / "verus"),
                "--output", str(output),
            ]
            with patch.object(sys, "argv", arguments), patch(
                "proof_sensitivity.check_semantic_cases", side_effect=ValueError("rejected control"),
            ), contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit) as stopped:
                    runpy.run_path(arguments[0], run_name="__main__")
            self.assertEqual(stopped.exception.code, 1)
            self.assertEqual(json.loads(output.read_text())["status"], "failed")


if __name__ == "__main__":
    unittest.main()

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
from check_mmr_reference_proofs import CASES
from proof_sensitivity import replace_once, weakened_contract


class MmrReferenceProofTests(unittest.TestCase):
    def test_unique_mutations_retain_original_input_and_result_bounds(self):
        source = (TOOLS.parent / "src/mmr.rs").read_text()
        self.assertEqual(len(CASES), 6)
        self.assertEqual(len({case["id"] for case in CASES}), 6)
        for case in CASES:
            with self.subTest(case=case["id"]):
                changed = replace_once(source, case["site"], case["replacement"])
                weaker = weakened_contract(changed, case)
                self.assertNotEqual(source, changed)
                self.assertNotEqual(changed, weaker)
                self.assertIn("mmr_spec::reference_line_valid(transitions@, width)", weaker)
                self.assertIn("x <= b1 <= b2 <= width", weaker)
                self.assertIn("*final(search_start) < transitions@.len()", weaker)
                self.assertIn("*old(search_start) < transitions@.len()", weaker)

    def test_missing_or_duplicate_sites_fail_closed(self):
        for case in CASES:
            for source in ("", case["site"] + "\n" + case["site"]):
                with self.subTest(case=case["id"]), self.assertRaises(ValueError):
                    replace_once(source, case["site"], case["replacement"])

    def test_missing_or_repeated_controls_fail_closed(self):
        source = (TOOLS.parent / "src/mmr.rs").read_text()
        for case in CASES:
            with self.subTest(case=case["id"]), self.assertRaises(ValueError):
                weakened_contract("", case)
            with self.subTest(case=case["id"]), self.assertRaises(ValueError):
                weakened_contract(weakened_contract(source, case), case)

    def test_cli_failure_is_nonzero(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "failed.json"
            arguments = [
                str(TOOLS / "check_mmr_reference_proofs.py"),
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

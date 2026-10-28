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
from check_mmr_horizontal_proofs import CASES
from proof_sensitivity import replace_once, weakened_contract


class MmrHorizontalProofTests(unittest.TestCase):
    def test_unique_mutations_and_retained_reader_endpoint_bounds(self):
        source = source_text(TOOLS.parent, "src/mmr.rs")
        self.assertEqual(len(CASES), 6)
        self.assertEqual(len({case["id"] for case in CASES}), 6)
        for case in CASES:
            with self.subTest(case=case["id"]):
                changed = replace_once(source, case["site"], case["replacement"])
                weaker = weakened_contract(changed, case)
                self.assertNotEqual(source, changed)
                self.assertNotEqual(weaker, changed)
                self.assertIn("x <= after_first <= end <= width", weaker)
                self.assertIn("final(reader).bytes@ == old(reader).bytes@", weaker)
                self.assertIn("mmr_spec::reader_valid(final(reader))", weaker)
                if case["id"] != "horizontal_accepts_zero_pair":
                    self.assertIn("&& x < end", weaker)

    def test_missing_or_duplicate_sites_fail_closed(self):
        for case in CASES:
            for source in ("", case["site"] + "\n" + case["site"]):
                with self.subTest(case=case["id"]), self.assertRaises(ValueError):
                    replace_once(source, case["site"], case["replacement"])

    def test_missing_or_repeated_controls_fail_closed(self):
        source = source_text(TOOLS.parent, "src/mmr.rs")
        for case in CASES:
            with self.subTest(case=case["id"]), self.assertRaises(ValueError):
                weakened_contract("", case)
            with self.subTest(case=case["id"]), self.assertRaises(ValueError):
                weakened_contract(weakened_contract(source, case), case)

    def test_cli_failure_is_nonzero(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "failed.json"
            arguments = [
                str(TOOLS / "check_mmr_horizontal_proofs.py"),
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

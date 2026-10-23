import contextlib
import io
import json
import runpy
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from check_mmr_span_proofs import CASES
from proof_sensitivity import replace_once, weakened_contract


class MmrSpanProofFixtureTests(unittest.TestCase):
    def setUp(self):
        self.source = (TOOLS.parent / "src/mmr.rs").read_text(encoding="utf-8")

    def test_distinct_invalid_white_and_empty_mutations_have_unique_sites(self):
        self.assertEqual(len({case["id"] for case in CASES}), 8)
        self.assertEqual(
            {case["function"] for case in CASES},
            {"fill_span", "fill_recorded_span", "record_transition_span"},
        )
        for case in CASES:
            with self.subTest(case=case["id"]):
                changed = replace_once(self.source, case["site"], case["replacement"])
                self.assertNotEqual(changed, self.source)
                self.assertIn(case["replacement"], changed)

    def test_weaker_controls_retain_storage_and_typed_error_guarantees(self):
        for case in CASES:
            with self.subTest(case=case["id"]):
                control = weakened_contract(self.source, case)
                self.assertIn("final(bitmap).data@.len() == old(bitmap).data@.len()", control)
                self.assertIn("mmr_spec::bitmap_storage_valid(final(bitmap))", control)
                self.assertIn("matches!(out, Err(MmrError::InvalidTransition))", control)
                self.assertNotEqual(self.source, control)

    def test_missing_ambiguous_and_already_weakened_sites_fail_closed(self):
        for case in CASES:
            with self.subTest(case=case["id"]):
                for source in ("", self.source + "\n" + case["site"]):
                    with self.assertRaises(ValueError):
                        replace_once(source, case["site"], case["replacement"])
                with self.assertRaises(ValueError):
                    weakened_contract(weakened_contract(self.source, case), case)

    def test_cli_failure_is_nonzero(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "failed.json"
            arguments = [
                str(TOOLS / "check_mmr_span_proofs.py"),
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

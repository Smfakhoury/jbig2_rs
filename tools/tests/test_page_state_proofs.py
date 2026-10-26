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
from check_page_state_proofs import CASES, CASE_GROUPS, FIELDS, FINISH_RESULT, HEIGHT, STRIPE_RESULT
from proof_sensitivity import replace_once, weakened_contract


class PageStateProofTests(unittest.TestCase):
    def test_scheduling_partitions_preserve_all_individual_cases_and_registrations(self):
        from run_semantic_proofs import CHECKS

        selected = [case["id"] for cases in CASE_GROUPS.values() for case in cases]
        self.assertEqual(len(selected), len(CASES))
        self.assertEqual(set(selected), {case["id"] for case in CASES})
        self.assertEqual({name: len(cases) for name, cases in CASE_GROUPS.items()}, {
            "page-state": 13, "page-finish": 7, "page-public": 9,
        })
        self.assertEqual({
            identity: count for identity, script, count, *_ in CHECKS
            if script == "check_page_state_proofs.py"
        }, {name: len(cases) for name, cases in CASE_GROUPS.items()})

    def test_individual_sites_and_controls_retain_independent_guarantees(self):
        source = (TOOLS.parent / "src/document.rs").read_text()
        self.assertEqual(len(CASES), 29)
        self.assertEqual(len({case["id"] for case in CASES}), 29)
        for case in CASES:
            with self.subTest(case=case["id"]):
                changed = replace_once(source, case["site"], case["replacement"])
                weaker = weakened_contract(changed, case)
                self.assertNotEqual(changed, source)
                self.assertNotEqual(weaker, changed)
                normalized = " ".join(weaker.split())
                self.assertIn(STRIPE_RESULT, weaker if "stripe_" not in case["id"] else source)
                if case["id"].startswith("public_changes_") and case["id"].split("_")[-1] not in ("error",):
                    self.assertIn(" ".join(HEIGHT.split()), normalized)
                if case["id"].startswith(("public_discards_", "public_omits_")) or case["id"].endswith("_error"):
                    self.assertIn(FIELDS, weaker)
                if case["id"].startswith("stripe_"):
                    self.assertIn(FINISH_RESULT.splitlines()[0], weaker)
                    if "state_on_error" not in case["id"] and "successful_update" not in case["id"]:
                        self.assertIn("final(self).last_stripe == if out is Ok", weaker)
                self.assertTrue(case["test"].startswith("document::tests::raw_page_state_"))

    def test_missing_or_ambiguous_sites_and_controls_fail_closed(self):
        source = (TOOLS.parent / "src/document.rs").read_text()
        for case in CASES:
            with self.subTest(case=case["id"]):
                for text in ("", case["site"] + "\n" + case["site"]):
                    with self.assertRaises(ValueError):
                        replace_once(text, case["site"], case["replacement"])
                with self.assertRaises(ValueError):
                    weakened_contract("", case)
                with self.assertRaises(ValueError):
                    weakened_contract(weakened_contract(source, case), case)

    def test_cli_failure_is_nonzero_and_retains_diagnostic(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "failed.json"
            arguments = [
                str(TOOLS / "check_page_state_proofs.py"),
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

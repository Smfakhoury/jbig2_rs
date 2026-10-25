import json
import sys
import unittest
from pathlib import Path
from subprocess import CompletedProcess
from unittest.mock import patch


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from proof_sensitivity import selected_proof, weakened_contract


class SelectedProofEvidenceTests(unittest.TestCase):
    def test_multi_clause_controls_are_explicit_and_nonempty(self):
        self.assertEqual(
            weakened_contract("first second", {"controls": (("first", "one"), ("second", "two"))}),
            "one two",
        )
        with self.assertRaises(ValueError):
            weakened_contract("unchanged", {"controls": ()})

    def verify(self, report, returncode=0, stderr="", expect_failure=False, function="finalize_pages"):
        process = CompletedProcess([], returncode, json.dumps(report), stderr)
        with patch("proof_sensitivity.subprocess.run", return_value=process):
            return selected_proof(
                Path("/verus"), TOOLS.parent, {}, expect_failure, "document", function,
            )

    def report(self):
        return {
            "verification-results": {
                "is-verifying-entire-crate": False, "verified": 1, "errors": 0,
            },
            "func-details": {"jbig2_rs_production::document::finalize_pages": {}},
        }

    def test_baseline_requires_nonzero_proofs_and_exact_target(self):
        self.assertEqual(self.verify(self.report())["errors"], 0)
        for key, value in [("verified", 0), ("errors", 1), ("is-verifying-entire-crate", True)]:
            report = self.report()
            report["verification-results"][key] = value
            with self.subTest(key=key), self.assertRaises(ValueError):
                self.verify(report)
        report = self.report()
        report["func-details"] = {"other::finalize_pages": {}}
        with self.assertRaises(ValueError):
            self.verify(report)

    def test_only_expected_failure_skips_additional_error_searches(self):
        for expected_failure in (False, True):
            report = self.report()
            report["verification-results"]["errors"] = int(expected_failure)
            process = CompletedProcess(
                [], int(expected_failure), json.dumps(report),
                "postcondition not satisfied" if expected_failure else "",
            )
            with self.subTest(expected_failure=expected_failure), patch(
                "proof_sensitivity.subprocess.run", return_value=process,
            ) as run:
                selected_proof(
                    Path("/verus"), TOOLS.parent, {}, expected_failure,
                    "document", "finalize_pages",
                )
            command = run.call_args.args[0]
            self.assertEqual("--multiple-errors" in command, expected_failure)
            if expected_failure:
                self.assertEqual(command[-2:], ["--multiple-errors", "0"])
            self.assertIn("--verify-function", command)
            self.assertIn("--rlimit", command)

    def test_compiler_or_timeout_diagnostics_do_not_count_as_semantic_rejections(self):
        report = self.report()
        report["verification-results"]["errors"] = 1
        for diagnostic in ["type mismatch", "resource limit exceeded", ""]:
            with self.subTest(diagnostic=diagnostic), self.assertRaises(ValueError):
                self.verify(report, 1, diagnostic, True)

    def test_semantic_diagnostic_alone_does_not_count_without_failed_obligations(self):
        with self.assertRaises(ValueError):
            self.verify(self.report(), 1, "postcondition not satisfied", True)
        report = self.report()
        report["verification-results"]["errors"] = 1
        self.assertEqual(
            self.verify(report, 1, "loop invariant not satisfied", True)["errors"], 1,
        )

    def test_shared_positive_module_baseline_requires_every_exact_target_and_partial_proofs(self):
        functions = ("finalize_pages", "build_document_info")
        report = self.report()
        report["verification-results"]["verified"] = 2
        report["func-details"]["jbig2_rs_production::document::build_document_info"] = {}
        process = CompletedProcess([], 0, json.dumps(report), "")
        with patch("proof_sensitivity.subprocess.run", return_value=process) as run:
            selected_proof(Path("/verus"), TOOLS.parent, {}, False, "document", functions)
        command = run.call_args.args[0]
        self.assertEqual(command[command.index("--verify-function") + 1], "*")
        self.assertEqual(command[command.index("--verify-only-module") + 1], "document")
        self.assertEqual(command[command.index("--num-threads") + 1], "1")
        self.assertEqual(command[command.index("--rlimit") + 1], "30")
        for changed in ("missing-target", "too-few-proofs", "whole-report"):
            broken = json.loads(json.dumps(report))
            if changed == "missing-target":
                del broken["func-details"]["jbig2_rs_production::document::build_document_info"]
            elif changed == "too-few-proofs":
                broken["verification-results"]["verified"] = 1
            else:
                broken["verification-results"]["is-verifying-entire-crate"] = True
            with self.subTest(changed=changed), self.assertRaises(ValueError):
                self.verify(broken, function=functions)

    def test_shared_baseline_rejects_empty_duplicate_and_failure_groups_before_execution(self):
        for functions, failure in (((), False), (("same", "same"), False), (("one",), True)):
            with self.subTest(functions=functions, failure=failure), patch(
                "proof_sensitivity.subprocess.run",
            ) as run, self.assertRaises(ValueError):
                selected_proof(Path("/verus"), TOOLS.parent, {}, failure, "document", functions)
            run.assert_not_called()


if __name__ == "__main__":
    unittest.main()

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

    def verify(self, report, returncode=0, stderr="", expect_failure=False):
        process = CompletedProcess([], returncode, json.dumps(report), stderr)
        with patch("proof_sensitivity.subprocess.run", return_value=process):
            return selected_proof(
                Path("/verus"), TOOLS.parent, {}, expect_failure, "document", "finalize_pages",
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


if __name__ == "__main__":
    unittest.main()

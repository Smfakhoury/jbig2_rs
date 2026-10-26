import json
import shutil
import sys
import tempfile
import unittest
from contextlib import contextmanager
from pathlib import Path
from subprocess import CompletedProcess
from unittest.mock import patch


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from proof_sensitivity import check_semantic_cases, positive_baseline, selected_proof, weakened_contract


class SelectedProofEvidenceTests(unittest.TestCase):
    def test_receipt_replaces_only_pristine_baseline_not_negative_or_control_queries(self):
        from run_semantic_proofs import baseline_from_whole

        whole = {
            "receipt_sha256": "1" * 64, "report_sha256": "2" * 64,
            "report": {
                "verification-results": {
                    "success": True, "is-verifying-entire-crate": True, "verified": 1, "errors": 0,
                },
                "func-details": {"jbig2_rs_production::document::finalize_pages": {}},
            },
        }
        with patch.dict("os.environ", {"JBIG2_WHOLE_BASELINE": "/receipt"}), patch(
            "run_semantic_proofs.load_whole_baseline", return_value=whole,
        ) as load, patch("proof_sensitivity.selected_proof") as verify:
            result = positive_baseline(
                TOOLS.parent, Path("/verus"), TOOLS.parent, {}, "document", "finalize_pages",
            )
            self.assertEqual(result, baseline_from_whole(
                whole, ["jbig2_rs_production::document::finalize_pages"],
            ))
            verify.assert_not_called()
            load.assert_called_once_with(TOOLS.parent, Path("/receipt"))
        for failure in (True, False):
            with patch.dict("os.environ", {"JBIG2_WHOLE_BASELINE": "/invalid"}), patch(
                "run_semantic_proofs.load_whole_baseline", side_effect=AssertionError("must not reuse"),
            ):
                report = self.report()
                report["verification-results"]["errors"] = int(failure)
                process = CompletedProcess(
                    [], int(failure), json.dumps(report),
                    "postcondition not satisfied" if failure else "",
                )
                with patch("proof_sensitivity.subprocess.run", return_value=process) as verify:
                    selected_proof(
                        Path("/verus"), TOOLS.parent, {}, failure, "document", "finalize_pages",
                    )
                verify.assert_called_once()
        with patch.dict("os.environ", {"JBIG2_WHOLE_BASELINE": "/invalid"}), patch(
            "run_semantic_proofs.load_whole_baseline", side_effect=ValueError("stale receipt"),
        ), patch("proof_sensitivity.selected_proof") as verify, self.assertRaisesRegex(ValueError, "stale"):
            positive_baseline(TOOLS.parent, Path("/verus"), TOOLS.parent, {}, "document", "finalize_pages")
        verify.assert_not_called()

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

    def test_missing_scope_reports_the_actual_verifier_diagnostic(self):
        report = self.report()
        del report["verification-results"]["is-verifying-entire-crate"]
        with self.assertRaisesRegex(ValueError, "trigger diagnostic"):
            self.verify(report, 1, "trigger diagnostic")

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

    def test_shared_baseline_uses_narrow_common_prefix_and_keeps_individual_failures(self):
        report = self.report()
        names = ("widen_u8_i32", "widen_u8_i64")
        report["verification-results"]["verified"] = 2
        report["func-details"] = {
            f"jbig2_rs_production::symbol_text::{name}": {} for name in names
        }
        process = CompletedProcess([], 0, json.dumps(report), "")
        with patch("proof_sensitivity.subprocess.run", return_value=process) as run:
            selected_proof(Path("/verus"), TOOLS.parent, {}, False, "symbol_text", names)
        command = run.call_args.args[0]
        self.assertEqual(command[command.index("--verify-function") + 1], "widen_u8_i*")
        for name in names:
            with patch("proof_sensitivity.subprocess.run", return_value=process) as run:
                selected_proof(Path("/verus"), TOOLS.parent, {}, False, "symbol_text", name)
            command = run.call_args.args[0]
            self.assertEqual(command[command.index("--verify-function") + 1], name)

    def test_shared_baseline_runs_once_on_pristine_source_and_keeps_every_case_query(self):
        with tempfile.TemporaryDirectory() as path:
            root = Path(path) / "source"
            directory = Path(path) / "proof"
            root.mkdir()
            directory.mkdir()
            original = "first second contract"
            (root / "fixture.rs").write_text(original)
            shutil.copyfile(root / "fixture.rs", directory / "fixture.rs")
            cases = tuple({
                "id": name, "file": "fixture.rs", "module": "module", "function": name,
                "site": name, "replacement": "bad_" + name, "controls": (("contract", "weak"),),
            } for name in ("first", "second"))
            calls = []

            @contextmanager
            def workspace(*args):
                yield directory, {}, "pinned", "rust"

            def verify(executable, checkout, environment, failure, module, function):
                calls.append((failure, module, function))
                if isinstance(function, tuple):
                    self.assertEqual((directory / "fixture.rs").read_text(), original)
                return {"verified": 2, "errors": int(failure)}

            groups = (("module", ("first", "second")),)
            with patch("proof_sensitivity.proof_workspace", workspace), patch(
                "proof_sensitivity.selected_proof", side_effect=verify,
            ), patch("proof_sensitivity.run"):
                result = check_semantic_cases(
                    root, Path("/verus"), cases, ("fixture.rs",), "fixture-", "scope",
                    baseline_groups=groups,
                )
            self.assertEqual(calls, [
                (False, "module", ("first", "second")),
                (True, "module", "first"), (False, "module", "first"),
                (True, "module", "second"), (False, "module", "second"),
            ])
            self.assertEqual((directory / "fixture.rs").read_text(), original)
            self.assertTrue(all(row["baseline_selected_functions"] == ["first", "second"]
                                for row in result["counterexamples"]))
            for invalid in (
                (("module", ("first", "unknown")),),
                (("module", ("first", "first")),),
                groups + groups,
            ):
                with self.subTest(invalid=invalid), patch(
                    "proof_sensitivity.proof_workspace",
                ) as workspace, self.assertRaises(ValueError):
                    check_semantic_cases(
                        root, Path("/verus"), cases, ("fixture.rs",), "fixture-", "scope",
                        baseline_groups=invalid,
                    )
                workspace.assert_not_called()


if __name__ == "__main__":
    unittest.main()

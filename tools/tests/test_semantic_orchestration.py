import copy
import json
import io
import os
from pathlib import Path
import sys
import tempfile
import threading
from types import SimpleNamespace
import unittest
from contextlib import redirect_stderr, redirect_stdout
from unittest.mock import patch


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))
import run_semantic_proofs as runner


def evidence():
    return {
        "schema_version": 2, "status": "passed", "verus_version": "pinned", "production_sources_modified": False,
        "counterexamples": [
            {"id": "case", "function": "module::function",
             "normal_rust_compiles": True, "strong_contract": "rejected",
             "weaker_control": "accepted", "strong_verifier_errors": 1,
             "control_verifier_errors": 0,
             "baseline_selected_verification": {
                 "success": True, "errors": 0, "verified": 1,
                 "encountered-error": False, "encountered-vir-error": False,
                 "is-verifying-entire-crate": False,
             }},
        ],
    }


class SemanticOrchestrationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        (self.root / "tools").mkdir()
        (self.root / "src").mkdir()
        (self.root / "src/example.rs").write_text("// source fixture\n")
        (self.root / "toolchain.json").write_text(json.dumps({"verus": {"version": "pinned"}}))
        self.checks = tuple((str(index), f"{index}.py", 1, "strong_contract", "weaker_control") for index in range(4))
        for _, script, *_ in self.checks:
            (self.root / "tools" / script).write_text(
                'CASES = ({"id": "case", "module": "module", "function": "function"},)\n'
            )
        self.output = self.root / "reports"
        self.environment = patch.dict(os.environ, {
            name: "local" for name in (
                "GITHUB_REPOSITORY", "GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT", "GITHUB_SHA",
            )
        })
        self.environment.start()
        self.addCleanup(self.environment.stop)

    def run_fixture(self, process):
        with patch.object(runner, "CHECKS", self.checks), patch.object(runner.subprocess, "run", side_effect=process):
            return runner.run_checks(self.root, self.root / "verus", self.output)

    def passing_process(self, command, **kwargs):
        report = evidence()
        report["source_bindings"] = {"src/example.rs": runner.digest(self.root / "src/example.rs")}
        Path(command[-1]).write_text(json.dumps(report))
        return SimpleNamespace(returncode=0)

    def test_manifest_covers_every_semantic_checker_without_fixed_group_or_case_totals(self):
        checks = runner.configured_checks()
        self.assertEqual(len({row[0] for row in checks}), len(checks))
        self.assertEqual(
            {row[1] for row in checks},
            {path.name for path in TOOLS.glob("check_*proof*.py")},
        )
        self.assertTrue(all(type(row[2]) is int and row[2] > 0 for row in checks))

    def test_actual_work_overlaps_but_never_exceeds_two_children(self):
        lock = threading.Lock()
        active = 0
        maximum = 0
        barrier = threading.Barrier(2)

        def process(command, **kwargs):
            nonlocal active, maximum
            with lock:
                active += 1
                maximum = max(maximum, active)
            barrier.wait(timeout=10)
            result = self.passing_process(command, **kwargs)
            with lock:
                active -= 1
            return result

        result = self.run_fixture(process)
        self.assertEqual(result["status"], "passed")
        self.assertEqual(maximum, 2)
        self.assertEqual([row["id"] for row in result["checks"]], ["0", "1", "2", "3"])

    def test_failed_child_cannot_reuse_a_stale_passing_report(self):
        self.output.mkdir()
        for identity, *_ in self.checks:
            (self.output / f"{identity}-proof-sensitivity.json").write_text(json.dumps(evidence()))
        result = self.run_fixture(lambda *args, **kwargs: SimpleNamespace(returncode=1))
        self.assertEqual(result["status"], "failed")
        self.assertTrue(all(row["status"] == "failed" for row in result["checks"]))
        self.assertFalse(any(self.output.glob("*-proof-sensitivity.json")))

    def test_exit_zero_without_report_fails(self):
        result = self.run_fixture(lambda *args, **kwargs: SimpleNamespace(returncode=0))
        self.assertEqual(result["status"], "failed")

    def test_invalid_evidence_shapes_and_weakened_results_fail(self):
        mutations = [
            lambda r: r.update(status="failed"),
            lambda r: r.update(verus_version="floating"),
            lambda r: r.update(production_sources_modified=True),
            lambda r: r.update(counterexamples=[]),
            lambda r: r["counterexamples"][0].update(normal_rust_compiles=False),
            lambda r: r["counterexamples"][0].update(strong_contract="accepted"),
            lambda r: r["counterexamples"][0].update(weaker_control="rejected"),
            lambda r: r["counterexamples"][0].update(strong_verifier_errors=0),
            lambda r: r["counterexamples"][0].update(control_verifier_errors=1),
            lambda r: r["counterexamples"][0].update(control_verifier_errors=False),
        ]
        for mutate in mutations:
            with self.subTest(mutate=mutate):
                report = copy.deepcopy(evidence())
                mutate(report)
                with self.assertRaises(ValueError):
                    runner.check_report(report, 1, "strong_contract", "weaker_control", "pinned")

    def test_duplicate_records_are_rejected(self):
        report = evidence()
        report["counterexamples"] *= 2
        with self.assertRaises(ValueError):
            runner.check_report(report, 2, "strong_contract", "weaker_control", "pinned")

    def test_malformed_json_and_launch_failures_are_explicit(self):
        def malformed(command, **kwargs):
            Path(command[-1]).write_text("{")
            return SimpleNamespace(returncode=0)

        for process in (malformed, lambda *args, **kwargs: (_ for _ in ()).throw(OSError("launch denied"))):
            with self.subTest(process=process):
                result = self.run_fixture(process)
                self.assertEqual(result["status"], "failed")
                self.assertTrue(all("error" in row for row in result["checks"]))

    def test_empty_and_duplicate_manifest_fail_and_remove_stale_summary(self):
        self.output.mkdir()
        summary = self.output / "semantic-proof-summary.json"
        for checks in ((), self.checks + self.checks):
            summary.write_text('{"status":"passed"}')
            with patch.object(runner, "CHECKS", checks), self.assertRaises(ValueError):
                runner.run_checks(self.root, self.root / "verus", self.output)
            self.assertFalse(summary.exists())

    def test_only_runs_one_known_complete_group_and_unknown_selector_fails(self):
        with patch.object(runner, "CHECKS", self.checks), patch.object(
            runner.subprocess, "run", side_effect=self.passing_process,
        ) as process:
            result = runner.run_checks(self.root, self.root / "verus", self.output, only="2")
            self.assertEqual(result["selected_groups"], ["2"])
            self.assertEqual(result["workers"], 1)
            self.assertEqual(len(result["manifest"]["groups"]), len(self.checks))
            self.assertEqual(process.call_count, 1)
            self.assertEqual(result["status"], "passed")
            with self.assertRaisesRegex(ValueError, "unknown"):
                runner.run_checks(self.root, self.root / "verus", self.output, only="../missing")
            self.assertFalse((self.output / runner.SUMMARY).exists())

    def test_matrix_is_generated_from_current_complete_checks(self):
        future_checks = self.checks + (("future-decision", "future.py", 4, "strong_contract", "weaker_control"),)
        with patch.object(runner, "CHECKS", future_checks), patch.object(
            sys, "argv", ["runner", "--matrix"],
        ), redirect_stdout(io.StringIO()) as stream:
            self.assertEqual(runner.main(), 0)
        self.assertEqual(json.loads(stream.getvalue()), {"group": [row[0] for row in future_checks]})

    def test_clean_shards_and_whole_job_outputs_have_identical_source_manifests(self):
        with patch.object(runner, "CHECKS", self.checks):
            before = runner.manifest(self.root)
            runner.write_json(self.root / "verus-production.json", {"generated": True})
            runner.write_json(self.root / "production-proof-coverage-ci.json", {"generated": True})
            self.assertEqual(runner.manifest(self.root), before)

    def test_selected_verus_baselines_do_not_require_whole_crate_only_success_field(self):
        baseline = evidence()["counterexamples"][0]["baseline_selected_verification"]
        baseline.pop("success")
        self.assertTrue(runner.baseline_passed(baseline))
        for field, value in (
            ("encountered-error", True), ("encountered-vir-error", True),
            ("errors", 1), ("verified", 0), ("is-verifying-entire-crate", True),
        ):
            changed = dict(baseline, **{field: value})
            with self.subTest(field=field):
                self.assertFalse(runner.baseline_passed(changed))

    def whole_fixture(self, whole):
        details = {"module::function": {"failed_proof_notes": []}}
        coverage = {
            "schema_version": 1, "source": "fixture", "verus_verified_obligations": 1,
            "native_assumptions": [],
            "counts": {"verified": 1, "incomplete": 0, "audited-forwarder": 0,
                       "reviewed-verifier-limitation": 0, "native-assumption": 0},
            "functions": [{"id": "fixture", "file": "src/example.rs",
                           "file_sha256": runner.digest(self.root / "src/example.rs"),
                           "status": "verified", "visibility": "pub", "disposition_reason": None,
                           "verus_functions": list(details)}],
        }
        runner.write_json(self.root / "production-proof-coverage.json", coverage)
        runner.write_json(whole / "production-proof-coverage-ci.json", coverage)
        runner.write_json(whole / "verus-production.json", {
            "verification-results": {
                "success": True, "errors": 0, "verified": 1, "is-verifying-entire-crate": True,
            },
            "func-details": details, "verus": {"version": "pinned"},
        })
        ledger = self.root / "validation/traceability/requirements.json"
        runner.write_json(ledger, {"requirements": [{"id": "fixture"}], "features": [{}]})
        runner.write_json(whole / runner.TRACE / "verifier-report.json", {
            "status": "references_valid_scope_incomplete", "verifier_report_checked": True,
            "requirements": [{"id": "fixture"}], "requirement_count": 1, "feature_count": 1,
            "ledger_sha256": runner.digest(ledger),
        })
        native = whole / "target/verus-production/libfixture.rlib"
        native.parent.mkdir(parents=True)
        native.write_bytes(b"native compilation fixture")

    def collection_fixture(self):
        inputs = self.root / "inputs"
        whole = inputs / (runner.artifact_prefix() + "whole")
        self.whole_fixture(whole)
        with patch.object(runner, "CHECKS", self.checks), patch.object(
            runner.subprocess, "run", side_effect=self.passing_process,
        ):
            runner.seal_whole(self.root, whole)
            for identity, *_ in self.checks:
                runner.run_checks(
                    self.root, self.root / "verus",
                    inputs / (runner.artifact_prefix() + identity) / runner.TRACE, only=identity,
                )
        return inputs

    def collect(self, inputs, whole="success", semantic="success"):
        with patch.object(runner, "CHECKS", self.checks):
            return runner.aggregate(self.root, inputs, self.output, whole, semantic)

    def test_full_aggregate_preserves_canonical_report_layout(self):
        inputs = self.collection_fixture()
        result = self.collect(inputs)
        self.assertEqual(result["status"], "passed")
        self.assertEqual(result["selected_groups"], [row[0] for row in self.checks])
        self.assertEqual(len(result["checks"]), len(self.checks))
        self.assertTrue((self.output / "verus-production.json").is_file())
        self.assertTrue((self.output / "production-proof-coverage-ci.json").is_file())
        self.assertTrue((self.output / runner.TRACE / runner.SUMMARY).is_file())
        for identity, *_ in self.checks:
            self.assertTrue((self.output / runner.TRACE / f"{identity}-proof-sensitivity.json").is_file())

    def test_missing_whole_or_each_group_fails_and_removes_stale_success(self):
        inputs = self.collection_fixture()
        for identity in ("whole", *(row[0] for row in self.checks)):
            path = inputs / (runner.artifact_prefix() + identity)
            moved = self.root / "removed"
            path.rename(moved)
            runner.write_json(self.output / runner.TRACE / runner.SUMMARY, {"status": "passed"})
            with self.subTest(identity=identity), self.assertRaises(ValueError):
                self.collect(inputs)
            self.assertFalse((self.output / runner.TRACE / runner.SUMMARY).exists())
            moved.rename(path)

    def test_job_results_fail_closed_even_with_complete_passing_artifacts(self):
        inputs = self.collection_fixture()
        for status in (None, "failure", "cancelled", "skipped", ""):
            for job in ("whole", "semantic"):
                with self.subTest(status=status, job=job), self.assertRaisesRegex(ValueError, "both succeed"):
                    self.collect(inputs, **{job: status})

    def test_failed_stale_duplicate_manifest_tool_and_source_evidence_are_rejected(self):
        inputs = self.collection_fixture()
        summary_path = inputs / (runner.artifact_prefix() + "0") / runner.TRACE / runner.SUMMARY
        original = runner.read_json(summary_path)
        mutations = [
            lambda r: r.update(status="failed"),
            lambda r: r.update(schema_version=1),
            lambda r: r["checks"][0].update(status="failed"),
            lambda r: r["checks"][0].update(exit_code=1),
            lambda r: r["checks"][0].update(exit_code=False),
            lambda r: r["checks"][0].update(checker_sha256="0" * 64),
            lambda r: r["checks"][0].update(counterexamples=2),
            lambda r: r["checks"][0].update(counterexamples=True),
            lambda r: r["checks"][0].update(report_sha256="0" * 64),
            lambda r: r["checks"].extend(r["checks"]),
            lambda r: r.update(selected_groups=["0", "0"]),
            lambda r: r["manifest"]["groups"].pop(),
            lambda r: r["manifest"]["verus"].update(version="floating"),
            lambda r: r["manifest"]["source_bindings"].update({"src/example.rs": "0" * 64}),
            lambda r: r["run_identity"].update(GITHUB_RUN_ATTEMPT="previous"),
        ]
        for mutate in mutations:
            report = copy.deepcopy(original)
            mutate(report)
            runner.write_json(summary_path, report)
            with self.subTest(mutate=mutate), self.assertRaises(ValueError):
                self.collect(inputs)
        runner.write_json(summary_path, original)
        (self.root / "src/example.rs").write_text("// changed source\n")
        with self.assertRaises(ValueError):
            self.collect(inputs)

    def test_success_shaped_report_rejected_even_with_updated_file_hash(self):
        inputs = self.collection_fixture()
        directory = inputs / (runner.artifact_prefix() + "0") / runner.TRACE
        report_path = directory / "0-proof-sensitivity.json"
        summary_path = directory / runner.SUMMARY
        original_report = runner.read_json(report_path)
        original_summary = runner.read_json(summary_path)
        mutations = [
            lambda r: r.update(schema_version=1),
            lambda r: r.pop("source_bindings"),
            lambda r: r.update(source_bindings={"src/example.rs": "0" * 64}),
            lambda r: r["counterexamples"][0].pop("baseline_selected_verification"),
            lambda r: r["counterexamples"][0].update(id="other-case"),
            lambda r: r["counterexamples"][0].update(function="other::target"),
            lambda r: r["counterexamples"][0].update(strong_contract="accepted"),
            lambda r: r["counterexamples"][0]["baseline_selected_verification"].update(success=False),
        ]
        for mutate in mutations:
            report = copy.deepcopy(original_report)
            mutate(report)
            runner.write_json(report_path, report)
            summary = copy.deepcopy(original_summary)
            summary["checks"][0]["report_sha256"] = runner.digest(report_path)
            runner.write_json(summary_path, summary)
            with self.subTest(mutate=mutate), self.assertRaises(ValueError):
                self.collect(inputs)

    def test_whole_failures_and_native_inventory_traceability_are_required(self):
        inputs = self.collection_fixture()
        whole = inputs / (runner.artifact_prefix() + "whole")
        for relative in (
            runner.WHOLE_SUMMARY, "verus-production.json", "production-proof-coverage-ci.json",
            "target/traceability/verifier-report.json", "target/verus-production/libfixture.rlib",
        ):
            path = whole / relative
            original = path.read_bytes()
            path.unlink()
            with self.subTest(missing=relative), self.assertRaises((ValueError, OSError)):
                self.collect(inputs)
            path.write_bytes(original)
        report = runner.read_json(whole / "verus-production.json")
        for mutate in (
            lambda r: r["verification-results"].update(success=False),
            lambda r: r["verification-results"].update(**{"is-verifying-entire-crate": False}),
            lambda r: r.update(**{"func-details": {}}),
            lambda r: r["verus"].update(version="floating"),
        ):
            changed = copy.deepcopy(report)
            mutate(changed)
            runner.write_json(whole / "verus-production.json", changed)
            with self.subTest(mutate=mutate), self.assertRaises(ValueError):
                self.collect(inputs)
        runner.write_json(whole / "verus-production.json", report)
        inventory = whole / "production-proof-coverage-ci.json"
        inventory.write_text(inventory.read_text() + " ")
        with self.assertRaisesRegex(ValueError, "inventory differs"):
            self.collect(inputs)
        inventory.write_bytes((self.root / "production-proof-coverage.json").read_bytes())
        trace = whole / runner.TRACE / "verifier-report.json"
        changed = runner.read_json(trace)
        changed["ledger_sha256"] = "0" * 64
        runner.write_json(trace, changed)
        with self.assertRaisesRegex(ValueError, "traceability"):
            self.collect(inputs)

    def test_extra_artifacts_and_duplicate_json_keys_fail(self):
        inputs = self.collection_fixture()
        extra = inputs / (runner.artifact_prefix() + "duplicate")
        extra.mkdir()
        with self.assertRaises(ValueError):
            self.collect(inputs)
        extra.rmdir()
        summary = inputs / (runner.artifact_prefix() + "0") / runner.TRACE / runner.SUMMARY
        summary.write_text('{"status":"failed","status":"passed"}')
        with self.assertRaisesRegex(ValueError, "duplicate JSON key"):
            self.collect(inputs)

    def test_aggregate_cli_missing_inputs_exits_nonzero_with_failure_diagnostic(self):
        with patch.object(runner, "ROOT", self.root), patch.object(sys, "argv", [
            "runner", "--aggregate", str(self.root / "missing"), "--output-dir", str(self.output),
            "--whole-result", "success", "--semantic-result", "success",
        ]), redirect_stderr(io.StringIO()):
            self.assertEqual(runner.main(), 1)
        self.assertEqual(runner.read_json(self.output / "aggregation-failure.json")["status"], "failed")

    def test_unknown_selector_cli_is_nonzero(self):
        with patch.object(runner, "ROOT", self.root), patch.object(sys, "argv", [
            "runner", "--only", "not-configured", "--output-dir", str(self.output),
            "--verus-executable", str(self.root / "verus"),
        ]), redirect_stderr(io.StringIO()):
            self.assertEqual(runner.main(), 1)


if __name__ == "__main__":
    unittest.main()

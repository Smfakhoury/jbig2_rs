import copy
import json
from pathlib import Path
import sys
import tempfile
import threading
from types import SimpleNamespace
import unittest
from unittest.mock import patch


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))
import run_semantic_proofs as runner


def evidence():
    return {
        "status": "passed", "verus_version": "pinned", "production_sources_modified": False,
        "counterexamples": [
            {"normal_rust_compiles": True, "strong_contract": "rejected",
             "weaker_control": "accepted", "strong_verifier_errors": 1,
             "control_verifier_errors": 0},
        ],
    }


class SemanticOrchestrationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        (self.root / "tools").mkdir()
        (self.root / "toolchain.json").write_text(json.dumps({"verus": {"version": "pinned"}}))
        self.checks = tuple((str(index), f"{index}.py", 1, "strong_contract", "weaker_control") for index in range(4))
        for _, script, *_ in self.checks:
            (self.root / "tools" / script).write_text("# generated checker fixture\n")
        self.output = self.root / "reports"

    def run_fixture(self, process):
        with patch.object(runner, "CHECKS", self.checks), patch.object(runner.subprocess, "run", side_effect=process):
            return runner.run_checks(self.root, self.root / "verus", self.output)

    def passing_process(self, command, **kwargs):
        Path(command[-1]).write_text(json.dumps(evidence()))
        return SimpleNamespace(returncode=0)

    def test_manifest_keeps_all_eleven_checkers_and_forty_six_contrasts(self):
        self.assertEqual(len(runner.CHECKS), 11)
        self.assertEqual(len({row[0] for row in runner.CHECKS}), 11)
        self.assertEqual(sum(row[2] for row in runner.CHECKS), 46)
        self.assertTrue(all((TOOLS / row[1]).is_file() for row in runner.CHECKS))

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


if __name__ == "__main__":
    unittest.main()

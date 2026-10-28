import copy
import subprocess
import sys
import unittest
from unittest.mock import patch
from pathlib import Path


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from run_performance import compare_runs, environment, markdown_report, resolve_base, validate_measurement
from metrics import relative_spread


def measurement(median=11):
    return {
        "schema_version": 2, "iterations": 20,
        "cases": [{
            "id": "white-64", "input_bytes": 102, "output_bytes": 512,
            "outcome": "decoded", "exact_output_checked": True,
            "work_quota": quota(1024),
            "samples_ns": list(range(1, 21)), "median_ns": median, "p95_ns": 19,
        }],
    }


def quota(admitted):
    return {
        "admitted_limit": admitted, "rejected_limit": admitted - 1,
        "rejected_actual": admitted, "rejected_kind": "Work",
    }


def row(median, work=1024):
    return {"white-64": {
        "input_bytes": 102, "output_bytes": 512, "outcome": "decoded",
        "median_ns": median, "p95_ns": median + 10,
        "work_quota": quota(work),
    }}


class PerformanceReportTests(unittest.TestCase):
    def test_unavailable_push_baseline_falls_back_to_origin_main(self):
        with patch("run_performance.subprocess.run", side_effect=[
            subprocess.CompletedProcess([], 128, "", "unknown revision"),
            subprocess.CompletedProcess([], 0, "main-sha\n", ""),
        ]) as run:
            base, selection = resolve_base("unavailable-sha", Path("."))
        self.assertEqual(base, "main-sha")
        self.assertEqual(selection, "requested baseline unavailable; fell back to origin/main")
        self.assertEqual(run.call_count, 2)

    def test_caller_profile_and_compiler_overrides_are_removed(self):
        with patch.dict("os.environ", {
            "CARGO_PROFILE_RELEASE_OPT_LEVEL": "0", "RUSTFLAGS": "--cfg alternate",
            "RUSTC_WRAPPER": "/alternate/compiler",
        }):
            result = environment()
        for name in ("CARGO_PROFILE_RELEASE_OPT_LEVEL", "RUSTFLAGS", "RUSTC_WRAPPER"):
            self.assertNotIn(name, result)

    def test_raw_samples_define_upper_median_and_nearest_rank_p95(self):
        self.assertEqual(validate_measurement(measurement(), 20, ["white-64"])["white-64"]["p95_ns"], 19)
        invalid = measurement()
        invalid["cases"][0]["p95_ns"] = 18
        with self.assertRaises(ValueError):
            validate_measurement(invalid, 20, ["white-64"])

    def test_empty_duplicate_wrong_shape_and_missing_validation_fail_closed(self):
        for change in (
            lambda report: report.update(cases=[]),
            lambda report: report["cases"].append(copy.deepcopy(report["cases"][0])),
            lambda report: report["cases"][0].update(exact_output_checked=False),
            lambda report: report["cases"][0].update(output_bytes=0),
            lambda report: report["cases"][0].update(samples_ns=[-1] * 20),
        ):
            report = measurement()
            change(report)
            with self.assertRaises(ValueError):
                validate_measurement(report, 20, ["white-64"])

    def test_exact_wall_threshold_is_inclusive(self):
        base = [row(100)] * 3
        self.assertEqual(compare_runs(base, [row(120)] * 3, 20)[0]["timing_status"], "within_policy")
        self.assertEqual(compare_runs(base, [row(121)] * 3, 20)[0]["timing_status"], "regression_observed")

    def test_work_quota_requires_exact_typed_one_below_evidence(self):
        changes = (
            lambda record: record.pop("work_quota"),
            lambda record: record.update(work_quota=None),
            lambda record: record["work_quota"].update(admitted_limit=True),
            lambda record: record["work_quota"].update(rejected_limit=-1),
            lambda record: record["work_quota"].update(rejected_limit=1022),
            lambda record: record["work_quota"].update(rejected_actual=1023),
            lambda record: record["work_quota"].update(rejected_actual=1025),
            lambda record: record["work_quota"].update(rejected_kind="PagePixels"),
            lambda record: record["work_quota"].update(admitted_limit=1 << 64),
        )
        for change in changes:
            report = measurement()
            change(report["cases"][0])
            with self.subTest(change=change), self.assertRaises(ValueError):
                validate_measurement(report, 20, ["white-64"])

    def test_fixed_input_guard_is_explicit_not_a_measured_admission_limit(self):
        report = measurement()
        record = report["cases"][0]
        record.update(id="work-boundary", outcome="work-limit", output_bytes=0)
        record["work_quota"] = {
            "admitted_limit": None, "rejected_limit": 101, "rejected_actual": 102,
            "rejected_kind": "Work",
        }
        validate_measurement(report, 20, ["work-boundary"])
        changed = copy.deepcopy(report)
        changed["cases"][0]["work_quota"]["admitted_limit"] = 102
        with self.assertRaises(ValueError):
            validate_measurement(changed, 20, ["work-boundary"])

    def test_work_regression_is_not_hidden_by_faster_or_noisy_timings(self):
        base = [row(100)] * 3
        for timings in ((90, 90, 90), (50, 100, 200)):
            result = compare_runs(base, [row(value, work=1025) for value in timings], 20)[0]
            self.assertEqual(result["work_quota_status"], "regression_observed")
            self.assertEqual(result["base_minimum_work_limit"], 1024)
            self.assertEqual(result["head_minimum_work_limit"], 1025)
        self.assertEqual(
            compare_runs(base, [row(100, work=1023)] * 3, 20)[0]["work_quota_status"],
            "within_policy",
        )

    def test_repeated_work_boundaries_must_be_identical_on_each_side(self):
        with self.assertRaisesRegex(ValueError, "repeated"):
            compare_runs([row(100)] * 3, [row(100), row(100), row(100, work=1025)], 20)
        with self.assertRaisesRegex(ValueError, "repeated"):
            compare_runs([row(100), row(100, work=1025), row(100)], [row(100)] * 3, 20)

    def test_previous_raw_schema_cannot_omit_the_resource_measurement(self):
        report = measurement()
        report["schema_version"] = 1
        with self.assertRaises(ValueError):
            validate_measurement(report, 20, ["white-64"])

    def test_empty_or_duplicate_selections_and_empty_pairs_fail_closed(self):
        for selected in ([], ["white-64", "white-64"]):
            with self.assertRaises(ValueError):
                validate_measurement(measurement(), 20, selected)
        with self.assertRaises(ValueError):
            compare_runs([{}, {}, {}], [{}, {}, {}], 20)

    def test_summary_separates_a_failed_quota_gate_from_informational_timing(self):
        cases = compare_runs([row(100)] * 3, [row(90, work=1025)] * 3, 20)
        summary = markdown_report({
            "status": "failed", "correctness": "exact outputs passed",
            "resource_gate": "failed", "cases": cases,
        })
        self.assertIn("Work-quota gate: failed", summary)
        self.assertIn("1024 / 1025", summary)
        self.assertIn("not internal work counters", summary)
        self.assertIn("not a release-performance pass", summary)

    def test_unstable_results_are_inconclusive_not_a_latency_pass(self):
        result = compare_runs([row(100)] * 3, [row(100), row(150), row(200)], 20)
        self.assertEqual(result[0]["timing_status"], "inconclusive")

    def test_descriptor_changes_and_short_run_counts_are_errors(self):
        changed = row(100)
        changed["white-64"]["output_bytes"] = 511
        with self.assertRaises(ValueError):
            compare_runs([row(100)] * 3, [changed] * 3, 20)
        with self.assertRaises(ValueError):
            compare_runs([row(100)] * 2, [row(100)] * 2, 20)

    def test_shared_spread_rejects_invalid_measurements(self):
        self.assertEqual(relative_spread([100, 100, 100]), 0)
        for values in ([], [0], [-1], [float("nan")]):
            with self.assertRaises(ValueError):
                relative_spread(values)

    def test_invalid_thresholds_cannot_make_regressions_look_accepted(self):
        for threshold in (-1, float("nan"), float("inf")):
            with self.assertRaises(ValueError):
                compare_runs([row(100)] * 3, [row(100)] * 3, threshold)

    def test_summary_does_not_present_unchecked_or_informational_timing_as_a_pass(self):
        summary = markdown_report({"status": "failed", "error": "missing samples"})
        self.assertIn("Correctness: not established", summary)
        self.assertIn("not a release-performance pass", summary)
        self.assertIn("missing samples", summary)

    def test_hosted_workflow_keeps_timing_informational_and_uploads_failures(self):
        workflow = (TOOLS.parent / ".github/workflows/performance.yml").read_text()
        self.assertIn("contents: read", workflow)
        self.assertIn("fetch-depth: 0", workflow)
        self.assertIn("timeout-minutes: 20", workflow)
        self.assertNotIn("--enforce-timing", workflow)
        self.assertIn("if: always()", workflow)
        self.assertIn("GITHUB_STEP_SUMMARY", workflow)
        self.assertIn("tools/check_performance_controls.py", workflow)
        self.assertIn("python -m pip install -r tools/requirements.txt", workflow)
        self.assertNotIn("            target/performance-controls/\n", workflow)


if __name__ == "__main__":
    unittest.main()

import copy
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
from pathlib import Path
from types import SimpleNamespace


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from run_performance import (
    RSS_SCOPE, RSS_SOURCE, compare_runs, comparison, environment, markdown_report,
    resolve_base, summarize_rss, validate_measurement, validate_rss,
)
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

def rss_measurement(peak=2048):
    return {
        "schema_version": 1, "id": "white-64", "input_bytes": 102, "output_bytes": 512,
        "outcome": "decoded", "exact_output_checked": True, "rss_source": RSS_SOURCE,
        "rss_unit": "KiB", "peak_rss_kib": peak, "scope": RSS_SCOPE,
    }


def rss_runs():
    return {"base": {"white-64": [1024, 2048, 1024]}, "head": {"white-64": [3072, 2048, 3072]}}


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

    def test_rss_requires_exact_single_case_output_and_linux_peak_scope(self):
        expected = measurement()["cases"][0]
        self.assertEqual(validate_rss(rss_measurement(), expected), 2048)
        for field, value in (
            ("schema_version", True), ("id", "white-512"), ("input_bytes", 103),
            ("output_bytes", 511), ("outcome", "eof"), ("exact_output_checked", False),
            ("rss_source", "launcher_wait4"), ("rss_unit", "bytes"),
            ("scope", "all_cases_and_quota_probes"), ("input_bytes", True),
            ("peak_rss_kib", 0), ("peak_rss_kib", -1), ("peak_rss_kib", True),
            ("peak_rss_kib", 2.5), ("peak_rss_kib", "2048"), ("peak_rss_kib", 1 << 64),
        ):
            changed = rss_measurement()
            changed[field] = value
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                validate_rss(changed, expected)

    def test_missing_rss_and_mixed_timing_or_quota_payloads_fail_closed(self):
        expected = measurement()["cases"][0]
        for invalid in (None, {}, [], {"peak_rss_kib": 2048}):
            with self.assertRaises(ValueError):
                validate_rss(invalid, expected)
        for extra in ("work_quota", "samples_ns", "cases"):
            changed = rss_measurement()
            changed[extra] = []
            with self.assertRaises(ValueError):
                validate_rss(changed, expected)

    def test_rss_summary_retains_all_raw_samples_without_a_memory_gate(self):
        report = summarize_rss(rss_runs(), ["white-64"], 3)
        self.assertEqual(report["status"], "observed")
        self.assertEqual(report["policy"], "informational_only")
        self.assertEqual(report["cases"][0]["base_median_kib"], 1024)
        self.assertEqual(report["cases"][0]["head_median_kib"], 3072)
        self.assertEqual(report["cases"][0]["median_ratio"], 3)
        self.assertEqual(report["cases"][0]["head_samples_kib"], [3072, 2048, 3072])
        self.assertIn("inherited launcher", report["excludes"])
        self.assertIn("allocator", report["not_claimed"])

    def test_rss_summary_rejects_missing_extra_and_invalid_process_samples(self):
        for mutate in (
            lambda runs: runs.pop("head"),
            lambda runs: runs["head"].pop("white-64"),
            lambda runs: runs["head"].update(extra=[1, 2, 3]),
            lambda runs: runs["head"].update({"white-64": [1024, 1024]}),
            lambda runs: runs["head"].update({"white-64": [1024, 0, 1024]}),
            lambda runs: runs["head"].update({"white-64": [1024, True, 1024]}),
            lambda runs: runs["head"].update({"white-64": [1024, float("nan"), 1024]}),
            lambda runs: runs["head"].update({"white-64": [1024, 1 << 64, 1024]}),
        ):
            invalid = rss_runs()
            mutate(invalid)
            with self.assertRaises(ValueError):
                summarize_rss(invalid, ["white-64"], 3)
        for selected, pairs in (([], 3), (["white-64", "white-64"], 3), (["white-64"], 2)):
            with self.assertRaises(ValueError):
                summarize_rss(rss_runs(), selected, pairs)

    def test_requested_rss_fails_explicitly_on_an_unsupported_host(self):
        with tempfile.TemporaryDirectory() as temporary, patch("run_performance.sys.platform", "win32"):
            result = comparison(SimpleNamespace(
                output_dir=Path(temporary) / "output", measure_rss=True,
            ))
        self.assertEqual(result["status"], "failed")
        self.assertEqual(result["rss"]["status"], "requested_not_established")
        self.assertIn("requires Linux", result["error"])

    def test_rss_markdown_cannot_be_confused_with_allocator_or_release_approval(self):
        report = {"status": "passed", "rss": summarize_rss(rss_runs(), ["white-64"], 3)}
        summary = markdown_report(report)
        self.assertIn("Process RSS: observed", summary)
        self.assertIn("not an allocator or release-memory gate", summary)
        self.assertIn("| white-64 | 1024 | 3072 | 3.000 |", summary)
        self.assertIn("Process RSS: not_requested", markdown_report({
            "status": "passed", "rss": {"status": "not_requested"},
        }))

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
        self.assertGreaterEqual(workflow.count("--measure-rss"), 2)
        self.assertIn("cargo test --locked --example performance_ci", workflow)
        self.assertNotIn("            target/performance-controls/\n", workflow)


if __name__ == "__main__":
    unittest.main()

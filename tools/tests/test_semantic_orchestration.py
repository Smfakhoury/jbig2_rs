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


DOCUMENT_PARTITION_GROUPS = (
    "iaid-state", "arithmetic-initialization", "text-batch", "black-span",
    "segment-data", "value-bridge", "mmr-span", "reader-error", "arithmetic-refill",
    "mmr-reference", "bitmap-copy", "retention", "bitmap-chunk", "symbol-cardinality",
    "segment-header", "document-count", "mmr-run", "arithmetic-codec",
    "arithmetic-decision", "halftone-header", "reset", "arithmetic-normalization",
    "arithmetic-integer", "mmr-horizontal", "integer-state", "operator-bit",
    "document-layout", "region-dispatch", "bitmap-compose", "mmr-prefix",
)


def document_partition_checks():
    checks = {row[0]: row for row in runner.configured_checks()}
    return tuple(checks[name] for name in DOCUMENT_PARTITION_GROUPS)


def document_partition_shards():
    with patch.object(runner, "CHECKS", document_partition_checks()):
        return runner.configured_shards()


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
        (self.root / "config").mkdir()
        (self.root / "src/example.rs").write_text("// source fixture\n")
        (self.root / "config/toolchain.json").write_text(json.dumps({"verus": {"version": "pinned"}}))
        self.checks = tuple((str(index), f"{index}.py", 1, "strong_contract", "weaker_control") for index in range(8))
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

    def test_manifest_binds_verification_cargo_inputs_without_build_outputs(self):
        verification = self.root / "verification"
        verification.mkdir()
        for name in ("Cargo.toml", "Cargo.lock"):
            (verification / name).write_text("pinned verification fixture\n")
        (verification / "target").mkdir()
        (verification / "target/Cargo.toml").write_text("not an input\n")
        with patch.object(runner, "CHECKS", self.checks):
            bindings = runner.manifest(self.root)["source_bindings"]
        self.assertIn("verification/Cargo.toml", bindings)
        self.assertIn("verification/Cargo.lock", bindings)
        self.assertNotIn("verification/target/Cargo.toml", bindings)

    def test_manifest_binds_relocated_metadata_and_opaque_agent_profiles_not_generated_json(self):
        (self.root / "verification/target").mkdir(parents=True)
        (self.root / "verification/proof-dispositions.json").write_text("{}")
        (self.root / "verification/target/generated.json").write_text("{}")
        profiles = {".github/agents/worker.agent.md": "1" * 40}
        with patch.object(runner, "CHECKS", self.checks), patch.object(
            runner, "agent_profile_blobs", return_value=profiles,
        ):
            manifest = runner.manifest(self.root)
            bindings = manifest["source_bindings"]
        self.assertIn("config/toolchain.json", bindings)
        self.assertIn("verification/proof-dispositions.json", bindings)
        self.assertEqual(manifest["agent_profile_blobs"], profiles)
        self.assertNotIn("verification/target/generated.json", bindings)

    def test_agent_profile_identity_uses_git_metadata_without_reading_instructions(self):
        (self.root / ".git").mkdir()
        outputs = [
            SimpleNamespace(stdout=""),
            SimpleNamespace(stdout="100644 blob " + "1" * 40 + "\t.github/agents/worker.agent.md\n"),
        ]
        with patch.object(runner.subprocess, "run", side_effect=outputs):
            self.assertEqual(runner.agent_profile_blobs(self.root), {
                ".github/agents/worker.agent.md": "1" * 40,
            })
        with patch.object(runner.subprocess, "run", return_value=SimpleNamespace(stdout=" M profile")):
            with self.assertRaisesRegex(ValueError, "committed opaque"):
                runner.agent_profile_blobs(self.root)

    def test_shared_document_baseline_requires_all_configured_function_identities(self):
        cases = [{"id": name, "function": name} for name in ("first", "second")]
        (self.root / "tools/document.py").write_text("CASES = " + repr(cases) + "\n")
        report = evidence()
        report.update(
            schema_version=1,
            baseline_verification_scope="document::*",
            baseline_selected_functions=["first", "second"],
            source_bindings={"src/example.rs": runner.digest(self.root / "src/example.rs")},
        )
        report["counterexamples"] = []
        for case in cases:
            record = copy.deepcopy(evidence()["counterexamples"][0])
            record.update(id=case["id"], function=case["function"], strong_count_contract="rejected")
            record["baseline_selected_verification"]["verified"] = 2
            report["counterexamples"].append(record)
        check = ("document-count", "document.py", 2, "strong_count_contract", "weaker_control")
        runner.check_bound_report(self.root, check, report, report["source_bindings"])
        for mutation in ("missing-scope", "missing-function", "duplicate-function", "too-few-proofs"):
            broken = copy.deepcopy(report)
            if mutation == "missing-scope":
                del broken["baseline_verification_scope"]
            elif mutation == "missing-function":
                broken["baseline_selected_functions"].pop()
            elif mutation == "duplicate-function":
                broken["baseline_selected_functions"] = ["first", "first"]
            else:
                broken["counterexamples"][0]["baseline_selected_verification"]["verified"] = 1
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                runner.check_bound_report(self.root, check, broken, report["source_bindings"])

    def test_document_partition_rejects_missing_wrong_or_cross_group_evidence(self):
        cases = [{"id": name, "function": name} for name in ("first", "second")]
        (self.root / "tools/document.py").write_text(
            "CASES = " + repr(cases) + "\nCASE_GROUPS = "
            + repr({"document-count": cases[:1], "document-layout": cases[1:]}) + "\n"
        )
        report = evidence()
        report.update(
            schema_version=1, case_group="document-count",
            baseline_verification_scope="document::*", baseline_selected_functions=["first"],
            source_bindings={"src/example.rs": runner.digest(self.root / "src/example.rs")},
        )
        report["counterexamples"][0].update(
            id="first", function="first", strong_count_contract="rejected",
        )
        check = ("document-count", "document.py", 1, "strong_count_contract", "weaker_control")
        runner.check_bound_report(self.root, check, report, report["source_bindings"])
        for mutation in ("missing-group", "wrong-group", "cross-group", "duplicate"):
            broken = copy.deepcopy(report)
            if mutation == "missing-group":
                del broken["case_group"]
            elif mutation == "wrong-group":
                broken["case_group"] = "document-layout"
            elif mutation == "cross-group":
                broken["counterexamples"][0].update(id="second", function="second")
            else:
                broken["counterexamples"].append(copy.deepcopy(broken["counterexamples"][0]))
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                runner.check_bound_report(self.root, check, broken, report["source_bindings"])

    def test_document_partition_is_explicitly_forwarded_to_the_shared_checker(self):
        script = "check_document_count_proof.py"
        (self.root / "tools" / script).write_text(
            "CASES = ({'id': 'case', 'function': 'function'},)\n"
            "CASE_GROUPS = {'document-count': CASES}\n"
        )
        check = ("document-count", script, 1, "strong_count_contract", "weaker_control")

        def process(command, **_keywords):
            self.assertEqual(command[command.index("--case-group") + 1], "document-count")
            report = evidence()
            report.update(
                schema_version=1, case_group="document-count",
                baseline_verification_scope="document::*", baseline_selected_functions=["function"],
                source_bindings={"src/example.rs": runner.digest(self.root / "src/example.rs")},
            )
            report["counterexamples"][0].update(function="function", strong_count_contract="rejected")
            Path(command[-1]).write_text(json.dumps(report))
            return SimpleNamespace(returncode=0)

        with patch.object(runner, "CHECKS", (check,)), patch.object(
            runner.subprocess, "run", side_effect=process,
        ):
            result = runner.run_checks(self.root, self.root / "verus", self.output, only="document-count")
        self.assertEqual(result["status"], "passed")
        self.assertEqual(result["checks"][0]["counterexamples"], 1)

    def test_shared_group_baselines_cannot_omit_or_invent_target_coverage(self):
        cases = [{"id": name, "module": "module", "function": name} for name in ("first", "second")]
        groups = (("module", ("first", "second")),)
        (self.root / "tools/shared.py").write_text(
            "CASES = " + repr(cases) + "\nBASELINE_GROUPS = " + repr(groups) + "\n"
        )
        report = evidence()
        report.update(
            baseline_groups=[{"module": "module", "functions": ["first", "second"]}],
            source_bindings={"src/example.rs": runner.digest(self.root / "src/example.rs")},
        )
        report["counterexamples"] = []
        for case in cases:
            row = copy.deepcopy(evidence()["counterexamples"][0])
            row.update(id=case["id"], function="module::" + case["function"],
                       baseline_selected_functions=["first", "second"])
            row["baseline_selected_verification"]["verified"] = 2
            report["counterexamples"].append(row)
        check = ("shared", "shared.py", 2, "strong_contract", "weaker_control")
        runner.check_bound_report(self.root, check, report, report["source_bindings"])
        for mutation in ("missing-group", "missing-function", "extra-function", "too-few-proofs"):
            broken = copy.deepcopy(report)
            if mutation == "missing-group":
                del broken["baseline_groups"]
            elif mutation == "missing-function":
                broken["counterexamples"][0]["baseline_selected_functions"].pop()
            elif mutation == "extra-function":
                broken["baseline_groups"][0]["functions"].append("unknown")
            else:
                broken["counterexamples"][0]["baseline_selected_verification"]["verified"] = 1
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                runner.check_bound_report(self.root, check, broken, report["source_bindings"])

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
        self.assertEqual([row["id"] for row in result["checks"]], [row[0] for row in self.checks])

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

    def test_measured_complete_profile_leaves_timeout_headroom(self):
        # Accepted current baseline 40222911541, rounded up.
        # The new metadata group uses its complete 269-second local run plus 10%.
        # Document partitions project the same 25-case family, charging 20 extra
        # seconds for a duplicated baseline measured at 15.45 seconds.
        # Scheduling estimates are not hosted acceptance.
        measured = {
            "reset": (2, 62), "document-count": (25, 511), "document-layout": (25, 511),
            "bitmap-copy": (8, 259), "retention": (3, 86),
            "value-bridge": (5, 104), "mmr-span": (8, 183),
            "region-dispatch": (3, 66), "text-batch": (11, 388),
            "bitmap-chunk": (4, 123), "bitmap-compose": (7, 192),
            "black-span": (6, 137), "arithmetic-decision": (15, 370),
            "arithmetic-refill": (6, 112), "arithmetic-initialization": (6, 150),
            "arithmetic-normalization": (6, 160), "operator-bit": (4, 120),
            "arithmetic-integer": (9, 157), "iaid-state": (6, 155),
            "integer-state": (9, 165), "arithmetic-codec": (19, 531),
            "halftone-header": (9, 189), "reader-error": (9, 270),
            "mmr-prefix": (4, 96), "mmr-run": (4, 93),
            "mmr-horizontal": (6, 180), "symbol-cardinality": (11, 271),
            "mmr-reference": (11, 371), "segment-header": (10, 171),
            "segment-data": (10, 296),
        }
        checks = document_partition_checks()
        self.assertEqual({check[0] for check in checks}, set(measured))
        estimates = []
        for shard in document_partition_shards():
            seconds = 0
            for identity, _, cases, *_ in shard:
                observed_cases, observed_seconds = measured[identity]
                seconds += (cases * observed_seconds + observed_cases - 1) // observed_cases
                if identity == "document-layout":
                    seconds += 20
            estimates.append(seconds)
        self.assertLessEqual(max(estimates), 1000)

    def test_completed_profile_has_measured_job_headroom_without_relaxed_limits(self):
        # Completed 40303484174 attempt-2 groups, with conservative ZIP-time
        # bounds for its three leading shard-5 groups; canceled jobs get no
        # acceptance credit. Document partitions use fresh complete local runs.
        observed = {
            "arithmetic-codec": 540, "arithmetic-decision": 308,
            "arithmetic-initialization": 160, "arithmetic-integer": 227,
            "arithmetic-normalization": 153, "arithmetic-refill": 155,
            "bitmap-chunk": 128, "bitmap-compose": 179, "bitmap-copy": 256,
            "black-span": 169, "document-count": 332, "document-layout": 327,
            "halftone-header": 225, "iaid-state": 163, "integer-state": 265,
            "mmr-horizontal": 155, "mmr-prefix": 124, "mmr-reference": 300,
            "mmr-run": 118, "mmr-span": 177, "operator-bit": 134,
            "reader-error": 169, "region-dispatch": 89, "reset": 46,
            "retention": 91, "segment-header": 207, "symbol-cardinality": 322,
            "text-batch": 312, "value-bridge": 120, "segment-data": 266,
        }
        self.assertEqual({row[0] for row in document_partition_checks()}, set(observed))
        self.assertEqual(sum(row[2] for row in document_partition_checks()), 236)
        self.assertLessEqual(max(
            sum(observed[row[0]] for row in shard)
            for shard in document_partition_shards()
        ), 1050)

    def test_document_partitions_use_different_shards_without_more_jobs_or_cases(self):
        shards = document_partition_shards()
        self.assertEqual(len(shards), 6)
        self.assertEqual(sum(row[2] for row in document_partition_checks()), 236)
        groups = [{row[0] for row in shard} for shard in shards]
        self.assertNotEqual(
            next(i for i, names in enumerate(groups) if "document-count" in names),
            next(i for i, names in enumerate(groups) if "document-layout" in names),
        )
        self.assertLessEqual(max(sum(row[2] for row in shard) for shard in shards), 42)

    def test_complete_measurement_guard_uses_all_current_groups_without_a_global_case_limit(self):
        checks = self.checks + (("future-decision", "future.py", 100, "strong", "weak"),)
        rows = [{"id": check[0], "status": "passed", "seconds": 1.0} for check in checks]
        with patch.object(runner, "CHECKS", checks):
            self.assertEqual(runner.measured_shard_seconds(rows), [2, 2, 2, 1, 1, 1])
            for value in (None, True, 0, -1, float("nan"), float("inf"), 1050):
                broken = copy.deepcopy(rows)
                broken[0]["seconds"] = value
                with self.subTest(seconds=value), self.assertRaises(ValueError):
                    runner.measured_shard_seconds(broken)
            for broken in (rows[:-1], rows + [rows[0]], [dict(row, status="failed") for row in rows]):
                with self.assertRaises(ValueError):
                    runner.measured_shard_seconds(broken)

    def test_matrix_is_generated_from_current_complete_checks(self):
        future_checks = self.checks + (("future-decision", "future.py", 4, "strong_contract", "weaker_control"),)
        with patch.object(runner, "CHECKS", future_checks), patch.object(
            sys, "argv", ["runner", "--matrix"],
        ), redirect_stdout(io.StringIO()) as stream:
            self.assertEqual(runner.main(), 0)
        self.assertEqual(json.loads(stream.getvalue()), {"shard": [0, 1, 2, 3, 4, 5]})
        with patch.object(runner, "CHECKS", future_checks):
            assigned = [check[0] for shard in runner.configured_shards() for check in shard]
        self.assertEqual(set(assigned), {row[0] for row in future_checks})
        self.assertEqual(len(assigned), len(future_checks))

    def test_shard_runs_every_assigned_group_with_one_worker(self):
        with patch.object(runner, "CHECKS", self.checks), patch.object(
            runner.subprocess, "run", side_effect=self.passing_process,
        ) as process:
            result = runner.run_checks(self.root, self.root / "verus", self.output, shard=0)
        self.assertEqual(result["selected_groups"], ["0", "6"])
        self.assertEqual(result["shard"], 0)
        self.assertEqual(result["workers"], 1)
        self.assertEqual(process.call_count, 2)
        self.assertEqual(result["manifest"]["shards"], [["0", "6"], ["1", "7"], ["2"], ["3"], ["4"], ["5"]])

    def test_invalid_or_mixed_shard_selectors_fail_without_stale_success(self):
        self.output.mkdir()
        for shard in (-1, 6, True):
            runner.write_json(self.output / runner.SUMMARY, {"status": "passed"})
            with patch.object(runner, "CHECKS", self.checks), self.subTest(shard=shard), self.assertRaises(ValueError):
                runner.run_checks(self.root, self.root / "verus", self.output, shard=shard)
            self.assertFalse((self.output / runner.SUMMARY).exists())
        with patch.object(runner, "CHECKS", self.checks), self.assertRaises(ValueError):
            runner.run_checks(self.root, self.root / "verus", self.output, only="0", shard=0)

    def test_smaller_configurations_do_not_create_empty_shards(self):
        with patch.object(runner, "CHECKS", self.checks[:2]):
            self.assertEqual(runner.configured_shards(), ((self.checks[0],), (self.checks[1],)))

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
        runner.write_json(self.root / "verification/production-proof-coverage.json", coverage)
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
            for index, _ in enumerate(runner.configured_shards()):
                runner.run_checks(
                    self.root, self.root / "verus",
                    inputs / (runner.artifact_prefix() + f"shard-{index}") / runner.TRACE, shard=index,
                )
        return inputs

    def test_whole_positive_receipt_is_source_tool_run_and_artifact_bound(self):
        whole = self.root / "whole"
        self.whole_fixture(whole)
        with patch.object(runner, "CHECKS", self.checks):
            runner.seal_whole(self.root, whole)
            baseline = runner.load_whole_baseline(self.root, whole)
            selected = runner.baseline_from_whole(baseline, ["module::function"])
            self.assertTrue(runner.baseline_passed(selected, baseline, ["module::function"]))
            self.assertFalse(runner.baseline_passed(selected))
            self.assertFalse(runner.baseline_passed(selected, baseline, ["other::function"]))
            for names in ([], ["unknown"], ["module::function"] * 2):
                with self.subTest(names=names), self.assertRaises(ValueError):
                    runner.baseline_from_whole(baseline, names)
            for field in ("receipt_sha256", "report_sha256", "functions"):
                changed = copy.deepcopy(selected)
                changed["whole_positive"][field] = [] if field == "functions" else "0" * 64
                self.assertFalse(runner.baseline_passed(changed, baseline, ["module::function"]))
            summary = whole / runner.WHOLE_SUMMARY
            original = summary.read_bytes()
            for field, value in (
                ("status", "failed"), ("schema_version", 0), ("manifest", {}),
                ("run_identity", {"GITHUB_RUN_ATTEMPT": "other"}), ("files", {}),
            ):
                document = json.loads(original)
                document[field] = value
                runner.write_json(summary, document)
                with self.subTest(field=field), self.assertRaises(ValueError):
                    runner.load_whole_baseline(self.root, whole)
                summary.write_bytes(original)
            library = whole / "target/verus-production/libfixture.rlib"
            library.write_bytes(b"changed library")
            with self.assertRaisesRegex(ValueError, "modified"):
                runner.load_whole_baseline(self.root, whole)
            library.write_bytes(b"native compilation fixture")
            (self.root / "src/example.rs").write_text("changed source")
            with self.assertRaisesRegex(ValueError, "source/tool identity"):
                runner.load_whole_baseline(self.root, whole)

    def test_invalid_explicit_whole_baseline_never_falls_back_or_runs_a_checker(self):
        with patch.object(runner, "CHECKS", self.checks), patch.object(
            runner.subprocess, "run",
        ) as process, self.assertRaises(FileNotFoundError):
            runner.run_checks(
                self.root, self.root / "verus", self.output,
                whole_baseline=self.root / "missing",
            )
        process.assert_not_called()
        self.assertFalse((self.output / runner.SUMMARY).exists())

    def test_runner_passes_only_explicit_whole_baseline_to_isolated_checkers(self):
        whole = self.root / "whole"
        self.whole_fixture(whole)
        with patch.object(runner, "CHECKS", self.checks):
            runner.seal_whole(self.root, whole)
            for supplied in (None, whole):
                with patch.dict(os.environ, {"JBIG2_WHOLE_BASELINE": "/stale"}), patch.object(
                    runner.subprocess, "run", side_effect=self.passing_process,
                ) as process:
                    result = runner.run_checks(
                        self.root, self.root / "verus", self.output,
                        only="0", whole_baseline=supplied,
                    )
                self.assertEqual(result["status"], "passed")
                environment = process.call_args.kwargs["env"]
                if supplied is None:
                    self.assertNotIn("JBIG2_WHOLE_BASELINE", environment)
                else:
                    self.assertEqual(environment["JBIG2_WHOLE_BASELINE"], str(whole.resolve()))

    def collect(self, inputs, whole="success", semantic="success"):
        with patch.object(runner, "CHECKS", self.checks):
            return runner.aggregate(self.root, inputs, self.output, whole, semantic)

    def test_full_aggregate_preserves_canonical_report_layout(self):
        inputs = self.collection_fixture()
        result = self.collect(inputs)
        self.assertEqual(result["status"], "passed")
        self.assertEqual(result["selected_groups"], [row[0] for row in self.checks])
        self.assertEqual(len(result["checks"]), len(self.checks))
        self.assertEqual(result["matrix_shards"], 6)
        self.assertEqual(result["matrix_max_parallel"], 4)
        self.assertTrue((self.output / "verus-production.json").is_file())
        self.assertTrue((self.output / "production-proof-coverage-ci.json").is_file())
        self.assertTrue((self.output / runner.TRACE / runner.SUMMARY).is_file())
        for identity, *_ in self.checks:
            self.assertTrue((self.output / runner.TRACE / f"{identity}-proof-sensitivity.json").is_file())

    def test_missing_whole_or_each_shard_fails_and_removes_stale_success(self):
        inputs = self.collection_fixture()
        for identity in ("whole", *(f"shard-{index}" for index in range(6))):
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
        summary_path = inputs / (runner.artifact_prefix() + "shard-0") / runner.TRACE / runner.SUMMARY
        original = runner.read_json(summary_path)
        mutations = [
            lambda r: r.update(status="failed"),
            lambda r: r.update(schema_version=1),
            lambda r: r.update(schema_version=2),
            lambda r: r.update(shard=1),
            lambda r: r.update(shard=False),
            lambda r: r.update(workers=True),
            lambda r: r.update(workers=2),
            lambda r: r["checks"][0].update(status="failed"),
            lambda r: r["checks"][0].update(exit_code=1),
            lambda r: r["checks"][0].update(exit_code=False),
            lambda r: r["checks"][0].update(checker_sha256="0" * 64),
            lambda r: r["checks"][0].update(counterexamples=2),
            lambda r: r["checks"][0].update(counterexamples=True),
            lambda r: r["checks"][0].update(report_sha256="0" * 64),
            lambda r: r["checks"].extend(r["checks"]),
            lambda r: r["checks"].pop(),
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
        directory = inputs / (runner.artifact_prefix() + "shard-0") / runner.TRACE
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
        inventory.write_bytes((self.root / "verification/production-proof-coverage.json").read_bytes())
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
        summary = inputs / (runner.artifact_prefix() + "shard-0") / runner.TRACE / runner.SUMMARY
        summary.write_text('{"status":"failed","status":"passed"}')
        with self.assertRaisesRegex(ValueError, "duplicate JSON key"):
            self.collect(inputs)

    def test_missing_or_unexpected_group_files_inside_shard_are_rejected(self):
        inputs = self.collection_fixture()
        shard = inputs / (runner.artifact_prefix() + "shard-0")
        report = shard / runner.TRACE / "0-proof-sensitivity.json"
        original = report.read_bytes()
        report.unlink()
        with self.assertRaises((ValueError, OSError)):
            self.collect(inputs)
        report.write_bytes(original)
        extra = shard / "unbound.json"
        extra.write_text("{}")
        with self.assertRaisesRegex(ValueError, "outside"):
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

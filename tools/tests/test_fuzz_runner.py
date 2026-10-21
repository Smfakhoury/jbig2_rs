import io
import json
import os
import re
import sys
import tempfile
import unittest
from contextlib import redirect_stderr
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from run_fuzz import (
    INPUT_TIMEOUT, MAX_INPUT_BYTES, PROFILES, ROOT, RSS_LIMIT_MB, TARGETS,
    execute, fuzz_command, fuzz_environment, inventory, run, source_identity, validate_targets,
)


@unittest.skipUnless(sys.platform == "linux", "bounded process-group fuzz runner is Linux-only")
class FuzzRunnerTests(unittest.TestCase):
    def test_all_declared_targets_are_covered_and_selectors_fail_closed(self):
        validate_targets(ROOT, list(TARGETS))
        self.assertEqual(len(TARGETS), 9)
        self.assertIn("document_context_structured", TARGETS)
        self.assertIn("codec_families_structured", TARGETS)
        for targets in ([], ["unknown"], [TARGETS[0], TARGETS[0]]):
            with self.subTest(targets=targets), self.assertRaises(ValueError):
                validate_targets(ROOT, targets)

    def test_campaign_and_replay_have_exact_resource_bounds(self):
        command = fuzz_command(["cargo", "+pinned"], TARGETS[0], Path("/input"), Path("/output"), 60)
        for value in (
            f"-timeout={INPUT_TIMEOUT}", f"-rss_limit_mb={RSS_LIMIT_MB}",
            f"-max_len={MAX_INPUT_BYTES}", "-max_total_time=60", "-artifact_prefix=/output/",
        ):
            self.assertIn(value, command)
        replay = fuzz_command(["cargo", "+pinned"], TARGETS[0], Path("/unit"), Path("/output"), 60, True)
        self.assertIn("-runs=1", replay)
        self.assertFalse(any(value.startswith("-max_total_time=") for value in replay))
        self.assertEqual(PROFILES, {"smoke": 60, "extended": 300})

    def test_user_compiler_and_sanitizer_overrides_are_not_inherited(self):
        with patch.dict(os.environ, {
            "RUSTFLAGS": "--cfg alternate", "RUSTC_BOOTSTRAP": "1",
            "ASAN_OPTIONS": "exitcode=0", "CARGO_TARGET_TEST_RUSTFLAGS": "alternate",
        }):
            environment = fuzz_environment()
        for name in ("RUSTFLAGS", "RUSTC_BOOTSTRAP", "ASAN_OPTIONS", "CARGO_TARGET_TEST_RUSTFLAGS"):
            self.assertNotIn(name, environment)

    def test_corpus_inventory_checks_sizes_and_regular_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            (directory / "seed").write_bytes(b"abc")
            self.assertEqual(inventory(directory, 3, 1)[0]["bytes"], 3)
            with self.assertRaises(ValueError):
                inventory(directory, 2, 1)
            with self.assertRaises(ValueError):
                inventory(directory, 3, 0)
            (directory / "link").symlink_to(directory / "seed")
            with self.assertRaises(ValueError):
                inventory(directory, 3, 2)

    def test_exit_failure_and_timeout_keep_logs(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            failure = execute(
                [sys.executable, "-c", "import sys; print('controlled failure'); sys.exit(7)"],
                directory, fuzz_environment(), directory / "failure.log", 5,
            )
            self.assertEqual(failure["exit_code"], 7)
            self.assertIn("controlled failure", (directory / "failure.log").read_text())
            timeout = execute(
                [sys.executable, "-c", "import time; time.sleep(10)"],
                directory, fuzz_environment(), directory / "timeout.log", 1,
            )
            self.assertTrue(timeout["timed_out"])
            self.assertNotEqual(timeout["exit_code"], 0)

    def test_source_identity_binds_harnesses_and_shared_generated_fixture(self):
        first = source_identity(ROOT)
        self.assertEqual(len(first), 64)
        self.assertEqual(first, source_identity(ROOT))

    def test_structured_helpers_explicitly_bound_every_public_limit(self):
        fields = set(re.findall(r"pub (max_[a-z_]+):", (ROOT / "src/limits.rs").read_text()))
        self.assertEqual(len(fields), 15)
        for name in ("context_scenarios.rs", "codec_scenarios.rs"):
            source = (ROOT / "tests/support" / name).read_text()
            body = source.split("fn limits() -> Limits {", 1)[1].split("\n}", 1)[0]
            assignments = re.findall(r"limits\.(max_[a-z_]+)\s*=\s*([^;\n]+);", body)
            with self.subTest(fixture=name):
                self.assertEqual(len(assignments), len(fields))
                self.assertEqual({field for field, _ in assignments}, fields)
                for _, value in assignments:
                    self.assertRegex(value, r"^[1-9][0-9_]*(?:\s*<<\s*[0-9]+)?$")

    def test_source_identity_changes_with_target_and_fixture_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            names = (
                "src/lib.rs", "fuzz/fuzz_targets/example.rs", "Cargo.toml", "Cargo.lock",
                "fuzz/Cargo.toml", "fuzz/Cargo.lock", "validation/runtime_access.rs",
                "examples/fuzz_seeds.rs", "tools/distribution_consumer.rs",
                "tools/run_fuzz.py", "toolchain.json",
                "tests/support/context_scenarios.rs", "benchmarks/generated.rs",
                "tests/support/codec_scenarios.rs",
            )
            for name in names:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(name, encoding="utf-8")
            original = source_identity(root)
            target = root / "fuzz/fuzz_targets/example.rs"
            target.write_text("changed target", encoding="utf-8")
            changed = source_identity(root)
            self.assertNotEqual(original, changed)
            (root / "tools/distribution_consumer.rs").write_text("changed fixture", encoding="utf-8")
            self.assertNotEqual(changed, source_identity(root))
            for name in (
                "tests/support/context_scenarios.rs", "benchmarks/generated.rs",
                "tests/support/codec_scenarios.rs",
            ):
                before = source_identity(root)
                (root / name).write_text("changed structured fixture", encoding="utf-8")
                self.assertNotEqual(before, source_identity(root))

    def test_existing_output_is_rejected_without_overwriting_report(self):
        from run_fuzz import run
        from types import SimpleNamespace
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            report = output / "report.json"
            report.write_text("preserved", encoding="utf-8")
            with self.assertRaises(FileExistsError):
                run(SimpleNamespace(output_dir=output))
            self.assertEqual(report.read_text(), "preserved")

    def test_workflow_is_bounded_and_trusted_corpus_is_not_available_to_prs(self):
        workflow = (ROOT / ".github/workflows/fuzz.yml").read_text()
        self.assertIn("contents: read", workflow)
        self.assertIn("timeout-minutes: 60", workflow)
        self.assertIn('cron: "30 2 * * *"', workflow)
        self.assertNotIn("pull_request_target:", workflow)
        self.assertEqual(workflow.count("env.TRUSTED_CORPUS == 'true'"), 2)
        self.assertIn("github.event_name == 'schedule' || github.event_name == 'workflow_dispatch'", workflow)
        self.assertIn("github.event.repository.default_branch", workflow)
        self.assertIn("if: always()", workflow)
        self.assertIn("tools/check_fuzz_controls.py", workflow)
        self.assertIn("target/fuzz-run/", workflow)
        self.assertIn("'tests/support/context_scenarios.rs', 'benchmarks/generated.rs'", workflow)
        self.assertIn("'tests/support/codec_scenarios.rs'", workflow)


@unittest.skipUnless(sys.platform == "linux", "bounded process-group fuzz runner is Linux-only")
class FuzzExecutionEvidenceTests(unittest.TestCase):
    def fixture(self, logs, mode="run", exit_code=0):
        configuration = json.loads((ROOT / "toolchain.json").read_text())
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            corpus = directory / "corpus"
            artifact = directory / "input"
            artifact.write_bytes(b"generated replay fixture")
            arguments = SimpleNamespace(
                mode=mode, profile="smoke", targets=list(logs),
                output_dir=directory / "output", corpus_dir=corpus, artifact=artifact,
            )

            def process(command, **kwargs):
                if command == ["git", "rev-parse", "HEAD"]:
                    text = "a" * 40
                elif command == ["git", "status", "--porcelain"]:
                    text = ""
                elif command == [
                    "cargo", "+" + configuration["fuzz_rust_toolchain"], "fuzz", "--version",
                ]:
                    text = "cargo-fuzz " + configuration["cargo_fuzz_version"]
                else:
                    self.fail(f"unexpected subprocess: {command}")
                return SimpleNamespace(stdout=text)

            def execute_fixture(command, root, environment, log, timeout):
                if log.name == "seeds.log":
                    for target in TARGETS:
                        path = corpus / target
                        path.mkdir(parents=True)
                        (path / "seed").write_bytes(b"generated test fixture")
                log.write_text(logs[log.stem] if log.stem in logs else "generated fixture")
                return {
                    "command": command, "exit_code": exit_code if log.stem in logs else 0,
                    "timed_out": False, "seconds": 0.0, "log": log.name,
                }

            errors = io.StringIO()
            with patch("run_fuzz.subprocess.run", side_effect=process), \
                    patch("run_fuzz.execute", side_effect=execute_fixture), redirect_stderr(errors):
                report = run(arguments)
            self.assertEqual(json.loads((arguments.output_dir / "report.json").read_text()), report)
            for case in report["targets"]:
                self.assertTrue((arguments.output_dir / case["log"]).is_file())
            if "error" in report:
                self.assertIn(report["error"], errors.getvalue())
            return report

    def test_successful_campaign_binds_per_target_counts_and_exact_total(self):
        report = self.fixture({
            TARGETS[0]: "stat::number_of_executed_units: 7\n",
            TARGETS[1]: "stat::number_of_executed_units: 11\n",
        })
        self.assertEqual(report["status"], "passed")
        self.assertEqual([case["executions"] for case in report["targets"]], [7, 11])
        self.assertEqual(report["executions"], 18)

    def test_zero_exit_without_unambiguous_positive_counts_is_not_success(self):
        for text in (
            "no execution statistics\n",
            "stat::number_of_executed_units: 0\n",
            "stat::number_of_executed_units: -1\n",
            "stat::number_of_executed_units: invalid\n",
            "stat::number_of_executed_units: 1.5\n",
            "stat::number_of_executed_units: \u0661\n",
            "stat::number_of_executed_units: 1\nstat::number_of_executed_units: 1\n",
            "stat::number_of_executed_units: 1\nstat::number_of_executed_units: invalid\n",
        ):
            with self.subTest(log=text):
                report = self.fixture({TARGETS[0]: text})
                self.assertEqual(report["status"], "failed")
                self.assertEqual(report["targets"][0]["status"], "failed")
                self.assertIn("execution counter", report["error"])
                self.assertNotIn("executions", report)

    def test_replay_must_execute_the_single_input_exactly_once(self):
        for count, expected in ((1, "passed"), (2, "failed")):
            with self.subTest(count=count):
                report = self.fixture({
                    TARGETS[0]: f"stat::number_of_executed_units: {count}\n",
                }, mode="replay")
                self.assertEqual(report["status"], expected)
                if expected == "passed":
                    self.assertEqual(report["executions"], 1)
                else:
                    self.assertEqual(report["targets"][0]["status"], "failed")
                    self.assertIn("exactly one execution", report["error"])

    def test_real_target_failure_is_preserved_without_requiring_final_statistics(self):
        report = self.fixture({TARGETS[0]: "controlled target failure\n"}, exit_code=7)
        self.assertEqual(report["status"], "failed")
        self.assertEqual(report["targets"][0]["exit_code"], 7)
        self.assertNotIn("executions", report)
        self.assertNotIn("error", report)


class FuzzPlatformGuardTests(unittest.TestCase):
    def test_unsupported_hosts_are_rejected_before_writing_outputs(self):
        from run_fuzz import run
        from types import SimpleNamespace
        with patch("run_fuzz.sys.platform", "win32"):
            with self.assertRaisesRegex(ValueError, "requires Linux"):
                run(SimpleNamespace())


if __name__ == "__main__":
    unittest.main()

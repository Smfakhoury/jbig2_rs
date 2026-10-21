import io
import json
import os
import sys
import tempfile
import unittest
from contextlib import redirect_stderr
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from maintain_fuzz_corpus import (
    BUILD_SECONDS, MAX_FILES_PER_TARGET, MAX_TOTAL_FILES, MAINTENANCE_STEP_SECONDS,
    MERGE_SECONDS, REPLAY_SECONDS, TOTAL_SECONDS, check_campaign, collection, maintain,
    merge_evidence, real_path, replay_evidence,
)
from run_fuzz import (
    CAMPAIGN_SECONDS, CAMPAIGN_STEP_SECONDS, INPUT_TIMEOUT, MAX_INPUT_BYTES, PROFILES,
    ROOT, RSS_LIMIT_MB, TARGETS, corpus_summary,
)
from check_workflows import load_workflow


def merge_log(count=2, retained=1):
    return (
        f"MERGE-OUTER: {count} files, 0 in the initial corpus, 0 processed earlier\n"
        f"MERGE-INNER: {count} total files; 0 processed earlier; will process {count} files now\n"
        f"#{count}\tDONE   cov: 58 ft: 60 exec/s: 0 rss: 32Mb\n"
        "MERGE-OUTER: successful in 1 attempt(s)\n"
        f"MERGE-OUTER: {retained} new files with 60 new features added; 58 new coverage edges\n"
    )


@unittest.skipUnless(sys.platform == "linux", "corpus filesystem controls require Linux")
class CorpusInventoryTests(unittest.TestCase):
    def fixture(self, root):
        for target in TARGETS:
            directory = root / target
            directory.mkdir(parents=True)
            (directory / "generated-a").write_bytes(b"generated-corpus-control-a")
            (directory / "generated-b").write_bytes(b"generated-corpus-control-b")

    def test_inventory_rejects_missing_extra_empty_symlink_and_nonregular_collections(self):
        for mutation in ("missing", "extra", "empty", "symlink", "nested", "fifo", "oversize", "name"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary) / "corpus"
                self.fixture(root)
                seed = root / TARGETS[0] / "generated-a"
                if mutation == "missing":
                    seed.unlink()
                    (seed.parent / "generated-b").unlink()
                    seed.parent.rmdir()
                elif mutation == "extra":
                    (root / "unsupported-target").mkdir()
                elif mutation == "empty":
                    seed.write_bytes(b"")
                elif mutation == "symlink":
                    seed.unlink()
                    seed.symlink_to("generated-b")
                elif mutation == "nested":
                    seed.unlink()
                    seed.mkdir()
                elif mutation == "fifo":
                    if not hasattr(os, "mkfifo"):
                        continue
                    seed.unlink()
                    os.mkfifo(seed)
                elif mutation == "oversize":
                    seed.write_bytes(b"generated" * (MAX_INPUT_BYTES // 9 + 1))
                else:
                    seed.rename(seed.with_name("unsupported name"))
                with self.assertRaises(ValueError):
                    collection(root)

    def test_parent_symlink_and_total_file_byte_bounds(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            root = directory / "corpus"
            self.fixture(root)
            (directory / "alias").symlink_to(root, target_is_directory=True)
            with self.assertRaises(ValueError):
                real_path(directory / "alias" / TARGETS[0])
            with patch("maintain_fuzz_corpus.MAX_FILES_PER_TARGET", 1), self.assertRaises(ValueError):
                collection(root)
            with patch("maintain_fuzz_corpus.MAX_TOTAL_FILES", 1), self.assertRaises(ValueError):
                collection(root)
            with patch("maintain_fuzz_corpus.MAX_CORPUS_BYTES", 1), self.assertRaises(ValueError):
                collection(root)


class CorpusEvidenceTests(unittest.TestCase):
    def test_merge_requires_actual_complete_unambiguous_results_not_final_stats(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "corpus"
            CorpusInventoryTests().fixture(root)
            before = collection(root)[TARGETS[0]]
            after = [before[0]]
            log = Path(temporary) / "merge.log"
            log.write_text(merge_log())
            self.assertEqual(merge_evidence(log, before, after)["processed_units"], 2)
            for text in (
                "", "stat::number_of_executed_units: 2\n",
                merge_log() + merge_log(), merge_log().replace("successful in 1", "successful in 2"),
                merge_log().replace("#2\tDONE", "#1\tDONE"),
                merge_log().replace("1 new files", "0 new files"),
                merge_log().replace("will process 2", "will process 1"),
                merge_log().replace("60 new features", "59 new features"),
            ):
                with self.subTest(text=text):
                    log.write_text(text)
                    with self.assertRaises(ValueError):
                        merge_evidence(log, before, after)
            log.write_text(merge_log())
            for invalid in ([], [dict(after[0], sha256="changed")], after * 3):
                with self.assertRaises(ValueError):
                    merge_evidence(log, before, invalid)

    def test_exact_replay_checks_paths_completions_and_counts(self):
        with tempfile.TemporaryDirectory() as temporary:
            log = Path(temporary) / "replay.log"
            files = [Path(temporary) / "generated-a", Path(temporary) / "generated-b"]
            text = "".join(f"Running: {path}\nExecuted {path} in 0 ms\n" for path in files)
            text += "stat::number_of_executed_units: 2\n"
            log.write_text(text)
            self.assertEqual(replay_evidence(log, files), 2)
            for invalid in (
                text.replace("Executed ", "Missing "), text.replace("units: 2", "units: 1"),
                text + "stat::number_of_executed_units: 2\n", text.replace("generated-a", "stale"),
                text.replace(f"Running: {files[1]}", f"Running: {files[0]}"),
            ):
                log.write_text(invalid)
                with self.assertRaises(ValueError):
                    replay_evidence(log, files)


@unittest.skipUnless(sys.platform == "linux", "bounded corpus maintenance requires Linux")
class CorpusLifecycleTests(unittest.TestCase):
    def run_fixture(self, mutate=None, install=False):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            inputs = directory / "corpus"
            CorpusInventoryTests().fixture(inputs)
            before = collection(inputs)
            identity = "c" * 64
            configuration = json.loads((ROOT / "toolchain.json").read_text())
            campaign = {
                "schema_version": 1,
                "status": "passed", "mode": "run", "source_sha256": identity,
                "source_revision": "a" * 40, "executions": len(TARGETS) * 7,
                "toolchain": configuration["fuzz_rust_toolchain"],
                "cargo_fuzz": "cargo-fuzz " + configuration["cargo_fuzz_version"],
                "targets": [{
                    "target": target, "status": "passed", "exit_code": 0, "timed_out": False,
                    "artifacts": [], "executions": 7, "corpus": corpus_summary(before[target]),
                } for target in TARGETS],
            }
            for name in ("seeds", "build"):
                campaign[name] = {"exit_code": 0, "timed_out": False, "log": name + ".log"}
                (directory / (name + ".log")).write_text("generated operation control")
            for item in campaign["targets"]:
                item["log"] = item["target"] + ".log"
                (directory / item["log"]).write_text("stat::number_of_executed_units: 7\n")
            path = directory / "campaign.json"
            path.write_text(json.dumps(campaign))
            arguments = SimpleNamespace(
                generated=False, install=install, corpus_dir=inputs, campaign_report=path,
                output_dir=directory / "output",
            )
            calls = []
            source_calls = []

            def source(_):
                source_calls.append(True)
                return "d" * 64 if mutate == "source" and len(source_calls) > 1 else identity

            def operation(command, root, environment, log, timeout):
                calls.append((command, timeout))
                text = "generated operation diagnostic\n"
                exit_code = 0
                if log.stem == "cargo-fuzz-version":
                    text = "cargo-fuzz " + configuration["cargo_fuzz_version"] + "\n"
                elif log.stem == "rustc-version":
                    text = "rustc pinned fixture\ncommit-hash: generated\n"
                elif log.stem.endswith("-cmin"):
                    target = command[4]
                    stage = Path(command[5])
                    (stage / "generated-b").unlink()
                    (stage / "generated-a").rename(stage / "generated-minimized")
                    text = merge_log()
                    if mutate == "ambiguous":
                        text += merge_log()
                    elif mutate == "failed":
                        exit_code = 7
                    elif mutate == "empty-output":
                        (stage / "generated-minimized").unlink()
                    elif mutate == "changed-output":
                        (stage / "generated-minimized").write_bytes(b"generated-unexpected-control")
                    elif mutate == "input":
                        (inputs / target / "generated-a").write_bytes(b"generated-changed-control")
                    elif mutate == "campaign":
                        path.write_text("{}")
                    elif mutate == "preserved":
                        (arguments.output_dir / "preserved-input" / target / "generated-a").write_bytes(
                            b"generated-changed-control"
                        )
                elif log.stem.endswith(("-before", "-after")):
                    files = command[5:command.index("--")]
                    text = "".join(f"Running: {name}\nExecuted {name} in 0 ms\n" for name in files)
                    text += f"stat::number_of_executed_units: {len(files)}\n"
                    if mutate == "replay":
                        text = "zero exit without replay evidence\n"
                if mutate != "missing-log":
                    log.write_text(text)
                return {
                    "command": command, "exit_code": exit_code,
                    "timed_out": mutate == "timeout", "seconds": 0.0, "log": log.name,
                }

            if mutate in (
                "stale", "revision", "campaign-failed", "inventory", "duplicate", "missing-count",
                "missing-build", "missing-raw", "ambiguous-raw", "symlink-raw",
            ):
                if mutate == "stale":
                    campaign["source_sha256"] = "d" * 64
                elif mutate == "campaign-failed":
                    campaign["status"] = "failed"
                elif mutate == "inventory":
                    campaign["targets"][0]["corpus"]["sha256"] = "changed"
                elif mutate == "duplicate":
                    campaign["targets"][-1] = campaign["targets"][0]
                elif mutate == "revision":
                    campaign["source_revision"] = "b" * 40
                elif mutate == "missing-build":
                    campaign.pop("build")
                elif mutate == "missing-raw":
                    (directory / campaign["targets"][0]["log"]).unlink()
                elif mutate == "ambiguous-raw":
                    (directory / campaign["targets"][0]["log"]).write_text(
                        "stat::number_of_executed_units: 7\nstat::number_of_executed_units: 7\n"
                    )
                elif mutate == "symlink-raw":
                    raw = directory / campaign["targets"][0]["log"]
                    raw.unlink()
                    raw.symlink_to(directory / campaign["targets"][1]["log"])
                else:
                    campaign.pop("executions")
                path.write_text(json.dumps(campaign))
            if mutate == "missing-campaign":
                arguments.campaign_report = None
            original_rename = Path.rename

            def rename(path_, destination):
                if mutate == "install-rename" and path_.name == "minimized":
                    raise OSError("generated installation failure control")
                return original_rename(path_, destination)

            with patch("maintain_fuzz_corpus.execute", side_effect=operation), \
                    patch("maintain_fuzz_corpus.source_identity", side_effect=source), \
                    patch("maintain_fuzz_corpus.subprocess.run", return_value=SimpleNamespace(stdout="a" * 40)), \
                    patch.object(Path, "rename", new=rename), \
                    redirect_stderr(io.StringIO()):
                if mutate == "deadline":
                    with patch("maintain_fuzz_corpus.TOTAL_SECONDS", 0):
                        report = maintain(arguments)
                else:
                    report = maintain(arguments)
            self.assertEqual(json.loads((arguments.output_dir / "report.json").read_text()), report)
            if mutate not in ("input",):
                if install and report["status"] == "passed":
                    self.assertEqual(collection(Path(report["preserved_input_path"])), before)
                    self.assertEqual(collection(inputs), report["after"])
                else:
                    self.assertEqual(collection(inputs), before)
            return report, calls

    def test_success_preserves_source_and_records_actual_before_after_and_all_operations(self):
        report, calls = self.run_fixture()
        self.assertEqual(report["status"], "passed", report.get("error"))
        self.assertEqual(len(report["targets"]), 9)
        self.assertEqual(report["merge_processed_units"], 18)
        self.assertEqual(report["replay_executions"], 27)
        self.assertEqual(report["before"], report["staged_before"])
        self.assertFalse(report["installed"])
        self.assertEqual(len(report["operations"]), 32)
        self.assertIn("not source-line coverage", report["coverage_claim"])
        for command, timeout in calls:
            self.assertGreater(timeout, 0)
            self.assertLessEqual(timeout, TOTAL_SECONDS)
            if "cmin" in command and "--help" not in command:
                for flag in (
                    f"-runs={MAX_FILES_PER_TARGET + 1}", f"-max_total_time={MERGE_SECONDS}",
                    f"-timeout={INPUT_TIMEOUT}", f"-rss_limit_mb={RSS_LIMIT_MB}",
                    f"-max_len={MAX_INPUT_BYTES}",
                ):
                    self.assertIn(flag, command)
                self.assertLessEqual(timeout, MERGE_SECONDS)
            elif "run" in command:
                self.assertIn("-runs=1", command)
                self.assertLessEqual(timeout, REPLAY_SECONDS)
            elif "build" in command:
                self.assertLessEqual(timeout, BUILD_SECONDS)

    def test_install_preserves_original_and_only_promotes_complete_success(self):
        report, _ = self.run_fixture(install=True)
        self.assertEqual(report["status"], "passed")
        self.assertTrue(report["installed"])

    def test_failures_and_stale_missing_ambiguous_evidence_never_install(self):
        for mutation in (
            "source", "input", "campaign", "stale", "revision", "campaign-failed", "inventory",
            "duplicate", "missing-count", "missing-campaign", "ambiguous", "failed",
            "empty-output", "changed-output", "replay", "timeout", "missing-log", "deadline",
            "missing-build", "missing-raw", "ambiguous-raw", "symlink-raw",
            "preserved", "install-rename",
        ):
            with self.subTest(mutation=mutation):
                report, _ = self.run_fixture(mutation, install=True)
                self.assertEqual(report["status"], "failed")
                self.assertIn("error", report)
                self.assertFalse(report.get("installed", False))
                self.assertFalse(any(item["timed_out"] for item in report["operations"])
                                 if mutation != "timeout" else False)

    def test_output_reuse_overlap_and_symlinks_are_rejected_without_clobbering(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            marker = directory / "report.json"
            marker.write_text("preserve")
            arguments = SimpleNamespace(generated=True, output_dir=directory)
            with self.assertRaises(FileExistsError):
                maintain(arguments)
            self.assertEqual(marker.read_text(), "preserve")
            arguments.generated = False
            arguments.corpus_dir = directory
            arguments.output_dir = directory / "nested"
            with self.assertRaisesRegex(ValueError, "overlap"):
                maintain(arguments)
            alias = directory / "alias"
            alias.symlink_to(directory, target_is_directory=True)
            arguments.generated = True
            arguments.output_dir = alias / "new-output"
            with self.assertRaisesRegex(ValueError, "symlink"):
                maintain(arguments)

    def test_unsupported_host_fails_before_creating_any_output(self):
        with patch("maintain_fuzz_corpus.sys.platform", "win32"):
            with self.assertRaisesRegex(ValueError, "requires Linux"):
                maintain(SimpleNamespace())


class CorpusWorkflowBudgetTests(unittest.TestCase):
    def test_extended_campaign_minimization_and_all_step_caps_fit_unchanged_hour(self):
        workflow = load_workflow((ROOT / ".github/workflows/fuzz.yml").read_text())
        job = workflow["jobs"]["fuzz"]
        self.assertEqual(int(job["timeout-minutes"]), 60)
        self.assertEqual(sum(int(step["timeout-minutes"]) for step in job["steps"]), 60)
        self.assertEqual(CAMPAIGN_STEP_SECONDS, 49 * 60)
        self.assertEqual(MAINTENANCE_STEP_SECONDS, 3 * 60)
        self.assertLess(CAMPAIGN_SECONDS, CAMPAIGN_STEP_SECONDS)
        self.assertLess(TOTAL_SECONDS, MAINTENANCE_STEP_SECONDS)
        self.assertGreater(CAMPAIGN_SECONDS - len(TARGETS) * PROFILES["extended"], 0)
        self.assertEqual(CAMPAIGN_SECONDS - 9 * 300, 235)
        self.assertIn("needs", job)
        self.assertEqual(MAX_TOTAL_FILES, 16_384)
        trusted = job["env"]["TRUSTED_CORPUS"]
        self.assertIn("github.ref == 'refs/heads/main'", trusted)
        self.assertIn("github.event.repository.default_branch == 'main'", trusted)
        maintenance = next(step for step in job["steps"] if "--install" in step.get("run", ""))
        self.assertEqual(maintenance["if"], "success() && env.TRUSTED_CORPUS == 'true'")
        self.assertIn("--campaign-report target/fuzz-run/report.json", maintenance["run"])
        restore = next(step for step in job["steps"] if "cache/restore@" in step.get("uses", ""))
        self.assertIn("steps.identity.outputs.sha256", restore["with"]["key"])
        self.assertNotIn("restore-keys", next(
            step for step in job["steps"] if "cache/save@" in step.get("uses", "")
        ).get("with", {}))


if __name__ == "__main__":
    unittest.main()

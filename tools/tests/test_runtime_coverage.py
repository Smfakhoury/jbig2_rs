import copy
import os
import subprocess
import sys
import tempfile
import textwrap
import tomllib
import unittest
from pathlib import Path
from unittest.mock import patch


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from coverage_report import (
    changed_lines, lcov_lines, ratio, regressions, scoped_report,
    test_source_hash as fingerprint_test_sources,
)
from evidence import load_json, validate


class RuntimeCoverageTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        (self.root / "src").mkdir()
        (self.root / "src/lib.rs").write_text("mod runtime;\n", encoding="utf-8")
        self.source = self.root / "src/runtime.rs"
        self.source.write_text(
            "pub fn f(flag: bool) -> bool {\n"
            "    if flag {\n"
            "        true\n"
            "    } else {\n"
            "        false\n"
            "    }\n"
            "}\n"
            "#[cfg(test)]\n"
            "mod tests {\n"
            "    fn helper() {}\n"
            "}\n",
            encoding="utf-8",
        )
        runtime = {
            "name": "runtime_function", "filenames": [str(self.source)],
            "regions": [[1, 1, 7, 2, 1, 0, 0, 0], [5, 9, 5, 14, 0, 0, 0, 0]],
            "branches": [[2, 8, 2, 12, 1, 0, 0, 0, 4]],
        }
        test = {
            "name": "test_function", "filenames": [str(self.source)],
            "regions": [[10, 5, 10, 19, 999, 0, 0, 0]],
            "branches": [[10, 5, 10, 19, 999, 999, 0, 0, 4]],
        }
        self.document = {
            "type": "llvm.coverage.json.export", "version": "3.1.0",
            "data": [{
                "files": [{"filename": str(self.source)}],
                "functions": [runtime, test],
            }],
        }
        self.lcov = f"SF:{self.source}\n" + "".join(
            f"DA:{number},{0 if number == 5 else 1}\n" for number in range(1, 8)
        ) + "DA:10,999\nend_of_record\n"

    def report(self):
        return scoped_report(self.document, self.lcov, self.root)

    def test_runtime_denominator_excludes_inline_test_functions_and_lines(self):
        report = self.report()
        self.assertEqual(report["totals"]["lines"], ratio(6, 7))
        self.assertEqual(report["totals"]["regions"], ratio(1, 2))
        self.assertEqual(report["totals"]["branches"], ratio(1, 2))
        self.assertEqual(report["files"]["src/runtime.rs"]["uncovered_lines"], [5])
        self.assertEqual(report["files"]["src/runtime.rs"]["excluded_test_instantiations"], 1)

    def test_separate_test_files_and_type_only_facades_are_not_runtime_denominators(self):
        library = self.root / "src/lib.rs"
        library.write_text(library.read_text() + "mod facade;\n", encoding="utf-8")
        (self.root / "src/facade.rs").write_text("pub struct Metadata;\n", encoding="utf-8")
        driver = self.root / "src/runtime/tests.rs"
        driver.parent.mkdir()
        driver.write_text("#![cfg(test)]\nfn helper() {}\n", encoding="utf-8")
        self.document["data"][0]["functions"].append({
            "name": "external_test_helper", "filenames": [str(driver)],
            "regions": [[2, 1, 2, 15, 999, 0, 0, 0]], "branches": [],
        })
        report = self.report()
        self.assertEqual(set(report["files"]), {"src/runtime.rs"})
        self.assertIn("src/facade.rs", report["excluded_files"])
        self.assertIn("src/runtime/tests.rs", report["excluded_files"])
        self.assertIn("src/runtime/tests.rs", report["excluded_integration_test_sources"])

    def test_reused_test_driver_is_excluded_even_without_a_file_summary(self):
        driver = self.root / "tools/distribution_consumer.rs"
        driver.parent.mkdir()
        driver.write_text("pub fn smoke() {}\n", encoding="utf-8")
        self.document["data"][0]["functions"].append({
            "name": "test_driver", "filenames": [str(driver)],
            "regions": [[1, 1, 1, 18, 999, 0, 0, 0]], "branches": [],
        })
        report = self.report()
        self.assertEqual(report["totals"]["lines"], ratio(6, 7))
        self.assertIn("tools/distribution_consumer.rs", report["excluded_integration_test_sources"])

    def test_unknown_function_only_driver_is_not_silently_excluded(self):
        self.document["data"][0]["functions"].append({
            "name": "unexpected", "filenames": [str(self.root / "tools/unapproved.rs")],
            "regions": [[1, 1, 1, 18, 999, 0, 0, 0]], "branches": [],
        })
        with self.assertRaisesRegex(ValueError, "outside runtime"):
            self.report()

    def test_shared_generated_fixture_is_excluded_and_fingerprinted(self):
        before = fingerprint_test_sources(self.root)
        fixture = self.root / "benchmarks/generated.rs"
        fixture.parent.mkdir()
        fixture.write_text("pub fn generate() {}\n", encoding="utf-8")
        self.document["data"][0]["functions"].append({
            "name": "generated_fixture", "filenames": [str(fixture)],
            "regions": [[1, 1, 1, 21, 999, 0, 0, 0]], "branches": [],
        })
        report = self.report()
        self.assertEqual(report["totals"]["lines"], ratio(6, 7))
        self.assertIn("benchmarks/generated.rs", report["excluded_integration_test_sources"])
        added = fingerprint_test_sources(self.root)
        self.assertNotEqual(before, added)
        fixture.write_text("pub fn changed() {}\n", encoding="utf-8")
        self.assertNotEqual(added, fingerprint_test_sources(self.root))

    def test_other_benchmark_sources_are_not_test_fixture_exclusions(self):
        self.document["data"][0]["functions"].append({
            "name": "unknown_benchmark", "filenames": [str(self.root / "benchmarks/other.rs")],
            "regions": [[1, 1, 1, 18, 999, 0, 0, 0]], "branches": [],
        })
        with self.assertRaisesRegex(ValueError, "outside runtime"):
            self.report()

    def test_test_driver_fingerprint_detects_edits_outside_runtime_source(self):
        before = fingerprint_test_sources(self.root)
        directory = self.root / "tests"
        directory.mkdir()
        path = directory / "external.rs"
        path.write_text("#[test] fn external() {}\n", encoding="utf-8")
        added = fingerprint_test_sources(self.root)
        self.assertNotEqual(before, added)
        path.write_text("#[test] fn changed() {}\n", encoding="utf-8")
        self.assertNotEqual(added, fingerprint_test_sources(self.root))

    def test_instantiations_are_merged_without_counting_denominator_twice(self):
        other = copy.deepcopy(self.document["data"][0]["functions"][0])
        other["name"] = "another_instantiation"
        other["regions"][1][4] = 1
        other["branches"][0][5] = 1
        self.document["data"][0]["functions"].append(other)
        report = self.report()
        self.assertEqual(report["totals"]["regions"], ratio(2, 2))
        self.assertEqual(report["totals"]["branches"], ratio(2, 2))

    def test_test_only_statements_inside_runtime_functions_do_not_count(self):
        self.source.write_text(
            "pub fn f(flag: bool) -> bool {\n"
            "    #[cfg(test)]\n"
            "    if flag { let ignored = 1; }\n"
            "    if flag {\n"
            "        true\n"
            "    } else {\n"
            "        false\n"
            "    }\n"
            "}\n", encoding="utf-8",
        )
        self.document["data"][0]["functions"] = [{
            "name": "runtime", "filenames": [str(self.source)],
            "regions": [[1, 1, 9, 2, 1, 0, 0, 0], [3, 5, 3, 32, 999, 0, 0, 0]],
            "branches": [[3, 8, 3, 12, 999, 999, 0, 0, 4], [4, 8, 4, 12, 1, 0, 0, 0, 4]],
        }]
        self.lcov = f"SF:{self.source}\n" + "".join(
            f"DA:{number},1\n" for number in range(1, 10)
        ) + "end_of_record\n"
        report = self.report()
        self.assertEqual(report["totals"]["branches"], ratio(1, 2))
        self.assertEqual(report["totals"]["regions"], ratio(1, 1))
        self.assertNotIn("3", report["files"]["src/runtime.rs"]["line_counts"])

    def test_ambiguous_same_line_runtime_and_test_counts_fail_closed(self):
        source = "pub fn f() { #[cfg(test)] { let ignored = 1; } let actual = 2; }\n"
        self.source.write_text(source, encoding="utf-8")
        start = source.index("let ignored") + 1
        self.document["data"][0]["functions"] = [{
            "name": "runtime", "filenames": [str(self.source)],
            "regions": [[1, 1, 1, len(source), 1, 0, 0, 0],
                        [1, start, 1, start + len("let ignored = 1;"), 999, 0, 0, 0]],
            "branches": [[1, 1, 1, 4, 1, 0, 0, 0, 4]],
        }]
        self.lcov = f"SF:{self.source}\nDA:1,999\nend_of_record\n"
        with self.assertRaisesRegex(ValueError, "mixed runtime/test"):
            self.report()

    def test_release_goals_are_not_empty_denominator_success(self):
        self.assertIsNone(ratio(0, 0)["percent"])
        for function in self.document["data"][0]["functions"]:
            function["branches"] = []
        with self.assertRaisesRegex(ValueError, "branch instrumentation"):
            self.report()

    def test_rejects_empty_unsupported_or_missing_source_reports(self):
        for document in (
            {}, dict(self.document, version="unknown"),
            dict(self.document, data=[]),
            {**self.document, "data": [{"files": [], "functions": []}]},
        ):
            with self.subTest(document=document), self.assertRaises(ValueError):
                scoped_report(document, self.lcov, self.root)

    def test_rejects_unexpected_region_encodings(self):
        self.document["data"][0]["functions"][0]["regions"][0][7] = 3
        with self.assertRaisesRegex(ValueError, "unsupported LLVM code region"):
            self.report()

    def test_excluded_lib_cannot_gain_a_runtime_function(self):
        (self.root / "src/lib.rs").write_text("mod runtime;\nfn hidden() {}\n", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "excluded source file"):
            self.report()

    def test_rejects_missing_duplicate_or_partial_lcov(self):
        for text in (
            "", "DA:1,1\n", f"SF:{self.source}\nDA:1,1\n",
            f"SF:{self.source}\nDA:1,1\nDA:1,0\nend_of_record\n",
            f"SF:{self.source}\nDA:1,-1\nend_of_record\n",
        ):
            with self.subTest(text=text), self.assertRaises((ValueError, IndexError)):
                lcov_lines(text)

    def test_nonregression_gate_fails_for_injected_missing_coverage(self):
        before = self.report()
        after = copy.deepcopy(before)
        after["files"]["src/runtime.rs"]["lines"]["covered"] -= 1
        self.assertTrue(regressions(after, before))
        after = copy.deepcopy(before)
        after["totals"]["branches"]["covered"] -= 1
        self.assertTrue(regressions(after, before))
        self.assertEqual(regressions(before, before), [])

    def test_missing_baseline_file_and_scope_change_are_not_silently_accepted(self):
        before = self.report()
        after = copy.deepcopy(before)
        after["files"] = {}
        self.assertTrue(regressions(after, before))
        after = copy.deepcopy(before)
        after["scope_version"] = 100
        with self.assertRaises(ValueError):
            regressions(after, before)

    def test_reviewed_floor_rejects_one_lost_coordinate_for_every_measured_metric(self):
        baseline = load_json(TOOLS.parent / "validation/coverage/baseline.json")
        self.assertEqual(regressions(baseline, baseline), [])
        entries = [("total", baseline["totals"])] + list(baseline["files"].items())
        for label, metrics in entries:
            for metric, value in metrics.items():
                if value["covered"] == 0:
                    continue
                with self.subTest(file=label, metric=metric):
                    report = copy.deepcopy(baseline)
                    report["totals"][metric]["covered"] -= 1
                    if label != "total":
                        report["files"][label][metric]["covered"] -= 1
                    self.assertIn(
                        f"{label}: {metric} below reviewed baseline",
                        regressions(report, baseline),
                    )

    def test_changed_line_coverage_reports_uncovered_runtime_lines(self):
        report = self.report()
        process = type("Result", (), {"stdout": "@@ -2,1 +2,1 @@\n@@ -5 +5 @@\n"})
        with patch("coverage_report.subprocess.run", return_value=process):
            changed = changed_lines(self.root, "HEAD", report["files"])
        self.assertEqual(changed["covered"], 1)
        self.assertEqual(changed["count"], 2)
        self.assertEqual(changed["uncovered"], {"src/runtime.rs": [5]})
        process.stdout = ""
        with patch("coverage_report.subprocess.run", return_value=process):
            changed = changed_lines(self.root, "HEAD", report["files"])
        self.assertEqual(changed["status"], "not_applicable")
        self.assertIsNone(changed["percent"])

    def test_checked_baseline_is_consistent_and_not_a_release_approval(self):
        baseline = load_json(TOOLS.parent / "validation/coverage/baseline.json")
        validate(baseline, "coverage_baseline")
        self.assertEqual(baseline["release_goals"], {"lines": 90, "branches": 85})
        self.assertNotEqual(baseline["status"], "release_approved")
        manifest = tomllib.loads((TOOLS.parent / "Cargo.toml").read_text())
        self.assertIn(manifest["package"]["publish"], (False, ["crates-io"]))
        broken = copy.deepcopy(baseline)
        broken["totals"]["lines"]["covered"] += 1
        with self.assertRaisesRegex(ValueError, "totals do not match"):
            validate(broken, "coverage_baseline")


@unittest.skipUnless(sys.platform == "linux", "the coverage workflow runs on Linux")
class CoverageWorkflowTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.origin = self.root / "origin"
        self.checkout = self.root / "checkout"
        self.environment = dict(os.environ)
        self.environment.update({
            "GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_TERMINAL_PROMPT": "0",
        })
        self.git("init", "--template=", "--initial-branch=original", str(self.origin))
        self.git("commit", "--allow-empty", "-m", "Before rewrite", cwd=self.origin)
        self.before = self.git("rev-parse", "HEAD", cwd=self.origin).stdout.strip()
        self.git("checkout", "--orphan", "rewritten", cwd=self.origin)
        self.git("commit", "--allow-empty", "-m", "After rewrite", cwd=self.origin)
        self.git("update-ref", "-d", "refs/heads/original", cwd=self.origin)
        self.git("clone", "--no-local", str(self.origin), str(self.checkout))
        self.arguments = self.root / "arguments"
        binary = self.root / "bin"
        binary.mkdir()
        python = binary / "python"
        python.write_text('#!/bin/sh\nprintf "%s\\n" "$@" > "$COVERAGE_ARGUMENTS"\n')
        python.chmod(0o755)
        self.environment.update({
            "PATH": str(binary) + os.pathsep + self.environment["PATH"],
            "COVERAGE_ARGUMENTS": str(self.arguments),
        })
        workflow = (TOOLS.parent / ".github/workflows/coverage.yml").read_text()
        step = workflow.split(
            "      - name: Collect runtime coverage and enforce non-regression baseline\n", 1,
        )[1].split("      - name:", 1)[0]
        self.script = textwrap.dedent(step.split("        run: |\n", 1)[1])

    def git(self, *arguments, cwd=None, check=True):
        return subprocess.run(
            ["git", "-c", "user.name=Coverage test", "-c", "user.email=coverage@example.test",
             "-c", "commit.gpgsign=false", *arguments],
            cwd=cwd or self.root, env=self.environment,
            capture_output=True, text=True, check=check,
        )

    def collect(self, reference):
        return subprocess.run(
            ["bash", "-e", "-o", "pipefail", "-c", self.script],
            cwd=self.checkout, env={**self.environment, "BASE_REF": reference},
            capture_output=True, text=True, check=False,
        )

    def test_rewritten_push_fetches_the_exact_missing_comparison_commit(self):
        self.assertNotEqual(self.git(
            "cat-file", "-e", self.before + "^{commit}", cwd=self.checkout, check=False,
        ).returncode, 0)
        process = self.collect(self.before)
        self.assertEqual(process.returncode, 0, process.stderr)
        self.assertEqual(
            self.git("rev-parse", "FETCH_HEAD", cwd=self.checkout).stdout.strip(), self.before,
        )
        self.assertEqual(
            self.arguments.read_text().splitlines(),
            ["-B", "tools/run_coverage.py", "--base-ref", self.before],
        )

    def test_existing_comparison_commit_does_not_require_network(self):
        reference = self.git("rev-parse", "HEAD", cwd=self.checkout).stdout.strip()
        self.git("remote", "set-url", "origin", str(self.root / "missing"), cwd=self.checkout)
        process = self.collect(reference)
        self.assertEqual(process.returncode, 0, process.stderr)
        self.assertEqual(self.arguments.read_text().splitlines()[-2:], ["--base-ref", reference])
        self.assertFalse((self.checkout / ".git/FETCH_HEAD").exists())

    def test_unfetchable_comparison_commit_fails_without_running_coverage(self):
        process = self.collect("f" * 40)
        self.assertNotEqual(process.returncode, 0)
        self.assertFalse(self.arguments.exists())
        self.assertIn("not our ref", process.stderr)

    def test_first_push_still_runs_baseline_coverage_without_a_comparison(self):
        self.git("remote", "set-url", "origin", str(self.root / "missing"), cwd=self.checkout)
        for reference in ("", "0" * 40):
            with self.subTest(reference=reference):
                process = self.collect(reference)
                self.assertEqual(process.returncode, 0, process.stderr)
                self.assertEqual(
                    self.arguments.read_text().splitlines(), ["-B", "tools/run_coverage.py"],
                )
                self.assertFalse((self.checkout / ".git/FETCH_HEAD").exists())


if __name__ == "__main__":
    unittest.main()

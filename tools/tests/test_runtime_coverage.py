import copy
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from coverage_report import changed_lines, lcov_lines, ratio, regressions, scoped_report
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
        self.assertLess(
            baseline["totals"]["lines"]["covered"] / baseline["totals"]["lines"]["count"] * 100,
            baseline["release_goals"]["lines"],
        )
        broken = copy.deepcopy(baseline)
        broken["totals"]["lines"]["covered"] += 1
        with self.assertRaisesRegex(ValueError, "totals do not match"):
            validate(broken, "coverage_baseline")


if __name__ == "__main__":
    unittest.main()

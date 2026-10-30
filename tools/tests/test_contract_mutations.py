import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from source_group import source_text
from check_contract_mutations import MUTATIONS, mutate
from check_huffman_assignment_proofs import CASES as HUFFMAN_CASES
import check_contract_mutations
from evidence import load_json, validate
from proof_sensitivity import replace_once, weakened_contract
from rust_tests import test_inventory as inventory_tests


class ContractMutationTests(unittest.TestCase):
    def test_total_line_controls_keep_original_success_bounds_and_error_extent(self):
        cases = [case for case in HUFFMAN_CASES if case["function"] == "decode_line"]
        self.assertEqual(len(cases), 4)
        for case in cases:
            with self.subTest(case=case["id"]):
                source = source_text(TOOLS.parent, case["file"])
                changed = replace_once(source, case["site"], case["replacement"])
                weaker = weakened_contract(changed, case)
                self.assertIn("huffman_spec::reader_valid(old(reader))", weaker)
                self.assertIn("huffman_spec::reader_valid(final(reader))", weaker)
                self.assertIn("final(reader).bytes@ == old(reader).bytes@", weaker)
                self.assertIn("huffman_spec::decoded_line_value(", weaker)
                self.assertIn("line.kind == LineKind::OutOfBand", weaker)
                self.assertIn("line.range_len <= 32", weaker)
                self.assertIn("final(reader).bit_position == old(reader).bit_position", weaker)
                self.assertNotIn("match crate::spec::readers::bit_read_error(", weaker)

    def test_build_selects_exact_existing_test_executable(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary).resolve()
            executable = directory / "target" / "selected.exe"
            executable.parent.mkdir()
            executable.write_bytes(b"test executable")
            for target, kind, name in [
                ("lib", ["lib"], "jbig2_decoder"),
                ("public_api", ["test"], "public_api"),
            ]:
                artifact = {
                    "reason": "compiler-artifact",
                    "target": {"kind": kind, "name": name},
                    "profile": {"test": True},
                    "executable": str(executable),
                }
                ordinary = {**artifact, "profile": {"test": False}}
                output = "\n".join(json.dumps(row) for row in [ordinary, artifact])
                with self.subTest(target=target), patch.object(
                    check_contract_mutations, "run", return_value=output,
                ) as command:
                    self.assertEqual(check_contract_mutations.build_test_binary(
                        ["cargo", "+test"], directory, {}, target,
                    ), executable)
                    arguments = command.call_args.args[0]
                    self.assertIn("--no-run", arguments)
                    self.assertIn("--message-format=json", arguments)
                    self.assertIn("--lib" if target == "lib" else "--test", arguments)

    def test_build_rejects_missing_ambiguous_malformed_and_outside_artifacts(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary).resolve()
            executable = directory / "target" / "selected"
            executable.parent.mkdir()
            executable.write_bytes(b"test executable")
            artifact = {
                "reason": "compiler-artifact",
                "target": {"kind": ["lib"], "name": "jbig2_decoder"},
                "profile": {"test": True},
                "executable": str(executable),
            }
            (directory / "outside").write_bytes(b"outside executable")
            for output in [
                "", "not JSON", "[]",
                json.dumps(artifact) + "\n" + json.dumps(artifact),
                json.dumps({**artifact, "profile": {"test": False}}),
                json.dumps({**artifact, "target": {"kind": ["test"], "name": "other"}}),
                json.dumps({**artifact, "target": {"kind": ["lib"], "name": "other"}}),
                json.dumps({**artifact, "profile": []}),
                json.dumps({**artifact, "executable": None}),
                json.dumps({**artifact, "executable": str(directory / "outside")}),
                json.dumps({**artifact, "executable": str(directory / "target" / "missing")}),
            ]:
                with self.subTest(output=output), patch.object(
                    check_contract_mutations, "run", return_value=output,
                ), self.assertRaises(ValueError):
                    check_contract_mutations.build_test_binary(
                        ["cargo", "+test"], directory, {}, "lib",
                    )

    def test_pristine_baseline_is_built_once_and_never_replaced_by_mutants(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            for name in ("src", "tests", "tools"):
                (root / name).mkdir()
            original = "fn target() {\n    before_one();\n    before_two();\n}\n"
            original_bytes = original.replace("\n", "\r\n").encode("utf-8")
            (root / "src/lib.rs").write_bytes(original_bytes)
            for name in (
                "Cargo.toml", "Cargo.lock", "README.md", "SECURITY.md", "LICENSE",
                "tools/distribution_consumer.rs",
            ):
                (root / name).write_text("")
            cases = tuple({
                "id": name, "file": "src/lib.rs", "before": "before_" + name,
                "after": "after_" + name, "target": "lib", "test": name,
            } for name in ("one", "two"))
            builds, baselines, executions = [], [], []

            def build(_cargo, directory, _environment, target):
                self.assertEqual(target, "lib")
                source = (directory / "src/lib.rs").read_text()
                builds.append(source)
                executable = directory / "target" / "test-binary"
                executable.parent.mkdir(exist_ok=True)
                executable.write_bytes(source.encode("utf-8"))
                return executable

            def baseline(command, directory, _environment, capture):
                self.assertTrue(capture)
                self.assertEqual(Path(command[0]).parent, directory / "baseline-tests")
                self.assertEqual(Path(command[0]).read_text(), original)
                self.assertEqual(command[2:], ["--exact"])
                baselines.append(command[1])
                return "test result: ok. 1 passed; 0 failed;"

            def execute(command, **kwargs):
                self.assertNotEqual(Path(command[0]).parent.name, "baseline-tests")
                self.assertEqual(Path(command[0]).read_text(),
                    original.replace("before_" + command[1], "after_" + command[1]))
                self.assertEqual((kwargs["cwd"] / "src/lib.rs").read_bytes(),
                    original_bytes.replace(
                        ("before_" + command[1]).encode("utf-8"),
                        ("after_" + command[1]).encode("utf-8"),
                    ))
                executions.append(command[1])
                return check_contract_mutations.subprocess.CompletedProcess(
                    command, 101, "test result: FAILED. 0 passed; 1 failed;", "",
                )

            write_text = Path.write_text

            def windows_write_text(path, data, encoding=None, errors=None, newline=None):
                return write_text(path, data, encoding=encoding, errors=errors,
                    newline="\r\n" if newline is None else newline)

            with (
                patch.object(check_contract_mutations, "ROOT", root),
                patch.object(check_contract_mutations, "MUTATIONS", cases),
                patch.object(check_contract_mutations, "build_test_binary", side_effect=build),
                patch.object(check_contract_mutations, "run", side_effect=baseline),
                patch.object(check_contract_mutations.subprocess, "run", side_effect=execute),
                patch.object(Path, "write_text", windows_write_text),
            ):
                result = check_contract_mutations.check_mutations("test")
            self.assertEqual(builds, [
                original, original.replace("before_one", "after_one"),
                original.replace("before_two", "after_two"),
            ])
            self.assertEqual(baselines, ["one", "two"])
            self.assertEqual(executions, ["one", "two"])
            self.assertEqual(len(result["mutations"]), 2)
            self.assertEqual((root / "src/lib.rs").read_bytes(), original_bytes)

    def test_mutation_sites_are_unique_and_not_noops(self):
        identifiers = [mutation["id"] for mutation in MUTATIONS]
        self.assertEqual(len(identifiers), 157)
        self.assertEqual(len(identifiers), len(set(identifiers)))
        for mutation in MUTATIONS:
            with self.subTest(mutation=mutation["id"]):
                source = source_text(TOOLS.parent, mutation["file"])
                changed = mutate(source, mutation)
                self.assertNotEqual(changed, source)
                self.assertEqual(changed.count(mutation["after"]), 1)

    def test_empty_or_duplicate_mutation_sets_fail_closed(self):
        for cases in ((), (MUTATIONS[0], MUTATIONS[0])):
            with (
                self.subTest(cases=len(cases)),
                patch.object(check_contract_mutations, "MUTATIONS", cases),
                self.assertRaisesRegex(ValueError, "missing or duplicate"),
            ):
                check_contract_mutations.check_mutations("unused-toolchain")

    def test_missing_or_ambiguous_sites_fail_closed(self):
        mutation = {"id": "example", "before": "before", "after": "after"}
        for source in ("", "before before"):
            with self.subTest(source=source), self.assertRaisesRegex(ValueError, "missing or ambiguous"):
                mutate(source, mutation)

    def test_whitespace_flexible_sites_still_fail_closed(self):
        mutation = {
            "id": "flexible-example", "before": "before site", "after": "after",
            "whitespace_flexible": True,
        }
        self.assertEqual(mutate("before\n    site", mutation), "after")
        for source in ("", "before site before\nsite"):
            with self.subTest(source=source), self.assertRaises(ValueError):
                mutate(source, mutation)

    def test_mutation_filters_name_real_nonignored_tests(self):
        tests = inventory_tests(TOOLS.parent)
        for mutation in MUTATIONS:
            identity = (mutation["target"], mutation["test"])
            self.assertIn(identity, tests)
            self.assertFalse(tests[identity]["ignored"])

    def test_all_catalog_cases_require_exact_executable_test_references(self):
        tests = inventory_tests(TOOLS.parent)
        structured = 0
        for kind in ("negative", "metamorphic"):
            document = load_json(TOOLS.parent / "validation" / kind / "cases.json")
            validate(document, "test_catalog")
            for case in document["cases"]:
                self.assertTrue(case["tests"], case["id"])
                structured += 1
                for reference in case["tests"]:
                    identity = (reference["target"], reference["name"])
                    self.assertIn(identity, tests, case["id"])
                    self.assertFalse(tests[identity]["ignored"], case["id"])
        self.assertEqual(structured, 88)


if __name__ == "__main__":
    unittest.main()

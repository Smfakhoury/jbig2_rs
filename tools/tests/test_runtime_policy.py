import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


TOOLS = Path(__file__).resolve().parents[1]


class RuntimeProofPolicyTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        for directory in ("src", "specs", "proofs", "tools"):
            (self.root / directory).mkdir()
        (self.root / "src/lib.rs").write_text("fn example() {}\n")
        for name in ("audit_runtime.py", "rust_source.py", "evidence.py", "evidence.schema.json"):
            shutil.copyfile(TOOLS / name, self.root / "tools" / name)

    def run_audit(self):
        return subprocess.run(
            [sys.executable, "-B", str(self.root / "tools/audit_runtime.py")],
            cwd=self.root, capture_output=True, text=True, check=False,
        )

    def assert_rejected_everywhere(self, source, expected):
        for directory in ("src", "specs", "proofs"):
            with self.subTest(directory=directory):
                path = self.root / directory / "policy.rs"
                path.write_text(source)
                try:
                    result = self.run_audit()
                    self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
                    self.assertIn(expected, result.stderr)
                    self.assertIn(f"{path.relative_to(self.root)}:", result.stderr)
                finally:
                    path.unlink()

    def test_function_module_and_crate_termination_waivers_are_rejected(self):
        for source in (
            "#![verifier::exec_allows_no_decreases_clause]\n",
            "#[verifier::exec_allows_no_decreases_clause] fn example() {}\n",
            "mod nested { #![verifier::exec_allows_no_decreases_clause] }\n",
            "#![cfg_attr(verus_keep_ghost, verifier::exec_allows_no_decreases_clause)]\n",
            "#[verifier::assume_termination] fn example() {}\n",
        ):
            with self.subTest(source=source):
                self.assert_rejected_everywhere(source, "termination waiver")

    def test_all_unreviewed_specification_entrypoints_are_rejected(self):
        for source in (
            "verus! { #[verifier::external_fn_specification] fn example() {} }\n",
            "verus! { #[verifier::external_type_specification] struct Example(u64); }\n",
            "verus! { #[verifier::external_trait_specification] trait Example {} }\n",
            "verus! { pub assume_specification [std::convert::identity]<T>(x: T) -> T; }\n",
            "#[cfg_attr(verus_keep_ghost, verifier::external_fn_specification)] fn example() {}\n",
        ):
            with self.subTest(source=source):
                self.assert_rejected_everywhere(source, "unreviewed Verus specification")

    def test_existing_external_body_and_assumption_bans_remain(self):
        for source, expected in (
            ("#[verifier::external_body] fn example() {}\n", "external body"),
            ("#[verifier::external] fn example() {}\n", "verification exclusion"),
            ("verus! { proof fn example() { assume(false); } }\n", "Verus assumption"),
            ("verus! { proof fn example() { admit(); } }\n", "Verus assumption"),
            ("verus! { proof fn example() { admit!(); } }\n", "Verus assumption"),
        ):
            with self.subTest(source=source):
                self.assert_rejected_everywhere(source, expected)

    def test_comments_strings_and_test_only_items_do_not_trigger_policy(self):
        source = '''
            // exec_allows_no_decreases_clause external_fn_specification
            fn example() {
                let _ = "assume_specification assume(false) external_body";
                let _ = r#"exec_allows_no_decreases_clause admit()"#;
            }
            #[cfg(test)]
            mod tests {
                #![verifier::exec_allows_no_decreases_clause]
                fn fixture() {}
            }
        '''
        (self.root / "src/lib.rs").write_text(source)
        result = self.run_audit()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("runtime audit passed", result.stdout)

    def test_missing_crate_root_cannot_pass_vacuously(self):
        (self.root / "src/lib.rs").unlink()
        result = self.run_audit()
        self.assertEqual(result.returncode, 1)
        self.assertIn("missing production crate root", result.stderr)

    def test_exact_source_line_budget_is_not_a_test_or_proof_exemption(self):
        for directory in ("src", "specs", "proofs", "tests", "examples", "verification"):
            with self.subTest(directory=directory):
                (self.root / directory).mkdir(exist_ok=True)
                path = self.root / directory / "line_budget.rs"
                path.write_text("// source line\n" * 800)
                result = self.run_audit()
                self.assertEqual(result.returncode, 0, result.stderr)
                path.write_text("// source line\n" * 801)
                result = self.run_audit()
                self.assertEqual(result.returncode, 1, result.stdout)
                self.assertIn("800-line source budget", result.stderr)
                path.unlink()

    def reviewed_interface(self):
        dependency = self.root / "verification/stdlib/vec-capacity-interface"
        dependency.mkdir(parents=True)
        source = dependency / "lib.rs"
        source.write_text(
            "verus! {\n"
            "pub assume_specification<T, A: Allocator>[ Vec::<T, A>::capacity ]"
            "(value: &Vec<T, A>) -> usize;\n}\n"
        )
        manifest = dependency / "Cargo.toml"
        manifest.write_text(
            '[package]\nname="jbig2_capacity_interface"\npublish=false\nbuild=false\n'
            '[package.metadata.verus]\nverify=true\n[lib]\npath="lib.rs"\n'
        )
        (self.root / "verification/Cargo.toml").write_text(
            '[dependencies]\njbig2_capacity_interface={path="stdlib/vec-capacity-interface"}\n'
        )
        review = {
            "schema_version": 1, "review_kind": "delegated_engineering_review",
            "package": "jbig2_capacity_interface",
            "manifest": "verification/stdlib/vec-capacity-interface/Cargo.toml",
            "source": "verification/stdlib/vec-capacity-interface/lib.rs",
            "manifest_sha256": hashlib.sha256(manifest.read_bytes()).hexdigest(),
            "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
            "target": "std::vec::Vec<T, A>::capacity", "requires": [], "ensures": [],
            "native_runtime_dependency": False, "normative_approval": False,
            "decision_reference": "https://github.com/microsoft/jbig2_rs/issues/116#issuecomment-1",
            "review": "The signature-only getter is a separately reviewed proof interface. "
                      "No capacity, length, reservation, allocation or normative facts are asserted.",
        }
        path = self.root / "verification/stdlib/review.json"
        path.write_text(json.dumps(review))
        return source, path, review

    def test_exact_reviewed_getter_is_separate_from_native_and_normative_trust(self):
        self.reviewed_interface()
        result = self.run_audit()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("reviewed proof-only standard interfaces: 1", result.stdout)
        self.assertIn("normative approval: false", result.stdout)

    def test_unreviewed_or_stale_standard_interface_cannot_pass(self):
        source, path, review = self.reviewed_interface()
        original = source.read_text()
        for mutation in (
            original.replace("-> usize;", "-> usize ensures true;"),
            original + "\nverus! { proof fn example() { assume(false); } }\n",
            original + "\nverus! { pub assume_specification [other]() -> usize; }\n",
            original + "\nunsafe fn example() {}\n",
        ):
            source.write_text(mutation)
            result = self.run_audit()
            self.assertEqual(result.returncode, 1, result.stdout)
            self.assertIn("stale standard-interface review", result.stderr)
            review["source_sha256"] = hashlib.sha256(source.read_bytes()).hexdigest()
            path.write_text(json.dumps(review))
            result = self.run_audit()
            self.assertEqual(result.returncode, 1, result.stdout)
            self.assertRegex(
                result.stderr,
                "exact unconstrained capacity|Verus assumption|unreviewed Verus specification|unsafe Rust",
            )
        source.write_text(original)
        path.unlink()
        result = self.run_audit()
        self.assertEqual(result.returncode, 1)
        self.assertIn("unreviewed Verus specification", result.stderr)

    def test_reviewed_getter_does_not_allow_another_standard_file_or_changed_graph(self):
        _, _, _ = self.reviewed_interface()
        other = self.root / "verification/stdlib/other.rs"
        other.write_text("verus! { pub assume_specification [other]() -> usize; }\n")
        result = self.run_audit()
        self.assertEqual(result.returncode, 1)
        self.assertIn(str(other.relative_to(self.root)) + ":", result.stderr)
        other.unlink()
        (self.root / "verification/Cargo.toml").write_text("[dependencies]\n")
        result = self.run_audit()
        self.assertEqual(result.returncode, 1)
        self.assertIn("exact proof-only dependency", result.stderr)

    def test_review_cannot_grant_value_facts_native_dependency_or_normative_approval(self):
        _, path, review = self.reviewed_interface()
        for field, value in (
            ("requires", ["value.len() <= value.capacity()"]),
            ("ensures", ["out == value.len()"]),
            ("native_runtime_dependency", True),
            ("normative_approval", True),
            ("review_kind", "named_human_approval"),
        ):
            with self.subTest(field=field):
                path.write_text(json.dumps({**review, field: value}))
                result = self.run_audit()
                self.assertEqual(result.returncode, 1, result.stdout)
                self.assertIn("verification/stdlib/review.json:", result.stderr)


if __name__ == "__main__":
    unittest.main()
import hashlib
import json

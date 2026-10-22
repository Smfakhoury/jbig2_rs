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
        for name in ("audit_runtime.py", "rust_source.py"):
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
                result = self.run_audit()
                self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
                self.assertIn(expected, result.stderr)
                self.assertIn(f"{directory}/policy.rs:", result.stderr)
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


if __name__ == "__main__":
    unittest.main()

import hashlib
from pathlib import Path
import shutil
import sys
import tempfile
import unittest


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from proof_sensitivity import replace_once, weakened_contract
from source_group import SourceGroup, bound_source_groups, relocated_target


class SourceGroupTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / "source"
        self.copy = Path(self.temporary.name) / "copy"
        (self.root / "src/codec").mkdir(parents=True)
        (self.root / "src/lib.rs").write_text("mod codec;\n")
        (self.root / "src/codec.rs").write_text(
            "mod data;\nmod run;\n#[cfg(test)] mod tests;\n"
        )
        (self.root / "src/codec/data.rs").write_bytes(b"pub const WIDTH: u8 = 1;\r\n")
        (self.root / "src/codec/run.rs").write_text("pub fn work() -> u8 { 2 }\n")
        (self.root / "src/codec/tests.rs").write_text("#[test] fn unit_case() {}\n")
        (self.root / "src/codec/orphan.rs").write_text("pub fn unreachable() {}\n")
        shutil.copytree(self.root, self.copy)

    def test_group_follows_actual_module_graph_including_data_only_files(self):
        group = SourceGroup(self.root, "src/codec.rs")
        self.assertEqual(set(group.names), {
            "src/codec.rs", "src/codec/data.rs", "src/codec/run.rs",
        })
        self.assertNotIn("unit_case", group.text)
        self.assertNotIn("unreachable", group.text)

    def test_mutation_and_weaker_guarantee_can_be_in_different_files(self):
        group = SourceGroup(self.root, "src/codec.rs")
        mutant = replace_once(group.text, "fn work() -> u8 { 2 }", "fn work() -> u8 { 3 }")
        group.write(self.copy, mutant)
        self.assertIn("{ 3 }", (self.copy / "src/codec/run.rs").read_text())
        self.assertEqual(
            (self.copy / "src/codec/data.rs").read_bytes(),
            (self.root / "src/codec/data.rs").read_bytes(),
        )
        control = weakened_contract(mutant, {"controls": (("WIDTH: u8 = 1", "WIDTH: u8 = 4"),)})
        group.write(self.copy, control)
        self.assertIn("= 4", (self.copy / "src/codec/data.rs").read_text())
        self.assertIn("{ 3 }", (self.copy / "src/codec/run.rs").read_text())
        group.restore(self.copy)
        for name, original in group.original.items():
            self.assertEqual((self.copy / name).read_bytes(), original)

    def test_changed_boundaries_fail_before_any_source_write(self):
        group = SourceGroup(self.root, "src/codec.rs")
        before = {name: (self.copy / name).read_bytes() for name in group.names}
        for changed in (
            group.text.replace(group.markers[1], "", 1),
            group.text + group.markers[0],
            group.text.replace(group.markers[0], group.markers[-1], 1),
        ):
            with self.subTest(changed=changed), self.assertRaisesRegex(ValueError, "boundar"):
                group.write(self.copy, changed)
            self.assertEqual({name: (self.copy / name).read_bytes() for name in group.names}, before)

    def test_source_hashes_bind_each_raw_file_and_preserve_line_endings(self):
        groups, sources, _ = bound_source_groups(self.root, ("src/codec.rs",))
        self.assertEqual(sources["src/codec/data.rs"], b"pub const WIDTH: u8 = 1;\r\n")
        for name, value in sources.items():
            self.assertEqual(hashlib.sha256(value).digest(), hashlib.sha256((self.root / name).read_bytes()).digest())
        groups["src/codec.rs"].write(self.copy, groups["src/codec.rs"].text)
        self.assertEqual((self.copy / "src/codec/data.rs").read_bytes(), sources["src/codec/data.rs"])

    def test_relocated_native_target_is_resolved_and_ambiguity_fails(self):
        group = SourceGroup(self.root, "src/codec.rs")
        self.assertEqual(
            relocated_target(self.root, "codec", "work", group.names),
            ("codec::run", "work"),
        )
        path = self.root / "src/codec/data.rs"
        path.write_text(path.read_text() + "pub fn work() -> u8 { 4 }\n")
        with self.assertRaisesRegex(ValueError, "ambiguous"):
            relocated_target(self.root, "codec", "work", group.names)

    def test_standalone_proof_input_needs_no_runtime_module_graph(self):
        proof = self.root / "fixture.rs"
        proof.write_text("first second contract")
        group = SourceGroup(self.root, "fixture.rs")
        self.assertEqual(group.text, "first second contract")
        self.assertEqual(group.names, ("fixture.rs",))
        with self.assertRaisesRegex(ValueError, "invalid"):
            SourceGroup(self.root, "../outside.rs")

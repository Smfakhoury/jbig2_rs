import copy
import io
import json
import os
import subprocess
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from audit_package import REQUIRED
from check_distribution import (
    check_package_metadata, check_unsafe_policy, extract_archive, isolated_environment,
    require_rejected_check,
)


class DistributionTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.archive = self.directory / "package.crate"
        self.package_root = "jbig2_rs-0.0.1"

    def write_archive(self, paths=None, extra=None):
        paths = paths if paths is not None else sorted(REQUIRED)
        with tarfile.open(self.archive, "w:gz") as archive:
            for path in paths:
                data = b"generated archive fixture"
                entry = tarfile.TarInfo(f"{self.package_root}/{path}")
                entry.size = len(data)
                archive.addfile(entry, io.BytesIO(data))
            if extra is not None:
                archive.addfile(extra)

    def test_extracts_only_audited_regular_files(self):
        self.write_archive()
        output = self.directory / "extracted"
        names = extract_archive(self.archive, output, self.package_root)
        self.assertEqual(len(names), len(REQUIRED))
        for path in REQUIRED:
            self.assertEqual(
                (output / self.package_root / path).read_bytes(), b"generated archive fixture",
            )

    def test_rejects_missing_license_and_proof_assets(self):
        for paths in (
            sorted(REQUIRED - {"LICENSE"}),
            sorted(REQUIRED) + ["proofs/included.rs"],
            [],
        ):
            with self.subTest(paths=paths):
                self.write_archive(paths)
                with self.assertRaises(ValueError):
                    extract_archive(self.archive, self.directory / "bad", self.package_root)
                self.assertFalse((self.directory / "bad").exists())

    def test_rejects_traversal_wrong_root_and_duplicate_entries(self):
        for extra in (
            tarfile.TarInfo(f"{self.package_root}/../outside"),
            tarfile.TarInfo("different-0.0.1/LICENSE"),
            tarfile.TarInfo(f"{self.package_root}/LICENSE"),
        ):
            with self.subTest(name=extra.name):
                self.write_archive(extra=extra)
                with self.assertRaises(ValueError):
                    extract_archive(self.archive, self.directory / "bad", self.package_root)
                self.assertFalse((self.directory / "bad").exists())

    def test_rejects_links_before_writing_anything(self):
        for kind in (tarfile.SYMTYPE, tarfile.LNKTYPE):
            link = tarfile.TarInfo(f"{self.package_root}/LICENSE")
            link.type = kind
            link.linkname = "../outside"
            self.write_archive(sorted(REQUIRED - {"LICENSE"}), link)
            with self.assertRaisesRegex(ValueError, "invalid archive member"):
                extract_archive(self.archive, self.directory / "bad", self.package_root)
            self.assertFalse((self.directory / "bad").exists())

    def test_removes_proof_flags_and_tool_overrides_from_environment(self):
        overrides = {
            "RUSTFLAGS": "--cfg verus_keep_ghost",
            "RUSTDOCFLAGS": "--cfg jbig2_validation",
            "CARGO_ENCODED_RUSTFLAGS": "--cfg=fuzzing",
            "RUSTC": "verus",
            "RUSTUP_TOOLCHAIN": "nightly",
            "RUSTC_WRAPPER": "wrapper",
            "CARGO_BUILD_RUSTC": "verus",
            "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS": "--cfg=fuzzing",
            "CARGO_BUILD_TARGET": "other-platform",
            "CARGO_HOME": "original-cargo-home",
        }
        with patch.dict(os.environ, overrides):
            environment = isolated_environment(self.directory)
        for key in overrides.keys() - {"CARGO_HOME"}:
            self.assertNotIn(key, environment)
        self.assertEqual(environment["CARGO_HOME"], str(self.directory / "cargo-home"))
        self.assertEqual(environment["CARGO_NET_OFFLINE"], "true")

    def test_metadata_rejects_proof_dependencies_or_features(self):
        extracted = self.directory / self.package_root
        metadata = {"packages": [{
            "name": "jbig2_rs", "license": "MIT", "rust_version": "1.89.0",
            "dependencies": [], "features": {}, "publish": [],
            "manifest_path": str(extracted / "Cargo.toml"),
        }]}
        check_package_metadata(metadata, extracted, "1.89.0")
        for change in (
            {"dependencies": [{"name": "vstd"}]}, {"features": {"verus": []}},
            {"license": None}, {"rust_version": "1.88.0"}, {"publish": None},
            {"manifest_path": str(self.directory / "checkout" / "Cargo.toml")},
        ):
            broken = copy.deepcopy(metadata)
            broken["packages"][0].update(change)
            with self.subTest(change=change), self.assertRaises(ValueError):
                check_package_metadata(broken, extracted, "1.89.0")

    def test_rejected_check_requires_failure_and_the_exact_compiler_diagnostic(self):
        diagnostic = {
            "reason": "compiler-message",
            "message": {"code": {"code": "unsafe_code"}, "message": "usage of an `unsafe` block"},
        }
        for exit_code, entries, accepted in (
            (1, [diagnostic], True),
            (0, [diagnostic], False),
            (1, [], False),
            (1, [{"reason": "build-finished"}], False),
            (1, [{**diagnostic, "message": {
                "code": {"code": "E0432"}, "message": "unrelated failure",
            }}], False),
        ):
            process = subprocess.CompletedProcess(
                [], exit_code, "\n".join(json.dumps(entry) for entry in entries), "",
            )
            with self.subTest(exit_code=exit_code, entries=entries), patch(
                "check_distribution.subprocess.run", return_value=process,
            ):
                if accepted:
                    require_rejected_check(
                        ["cargo"], self.directory, {}, "unsafe_code", "usage of an `unsafe` block",
                    )
                else:
                    with self.assertRaisesRegex(ValueError, "expected compilation rejection"):
                        require_rejected_check(
                            ["cargo"], self.directory, {}, "unsafe_code", "usage of an `unsafe` block",
                        )

    def test_unsafe_policy_uses_the_real_guard_then_a_weaker_control_and_restores_source(self):
        library = self.directory / "src" / "lib.rs"
        library.parent.mkdir()
        original = b"#![forbid(unsafe_code)]\npub fn example() {}\n"
        library.write_bytes(original)

        def reject(*_arguments):
            self.assertIn(b"#![forbid(unsafe_code)]", library.read_bytes())
            self.assertIn(b"unsafe {}", library.read_bytes())

        def accept(*_arguments, **_keywords):
            self.assertNotIn(b"#![forbid(unsafe_code)]", library.read_bytes())
            self.assertIn(b"#![allow(unsafe_code)]", library.read_bytes())
            self.assertIn(b"unsafe {}", library.read_bytes())

        with patch("check_distribution.require_rejected_check", side_effect=reject) as strong, patch(
            "check_distribution.run", side_effect=accept,
        ) as weaker:
            check_unsafe_policy(["cargo"], self.directory, {})
        strong.assert_called_once()
        weaker.assert_called_once()
        self.assertEqual(library.read_bytes(), original)

    def test_unsafe_policy_restores_source_when_either_control_fails(self):
        library = self.directory / "src" / "lib.rs"
        library.parent.mkdir()
        original = b"#![forbid(unsafe_code)]\n"
        for failed in ("strong", "weaker"):
            library.write_bytes(original)
            with self.subTest(failed=failed), patch(
                "check_distribution.require_rejected_check",
                side_effect=ValueError("strong failure") if failed == "strong" else None,
            ), patch(
                "check_distribution.run",
                side_effect=subprocess.CalledProcessError(1, ["cargo"])
                if failed == "weaker" else None,
            ), self.assertRaises((ValueError, subprocess.CalledProcessError)):
                check_unsafe_policy(["cargo"], self.directory, {})
            self.assertEqual(library.read_bytes(), original)

    def test_unsafe_policy_rejects_missing_or_ambiguous_guards_before_compiling(self):
        library = self.directory / "src" / "lib.rs"
        library.parent.mkdir()
        for original in (b"pub fn example() {}", b"#![forbid(unsafe_code)]\n" * 2):
            library.write_bytes(original)
            with self.subTest(original=original), patch(
                "check_distribution.require_rejected_check",
            ) as strong, patch("check_distribution.run") as weaker, self.assertRaisesRegex(
                ValueError, "one explicit unsafe-code prohibition",
            ):
                check_unsafe_policy(["cargo"], self.directory, {})
            strong.assert_not_called()
            weaker.assert_not_called()
            self.assertEqual(library.read_bytes(), original)


if __name__ == "__main__":
    unittest.main()

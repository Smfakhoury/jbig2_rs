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

from audit_package import REQUIRED, audit
from check_distribution import (
    check_package_metadata, check_publish_dry_run, check_unsafe_policy, extract_archive, isolated_environment,
    require_rejected_check,
)


class DistributionTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.archive = self.directory / "package.crate"
        self.package_root = "jbig2-decoder-0.0.1"

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

    def test_old_package_root_is_not_accepted_as_the_requested_public_crate(self):
        self.assertTrue(audit([f"jbig2_rs-0.0.1/{path}" for path in sorted(REQUIRED)]))
        self.assertFalse(audit([f"jbig2-decoder-0.0.1/{path}" for path in sorted(REQUIRED)]))

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
            "CARGO_REGISTRY_TOKEN": "fixture-token",
            "CARGO_REGISTRIES_CRATES_IO_TOKEN": "fixture-token",
            "CARGO_REGISTRIES_CRATES_IO_INDEX": "https://example.com/untrusted",
            "CARGO_REGISTRY_GLOBAL_CREDENTIAL_PROVIDERS": "fixture-provider",
        }
        with patch.dict(os.environ, overrides):
            environment = isolated_environment(self.directory)
        for key in overrides.keys() - {"CARGO_HOME"}:
            self.assertNotIn(key, environment)
        self.assertEqual(environment["CARGO_HOME"], str(self.directory / "cargo-home"))
        self.assertEqual(environment["CARGO_NET_OFFLINE"], "true")

    def publish_fixture(self):
        manifest = self.directory / "Cargo.toml"
        manifest.write_text(
            '[package]\nname = "jbig2-decoder"\nversion = "0.0.1"\npublish = false\n'
        )
        for path in REQUIRED - {"Cargo.toml"}:
            target = self.directory / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(b"first-party archive source fixture")
        return manifest

    def fake_publish(self, manifest, archive, *, corrupt=None):
        def dry_run(command, **keywords):
            self.assertEqual(command, [
                "cargo", "+1.88.0", "publish", "--dry-run", "--locked", "--registry", "crates-io",
            ])
            self.assertEqual(keywords["cwd"], self.directory)
            self.assertEqual(keywords["timeout"], 300)
            self.assertEqual(keywords["env"]["CARGO_NET_OFFLINE"], "false")
            self.assertIn(b'publish = ["crates-io"]', manifest.read_bytes())
            with tarfile.open(archive, "w:gz") as packed:
                for path in sorted(REQUIRED):
                    source = self.directory / path
                    info = tarfile.TarInfo(f"{self.package_root}/{path}")
                    data = source.read_bytes()
                    if corrupt == "source" and path == "src/lib.rs":
                        data += b"\n// changed runtime\n"
                    if corrupt == "manifest" and path == "Cargo.toml":
                        data = data.replace(b'version = "0.0.1"', b'version = "9.0.0"')
                    info.size = len(data)
                    packed.addfile(info, io.BytesIO(data))
            return subprocess.CompletedProcess(command, 0, "", "warning: aborting upload due to dry run\n")
        return dry_run

    def test_publication_rehearsal_is_dry_run_only_and_restores_manifest(self):
        manifest = self.publish_fixture()
        original = manifest.read_bytes()
        environment = isolated_environment(self.directory)
        archive = Path(environment["CARGO_TARGET_DIR"]) / "package" / f"{self.package_root}.crate"
        archive.parent.mkdir(parents=True)
        with patch(
            "check_distribution.subprocess.run",
            side_effect=self.fake_publish(manifest, archive),
        ):
            report = check_publish_dry_run(["cargo", "+1.88.0"], self.directory, environment)
        self.assertEqual(report["status"], "passed")
        self.assertFalse(report["uploaded"])
        self.assertTrue(report["source_bytes_match_distribution"])
        self.assertTrue(report["manifest_restored"])
        self.assertEqual(manifest.read_bytes(), original)
        self.assertEqual(environment["CARGO_NET_OFFLINE"], "true")

    def test_publication_rehearsal_rejects_repackaged_source_and_manifest_changes(self):
        manifest = self.publish_fixture()
        original = manifest.read_bytes()
        environment = isolated_environment(self.directory)
        archive = Path(environment["CARGO_TARGET_DIR"]) / "package" / f"{self.package_root}.crate"
        archive.parent.mkdir(parents=True)
        for corruption in ("source", "manifest"):
            with (
                self.subTest(corruption=corruption),
                patch("check_distribution.subprocess.run",
                      side_effect=self.fake_publish(manifest, archive, corrupt=corruption)),
                self.assertRaisesRegex(ValueError, "differs"),
            ):
                check_publish_dry_run(["cargo", "+1.88.0"], self.directory, environment)
            self.assertEqual(manifest.read_bytes(), original)

    def test_publication_rehearsal_restores_manifest_after_failed_commands_or_archive_checks(self):
        manifest = self.publish_fixture()
        original = manifest.read_bytes()
        for failure in ("cargo", "timeout", "missing_confirmation", "missing_archive"):
            result = subprocess.CompletedProcess([], 101 if failure == "cargo" else 0, "", "")
            if failure == "missing_archive":
                result.stderr = "warning: aborting upload due to dry run"
            with (
                self.subTest(failure=failure),
                patch("check_distribution.subprocess.run",
                      side_effect=subprocess.TimeoutExpired(["cargo"], 300)
                      if failure == "timeout" else None, return_value=result),
                self.assertRaises((ValueError, OSError, subprocess.SubprocessError)),
            ):
                check_publish_dry_run(["cargo"], self.directory, isolated_environment(self.directory))
            self.assertEqual(manifest.read_bytes(), original)

    def test_publication_rehearsal_rejects_checkout_or_enabled_or_ambiguous_manifest(self):
        manifest = self.publish_fixture()
        original = manifest.read_bytes()
        with patch("check_distribution.ROOT", self.directory), self.assertRaisesRegex(
            ValueError, "outside the source",
        ):
            check_publish_dry_run(["cargo"], self.directory, {})
        for data in (
            original.replace(b"publish = false", b"publish = true"),
            original + b'\n[unrelated]\npublish = false\n',
        ):
            manifest.write_bytes(data)
            with patch("check_distribution.subprocess.run") as command, self.assertRaises(ValueError):
                check_publish_dry_run(["cargo"], self.directory, {})
            command.assert_not_called()
            self.assertEqual(manifest.read_bytes(), data)

    def test_metadata_rejects_proof_dependencies_or_features(self):
        extracted = self.directory / self.package_root
        metadata = {"packages": [{
            "name": "jbig2-decoder", "license": "MIT", "rust_version": "1.89.0",
            "dependencies": [], "features": {}, "publish": [],
            "manifest_path": str(extracted / "Cargo.toml"),
            "targets": [{"kind": ["lib"], "name": "jbig2_decoder"}],
        }]}
        check_package_metadata(metadata, extracted, "1.89.0")
        for change in (
            {"dependencies": [{"name": "vstd"}]}, {"features": {"verus": []}},
            {"license": None}, {"rust_version": "1.88.0"}, {"publish": None},
            {"manifest_path": str(self.directory / "checkout" / "Cargo.toml")},
            {"name": "jbig2_rs"}, {"targets": None},
            {"targets": [{"kind": ["lib"], "name": "jbig2_rs"}]},
            {"targets": []},
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

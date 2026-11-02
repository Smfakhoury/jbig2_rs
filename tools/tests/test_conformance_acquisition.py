import contextlib
import copy
import hashlib
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch
import zipfile


ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "validation/conformance"))
import fetch_itu


def zipped(entries):
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w") as archive:
        for name, data in entries:
            archive.writestr(name, data)
    return buffer.getvalue()


def digest(data):
    return hashlib.sha256(data).hexdigest()


class ConformanceAcquisitionTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.output = self.root / "vectors"
        self.entries = [
            ("generated/case.jb2", b"first-party acquisition fixture"),
            ("generated/case_TT00.bmp", b"first-party expected bitmap fixture"),
        ]
        self.manifest = {
            "files": [
                {"path": name, "size_bytes": len(data), "sha256": digest(data),
                 "kind": "encoded_stream" if name.endswith(".jb2") else "expected_bitmap"}
                for name, data in self.entries
            ],
        }
        self.package = self.package_for(self.entries)

    def package_for(self, entries, extras=()):
        archive = zipped(entries)
        self.manifest["container_sha256"] = digest(archive)
        return zipped([(fetch_itu.ARCHIVE_MEMBER, archive), *extras])

    def extract(self, package=None, manifest=None):
        package = self.package if package is None else package
        return fetch_itu.extract_vectors(
            package, digest(package), self.manifest if manifest is None else manifest,
            self.output,
        )

    def test_extracts_only_the_exact_manifest_not_other_package_members(self):
        package = self.package_for(self.entries, [("unapproved.txt", b"must not be extracted")])
        self.assertEqual(self.extract(package), 2)
        self.assertEqual(
            sorted(path.relative_to(self.output).as_posix()
                   for path in self.output.rglob("*") if path.is_file()),
            sorted(name for name, _ in self.entries),
        )
        for name, data in self.entries:
            self.assertEqual((self.output / name).read_bytes(), data)

    def test_both_package_and_inner_archive_hashes_are_mandatory(self):
        with self.assertRaisesRegex(ValueError, "package hash"):
            fetch_itu.extract_vectors(self.package, "0" * 64, self.manifest, self.output)
        self.manifest["container_sha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "archive hash"):
            self.extract()
        self.assertFalse(self.output.exists())

    def test_missing_duplicate_and_oversized_inner_member_fail(self):
        archive = zipped(self.entries)
        packages = [
            zipped([("wrong.zip", archive)]),
            zipped([(fetch_itu.ARCHIVE_MEMBER, archive),
                    (fetch_itu.ARCHIVE_MEMBER, archive)]),
        ]
        for package in packages:
            with self.subTest(package=digest(package)), self.assertRaises(ValueError):
                self.extract(package)
        with patch.object(fetch_itu, "MAX_ARCHIVE_BYTES", 1), self.assertRaises(ValueError):
            self.extract()

    def test_missing_extra_duplicate_and_traversal_archive_members_fail(self):
        for entries in (
            self.entries[:1], self.entries + [("generated/extra.jb2", b"extra")],
            self.entries + [self.entries[0]],
            self.entries + [("../outside.txt", b"escape")],
        ):
            with self.subTest(entries=entries), self.assertRaises(ValueError):
                self.extract(self.package_for(entries))
            self.assertFalse(self.output.exists())
            self.assertFalse((self.root / "outside.txt").exists())

    def test_invalid_manifest_paths_and_empty_or_duplicate_inventory_fail(self):
        original = copy.deepcopy(self.manifest)
        for name in ("../escape.jb2", "/generated/case.jb2", "case.jb2", "a\\b.jb2"):
            manifest = copy.deepcopy(original)
            manifest["files"][0]["path"] = name
            with self.subTest(name=name), self.assertRaises(ValueError):
                self.extract(manifest=manifest)
        for files in ([], original["files"] + [original["files"][0]]):
            manifest = copy.deepcopy(original)
            manifest["files"] = files
            with self.subTest(files=files), self.assertRaises(ValueError):
                self.extract(manifest=manifest)

    def test_file_size_hash_and_symlink_are_checked_without_partial_output(self):
        original = copy.deepcopy(self.manifest)
        for field, value in (("size_bytes", 0), ("size_bytes", True), ("sha256", "0" * 64)):
            manifest = copy.deepcopy(original)
            manifest["files"][1][field] = value
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                self.extract(manifest=manifest)
            self.assertFalse(self.output.exists())
        link = zipfile.ZipInfo(self.entries[0][0])
        link.create_system = 3
        link.external_attr = (0o120777 << 16)
        with self.assertRaisesRegex(ValueError, "link"):
            self.extract(self.package_for([(link, self.entries[0][1]), self.entries[1]]))
        self.assertFalse(self.output.exists())

    def test_existing_output_is_never_replaced(self):
        self.output.mkdir()
        marker = self.output / "preserve.txt"
        marker.write_bytes(b"existing data")
        with self.assertRaisesRegex(ValueError, "new directory"):
            self.extract()
        self.assertEqual(marker.read_bytes(), b"existing data")

    def test_bounded_read_rejects_larger_data(self):
        self.assertEqual(fetch_itu.bounded_read(io.BytesIO(b"abc"), 3), b"abc")
        with self.assertRaisesRegex(ValueError, "acquisition bound"):
            fetch_itu.bounded_read(io.BytesIO(b"abcd"), 3)

    def test_cli_fails_closed_for_wrong_host_or_unapproved_execution(self):
        (self.root / "validation/conformance").mkdir(parents=True)
        (self.root / "validation/conformance/itu-t88-2018-manifest.json").write_text(
            json.dumps(self.manifest)
        )
        provenance = {
            "approvals": {"conformance_vector_use": "approved for internal/private-CI execution"},
            "normative_documents": [{"id": "ITU-T T.88 (08/2018)",
                                    "package_sha256": digest(self.package)}],
        }
        policy = self.root / "validation/source-provenance.json"
        policy.write_text(json.dumps(provenance))
        response = io.BytesIO(self.package)
        response.status = 200
        response.url = "https://example.com/not-the-approved-host"
        arguments = ["fetch_itu.py", "--download", "--output", str(self.root / "target/vectors")]
        with (
            patch.object(fetch_itu, "ROOT", self.root),
            patch.object(sys, "argv", arguments),
            patch.object(fetch_itu.urllib.request, "urlopen", return_value=response) as download,
            contextlib.redirect_stderr(io.StringIO()) as errors,
        ):
            self.assertEqual(fetch_itu.main(), 1)
            self.assertIn("official HTTPS host", errors.getvalue())
            download.assert_called_once()
            provenance["approvals"]["conformance_vector_use"] = "not approved"
            policy.write_text(json.dumps(provenance))
            self.assertEqual(fetch_itu.main(), 1)
            download.assert_called_once()

    def test_cli_rejects_external_output_without_downloading(self):
        with (
            patch.object(fetch_itu, "ROOT", self.root),
            patch.object(sys, "argv", ["fetch_itu.py", "--download", "--output", str(self.output)]),
            patch.object(fetch_itu.urllib.request, "urlopen") as download,
            contextlib.redirect_stderr(io.StringIO()) as errors,
        ):
            self.assertEqual(fetch_itu.main(), 1)
            self.assertIn("under target", errors.getvalue())
            download.assert_not_called()

    def test_cli_uses_approved_package_without_network_or_reference_extraction(self):
        (self.root / "validation/conformance").mkdir(parents=True)
        (self.root / "validation/conformance/itu-t88-2018-manifest.json").write_text(
            json.dumps(self.manifest)
        )
        (self.root / "validation/source-provenance.json").write_text(json.dumps({
            "approvals": {"conformance_vector_use": "approved for internal/private-CI execution"},
            "normative_documents": [{"id": "ITU-T T.88 (08/2018)",
                                    "package_sha256": digest(self.package)}],
        }))
        package = self.root / "approved.zip"
        package.write_bytes(self.package)
        with (
            patch.object(fetch_itu, "ROOT", self.root),
            patch.object(sys, "argv", ["fetch_itu.py", "--package", str(package),
                                      "--output", str(self.root / "target/vectors")]),
            patch.object(fetch_itu.urllib.request, "urlopen") as download,
            contextlib.redirect_stdout(io.StringIO()),
        ):
            self.assertEqual(fetch_itu.main(), 0)
            download.assert_not_called()
        self.assertEqual(
            (self.root / "target/vectors/generated/case.jb2").read_bytes(),
            self.entries[0][1],
        )


if __name__ == "__main__":
    unittest.main()

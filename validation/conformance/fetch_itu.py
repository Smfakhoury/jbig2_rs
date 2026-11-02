#!/usr/bin/env python3
"""Acquire only the approved conformance vectors, never the reference sources."""

import argparse
import hashlib
import io
import os
from pathlib import Path, PurePosixPath
import stat
import sys
import tempfile
import urllib.error
import urllib.request
import zipfile

from run_itu import read_json, validate_vectors


ROOT = Path(__file__).resolve().parents[2]
PACKAGE_URL = (
    "https://www.itu.int/rec/dologin_pub.asp?lang=e&"
    "id=T-REC-T.88-201808-I!!SOFT-ZST-E&type=items"
)
ARCHIVE_MEMBER = "Software/JBIG2_ConformanceData-A20180829.zip"
MAX_PACKAGE_BYTES = 16 * 1024 * 1024
MAX_ARCHIVE_BYTES = 2 * 1024 * 1024


def bounded_read(stream, maximum):
    data = stream.read(maximum + 1)
    if len(data) > maximum:
        raise ValueError(f"asset exceeds the {maximum}-byte acquisition bound")
    return data


def extract_vectors(package, package_sha256, manifest, output):
    if hashlib.sha256(package).hexdigest() != package_sha256:
        raise ValueError("official package hash mismatch")
    if output.exists() or output.is_symlink():
        raise ValueError("vector output must be a new directory")
    entries = manifest["files"]
    if not entries:
        raise ValueError("empty vector manifest")
    names = [entry["path"] for entry in entries]
    if len(names) != len(set(names)) or any(
        PurePosixPath(name).is_absolute()
        or ".." in PurePosixPath(name).parts
        or "\\" in name
        or len(PurePosixPath(name).parts) != 2
        for name in names
    ):
        raise ValueError("invalid or duplicate vector manifest paths")
    with zipfile.ZipFile(io.BytesIO(package)) as outer:
        matches = [entry for entry in outer.infolist() if entry.filename == ARCHIVE_MEMBER]
        if len(matches) != 1 or matches[0].file_size > MAX_ARCHIVE_BYTES:
            raise ValueError("missing, duplicate or oversized conformance archive member")
        with outer.open(matches[0]) as stream:
            archive = bounded_read(stream, MAX_ARCHIVE_BYTES)
    if hashlib.sha256(archive).hexdigest() != manifest["container_sha256"]:
        raise ValueError("conformance archive hash mismatch")
    with zipfile.ZipFile(io.BytesIO(archive)) as inner:
        files = [entry for entry in inner.infolist() if not entry.is_dir()]
        if len(files) != len(names) or {entry.filename for entry in files} != set(names):
            raise ValueError("conformance archive file set mismatch")
        by_name = {entry.filename: entry for entry in files}
        output.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="itu-vectors-", dir=output.parent) as temporary:
            staging = Path(temporary) / "vectors"
            staging.mkdir()
            for entry in entries:
                info = by_name[entry["path"]]
                size = entry["size_bytes"]
                if (
                    type(size) is not int or not 0 <= size <= MAX_PACKAGE_BYTES
                    or info.file_size != size
                    or stat.S_ISLNK(info.external_attr >> 16)
                ):
                    raise ValueError(f"invalid vector extent or link: {entry['path']}")
                with inner.open(info) as stream:
                    data = bounded_read(stream, size)
                if len(data) != size or hashlib.sha256(data).hexdigest() != entry["sha256"]:
                    raise ValueError(f"vector hash/size mismatch: {entry['path']}")
                target = staging / entry["path"]
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(data)
            validate_vectors(staging, manifest)
            os.replace(staging, output)
    return len(files)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--package", type=Path, help="existing approved official package")
    source.add_argument("--download", action="store_true", help="fetch the pinned official package")
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    try:
        output = arguments.output.absolute()
        target = ROOT / "target"
        if (
            target.is_symlink()
            or output.resolve() == target.resolve()
            or not output.resolve().is_relative_to(target.resolve())
        ):
            raise ValueError("vector acquisition output must be a new directory under target/")
        provenance = read_json(ROOT / "validation/source-provenance.json")
        if provenance["approvals"]["conformance_vector_use"] != "approved for internal/private-CI execution":
            raise ValueError("internal/private-CI conformance execution is not approved")
        identity = next(
            document for document in provenance["normative_documents"]
            if document["id"] == "ITU-T T.88 (08/2018)"
        )
        manifest = read_json(ROOT / "validation/conformance/itu-t88-2018-manifest.json")
        if arguments.download:
            request = urllib.request.Request(PACKAGE_URL, headers={
                "User-Agent": "jbig2-decoder-authorized-conformance",
            })
            with urllib.request.urlopen(request, timeout=60) as response:
                if response.status != 200 or not response.url.startswith("https://www.itu.int/"):
                    raise ValueError("asset download did not remain on the official HTTPS host")
                package = bounded_read(response, MAX_PACKAGE_BYTES)
        else:
            with arguments.package.open("rb") as stream:
                package = bounded_read(stream, MAX_PACKAGE_BYTES)
        count = extract_vectors(package, identity["package_sha256"], manifest, output)
        print(f"Verified and extracted {count} approved vector files; no other package members read.")
        return 0
    except (OSError, ValueError, KeyError, StopIteration, urllib.error.URLError, zipfile.BadZipFile) as error:
        print(f"official vector acquisition failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())

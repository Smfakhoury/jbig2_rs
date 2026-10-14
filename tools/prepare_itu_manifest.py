#!/usr/bin/env python3
import argparse
import hashlib
import json
import zipfile
from pathlib import Path


parser = argparse.ArgumentParser()
parser.add_argument("--zip", type=Path, required=True)
parser.add_argument("--output", type=Path, required=True)
arguments = parser.parse_args()

entries = []
with zipfile.ZipFile(arguments.zip) as archive:
    for info in sorted(archive.infolist(), key=lambda item: item.filename):
        if info.is_dir():
            continue
        data = archive.read(info)
        entries.append(
            {
                "path": info.filename,
                "sha256": hashlib.sha256(data).hexdigest(),
                "size_bytes": len(data),
                "kind": "encoded_stream"
                if info.filename.lower().endswith(".jb2")
                else "expected_bitmap",
                "license": "ITU T.88 electronic attachment; standalone redistribution terms not established",
                "redistribution": "prohibited pending public-repository legal review",
                "expected_use": "internal normative conformance validation",
            }
        )

document = {
    "schema_version": 1,
    "source_url": "https://www.itu.int/rec/T-REC-T.88-201808-I/en",
    "container": arguments.zip.name,
    "container_sha256": hashlib.sha256(arguments.zip.read_bytes()).hexdigest(),
    "approval": "CELA-approved for internal/private-CI execution on 2026-10-14",
    "redistribution": False,
    "files": entries,
}
arguments.output.parent.mkdir(parents=True, exist_ok=True)
arguments.output.write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")
print(f"wrote provenance for {len(entries)} files")

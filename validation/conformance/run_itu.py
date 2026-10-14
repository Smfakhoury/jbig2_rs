#!/usr/bin/env python3
import argparse
import hashlib
import json
import re
import shutil
import struct
import subprocess
import tempfile
from pathlib import Path


parser = argparse.ArgumentParser()
parser.add_argument("--vectors", type=Path, required=True)
parser.add_argument("--decoder", type=Path, required=True)
parser.add_argument("--output", type=Path, required=True)
parser.add_argument("--expected-differences", type=Path)
arguments = parser.parse_args()

expected_differences = {}
if arguments.expected_differences:
    document = json.loads(arguments.expected_differences.read_text(encoding="utf-8"))
    expected_differences = {entry["id"]: entry for entry in document["cases"]}


def parse_raw(path: Path) -> tuple[int, int, bytes]:
    data = path.read_bytes()
    if len(data) < 20 or data[:4] != b"JB2R":
        raise ValueError(f"invalid decoder output: {path}")
    _page, width, height, stride = struct.unpack(">IIII", data[4:20])
    expected = stride * height
    if len(data) != 20 + expected:
        raise ValueError(f"invalid decoder output length: {path}")
    return width, height, data[20:]


def parse_bmp(path: Path) -> tuple[int, int, bytes]:
    data = path.read_bytes()
    if len(data) < 54 or data[:2] != b"BM":
        raise ValueError(f"invalid BMP: {path}")
    pixel_offset = struct.unpack_from("<I", data, 10)[0]
    dib_size = struct.unpack_from("<I", data, 14)[0]
    width = struct.unpack_from("<i", data, 18)[0]
    signed_height = struct.unpack_from("<i", data, 22)[0]
    planes, bits = struct.unpack_from("<HH", data, 26)
    compression = struct.unpack_from("<I", data, 30)[0]
    if width <= 0 or signed_height == 0 or planes != 1 or bits != 1 or compression != 0:
        raise ValueError(f"unsupported BMP layout: {path}")
    height = abs(signed_height)
    packed_stride = (width + 7) // 8
    bmp_stride = ((width + 31) // 32) * 4
    if pixel_offset + bmp_stride * height > len(data):
        raise ValueError(f"truncated BMP: {path}")

    palette_offset = 14 + dib_size
    palette = data[palette_offset : palette_offset + 8]
    if len(palette) != 8:
        raise ValueError(f"missing BMP palette: {path}")
    luminance0 = sum(palette[0:3])
    luminance1 = sum(palette[4:7])
    invert = luminance0 < luminance1

    rows = bytearray()
    for y in range(height):
        source_y = y if signed_height < 0 else height - 1 - y
        start = pixel_offset + source_y * bmp_stride
        row = bytearray(data[start : start + packed_stride])
        if invert:
            for index in range(len(row)):
                row[index] ^= 0xFF
        if width % 8:
            row[-1] &= 0xFF << (8 - width % 8)
        rows.extend(row)
    return width, height, bytes(rows)


stream_root = next(
    path for path in arguments.vectors.iterdir() if path.is_dir()
)
streams = sorted(stream_root.glob("*.jb2"))
cases = []
passed = 0
reviewed = 0
unsupported = 0

with tempfile.TemporaryDirectory(prefix="jbig2-rs-conformance-") as temporary:
    temporary_root = Path(temporary)
    for stream in streams:
        case_output = temporary_root / stream.stem
        process = subprocess.run(
            [str(arguments.decoder), str(stream), str(case_output)],
            capture_output=True,
            text=True,
            check=False,
        )
        expected_paths = sorted(
            stream_root.glob(f"{stream.stem}_TT*.bmp"),
            key=lambda path: int(re.search(r"_TT(\d+)\.bmp$", path.name).group(1)),
        )
        case = {
            "id": stream.stem,
            "stream_sha256": hashlib.sha256(stream.read_bytes()).hexdigest(),
            "expected_pages": len(expected_paths),
            "decoder_exit": process.returncode,
            "pages": [],
        }
        if process.returncode != 0:
            case["stderr"] = process.stderr.strip()
            expected = expected_differences.get(stream.stem)
            if expected and expected["expected_error_contains"] in case["stderr"]:
                case["status"] = expected["classification"]
                case["review"] = expected["rationale"]
                if case["status"] == "reviewed_strict_rejection":
                    reviewed += 1
                elif case["status"] == "unsupported_profile":
                    unsupported += 1
            else:
                case["status"] = "decoder_error"
            cases.append(case)
            continue

        actual_paths = sorted(case_output.glob("page-*.raw"))
        if len(actual_paths) != len(expected_paths):
            case["status"] = "page_count_mismatch"
            case["actual_pages"] = len(actual_paths)
            cases.append(case)
            continue

        matches = True
        for page_index, (expected_path, actual_path) in enumerate(
            zip(expected_paths, actual_paths, strict=True)
        ):
            expected_width, expected_height, expected_bytes = parse_bmp(expected_path)
            actual_width, actual_height, actual_bytes = parse_raw(actual_path)
            page_matches = (
                expected_width == actual_width
                and expected_height == actual_height
                and expected_bytes == actual_bytes
            )
            matches &= page_matches
            case["pages"].append(
                {
                    "index": page_index,
                    "expected_width": expected_width,
                    "expected_height": expected_height,
                    "actual_width": actual_width,
                    "actual_height": actual_height,
                    "expected_sha256": hashlib.sha256(expected_bytes).hexdigest(),
                    "actual_sha256": hashlib.sha256(actual_bytes).hexdigest(),
                    "match": page_matches,
                }
            )
        case["status"] = "passed" if matches else "bitmap_mismatch"
        if matches:
            passed += 1
        cases.append(case)

result = {
    "schema_version": 1,
    "source": "ITU-T T.88 (08/2018) electronic conformance data",
    "redistribution": False,
    "total": len(cases),
    "passed": passed,
    "reviewed_strict_rejections": reviewed,
    "unsupported_profile": unsupported,
    "unresolved_failures": len(cases) - passed - reviewed - unsupported,
    "cases": cases,
}
arguments.output.parent.mkdir(parents=True, exist_ok=True)
arguments.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
print(
    json.dumps(
        {
            "total": len(cases),
            "passed": passed,
            "reviewed_strict_rejections": reviewed,
            "unsupported_profile": unsupported,
            "unresolved_failures": len(cases) - passed - reviewed - unsupported,
        }
    )
)

shutil.rmtree(temporary_root, ignore_errors=True)

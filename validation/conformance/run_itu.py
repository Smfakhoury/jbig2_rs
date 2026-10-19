#!/usr/bin/env python3
import argparse
import hashlib
import json
import math
import re
import struct
import subprocess
import sys
import tempfile
from pathlib import Path, PurePosixPath


def unique_object(pairs: list[tuple]) -> dict:
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def read_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=unique_object)


def parse_raw(path: Path, expected_page: int) -> tuple[int, int, bytes]:
    data = path.read_bytes()
    if len(data) < 20 or data[:4] != b"JB2R":
        raise ValueError(f"invalid decoder output: {path}")
    page, width, height, stride = struct.unpack(">IIII", data[4:20])
    if page != expected_page:
        raise ValueError(f"invalid decoder page number: {path}")
    if width == 0 or height == 0 or stride != (width + 7) // 8:
        raise ValueError(f"invalid decoder bitmap layout: {path}")
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


def validate_vectors(vectors: Path, manifest: dict) -> list[Path]:
    files = manifest["files"]
    if not files:
        raise ValueError("empty conformance manifest")
    paths = [PurePosixPath(entry["path"]) for entry in files]
    if len(set(paths)) != len(paths):
        raise ValueError("duplicate manifest path")
    if any(path.is_absolute() or ".." in path.parts or len(path.parts) != 2 for path in paths):
        raise ValueError("manifest paths must name files in one vector directory")
    parents = {path.parts[0] for path in paths}
    if len(parents) != 1:
        raise ValueError("manifest must describe one vector directory")
    parent = next(iter(parents))
    root = vectors if vectors.name == parent else vectors / parent
    if not root.is_dir():
        raise ValueError(f"missing vector directory: {root}")
    expected_names = {path.name for path in paths}
    actual_names = {
        path.relative_to(root).as_posix()
        for path in root.rglob("*")
        if path.is_file() and path.suffix.lower() in {".jb2", ".bmp"}
    }
    if actual_names != expected_names:
        raise ValueError(
            f"vector file set mismatch: missing={sorted(expected_names - actual_names)}, "
            f"unexpected={sorted(actual_names - expected_names)}"
        )
    streams = []
    for entry, path in zip(files, paths, strict=True):
        actual = root / path.name
        data = actual.read_bytes()
        if (
            len(data) != entry["size_bytes"]
            or hashlib.sha256(data).hexdigest() != entry["sha256"]
        ):
            raise ValueError(f"vector hash/size mismatch: {path}")
        if entry["kind"] == "encoded_stream" and actual.suffix == ".jb2":
            streams.append(actual)
        elif entry["kind"] != "expected_bitmap" or actual.suffix != ".bmp":
            raise ValueError(f"invalid vector kind: {path}")
    if not streams:
        raise ValueError("manifest contains no encoded streams")
    return sorted(streams)


def expected_pages(stream: Path) -> list[Path]:
    paths = sorted(stream.parent.glob(f"{stream.stem}_TT*.bmp"))
    if not paths or any(not re.search(r"_TT\d+\.bmp$", path.name) for path in paths):
        raise ValueError(f"missing or invalid expected pages for {stream.stem}")
    numbers = [int(re.search(r"_TT(\d+)\.bmp$", path.name).group(1)) for path in paths]
    if sorted(numbers) != list(range(len(paths))):
        raise ValueError(f"non-contiguous expected pages for {stream.stem}")
    return sorted(paths, key=lambda path: int(re.search(r"_TT(\d+)\.bmp$", path.name).group(1)))


def run_case(stream: Path, decoder: Path, output: Path, timeout: float, difference: dict | None) -> dict:
    expected_paths = expected_pages(stream)
    case = {
        "id": stream.stem,
        "stream_sha256": hashlib.sha256(stream.read_bytes()).hexdigest(),
        "expected_pages": len(expected_paths),
        "decoder_exit": None,
        "pages": [],
    }
    try:
        process = subprocess.run(
            [str(decoder), str(stream), str(output)],
            capture_output=True,
            text=True,
            check=False,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired:
        case["status"] = "decoder_timeout"
        return case
    case["decoder_exit"] = process.returncode
    if process.returncode != 0:
        case["stderr"] = process.stderr.strip()
        # Signals and infrastructure failures are never reviewed malformed input.
        if (
            process.returncode == 1
            and difference
            and difference["expected_error_contains"] in case["stderr"]
        ):
            case["status"] = difference["classification"]
            case["review"] = difference["rationale"]
        else:
            case["status"] = "decoder_error"
        return case
    if difference:
        case["status"] = "expected_difference_changed"
        return case
    expected_bitmaps = [parse_bmp(path) for path in expected_paths]
    actual_paths = sorted(output.glob("page-*.raw"))
    if len(actual_paths) != len(expected_paths):
        case["status"] = "page_count_mismatch"
        case["actual_pages"] = len(actual_paths)
        return case
    matches = True
    for page_index, (expected_bitmap, actual_path) in enumerate(
        zip(expected_bitmaps, actual_paths, strict=True)
    ):
        expected_width, expected_height, expected_bytes = expected_bitmap
        actual_width, actual_height, actual_bytes = parse_raw(actual_path, page_index + 1)
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
    return case


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--vectors", type=Path, required=True)
    parser.add_argument("--decoder", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--expected-differences", type=Path)
    parser.add_argument(
        "--manifest", type=Path,
        default=Path(__file__).with_name("itu-t88-2018-manifest.json"),
    )
    parser.add_argument("--archive", type=Path, help="also verify the original archive hash")
    parser.add_argument("--timeout-seconds", type=float, default=60)
    arguments = parser.parse_args()
    if not math.isfinite(arguments.timeout_seconds) or arguments.timeout_seconds <= 0:
        parser.error("--timeout-seconds must be finite and positive")
    try:
        manifest = read_json(arguments.manifest)
        if arguments.archive:
            actual_hash = hashlib.sha256(arguments.archive.read_bytes()).hexdigest()
            if actual_hash != manifest["container_sha256"]:
                raise ValueError("conformance archive hash mismatch")
        streams = validate_vectors(arguments.vectors.resolve(), manifest)
        differences = {}
        if arguments.expected_differences:
            for entry in read_json(arguments.expected_differences)["cases"]:
                if entry["id"] in differences:
                    raise ValueError(f"duplicate expected difference: {entry['id']}")
                if entry["classification"] not in {"reviewed_strict_rejection", "unsupported_profile"}:
                    raise ValueError(f"invalid expected classification: {entry['id']}")
                if not entry["expected_error_contains"] or not entry["rationale"]:
                    raise ValueError(f"empty expected difference: {entry['id']}")
                differences[entry["id"]] = entry
        if set(differences) - {stream.stem for stream in streams}:
            raise ValueError("expected differences refer to unknown cases")
        with tempfile.TemporaryDirectory(prefix="jbig2-rs-conformance-") as temporary:
            cases = [
                run_case(
                    stream, arguments.decoder.resolve(), Path(temporary) / stream.stem,
                    arguments.timeout_seconds, differences.get(stream.stem),
                )
                for stream in streams
            ]
        passed = sum(case["status"] == "passed" for case in cases)
        reviewed = sum(case["status"] == "reviewed_strict_rejection" for case in cases)
        unsupported = sum(case["status"] == "unsupported_profile" for case in cases)
        result = {
            "schema_version": 1,
            "source": "ITU-T T.88 (08/2018) electronic conformance data",
            "manifest_sha256": hashlib.sha256(arguments.manifest.read_bytes()).hexdigest(),
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
        print(json.dumps({key: result[key] for key in (
            "total", "passed", "reviewed_strict_rejections",
            "unsupported_profile", "unresolved_failures",
        )}))
        return 1 if result["unresolved_failures"] else 0
    except (OSError, ValueError, KeyError, TypeError, struct.error) as error:
        print(f"conformance validation failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())

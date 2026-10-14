#!/usr/bin/env python3
import argparse
import hashlib
import json
import re
import struct
import subprocess
import tempfile
from pathlib import Path


parser = argparse.ArgumentParser()
parser.add_argument("--vectors", type=Path, required=True)
parser.add_argument("--jbig2dec", type=Path, required=True)
parser.add_argument("--pdfimages", type=Path, required=True)
parser.add_argument("--poppler-revision")
parser.add_argument("--output", type=Path, required=True)
arguments = parser.parse_args()


def parse_pbm(path: Path) -> list[tuple[int, int, bytes]]:
    data = path.read_bytes()
    pages = []
    position = 0
    while position < len(data):
        match = re.match(
            rb"P4\s+(?:#[^\n]*\n\s*)*(\d+)\s+(\d+)\s", data[position:]
        )
        if not match:
            raise ValueError(f"invalid PBM at offset {position}: {path}")
        width = int(match.group(1))
        height = int(match.group(2))
        position += match.end()
        length = ((width + 7) // 8) * height
        raw = data[position : position + length]
        if len(raw) != length:
            raise ValueError(f"invalid PBM length: {path}")
        pages.append((width, height, raw))
        position += length
    return pages


def normalized_text(text: str, temporary: Path) -> str:
    normalized = text.replace(str(temporary), "<temporary>")
    return normalized.replace(str(arguments.vectors), "<vectors>")


def diagnostics(text: str, temporary: Path) -> dict:
    normalized = normalized_text(text, temporary)
    lowered = normalized.lower()
    return {
        "present": bool(normalized.strip()),
        "normalized_sha256": hashlib.sha256(normalized.encode()).hexdigest(),
        "fatal_count": lowered.count("fatal"),
        "warning_count": lowered.count("warning"),
        "error_count": lowered.count("error"),
    }


def tool_version(command: list[str], prefix: str) -> str:
    process = subprocess.run(command, capture_output=True, text=True, check=False)
    output = "\n".join((process.stdout, process.stderr)).strip()
    first_line = output.splitlines()[0] if output else "unknown"
    return first_line.removeprefix(prefix)


def parse_bmp(path: Path) -> tuple[int, int, bytes]:
    data = path.read_bytes()
    pixel_offset = struct.unpack_from("<I", data, 10)[0]
    dib_size = struct.unpack_from("<I", data, 14)[0]
    width = struct.unpack_from("<i", data, 18)[0]
    signed_height = struct.unpack_from("<i", data, 22)[0]
    planes, bits = struct.unpack_from("<HH", data, 26)
    compression = struct.unpack_from("<I", data, 30)[0]
    if (
        data[:2] != b"BM"
        or width <= 0
        or signed_height == 0
        or planes != 1
        or bits != 1
        or compression != 0
    ):
        raise ValueError(f"unsupported BMP: {path}")
    height = abs(signed_height)
    packed_stride = (width + 7) // 8
    bmp_stride = ((width + 31) // 32) * 4
    palette = data[14 + dib_size : 14 + dib_size + 8]
    invert = sum(palette[0:3]) < sum(palette[4:7])
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


def minimal_pdf(encoded: bytes, width: int, height: int) -> bytes:
    body = encoded[13:]
    objects = []

    def add(number: int, data: bytes) -> None:
        objects.append((number, data))

    add(1, b"<< /Type /Catalog /Pages 2 0 R >>")
    add(2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>")
    add(
        3,
        (
            f"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width} {height}] "
            "/Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>"
        ).encode(),
    )
    add(
        4,
        (
            f"<< /Type /XObject /Subtype /Image /Width {width} /Height {height} "
            f"/ColorSpace /DeviceGray /BitsPerComponent 1 /Filter /JBIG2Decode "
            f"/Length {len(body)} >>\nstream\n"
        ).encode()
        + body
        + b"\nendstream",
    )
    content = f"q {width} 0 0 {height} 0 0 cm /Im0 Do Q".encode()
    add(
        5,
        f"<< /Length {len(content)} >>\nstream\n".encode()
        + content
        + b"\nendstream",
    )

    output = bytearray(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")
    offsets = []
    for number, data in objects:
        offsets.append(len(output))
        output += f"{number} 0 obj\n".encode() + data + b"\nendobj\n"
    xref = len(output)
    output += f"xref\n0 {len(objects) + 1}\n0000000000 65535 f \n".encode()
    for offset in offsets:
        output += f"{offset:010d} 00000 n \n".encode()
    output += (
        f"trailer << /Size {len(objects) + 1} /Root 1 0 R >>\n"
        f"startxref\n{xref}\n%%EOF\n"
    ).encode()
    return bytes(output)


root = next(path for path in arguments.vectors.iterdir() if path.is_dir())
cases = []
with tempfile.TemporaryDirectory(prefix="jbig2-rs-oracles-") as temporary:
    temporary = Path(temporary)
    for stream in sorted(root.glob("*.jb2")):
        expected_paths = sorted(root.glob(f"{stream.stem}_TT*.bmp"))
        colour_case = "TT8" in stream.stem
        expected = [] if colour_case else [parse_bmp(path) for path in expected_paths]
        case = {
            "id": stream.stem,
            "expected_pages": len(expected_paths),
            "expected_bitmap_sha256": [
                hashlib.sha256(page[2]).hexdigest() for page in expected
            ],
            "oracles": {},
        }

        output = temporary / f"{stream.stem}.pbm"
        process = subprocess.run(
            [str(arguments.jbig2dec), "-o", str(output), str(stream)],
            capture_output=True,
            text=True,
            check=False,
        )
        oracle = {
            "exit": process.returncode,
            "diagnostics": diagnostics(process.stderr, temporary),
        }
        if output.exists() and expected:
            try:
                pages = parse_pbm(output)
            except ValueError as error:
                oracle["match"] = None
                oracle["output_error"] = normalized_text(str(error), temporary)
            else:
                oracle["bitmap_sha256"] = [
                    hashlib.sha256(page[2]).hexdigest() for page in pages
                ]
                oracle["match"] = len(pages) == len(expected) and all(
                    actual[0:2] == wanted[0:2] and actual[2] == wanted[2]
                    for actual, wanted in zip(pages, expected)
                )
        else:
            oracle["match"] = None
        case["oracles"]["jbig2dec"] = oracle

        if len(expected) == 1 and not colour_case:
            pdf = temporary / f"{stream.stem}.pdf"
            pdf.write_bytes(minimal_pdf(stream.read_bytes(), expected[0][0], expected[0][1]))
            prefix = temporary / f"{stream.stem}-image"
            process = subprocess.run(
                [str(arguments.pdfimages), str(pdf), str(prefix)],
                capture_output=True,
                text=True,
                check=False,
            )
            images = sorted(temporary.glob(f"{prefix.name}-*.pbm"))
            oracle = {
                "exit": process.returncode,
                "diagnostics": diagnostics(process.stderr, temporary),
            }
            if len(images) == 1:
                try:
                    pages = parse_pbm(images[0])
                except ValueError as error:
                    oracle["match"] = None
                    oracle["output_error"] = normalized_text(str(error), temporary)
                else:
                    width, height, raw = pages[0]
                    oracle["bitmap_sha256"] = [hashlib.sha256(raw).hexdigest()]
                    oracle["match"] = (
                        len(pages) == 1
                        and (width, height) == expected[0][0:2]
                        and raw == expected[0][2]
                    )
            else:
                oracle["match"] = None
            case["oracles"]["poppler"] = oracle
        else:
            case["oracles"]["poppler"] = {
                "match": None,
                "status": "skipped_multi_page_or_colour"
            }
        cases.append(case)

result = {
    "schema_version": 1,
    "jbig2dec": {
        "revision": "6e8205ba61a206a0830f4884ebe52be615735f3d",
        "version": tool_version([str(arguments.jbig2dec), "--version"], "jbig2dec "),
        "binary_sha256": hashlib.sha256(arguments.jbig2dec.read_bytes()).hexdigest(),
    },
    "poppler": {
        "revision": arguments.poppler_revision,
        "version": tool_version([str(arguments.pdfimages), "-v"], "pdfimages version "),
        "binary_sha256": hashlib.sha256(arguments.pdfimages.read_bytes()).hexdigest(),
    },
    "cases": cases,
}
arguments.output.parent.mkdir(parents=True, exist_ok=True)
arguments.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
print(json.dumps({"cases": len(cases)}))

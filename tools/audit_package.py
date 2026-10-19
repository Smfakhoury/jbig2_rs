#!/usr/bin/env python3
import sys


FORBIDDEN_PREFIXES = (
    "proofs/",
    "specs/",
    "validation/",
    "fuzz/",
    "benchmarks/",
    "integration/",
    "target/",
)
FORBIDDEN_SUFFIXES = (".pdf", ".doc", ".docx", ".jb2", ".jbig2")

entries = []
for line in sys.stdin:
    entry = line.strip()
    if not entry:
        continue
    first, separator, remainder = entry.partition("/")
    if separator and first.startswith("jbig2_rs-"):
        entry = remainder
    entries.append(entry)
failures = [
    entry
    for entry in entries
    if entry.startswith(FORBIDDEN_PREFIXES) or entry.lower().endswith(FORBIDDEN_SUFFIXES)
]
if failures:
    print("package audit failed:", file=sys.stderr)
    for failure in failures:
        print(f"  {failure}", file=sys.stderr)
    raise SystemExit(1)

print("package audit passed")

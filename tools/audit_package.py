#!/usr/bin/env python3
import sys
from pathlib import Path, PurePosixPath, PureWindowsPath


ROOT = Path(__file__).resolve().parents[1]
REQUIRED = {"Cargo.toml", "README.md", "SECURITY.md"} | {
    path.relative_to(ROOT).as_posix() for path in (ROOT / "src").rglob("*.rs")
}
OPTIONAL = {
    ".cargo_vcs_info.json",
    "Cargo.lock",
    "Cargo.toml.orig",
    "LICENSE",
    "LICENSE-MIT",
    "LICENSE-APACHE",
    "NOTICE",
}


def audit(lines: list[str]) -> list[str]:
    entries = set()
    prefixes = set()
    failures = []
    for line in lines:
        entry = line.strip()
        if not entry:
            continue
        if PureWindowsPath(entry).drive:
            failures.append(f"invalid package path: {entry}")
            continue
        entry = entry.replace("\\", "/")
        path = PurePosixPath(entry)
        if (
            path.is_absolute()
            or ".." in path.parts
            or path.as_posix() != entry
        ):
            failures.append(f"invalid package path: {entry}")
            continue
        first, separator, remainder = entry.partition("/")
        prefix = first if separator and first.startswith("jbig2_rs-") else ""
        prefixes.add(prefix)
        if prefix:
            entry = remainder
        if entry in entries:
            failures.append(f"duplicate package entry: {entry}")
        entries.add(entry)
        if entry not in REQUIRED | OPTIONAL:
            failures.append(f"unexpected package entry: {entry}")
    if len(prefixes) > 1:
        failures.append("mixed package roots")
    if not entries:
        failures.append("empty package inventory")
    for entry in sorted(REQUIRED - entries):
        failures.append(f"missing required package entry: {entry}")
    return failures


def main() -> int:
    failures = audit(list(sys.stdin))
    if failures:
        print("package audit failed:", file=sys.stderr)
        for failure in failures:
            print(f"  {failure}", file=sys.stderr)
        return 1
    print("package audit passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

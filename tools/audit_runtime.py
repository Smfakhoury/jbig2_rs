#!/usr/bin/env python3
import re
import sys
from pathlib import Path

from jsonschema import ValidationError

from evidence import reviewed_standard_interface
from rust_source import production_code


ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "src"
PROOF_ROOTS = (ROOT / "proofs", ROOT / "specs")
CAPACITY_DECLARATION = (
    "pub assume_specification<T, A: Allocator>[ Vec::<T, A>::capacity ]"
    "(value: &Vec<T, A>) -> usize;"
)
PROOF_POLICY_PATTERNS = {
    "Verus external body": re.compile(r"\bexternal_body\b"),
    "Verus assumption": re.compile(r"\b(?:assume|admit)\s*(?:!\s*)?\("),
    "unreviewed Verus specification": re.compile(
        r"\b(?:external_(?:fn|type|trait)_specification|assume_specification)\b"
    ),
    "Verus verification exclusion": re.compile(r"\bverifier\s*::\s*external\b"),
    "Verus termination waiver": re.compile(
        r"\b(?:exec_allows_no_decreases_clause|assume_termination)\b"
    ),
}
PATTERNS = {
    "unsafe Rust": re.compile(r"\bunsafe\b"),
    "panic macro": re.compile(r"\bpanic!\s*\("),
    "unfinished macro": re.compile(r"\b(?:todo|unimplemented)!\s*\("),
    "unreachable macro": re.compile(r"\bunreachable!\s*\("),
    **PROOF_POLICY_PATTERNS,
}


failures: list[str] = []
for directory in ("src", "specs", "proofs", "tests", "examples", "fuzz/fuzz_targets",
                  "benchmarks", "tools", "verification"):
    for path in sorted((ROOT / directory).rglob("*.rs")):
        if any(part in {"target", "corpus", "artifacts"} for part in path.relative_to(ROOT).parts):
            continue
        lines = len(path.read_bytes().splitlines())
        if lines > 800:
            failures.append(f"{path.relative_to(ROOT)}: {lines} lines exceeds the 800-line source budget")
if not (SRC / "lib.rs").is_file():
    failures.append("src/lib.rs: missing production crate root")
for path in sorted(SRC.rglob("*.rs")):
    production_text = production_code(path.read_bytes(), exclude_proofs=False)
    for label, pattern in PATTERNS.items():
        match = pattern.search(production_text)
        if match is not None:
            line = production_text.count("\n", 0, match.start()) + 1
            failures.append(f"{path.relative_to(ROOT)}:{line}: {label}")

executable_function = re.compile(
    r"(?m)^[ \t]*(?:pub(?:\([^)]*\))?[ \t]+)?"
    r"(?:(?:async|const|unsafe|extern)[ \t]+)*fn[ \t]+[A-Za-z_]"
)
for proof_root in PROOF_ROOTS:
    for path in sorted(proof_root.rglob("*.rs")):
        text = production_code(path.read_bytes(), exclude_proofs=False)
        for label, pattern in PROOF_POLICY_PATTERNS.items():
            trust_match = pattern.search(text)
            if trust_match is not None:
                line = text.count("\n", 0, trust_match.start()) + 1
                failures.append(f"{path.relative_to(ROOT)}:{line}: {label}")
        executable_match = executable_function.search(text)
        if executable_match is not None:
            line = text.count("\n", 0, executable_match.start()) + 1
            failures.append(
                f"{path.relative_to(ROOT)}:{line}: duplicate executable proof source"
            )

reviewed_source = None
try:
    review = reviewed_standard_interface(ROOT)
    if review is not None:
        reviewed_source = ROOT / review["source"]
except (OSError, ValueError, KeyError, ValidationError) as error:
    failures.append(f"verification/stdlib/review.json: {error}")

for path in sorted((ROOT / "verification").rglob("*.rs")):
    if "target" in path.relative_to(ROOT).parts:
        continue
    try:
        text = production_code(path.read_bytes(), exclude_proofs=False)
    except ValueError as error:
        failures.append(f"{path.relative_to(ROOT)}: {error}")
        continue
    if path == reviewed_source:
        if text.count(CAPACITY_DECLARATION) != 1:
            failures.append(f"{path.relative_to(ROOT)}: expected the exact unconstrained capacity interface")
        else:
            text = text.replace(CAPACITY_DECLARATION, "", 1)
    for label, pattern in PATTERNS.items():
        match = pattern.search(text)
        if match is not None:
            line = text.count("\n", 0, match.start()) + 1
            failures.append(f"{path.relative_to(ROOT)}:{line}: {label}")

if failures:
    print("runtime audit failed:", file=sys.stderr)
    for failure in failures:
        print(f"  {failure}", file=sys.stderr)
    raise SystemExit(1)

print("runtime audit passed")
if reviewed_source is not None:
    print("reviewed proof-only standard interfaces: 1; normative approval: false")

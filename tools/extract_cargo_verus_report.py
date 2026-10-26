#!/usr/bin/env python3
"""Separate the exact production report from Cargo's dependency-report stream."""

import argparse
import json
from pathlib import Path

from evidence import invalid_constant, read_evidence, unique_object, validate


ROOT = Path(__file__).resolve().parents[1]
PREFIX = "jbig2_rs_production::"


def extract_report(text, pin):
    decoder = json.JSONDecoder(
        object_pairs_hook=unique_object, parse_constant=invalid_constant,
    )
    production = []
    while text.strip():
        report, end = decoder.raw_decode(text.lstrip())
        text = text.lstrip()[end:]
        validate(report, "verus")
        result = report["verification-results"]
        if (
            result.get("success") is not True
            or type(result.get("verified")) is not int or result["verified"] <= 0
            or type(result.get("errors")) is not int or result["errors"] != 0
            or result.get("encountered-error") is not False
            or result.get("encountered-vir-error") is not False
            or result.get("is-verifying-entire-crate") is not True
        ):
            raise ValueError("Cargo stream contains failed, partial or vacuous verification")
        identity = report.get("verus", {})
        if any(identity.get(key) != pin[key] for key in ("version", "commit")):
            raise ValueError("Cargo report does not match the pinned Verus identity")
        if any(name.startswith(PREFIX) for name in report["func-details"]):
            production.append(report)
    if len(production) != 1:
        raise ValueError("Cargo stream must contain exactly one production-crate report")
    return production[0]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--compiled-library", type=Path, required=True)
    arguments = parser.parse_args()
    if arguments.input.resolve() == arguments.output.resolve():
        raise ValueError("keep the raw Cargo report stream separate from the production report")
    if not arguments.compiled_library.is_file() or arguments.compiled_library.stat().st_size == 0:
        raise ValueError("Cargo verification must also produce the compiled production library")
    report = extract_report(
        arguments.input.read_text(encoding="utf-8"),
        read_evidence(ROOT / "config/toolchain.json", "toolchain")["verus"],
    )
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"Cargo production verification: {report['verification-results']['verified']} obligations")


if __name__ == "__main__":
    main()

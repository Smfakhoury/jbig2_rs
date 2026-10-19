#!/usr/bin/env python3
"""Produce unreviewed templates; never approve or overwrite dispositions."""

import argparse
import json
from pathlib import Path

from evidence import read_evidence


ROOT = Path(__file__).resolve().parents[1]


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--coverage", type=Path, default=ROOT / "production-proof-coverage.json")
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    if arguments.output.resolve() in {
        (ROOT / "proof-dispositions.json").resolve(), arguments.coverage.resolve()
    }:
        parser.error("templates must not overwrite reviewed dispositions or coverage")
    coverage = read_evidence(arguments.coverage, "coverage")
    functions = [
        {
            "id": entry["id"], "file": entry["file"],
            "file_sha256": entry["file_sha256"],
            "status": "unreviewed", "reason": None,
        }
        for entry in coverage["functions"] if entry["status"] == "incomplete"
    ]
    document = {
        "schema_version": 1,
        "policy": "Proposal only. Every entry requires explicit source review before acceptance.",
        "functions": functions,
    }
    with arguments.output.open("x", encoding="utf-8") as output:
        output.write(json.dumps(document, indent=2) + "\n")
    print(f"wrote {len(functions)} UNREVIEWED templates; no proof gate has been closed")


if __name__ == "__main__":
    main()

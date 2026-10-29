#!/usr/bin/env python3
"""Verify the owned sink's success/error state relation, not arbitrary callbacks."""

import argparse
import json
from pathlib import Path
import subprocess
import sys

from proof_sensitivity import check_semantic_cases


ROOT = Path(__file__).resolve().parents[1]
RELATION = "decode_spec::collector_result(old(self), final(self), page, out)"
CONTRACT = (
    "#[cfg_attr(verus_keep_ghost, verifier::verify)] "
    "impl PageSink for PageCollector { "
    "#[cfg_attr(verus_keep_ghost, verus_spec(out => ensures "
    + RELATION + ", ))]"
)
SITE = (
    "fn write_page(&mut self, page: Page) -> Result<(), DecodeError> "
    "{ self.push_page(page) }"
)


def case(identity, body, success, failure):
    weakened = CONTRACT.replace(
        RELATION, f"match out {{ Ok(()) => {success}, Err(_) => {failure}, }}"
    )
    return {
        "id": identity, "file": "src/decode.rs",
        "module": "decode", "verification_module": "decode",
        "function": "PageCollector::write_page", "impl_trait": "PageSink",
        "site": SITE, "replacement": SITE.replace("self.push_page(page)", body),
        "controls": ((CONTRACT, weakened),),
    }


CASES = (
    case("collector-sink-drops-successful-page", "Ok(())", "true", RELATION),
    case(
        "collector-sink-errors-after-append",
        "self.push_page(page)?; Err(DecodeError::Malformed(MalformedError::IntegerOverflow))",
        RELATION, "true",
    ),
)
FILES = ("src/decode.rs",)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--verus-executable", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    try:
        report = check_semantic_cases(
            ROOT, arguments.verus_executable.resolve(), CASES, FILES,
            "jbig2-owned-sink-",
            "Owned collector append/accounting and error-state preservation only; "
            "not arbitrary callbacks, exact error variants or observed capacity.",
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"owned collector sink proof sensitivity failed: {error}", file=sys.stderr)
        report = {"schema_version": 2, "status": "failed", "error": str(error)}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

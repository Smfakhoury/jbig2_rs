import copy
import json
from pathlib import Path
import sys
import unittest

from jsonschema import ValidationError

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from extract_cargo_verus_report import extract_report


PIN = {"version": "pinned", "commit": "a" * 40}


def report(crate):
    return {
        "func-details": {f"{crate}::module::function": {
            "obligation_proof_notes": [], "failed_proof_notes": [],
        }},
        "verus": PIN,
        "verification-results": {
            "success": True, "verified": 1, "errors": 0,
            "encountered-error": False, "encountered-vir-error": False,
            "is-verifying-entire-crate": True,
        },
    }


class CargoVerusReportTests(unittest.TestCase):
    def test_production_and_upstream_reports_are_not_conflated(self):
        production = report("jbig2_rs_production")
        stream = json.dumps(report("vstd")) + "\n" + json.dumps(production)
        self.assertEqual(extract_report(stream, PIN), production)

    def test_empty_dependency_only_wrong_crate_and_duplicate_roots_fail(self):
        for stream in (
            "", " \n", json.dumps(report("vstd")),
            json.dumps(report("other_jbig2_rs_production")),
            json.dumps(report("jbig2_rs_production")) * 2,
        ):
            with self.subTest(stream=stream), self.assertRaises(ValueError):
                extract_report(stream, PIN)

    def test_partial_failed_and_vacuous_reports_fail(self):
        for key, value in (
            ("success", False), ("errors", 1), ("errors", False),
            ("verified", 0), ("verified", True),
            ("encountered-error", True), ("encountered-vir-error", True),
            ("is-verifying-entire-crate", False),
        ):
            production = report("jbig2_rs_production")
            production["verification-results"][key] = value
            with self.subTest(key=key, value=value), self.assertRaises((ValueError, ValidationError)):
                extract_report(json.dumps(production), PIN)

    def test_failed_dependency_cannot_be_hidden_by_a_passing_root(self):
        dependency = report("vstd")
        dependency["verification-results"]["errors"] = 1
        with self.assertRaises((ValueError, ValidationError)):
            extract_report(
                json.dumps(dependency) + json.dumps(report("jbig2_rs_production")), PIN,
            )

    def test_wrong_version_or_commit_is_rejected(self):
        for key in PIN:
            production = copy.deepcopy(report("jbig2_rs_production"))
            production["verus"][key] = "wrong"
            with self.subTest(key=key), self.assertRaisesRegex(ValueError, "pinned"):
                extract_report(json.dumps(production), PIN)

    def test_duplicate_keys_non_json_constants_and_trailing_text_fail(self):
        valid = json.dumps(report("jbig2_rs_production"))
        for stream in (
            '{"a":1,"a":2}', valid + " garbage",
            valid.replace('"verified": 1', '"verified": NaN'),
        ):
            with self.subTest(stream=stream), self.assertRaises(ValueError):
                extract_report(stream, PIN)

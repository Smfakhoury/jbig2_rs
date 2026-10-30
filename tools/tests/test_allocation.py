import copy
from pathlib import Path
import sys
import unittest


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from run_allocation import COUNTERS, INFLATION, validate_control, validate_sample


def sample(case="white-64"):
    return {
        "schema_version": 1, "id": case, "outcome": "decoded",
        "input_bytes": 100, "output_bytes": 512, "exact_output_checked": True,
        "inflation_bytes": 0, "tracked_allocations_total": 8, "peak_live_allocations": 3,
        "final_live_allocations": 0, "total_requested_payload_bytes": 1000,
        "peak_requested_payload_bytes": 600, "final_requested_payload_bytes": 0,
    }


def inflated():
    row = sample()
    row["inflation_bytes"] = INFLATION
    for field in ("tracked_allocations_total", "peak_live_allocations"):
        row[field] += 1
    for field in ("total_requested_payload_bytes", "peak_requested_payload_bytes"):
        row[field] += INFLATION
    return row


class AllocationMeasurementTests(unittest.TestCase):
    def test_exact_outcomes_and_zero_allocation_error_path_are_accepted(self):
        self.assertEqual(validate_sample(sample(), "white-64"), sample())
        for case, outcome in (("work-boundary", "work-limit"), ("late-truncation", "eof")):
            row = sample(case)
            row.update(outcome=outcome, output_bytes=0)
            row.update({field: 0 for field in COUNTERS})
            self.assertEqual(validate_sample(row, case), row)

    def test_missing_unknown_or_incorrect_identity_fields_fail_closed(self):
        for field in sample():
            row = sample()
            del row[field]
            with self.subTest(field=field), self.assertRaises((ValueError, KeyError)):
                validate_sample(row, "white-64")
        for changes in (
            {"extra": 0}, {"schema_version": True}, {"schema_version": 2},
            {"id": "white-512"}, {"exact_output_checked": False},
            {"exact_output_checked": 1}, {"outcome": "eof"}, {"output_bytes": 0},
            {"input_bytes": 0}, {"inflation_bytes": INFLATION},
        ):
            row = sample()
            row.update(changes)
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                validate_sample(row, "white-64")
        with self.assertRaises(ValueError):
            validate_sample(sample("unknown"), "unknown")

    def test_counter_types_bounds_peaks_and_unreleased_payloads_are_rejected(self):
        for field in (*COUNTERS, "input_bytes", "output_bytes", "inflation_bytes"):
            for value in (True, -1, 2**64, 1.5, None):
                row = sample()
                row[field] = value
                with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                    validate_sample(row, "white-64")
        for changes in (
            {"peak_live_allocations": 9}, {"peak_requested_payload_bytes": 1001},
            {"final_live_allocations": 1}, {"final_requested_payload_bytes": 1},
        ):
            row = sample()
            row.update(changes)
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                validate_sample(row, "white-64")

    def test_exact_live_inflation_and_restoration_control(self):
        validate_control(sample(), inflated(), sample())

    def test_every_inflation_and_restoration_counter_is_checked(self):
        for field in (
            "tracked_allocations_total", "peak_live_allocations",
            "total_requested_payload_bytes", "peak_requested_payload_bytes",
        ):
            row = inflated()
            row[field] -= 1
            with self.subTest(inflated=field), self.assertRaises(ValueError):
                validate_control(sample(), row, sample())
        for field in COUNTERS:
            row = copy.deepcopy(sample())
            row[field] += 1
            with self.subTest(restored=field), self.assertRaises(ValueError):
                validate_control(sample(), inflated(), row)

    def test_control_cannot_hide_a_changed_decode_result(self):
        for field, value in (("input_bytes", 101), ("output_bytes", 513), ("outcome", "eof")):
            for side in ("inflated", "restored"):
                changed = inflated() if side == "inflated" else sample()
                changed[field] = value
                with self.subTest(field=field, side=side), self.assertRaises(ValueError):
                    validate_control(
                        sample(), changed if side == "inflated" else inflated(),
                        changed if side == "restored" else sample(),
                    )


if __name__ == "__main__":
    unittest.main()

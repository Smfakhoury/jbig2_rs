import copy
from pathlib import Path
import sys
import unittest


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from run_allocation import (
    INFLATION, PHASE_COUNTERS, PHASE_NET, validate_phase_control,
    validate_phase_matches_combined, validate_phase_sample,
)


def counters(total=0, net=0, peak=0, byte_total=0, byte_net=0, byte_peak=0):
    return dict(zip(PHASE_COUNTERS, (total, net, peak, byte_total, byte_net, byte_peak)))


def sample():
    return {
        "schema_version": 2, "id": "white-64", "outcome": "decoded",
        "input_bytes": 102, "output_bytes": 512, "exact_output_checked": True,
        "inflation_phase": None, "inflation_bytes": 0,
        "decode_scope": "public_decode_includes_envelope_parsing",
        "phase_peaks_additive": False, "measurement_windows_nested": False,
        "decode_executions": 2, "inspection_executions": 1,
        "inspection_scope": "public_inspect_envelope_metadata_validation_and_drop_no_entropy",
        "inspection_not_additive_with_decode": True,
        "inspection": counters(4, 0, 2, 400, 0, 200),
        "overall": counters(8, 0, 5, 2000, 0, 1600),
        "phases": {
            "decode": counters(8, 3, 5, 2000, 800, 1600),
            "output_validation": counters(),
            "output_drop": counters(net=-3, byte_net=-800),
        },
    }


def inflated():
    row = sample()
    row.update(inflation_phase="output_validation", inflation_bytes=INFLATION)
    row["phases"]["output_validation"] = counters(1, 0, 1, INFLATION, 0, INFLATION)
    row["overall"] = counters(9, 0, 5, 2000 + INFLATION, 0, 800 + INFLATION)
    return row


class PhaseAllocationTests(unittest.TestCase):
    def test_signed_release_and_non_additive_peaks_match_the_flat_replay(self):
        self.assertEqual(validate_phase_sample(sample(), "white-64"), sample())
        self.assertEqual(validate_phase_sample(inflated(), "white-64", INFLATION), inflated())
        validate_phase_control(sample(), inflated(), sample())

    def test_missing_unknown_and_false_scope_fields_fail_closed(self):
        for field in sample():
            row = sample()
            del row[field]
            with self.subTest(field=field), self.assertRaises((ValueError, KeyError)):
                validate_phase_sample(row, "white-64")
        for changes in (
            {"extra": 0}, {"schema_version": True}, {"schema_version": 1},
            {"exact_output_checked": 1}, {"decode_executions": True},
            {"inspection_executions": 2}, {"phase_peaks_additive": True},
            {"measurement_windows_nested": True}, {"decode_scope": "entropy_only"},
            {"inspection_not_additive_with_decode": False},
            {"inspection_scope": "internal_parse_phase"},
            {"outcome": "eof"}, {"output_bytes": 0}, {"input_bytes": 0},
            {"inflation_phase": "decode"}, {"inflation_bytes": True},
        ):
            row = sample()
            row.update(changes)
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                validate_phase_sample(row, "white-64")
        row = sample()
        row["phases"]["unknown"] = counters()
        with self.assertRaises(ValueError):
            validate_phase_sample(row, "white-64")

    def test_every_counter_type_and_bound_is_checked(self):
        for window in ("overall", "inspection", "decode", "output_validation", "output_drop"):
            for field in PHASE_COUNTERS:
                values = (True, 1.5, None, -2**63 - 1, 2**64)
                if field not in PHASE_NET:
                    values += (-1,)
                else:
                    values += (2**63,)
                for value in values:
                    row = sample()
                    target = row[window] if window in ("overall", "inspection") else row["phases"][window]
                    target[field] = value
                    with self.subTest(window=window, field=field, value=value), self.assertRaises(ValueError):
                        validate_phase_sample(row, "white-64")

    def test_missing_release_untracked_release_or_forged_peak_is_rejected(self):
        for phase, field, value in (
            ("output_drop", "live_allocations_delta", 0),
            ("output_drop", "live_requested_payload_bytes_delta", -801),
            ("decode", "live_allocations_peak", 9),
            ("decode", "live_requested_payload_bytes_peak", 799),
        ):
            row = sample()
            row["phases"][phase][field] = value
            with self.subTest(phase=phase, field=field), self.assertRaises(ValueError):
                validate_phase_sample(row, "white-64")
        row = inflated()
        row["overall"]["live_requested_payload_bytes_peak"] = 1600 + INFLATION
        with self.assertRaises(ValueError):
            validate_phase_sample(row, "white-64", INFLATION)

    def test_complete_replays_and_inspection_cannot_retain_allocations(self):
        for window in ("overall", "inspection"):
            for field in PHASE_NET:
                row = sample()
                row[window][field] = 1
                with self.subTest(window=window, field=field), self.assertRaises(ValueError):
                    validate_phase_sample(row, "white-64")

    def test_validation_control_cannot_change_other_workloads_or_miss_restoration(self):
        for phase in ("decode", "output_drop"):
            row = inflated()
            row["phases"][phase]["requested_payload_bytes_total"] += 1
            row["overall"]["requested_payload_bytes_total"] += 1
            with self.subTest(phase=phase), self.assertRaises(ValueError):
                validate_phase_control(sample(), row, sample())
        row = inflated()
        row["inspection"]["allocations_total"] += 1
        with self.assertRaises(ValueError):
            validate_phase_control(sample(), row, sample())
        row = sample()
        row["output_bytes"] += 1
        with self.assertRaises(ValueError):
            validate_phase_control(sample(), inflated(), row)

    def test_original_combined_window_must_independently_agree(self):
        row = sample()
        combined = {
            "id": row["id"], "outcome": row["outcome"], "input_bytes": row["input_bytes"],
            "output_bytes": row["output_bytes"], "exact_output_checked": True,
            "tracked_allocations_total": 8, "final_live_allocations": 0,
            "peak_live_allocations": 5, "total_requested_payload_bytes": 2000,
            "final_requested_payload_bytes": 0, "peak_requested_payload_bytes": 1600,
        }
        validate_phase_matches_combined(row, combined)
        for field, value in combined.items():
            changed = copy.deepcopy(combined)
            changed[field] = not value if isinstance(value, bool) else (
                value + 1 if isinstance(value, int) else value + "changed"
            )
            with self.subTest(field=field), self.assertRaises(ValueError):
                validate_phase_matches_combined(row, changed)


if __name__ == "__main__":
    unittest.main()

#!/usr/bin/env python3
import json
import statistics
import sys
from pathlib import Path


def main() -> int:
    if len(sys.argv) < 4:
        raise SystemExit(
            "usage: check-stability.py MAX_RELATIVE_SPREAD OUTPUT_JSON RUN_JSON..."
        )
    maximum = float(sys.argv[1])
    output = Path(sys.argv[2])
    paths = [Path(argument) for argument in sys.argv[3:]]
    runs = [json.loads(path.read_text(encoding="utf-8")) for path in paths]
    if len(runs) < 3:
        raise SystemExit("at least three runs are required")
    if any(len(run) != len(runs[0]) for run in runs):
        raise SystemExit("run case counts differ")

    report = {"maximum_relative_spread": maximum, "cases": [], "passed": True}
    for case_index, baseline in enumerate(runs[0]):
        case = {
            "width": baseline["width"],
            "height": baseline["height"],
            "metrics": {},
            "passed": True,
        }
        for metric in (
            "jbig2_rs_parse_median_ns",
            "jbig2_rs_median_ns",
            "hayro_jbig2_parse_median_ns",
            "hayro_jbig2_median_ns",
        ):
            values = [run[case_index][metric] for run in runs]
            median = statistics.median(values)
            spread = (max(values) - min(values)) / median
            passed = spread <= maximum
            case["metrics"][metric] = {
                "values": values,
                "median": median,
                "relative_spread": spread,
                "passed": passed,
            }
            case["passed"] &= passed
        report["cases"].append(case)
        report["passed"] &= case["passed"]

    output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())

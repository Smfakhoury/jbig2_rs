"""Shared spread calculation for same-host measurement reports."""

import math
import statistics


def relative_spread(values):
    if not values or any(not math.isfinite(value) or value <= 0 for value in values):
        raise ValueError("timings must contain positive finite measurements")
    return (max(values) - min(values)) / statistics.median(values)

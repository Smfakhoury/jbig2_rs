#!/usr/bin/env bash
set -euo pipefail

iterations="${1:-50}"
output_dir="${2:-target/comparison-stability}"
runs="${3:-3}"
maximum_spread="${MAX_RELATIVE_SPREAD:-0.03}"

if (( runs < 3 )); then
  echo "at least three runs are required" >&2
  exit 2
fi

mkdir -p "$output_dir"
default_runs=()
scalar_runs=()
for ((run = 1; run <= runs; run++)); do
  run_dir="$output_dir/run-$run"
  benchmarks/hayro-comparison/run-comparison.sh "$iterations" "$run_dir"
  default_runs+=("$run_dir/generated-default.json")
  scalar_runs+=("$run_dir/generated-scalar.json")
done

python3 benchmarks/hayro-comparison/check-stability.py \
  "$maximum_spread" "$output_dir/default-stability.json" "${default_runs[@]}"
python3 benchmarks/hayro-comparison/check-stability.py \
  "$maximum_spread" "$output_dir/scalar-stability.json" "${scalar_runs[@]}"

echo "stability gate passed"

#!/usr/bin/env bash
set -euo pipefail

iterations="${1:-50}"
output_dir="${2:-target/comparison-results}"
vector_dir="${JBIG2_VECTOR_DIR:-}"
bench_cpu="${BENCH_CPU:-0}"

mkdir -p "$output_dir"
rustc +1.98.1 --version >"$output_dir/toolchain.txt"
uname -a >>"$output_dir/toolchain.txt"

taskset -c "$bench_cpu" cargo +1.98.1 run --quiet --release \
  --manifest-path benchmarks/hayro-comparison/Cargo.toml -- \
  generated "$iterations" >"$output_dir/generated-default.json"

taskset -c "$bench_cpu" cargo +1.98.1 run --quiet --release --no-default-features \
  --manifest-path benchmarks/hayro-comparison/Cargo.toml -- \
  generated "$iterations" >"$output_dir/generated-scalar.json"

if [[ -n "$vector_dir" ]]; then
  taskset -c "$bench_cpu" cargo +1.98.1 run --quiet --release \
    --manifest-path benchmarks/hayro-comparison/Cargo.toml -- \
    vectors "$vector_dir" >"$output_dir/vectors.json"
fi

echo "comparison results written to $output_dir"

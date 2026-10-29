#!/usr/bin/env bash
set -euo pipefail

engine="${1:-jbig2-decoder}"
width="${2:-2048}"
height="${3:-2048}"
iterations="${4:-100}"
output_dir="${5:-target/comparison-counters}"
bench_cpu="${BENCH_CPU:-0}"
paranoid="$(cat /proc/sys/kernel/perf_event_paranoid 2>/dev/null || echo 4)"

if (( paranoid > 2 )); then
  echo "hardware counters unavailable: perf_event_paranoid=$paranoid; use a host with perf access" >&2
  exit 2
fi

mkdir -p "$output_dir"
cargo +1.98.1 build --quiet --release \
  --manifest-path benchmarks/hayro-comparison/Cargo.toml
binary="benchmarks/hayro-comparison/target/release/jbig2-hayro-comparison"

/usr/bin/time -v -o "$output_dir/$engine.time.txt" \
  taskset -c "$bench_cpu" \
  perf stat -x, \
    -e cycles,instructions,branches,branch-misses,cache-references,cache-misses \
    -o "$output_dir/$engine.perf.csv" \
    "$binary" single "$engine" "$width" "$height" "$iterations" \
    >"$output_dir/$engine.result.json"

echo "counter results written to $output_dir"

# `hayro-jbig2` comparison

This comparison uses `hayro-jbig2` 0.3.1 from source revision
`ea9c81dc70cde40c64205986fdcfcd481fe934d8` and `jbig2_rs` optimized
revision `6ae6c7ab0e020eeeb4571f228f38151b22ab31d4`.

## Reproduction

Build and run the unpublished harness:

```sh
cargo +1.98.1 run --release \
  --manifest-path benchmarks/hayro-comparison/Cargo.toml -- \
  generated 50
```

Run both the default-SIMD and scalar-to-scalar tracks:

```sh
benchmarks/hayro-comparison/run-comparison.sh 50
```

Generated timing records include both median and p95 latency. Set
`JBIG2_VECTOR_DIR` to add the private vector outcome comparison to the run.

Run the required three-run stability gate:

```sh
benchmarks/hayro-comparison/run-stability.sh 50
```

The gate fails if the cross-run spread of any generated parse or end-to-end
median exceeds 3%. Override only for investigation with
`MAX_RELATIVE_SPREAD`. Runs are pinned to CPU 0 by default; set `BENCH_CPU` to
another isolated core when needed.

Capture peak RSS and Linux hardware counters on a host with `perf` access:

```sh
benchmarks/hayro-comparison/capture-counters.sh jbig2_rs
benchmarks/hayro-comparison/capture-counters.sh hayro_jbig2
```

The script fails explicitly when `perf_event_paranoid` prevents access rather
than returning success-shaped empty counter data.

Private ITU vectors can be compared without copying them into the repository:

```sh
cargo +1.98.1 run --release \
  --manifest-path benchmarks/hayro-comparison/Cargo.toml -- \
  vectors /path/to/JBIG2_ConformanceData-A20180829
```

The harness checks packed bitmap equality before reporting timings. Timings
include parsing, decoding, and hashing packed output. They do not represent a
diverse PDF corpus. The generated cases are intentionally simple all-white MMR
pages. `hayro-jbig2` uses its default SIMD feature; `jbig2_rs` has no SIMD
backend.

## Findings

- Both decoders produced identical bitmaps for the two valid supported ITU
  vectors and rejected the other eight streams.
- `jbig2_rs` was 1.58x to 2.69x faster on generated MMR and 2.3% faster on
  TT9. `hayro-jbig2` remained 1.50x faster at TT10 median.
- Across generated 64/512/2048 and TT9/TT10, the geometric-mean
  `jbig2_rs`/Hayro ratio was 0.694 at median and 0.636 at p95: `jbig2_rs` was
  30.6% and 36.4% faster respectively on the aggregate suite.
- TT10 p95 was slightly better in `jbig2_rs` despite its slower median.
- Peak RSS for 100 decodes of the generated 2048x2048 case was 3408 KiB for
  `jbig2_rs` and 3380 KiB for `hayro-jbig2`.
- `jbig2_rs` has stronger document-level support: explicit multi-page output,
  page/global segment lifecycle validation, retention policy, configurable
  resource/work limits, typed unsupported-feature errors, and reusable globals.
- `hayro-jbig2` exposes a single-image API. It parses retention and page
  association fields but does not apply general retention/page lifecycle
  policy, and its dispatch ignores profiles, colour palettes, extensions, and
  end-of-stripe segments after its initial stripe-height pre-scan.
- `hayro-jbig2` has no comparable formal proof inventory. `jbig2_rs` directly
  verifies every codec algorithm with Verus; its reviewed boundaries are
  limited to standard formatting/I/O and caller callbacks.

The optimized candidate is stronger on verified correctness, defensive limits,
document orchestration, and aggregate throughput. TT10 arithmetic-generic
median remains the disclosed performance exception.

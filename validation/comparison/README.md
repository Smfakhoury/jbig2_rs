# `hayro-jbig2` comparison

This comparison uses `hayro-jbig2` 0.3.1 from source revision
`ea9c81dc70cde40c64205986fdcfcd481fe934d8` and `jbig2_rs` production
revision `cb73e895935ccac9ef821105071e98d720737bec`.

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
- `hayro-jbig2` was 5.3x to 8.8x faster on generated MMR cases and 7.1x to
  16.4x faster on the two valid official vectors.
- Peak RSS for 100 decodes of the generated 2048x2048 case was 3436 KiB for
  `jbig2_rs` and 3344 KiB for `hayro-jbig2`.
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

The current candidate is therefore stronger on verified correctness,
defensive limits, and document orchestration, but it is not faster. Performance
work should focus first on bitmap representation, MMR span filling, arithmetic
context access, and eliminating repeated validation/allocation in hot paths.

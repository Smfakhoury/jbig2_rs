# Optional Hayro comparison

This development harness compares `jbig2-decoder` with `hayro-jbig2` 0.3.1.
It is separate from the dependency-free runtime crate and is not published as
part of the decoder. Results describe the tested source, workload, compiler and
host, not a general performance or conformance ranking.

## Reproduction

```sh
cargo +1.98.1 run --release \
  --manifest-path benchmarks/hayro-comparison/Cargo.toml -- generated 50
benchmarks/hayro-comparison/run-comparison.sh 50
benchmarks/hayro-comparison/run-stability.sh 50
```

The scripts run default-SIMD and scalar-to-scalar tracks. Hayro uses its default
SIMD feature; `jbig2-decoder` has no SIMD backend. Generated fixtures are shared
with the runtime performance lane. Reports include actual source/tool/host
identities, median and nearest-rank p95.

The stability script rejects cross-run spread above 3% for generated parse or
end-to-end medians. `MAX_RELATIVE_SPREAD` changes the investigation threshold;
it does not change runtime CI gates. CPU 0 is the default affinity and
`BENCH_CPU` selects another core. Stable measurements are necessary for useful
timing, not proof that the workload represents real documents.

On Linux with hardware-counter access:

```sh
benchmarks/hayro-comparison/capture-counters.sh jbig2-decoder
benchmarks/hayro-comparison/capture-counters.sh hayro_jbig2
```

Unavailable `perf` access fails explicitly rather than producing empty counters.
The preferred engine argument is `jbig2-decoder`; `jbig2_rs` remains a legacy
engine ID in compatible metrics, not the crate name.

## External-vector outcomes

With a local copy whose terms permit the intended use:

```sh
cargo +1.98.1 run --release \
  --manifest-path benchmarks/hayro-comparison/Cargo.toml -- \
  vectors /path/to/JBIG2_ConformanceData-A20180829
```

`JBIG2_VECTOR_DIR` adds this outcome lane to the scripts. External assets are
not copied into the runtime package. See [conformance](../README.md#official-conformance).

## Interpretation

Generated cases compare packed pixels before timing. Timed work here includes
parsing, decoding and output hashing, unlike the decode-call-only
[base/head lane](../README.md#basehead-performance). Simple generated MMR pages
are not a diverse PDF corpus, and an aggregate must not hide per-case regressions.

The vector wrapper uses a single-image API and FNV64 summaries; it cannot
establish full-byte multi-page equality. Shared errors or warnings do not prove
normative validity or an entropy offset that the wrapper does not expose.
The [case records](../conformance/seven-rejection-review.json) distinguish
expected rejections from bitmap passes.

API differences and proof-inventory counts are not comparative correctness
certificates. See [verification scope](../../docs/verifier-limitations.md).

# Optional Hayro comparison

This unpublished harness compares `jbig2_rs` with `hayro-jbig2` 0.3.1.
Superseded single-run measurements are recoverable in Git history and retained
CI artifacts, not copied into a source archive or used as live release baselines.
**No current-main performance or normative conformance claim follows from those
measurements.** Current base/head policy and missing P09 evidence are documented
in [validation](../README.md#basehead-performance).

## Reproduction

The unpublished harness is separate from the dependency-free runtime crate:

```sh
cargo +1.98.1 run --release \
  --manifest-path benchmarks/hayro-comparison/Cargo.toml -- \
  generated 50
benchmarks/hayro-comparison/run-comparison.sh 50
benchmarks/hayro-comparison/run-stability.sh 50
```

The comparison script runs default-SIMD and scalar-to-scalar tracks. Hayro uses
its default SIMD feature; `jbig2_rs` has no SIMD backend. Generated fixtures are
shared with the runtime performance lane. New records include median and
nearest-rank p95. Record actual source/tool/host identities for each run rather
than reusing historical revision labels or measurements.

The stability gate rejects cross-run spread above 3% for any generated parse or
end-to-end median. `MAX_RELATIVE_SPREAD` is an investigation override, not
permission to weaken a release gate. CPU 0 is the default affinity;
`BENCH_CPU` can select another isolated core. Stability is necessary for useful
timing, not sufficient dedicated-host release evidence.

For an authorized host with hardware-counter access:

```sh
benchmarks/hayro-comparison/capture-counters.sh jbig2_rs
benchmarks/hayro-comparison/capture-counters.sh hayro_jbig2
```

Unavailable Linux `perf` access fails explicitly rather than producing empty
success-shaped counters.

Only with already approved external assets:

```sh
cargo +1.98.1 run --release \
  --manifest-path benchmarks/hayro-comparison/Cargo.toml -- \
  vectors /path/to/JBIG2_ConformanceData-A20180829
```

`JBIG2_VECTOR_DIR` optionally adds that vector outcome lane to the scripts.
Neither command grants acquisition/redistribution permission or copies assets
into the project.

## Interpretation

Generated comparisons check packed bitmap equality before timing. This
harness's timed scope includes parsing, decoding and output hashing, unlike the
decode-call-only base/head lane. Simple generated MMR pages are not a diverse
PDF corpus, and aggregate ratios must not conceal individual-case regressions.
Historical TT9/TT10 latency and RSS observations do not establish improvements
on today's decoder or close current allocator/RSS telemetry gaps.

The historical official-vector wrapper reports dimensions and FNV64 summaries
through a single-image API. That is not full-byte multi-page equality.
Shared errors/warnings do not show an official positive malformed or confirm
an entropy offset the wrapper does not expose. See the
[source-bound seven-case review](../conformance/seven-rejection-review.json).
The unresolved official positives have no reviewed rejection credit.

API differences and the existence of a Verus inventory are not a comparative
correctness certificate. Full exact-source and normative functional claims
remain blocked as described in
[verification limitations](../../docs/verifier-limitations.md).

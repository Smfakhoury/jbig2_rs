# Verification scope and limitations

Verus proves submitted contracts under their preconditions and trusted
interfaces. The decoder uses the same Rust implementation for native and proof
builds, but its contract coverage is partial.

A zero-error result is not complete JBIG2 conformance, universal panic freedom
or an end-to-end CPU/memory bound. The [release results](publication-readiness.md)
and [traceability ledger](spec-traceability.md) describe the measured scope.

## Accounting categories

| Category | Meaning |
|---|---|
| Directly verified | A qualified native declaration has proof evidence for its actual contract. |
| Reviewed verifier limitation | A complete production implementation has an explicit standard-library, formatting, I/O, callback or orchestration boundary; its body is not claimed verified. |
| Audited forwarder | A source-bound, narrowly classified wrapper; not a generic callback exemption. |
| Runtime/proof divergence | Native behavior differs from the proof-selected path and cannot support an exact-source claim. |
| Incomplete | Required accounting or evidence is missing. |

The 0.1.0 inventory has 642 direct declarations and 20 explicit limitations,
with zero forwarders, divergences, incomplete functions or native assumptions.
These are declaration counts, not percentages of the standard.

The inventory follows syntax, cfg and module scopes, including executing code
after test-only declarations. Qualified identity includes file, lexical scope
and trait/type where needed; a same-named method is not interchangeable evidence.
Unknown exclusions, ambiguous paths and unaccounted source fail checks.

## Contracts and external boundaries

Typed error conversions prove project variant/payload mappings, including
wrapped errors. Default constructors prove their configured resource values.
The concrete page collector proves its own append/result relation and counter
updates. None of these results verifies arbitrary caller-provided I/O or sink
callbacks.

Formatting traits, `std::io::Read`, dynamic errors and callback behavior remain
important boundaries. Current dispositions are recorded in
[`proof-dispositions.json`](../verification/proof-dispositions.json);
[`production-proof-coverage.json`](../verification/production-proof-coverage.json)
binds those statuses to native declarations.

Selected contracts cover exact reader cursor/value/error frames, arithmetic
state transitions, packed bitmap contents/operators, MMR input/raster histories,
Huffman results and symbol/container metadata. Their local guarantees do not
automatically compose into complete stream, page or document semantics.
See the [auditor index](verus-auditor-index.md) for specific models.

### IAID range and context frames on the 0.2 development line

The production IAID loop proves a leading-one prefix at every decoded depth
for both 32-bit and 64-bit `usize`. Successful values are strictly below
`2^code_length`; a valid zero-length request returns zero without changing
decoder or contexts. Context zero and entries outside the requested tree remain
unchanged, including on partial errors. The value bound propagates through the
arithmetic symbol-ID adapter.

These are explicit algebraic/frame consequences of the existing exact
operational model, not an independently reviewed Annex A/Qe translation or
complete caller/stream proof. Endpoint, empty-input and partial-error native
tests accompany eight compiling IAID mutation contrasts. Historical review
confirmations retain their original source identities; new proof/contract bytes
do not silently renew those approvals. The unpublished development scope still
requires actual proofs and test discovery without claiming completion.

### Arithmetic construction and enum composition on the 0.2 line

Fresh symbol/text arithmetic construction specifies zero integer and IAID
probability banks, the requested IAID size, zero refinement work, requested
refinement presence, and literal input/initialized decoder registers.
This proves the result of fresh construction, not when every text/dictionary
caller must construct, retain or reset statistics.

The arithmetic branches of `SymbolCodec` preserve the concrete integer/IAID
result, partial-error state and untouched-bank/refinement frames. IAID bounds
propagate through this enum layer; the separate Huffman contracts are retained.
Existing exchange records are linked to their already-proved conditional
decision/index/MPS and normalization results. That is evidence correction,
not newly authored exchange proofs or independent standards approval.

## Observed-capacity integration

Allocation accounting checks actual post-reservation `Vec::capacity()` rather
than substituting the requested length. Bitmap allocation, borrowed-pointer
storage and owned page collection retain checked conversion/multiplication,
resource-limit ordering and failure-state guarantees.

Logical decoded bytes and allocated bitmap bytes are distinct counters.
Tests exercise over-allocation, cumulative managed totals and overflow;
removing the post-reservation checks must make their regressions fail.

The proof-only capacity getter is an explicit additional trusted signature,
isolated under [`verification/stdlib/`](../verification/stdlib/).
It has no preconditions or postconditions about capacity, length, reservation,
usable allocation size or equality between observations. Numeric boundary
checking is proved separately rather than assumed. The runtime crate remains
dependency-free and forbids handwritten unsafe code.

These are vector-capacity observations, not allocator usable-size, total
process-memory or RSS guarantees.
See the [interface description](../verification/stdlib/STATUS.txt).

## Exact-source and trust gates

The proof manifest targets the same `src/lib.rs` as ordinary Cargo. `vstd`
and the capacity-interface crate are proof-only dependencies.

```sh
cargo +1.98.1 verus build --manifest-path verification/Cargo.toml --locked -- \
  --num-threads 1 --rlimit 30 --triggers-mode silent
python3 -B tools/audit_production_coverage.py \
  --verus-json target/verification/verus-production.json \
  --output target/verification/production-proof-coverage-ci.json \
  --require-complete --require-exact
```

The Python audit needs a freshly extracted verifier report; the full report
generation command is in the [validation guide](../validation/README.md).
`--require-complete` requires complete accounting, including explicit
non-direct statuses. `--require-exact` additionally rejects divergences and
incomplete functions. Neither flag means exhaustive functional coverage.

The runtime trust audit rejects unregistered trusted specifications,
verification exclusions, `assume`/`admit` shortcuts and execution-termination
waivers. The capacity interface's declaration and manifest hashes are checked
separately; unknown declarations or added facts fail.

Rust/LLVM, pinned Verus/vstd, specified standard operations, compiler-generated
code and standard-library internals remain trusted. The crate's unsafe
prohibition and extracted compiler controls check crate-authored source, not a
binary-wide absence of unsafe operations. Loop termination reasoning is
conditional on callees terminating, not a service-availability proof.

### Focused debugging

```sh
cargo +1.98.1 verus focus --manifest-path verification/Cargo.toml --locked -- \
  --verify-module document --num-threads 1 --rlimit 30 --triggers-mode silent
```

Selectors go after `--`. If explicit forwarding is needed, the pinned option is
`--fwd-verus-args-to roots` before `--`. Focused results are debugging evidence,
not replacements for whole verification, compilation, lifetime or erasure checks.

## Semantic evidence is separate

Each configured contrast requires:

1. A pristine positive proof covering the exact selected target.
2. A mutant that compiles as ordinary Rust.
3. Rejection by the specific strong contract or invariant.
4. Success when only that selected guarantee is weakened.

Compilation failure, zero selected functions and unrelated errors are not
detection. Native mutation controls separately require the original selected
test to pass and the compiling mutant to fail that exact test.

Sealed whole-proof artifacts may supply only pristine positive baselines when
source, tools, run/attempt, function coverage and hashes match. Every negative
and weaker query remains fresh. In-process caches contain deterministic parsed
source metadata, not verifier/compiler results.

CI distributes semantic work across whole/collector roles and fourteen
sequential one-worker shards, with at most four semantic jobs active at once.
All sixteen producer receipts and every configured group are required.
Measured role workloads must not exceed 1,050 seconds; whole/collector jobs
have 20-minute caps and semantic jobs 30-minute caps. Partial, canceled,
misrouted, duplicate or stale receipts fail collection.

## Remaining functional coverage

Exhaustive requirement enumeration, independent table/coordinate fidelity and
complete codec-to-page/document composition remain unfinished. State/bounds
contracts and helper result relations must not be presented as complete
decoded-output proofs.

Other limits include arbitrary callback semantics, full allocation-failure
coverage, total process-memory/allocator-overhead bounds and representative
performance guarantees. Remaining work is listed in the
[roadmap](open-source-readiness-plan.md) and
[#135](https://github.com/microsoft/jbig2_rs/issues/135).

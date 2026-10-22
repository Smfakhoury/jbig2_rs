# Runtime coverage: measurement and remaining gaps

P06 is in progress. This tranche establishes a reproducible baseline and
non-regression checks; it does not meet or ratify the proposed release goals,
complete the missing tests, or establish normative correctness.

## Current ratcheted baseline

Pinned compiler: `nightly-2026-10-12`; collector: `cargo-llvm-cov 0.9.1`;
component: matching `llvm-tools-preview`. Nightly is used for unstable branch
instrumentation, not for Verus or ghost compilation. The decoder's MSRV and
ordinary native build policy are unchanged.

| Runtime metric | Covered / count | Coverage |
|---|---:|---:|
| Lines | 10,325 / 12,622 | 81.80% |
| Code regions | 9,945 / 12,933 | 76.90% |
| Branch outcomes | 1,908 / 2,638 | 72.33% |

The public reference-contract floor is bound to immutable source
`ca80b60785acc3c0cdd3ae90e8c430f39c7d2b99`, before its separate evidence
commit. Six integration tests cover 216 role/cardinality/table/global cases
and three exact table-error precedence cases. Fresh collection runs
343 native tests, with the same 14 ignored microbenchmarks excluded.
All eight targeted runtime mutants compile and are detected by the exact
selected regression tests.

The comparison collection has exactly the Rust source and test identities of
the CI parent `33be03f`; its actual totals are 10,294/12,622 lines,
9,907/12,933 regions and 1,896/2,638 branch outcomes. Exact source-coordinate
comparison gains **31 lines, 38 regions and 12 branch outcomes**, with no lost
covered coordinate and every global/per-file denominator unchanged. All floors
are nondecreasing against current main and the earlier unpublished reference
candidate; the totals above are a fresh combined measurement, not summed
branch deltas. Production/model/proof/Cargo inputs are unchanged from the CI
parent. Positive opaque-payload cases assert inspection metadata only, not
valid entropy streams, new coding families or normative conformance.

The earlier MMR contract floor was bound to immutable test-source commit
`8fcd9e93926049492a684e16c7153f26250b7611`, before its separate evidence commit.
It adds **57 covered lines, 50 regions and 19 branch outcomes** over a fresh
`d970ac4` main collection with identical source-coordinate denominators and no
lost covered coordinates. The older reviewed floor was 10,207/12,620 lines,
9,823/12,932 regions and 1,861/2,640 branches; main already contributes two
covered bitmap lines/regions and their denominator entries. Those two are not
counted as new test gains. All global/per-file fractions are nondecreasing.
[`mmr-contracts.json`](../validation/coverage/mmr-contracts.json) records the raw
identities, exact coordinate deltas and complete native-source identity review.
The following paragraphs retain the history of earlier tranches.

The source-offset dispatch floor was established with 262 correctness tests, with
14 ignored microbenchmarks excluded. Focused invalid/zero-area cases cover five
previously uncovered bitmap lines/regions and eight branch outcomes. Naming
the wrapper's unchanged chunked-call result adds one covered line/region to the
denominator. The optimized kernel and exclusions are unchanged; only bitmap
and global floors increase, and all per-file/global fractions are nondecreasing.
This is not a broad codec coverage improvement, and the proposed 90%/85% release
goals remain unmet.

The subsequent executing text-batch tranche runs 264 correctness tests with
the same 14 ignored microbenchmarks. New all-operator empty/zero-height tests
check complete destination storage, pending-entry clearing, retained capacity
and unused geometry. Existing randomized/exhaustive placement oracles remain.
All per-file/global metrics, denominators and exclusions are unchanged, so the
reviewed baseline is retained byte-for-byte rather than falsely ratcheted.

The packed-byte contract tranche runs 266 correctness tests. Its independent
endian and complete/short/overflowing-range oracles strengthen value/frame
evidence without changing any per-file/global count, denominator or exclusion.
The same baseline is retained byte-for-byte; no coverage increase is claimed.

The structured codec-family tranche adds five ordinary integration tests using
the same bounded program as `codec_families_structured`. First-party encoders
cover MMR pattern/halftone and raw/MMR collective Huffman dictionary/text
documents through public standalone, random-access, embedded/global, reader,
sink and reusable-context APIs. Scalar Boolean painting and literal packed
pixels are the oracles, including dimensions and padding; fresh/reused agreement
is not an independent pixel oracle. Fourteen limit kinds and typed malformed
paths are tested with recovery. Runtime source and coverage exclusions are
unchanged. The pinned before/after run increases covered source-coordinate unions
by **192 lines, 189 regions and 38 branch outcomes**, with exactly unchanged
global and per-file denominators and no lost covered coordinates. All per-file
fractions are nondecreasing. In particular `halftone.rs` gains 57/49/8 and
`symbol_text.rs` gains 113/111/24 lines/regions/branch outcomes. The baseline is
bound to immutable fixture commit
`f338287afe3e6026fe0104436ec4c25c5f726f5a`; ordinary native correctness tests
increase from 266 to 271, with the same fourteen ignored microbenchmarks.
Separate pinned family collections attribute 67/63/11 new runtime
lines/regions/branch outcomes to MMR pattern/halftone and 133/132/27 to Huffman
dictionary/text. Their union is 192/189/37 (overlap 8/6/1), not the sum of family
counts. Shared seed/control tests add the remaining bitmap branch outcome.
[`structured-families.json`](../validation/coverage/structured-families.json)
records exact raw-report and source-coordinate identities, per-file gains,
unchanged denominators and the separate shared branch. Reproduce the full
collection below; for an individual family use the pinned `cargo llvm-cov
--branch --test fuzz_codec_families --json --output-path <report> -- <test-name>`
with the exact test name recorded in that evidence and a separate
`CARGO_LLVM_COV_TARGET_DIR`.
See the detailed grammar and remaining family gaps in
[`validation/README.md`](../validation/README.md#structured-codec-families).

The additional four clause-specific header tests from P04 cover all flag bytes,
truncated valid prefixes, identifier-byte corruption, and unsigned count order.
They raise `file_header.rs` to 100% of its emitted line/region/branch mappings
without changing runtime source. This does not prove complete document semantics.

The proposed release goals remain **90% lines and 85% branches**, pending review
and substantially more tests. The CI baseline pass means **no regression below
the reviewed, upward-ratcheted fractions**, not that release coverage is adequate.

## Scope and metric definitions

- Runtime source is the executing `src/` tree under ordinary cfgs, including
  error formatting/conversions and compiler-generated methods located there.
- Inline `cfg(test)` modules, helpers, reference oracles and microbenchmark
  drivers are removed from the denominator using the Rust syntax audit. Their
  calls still contribute execution counts to production code.
- Test-only statements/branches inside otherwise production functions are also
  excluded. Cfg-gated proof statement macros are handled without swallowing a
  following runtime dereference or tuple, a known syntax-grammar ambiguity.
- Positive ghost-only items, separate proofs/models, external test drivers,
  benchmarks and developer tools do not enter the runtime denominator.
- Integration tests reuse `tools/distribution_consumer.rs` and the pure
  `benchmarks/generated.rs` fixture through the structured context scenarios.
  These exact first-party fixtures are excluded even when LLVM emits function
  mappings without a file summary. Their bytes and all Rust test/support files
  participate in the test-source identity. Unknown non-runtime mappings,
  including other benchmark sources, still fail closed.
- The only source-file exclusion is `src/lib.rs`, which declares modules and
  re-exports. The gate fails if it gains an executing function. Every other
  current runtime file must appear with line instrumentation.
- Lines use LLVM's native LCOV line counts, restricted to active runtime
  source locations. A line shared by runtime and excluded test mappings is
  rejected until its scope can be represented unambiguously.
- Regions use exported LLVM code-region source coordinates. Generic
  instantiations at the same coordinates are merged; a region is covered if
  any runtime instantiation executes it.
- Branch coverage counts the true/false outcomes of compiler-emitted branch
  regions, similarly merged by source coordinates. This is not MC/DC or proof
  of every semantic path. The unstable compiler instrumentation does not
  represent every Rust construct as a branch site.
- A zero per-file branch denominator is reported as unavailable (`null`), not
  100%. An empty aggregate line/region/branch denominator is an error.
- Measurements are Linux-only. Native correctness and distribution testing on
  Windows/macOS are separate; this report does not claim their cfg-only paths
  are covered by the Linux instrumentation.

## Reproduce

Install the pinned components and collector after preparing the normal audit
dependencies:

```text
python -m pip install -r tools/requirements.txt
rustup component add llvm-tools-preview --toolchain nightly-2026-10-12
cargo +nightly-2026-10-12 install cargo-llvm-cov --version 0.9.1 --locked
python -B tools/run_coverage.py --base-ref HEAD
```

`--base-ref` is optional for local collection. In CI it identifies the PR base
or previous push revision. Changed executable runtime lines are reported, with
uncovered line numbers. Their separate threshold is explicitly informational
pending P06 policy review; no changed runtime lines is `not_applicable`, not
an invented 100% pass.

The collector clears inherited proof/compiler instrumentation overrides, uses
a separate coverage build directory, runs library/integration tests, and
collects JSON, LCOV and raw HTML. Doc tests run in ordinary native CI, but are
not currently included in this coverage collection.

## Artifacts and checks

- `target/coverage/runtime.json`: authoritative runtime-scoped per-file metrics,
  uncovered lines and counts, changed-line information, release-goal status.
- `target/coverage/runtime.md`: concise CI job summary.
- `target/coverage/raw.json`, `raw.lcov`, `html/index.html`: original LLVM
  diagnostic data. **Raw HTML includes test code** and has a different scope
  from the authoritative runtime summary.
- `target/coverage/collection.json`: source identity, actual compiler, collector
  pins and hashes of the raw reports, integration tests and shared fixture.
  Edits to tests during/after collection invalidate evidence, even if runtime
  source is unchanged.
- `target/coverage/failure.json`: explicit collector failure details.
- `target/coverage/mutations.json`: results from the isolated contract-sensitivity checks.

The compact reviewed baseline is
[`validation/coverage/baseline.json`](../validation/coverage/baseline.json).
CI never updates it automatically. Both aggregate and existing per-file
fractions must not decrease; comparisons use integer cross multiplication,
not rounded percentages. Missing baseline files, scope changes, unsupported
export formats, missing branch data, malformed/duplicate LCOV records and
inconsistent baselines fail closed.

Reports are uploaded as GitHub Actions artifacts only. No external coverage
service or third-party upload is introduced. The coverage workflow has read-only
repository permissions and requires no private vectors or secrets.

## Test-improvement priorities

The negative/metamorphic catalog migration now requires exact executable
references for every case, with native Cargo discovery and five additional
typed public-API regressions. This closes the file-only reference gap, not the
line/branch coverage goals or normative proof gaps. The catalog checker records
discovery separately from execution; native CI runs the tests independently.

1. Symbol/text decoding: 69.03% lines, 52.44% branch outcomes; largest uncovered
   runtime surface. Add valid arithmetic/Huffman/refinement combinations and
   malformed export/placement/aggregation regressions.
2. High-level decode and document lifecycle: now 75.36% / 79.20% lines and
   58.70% / 64.42% branches. New tests cover short reads, late reader failures,
   sink failure propagation, inclusive input/dimension/work limits, mandatory
   retained state versus optional scratch, and reusable-context recovery.
   More resource, stripe and reference-graph cases remain.
3. Typed error formatting/conversions: raised from 19.21% to 100% lines and
   regions. Tests verify exact diagnostics and parameter values, I/O kind
   preservation without retaining private messages, full-width sizes, and
   formatter failure propagation. No branch sites are emitted for this file;
   that is unavailable branch data, not 100% branch coverage.
4. Other coding families: use the uncovered-line artifacts to add exact-output
   and boundary tests, tied to P04 requirement IDs as that ledger is built.

## Targeted mutation sensitivity

```text
python -B tools/check_contract_mutations.py --toolchain 1.89.0 \
  --output target/coverage/mutations.json
```

CI runs the same check with the pinned coverage nightly. It copies source and
fixtures outside the checkout, uses a fresh offline Cargo home, and tests three
single-site mutations: wrong I/O classification, an inclusive-reader-limit
off-by-one, and a swallowed sink error. For each case the original selected test
must pass, the mutant must compile, and that exact test must fail. Missing or
ambiguous mutation sites, compile failures and surviving mutants fail the gate.
No mutation is written to repository runtime files.

This is sensitivity evidence for three contracts, not exhaustive codec mutation
coverage. New catalog cases have exact ordinary Cargo target/name references
checked against non-ignored `#[test]` functions. The 18 legacy file-only case
references are an explicit migration backlog, not accepted for new cases.

Remaining P06 work includes migrating those legacy IDs, broader coding-family
and all-limit tests, additional generated valid streams, broader mutation
testing, and review/enforcement of release and changed-line thresholds.
Coverage is distinct from proof coverage and cannot close P05's known
runtime/proof divergences.

## Generated segment-header tranche

The segment-header corpus now covers every flag/count byte, all 65,536 two-byte
retention patterns, inclusive reference-width boundaries, both page-field forms,
every generated-header truncation and exact count/limit/reference errors.
The reserved-type catalog entry now points to an exact executable test; new
negative/metamorphic entries use ordinary library-test identities as well as
integration targets. The legacy-exception list must match the remaining
file-only cases exactly, so a migrated ID cannot silently regress.

This raises `src/segment_header.rs` to 95.27% emitted runtime lines and 88.64%
branch outcomes without coverage exclusions. Global release goals remain
unmet. The corpus also exposed the long-retention byte-order and final-byte
padding mismatch; runtime and model now follow the explicit 7.2.4 field rules.
The conflicting 7.2.8 worked example is documented in
[spec-traceability.md](spec-traceability.md), with owner adjudication still open.
Coverage and passing synthetic tests do not resolve that normative-review gap.

## MMR resource, state, framing and bit-reader contracts

The MMR contract tranche adds bounded inline unit tests, not a new encoded
family, generator, grammar or fuzz target. Static T.6 bitcodes and existing
first-party all-white/all-black fixtures exercise retained-scratch cleanup,
release under a smaller memory budget, recovery after both decoding and framing
errors, and inclusive stream/work/memory limits. Expected requested charges use
bitmap storage plus two typed `width + 2` vectors; expected actual charges use
independent safe `Vec<u32>::capacity()` observations, never the scratch's cached
charge as an oracle. The width-one case distinguishes requested storage from
the allocator's minimum capacity without forcing an allocation failure.

EOFB tests cover all eight bit alignments, exact prefix consumption and preserved
suffix bytes, required-versus-optional framing, invalid/truncated markers,
trailing data and nonzero alignment padding, including reader positions on
failure. Literal pixel oracles cover vertical displacements of two/three;
static run cases cover terminating/make-up codes, exact overshoot and truncation
positions, and zero-pair nonprogress. Exhaustive bounded bit-reader count/offset
cases use a separate packed-integer value oracle and check exact errors and
unchanged bytes/positions. The largest new bitmap width is 128 and all new
allocations are far below 1 MiB.

Direct conversion tests cover every MMR error classification without claiming
that every variant can be produced by a valid decoder state. Direct native
reference-search tests also check defensive rejection of short/unordered
transition slices and invalid cursors; these inputs are not reachable from the
decoder's valid transition invariant. No production hooks or allocation-failure
injection are used.

Before refreshing existing whole-file hashes, the complete parsed non-test AST
(including attributes, proof cfgs, types, constants, macros and bodies) and the
source-function inventory are compared against the collected main baseline.
Production/model/proof behavior and coverage scope/exclusions remain unchanged.
In particular the one explicit `prepare_transition_storage` runtime/proof
divergence remains: pinned Verus does not support the normal capacity accounting
through `Vec::capacity` and `Result::unwrap_or`.

Remaining gaps include real allocator failures, unrepresentable slice sizes and
integer-overflow defenses, unreachable mode/make-up budget guards implied by
progress and minimum run lengths, and internal span/transition errors excluded
by valid bitmap/reference invariants. Byte-aligned slices cannot end midway
through EOFB byte-alignment padding. These are not marked covered, excluded or
proved merely because adjacent error conversions execute.

Fourteen new correctness tests increase the full native/coverage scope from
314 to 328 passing tests, with the same fourteen ignored microbenchmarks.
`mmr.rs` increases from 518/615 to 574/615 lines, 471/606 to 520/606 regions,
and 118/150 to 136/150 branch outcomes. `reader.rs` increases from 98/101 to
99/101 lines, 88/93 to 89/93 regions, and 9/12 to 10/12 branch outcomes.
Three typed constructors used by the proof-facing implementation remain
unexecuted in ordinary runtime tests; their lines are still in the denominator.
These measurements are not completion of all P06 work or release coverage goals.

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
| Lines | 10,345 / 12,622 | 81.96% |
| Code regions | 9,964 / 12,933 | 77.04% |
| Branch outcomes | 1,909 / 2,638 | 72.37% |

The floor is now bound to merged main
`5d95c52f2bb7394231ab4802d76f0a9f32b454fc`, after the exact normalization and
typed-error adapter tests merged. A fresh complete pinned collection runs
349 native tests with the same 14 ignored microbenchmarks. Global and per-file
denominators are unchanged; every covered-count floor is nondecreasing against
the previous reviewed baseline. Arithmetic increases by one line, one region
and one branch outcome; decode increases by 19 lines and 18 regions, with no
branch gain. These are actual combined counts, not summed historical deltas.
This ratchet adds no runtime behavior, native tests, coverage exclusions, CI
jobs or release approvals. A tooling regression checks that losing even one
covered coordinate fails each nonzero global/per-file metric.

The previous public reference-contract floor was bound to immutable source
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
were nondecreasing against its main parent and the earlier unpublished reference
candidate. That prior floor was 10,325/12,622 lines, 9,945/12,933 regions and
1,908/2,638 branch outcomes: a fresh combined measurement, not summed branch
deltas. Production/model/proof/Cargo inputs were unchanged from the CI
parent. Positive opaque-payload cases assert inspection metadata only, not
valid entropy streams, new coding families or normative conformance.

The independent error-adapter follow-up adds four direct tests for every
generic/symbol/Huffman adapter variant, extreme fields, literal reasons and
delegated I/O/allocation/limit/malformed errors. The combined mutation set has
twelve cases, each requiring a compiled mutant and exact library/integration
selector. No entropy payload, coding-family fixture, fuzz seed or normative
approval is added. Fresh combined measurements after owner-main integration
are **10,345/12,622 lines (81.96%)**, **9,964/12,933 regions (77.04%)** and
**1,909/2,638 branch outcomes (72.37%)**. The exact normalization parent
measures 10,326/9,946/1,909 covered coordinates respectively, so this adapter
tranche adds 19 lines and 18 regions with zero branch gain. All 349 native
tests and twelve compiling mutation controls pass; denominator and production
coordinate identities are unchanged. Do not add historical branch deltas or
lower the merged reviewed floor. The 17-group/92-contrast gate remains required.

The arithmetic error-context follow-up adds three direct-state unit tests,
not encoded streams or a new coding family. They check partial EOF registers,
context updates and the input frame through both decision entry APIs; all
65,536 stored context-byte pairs through restore/reset/default; and every
arithmetic error adapter/formatter variant, including formatter failures.
On immutable source `d9c6d5880930ec4a10d0040388056b806834214b`, all 352 native
tests pass with the same 14 ignored tests. A fresh complete pinned collection
measures **10,379/12,622 lines (82.23%)**, **9,996/12,933 regions (77.29%)**
and **1,910/2,638 branch outcomes (72.40%)**. Against the merged-parent
measurement, only arithmetic increases: 34 lines, 32 regions and one branch
outcome. Every denominator is unchanged; arithmetic reaches 316/340 lines and
72/90 branch outcomes. This is a separate actual measurement, not a ratification
of a new floor or a sum of historical deltas. The reviewed baseline remains
unchanged. The formally unreachable subtraction-underflow defense remains in
the emitted denominator, not marked covered or excluded.

The complete decision-state follow-up adds wide numeric register oracles across
all 47 Qe rows through both entry APIs, with actual success, EOF and overflow
outcomes; full-width EOF projection; and all-signal/cursor-boundary parity with
the prior conversion path. Actual immutable source
`3f7b84e2d4de3e4415d127ef821766e1260a23c3` passes 355 native tests with the same
14 ignored tests. Fresh coverage measures **10,380/12,622 lines (82.24%)**,
**9,997/12,933 regions (77.30%)**, and **1,910/2,638 branch outcomes (72.40%)**.
Only arithmetic gains one covered line and one region over the prior measured
parent; every denominator and branch count is unchanged. This is not a branch
gain or a new reviewed floor. Sixteen native source files are unchanged;
arithmetic changes only by two explicit error constructors equivalent to the
old reviewed conversion. No coded family, entropy encoder, fuzz seed, coverage
exclusion or policy weakening is introduced.

Five subsequent direct unit tests cover packed-storage/page-state boundaries,
without any decoder invocation, entropy fixture or new coding family. On
immutable source `0c7b4680864a5eb5cc3b0a2b95d171a872fbe783`, all 360 native
tests pass with 14 existing ignored tests. Fresh measured coverage is
**10,398/12,622 lines (82.38%)**, **10,018/12,933 regions (77.46%)** and
**1,919/2,638 branch outcomes (72.74%)**. The actual parent-relative gain is
**18 lines / 21 regions / nine branch outcomes**, with unchanged denominators:
document gains 6/3/1, shared generic error helpers 3/3/0, and row-ring storage
9/15/8. Counts are a fresh combined collection, not summed historical deltas.

Tests check inclusive storage budgets, zero/max shapes without huge allocations,
row-slot mapping, scalar clipped-bit/raw-word windows, clear/copy frames and
exact big-endian bitmap flush, stripe progress, finish error precedence and
public metadata. Some direct typed states exercise defensive helper branches
not reached by validated parser state; they are not represented as valid coded
streams. Real allocation failures and architecture/invariant-unreachable
defenses are not manufactured, excluded or counted as covered. The reviewed
baseline and release goals remain unchanged.

The integer-history follow-up adds two direct-state tests, not valid encoded
integer/IAID fixtures. They check 144,384 single-step history/Qe/MPS/outcome
cases against wide numeric state/history and complete context-bank oracles,
and every IAID length byte against bounded insufficient-bank cases without
decoding. On source `5c5fac702de5724d0c40707d3e3498a6848ec38b`, all 362 native
tests pass with 14 ignored. Fresh coverage is **10,403/12,622 lines (82.42%)**,
**10,021/12,933 regions (77.48%)**, and **1,921/2,638 branches (72.82%)**.
The actual parent-relative gain is five lines, three regions and two branch
outcomes; all denominators, reviewed floors and release goals remain unchanged.
These tests do not complete integer/IAID folds, caller or normative semantics.

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

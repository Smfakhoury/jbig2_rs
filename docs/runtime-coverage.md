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
| Lines | 9,823 / 12,616 | 77.86% |
| Code regions | 9,471 / 12,930 | 73.25% |
| Branch outcomes | 1,736 / 2,640 | 65.76% |

The exact value-bridge tranche runs 258 correctness tests, with 14 ignored
microbenchmarks excluded. Shared executing Boolean/operator bodies change the
emitted denominators; the seven added line sites, 12 regions and two branch
outcomes are fully covered. All per-file/global fractions remain nondecreasing
and exclusions are unchanged. These source-shape changes are not a broad codec
coverage improvement. The proposed 90%/85% release goals remain unmet.

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

1. Symbol/text decoding: 65.25% lines, 47.74% branch outcomes; largest uncovered
   runtime surface. Add valid arithmetic/Huffman/refinement combinations and
   malformed export/placement/aggregation regressions.
2. High-level decode and document lifecycle: now 74.91% / 77.29% lines and
   58.70% / 61.96% branches. New tests cover short reads, late reader failures,
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

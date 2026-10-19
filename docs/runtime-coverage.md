# Runtime coverage: measurement and remaining gaps

P06 is in progress. This tranche establishes a reproducible baseline and
non-regression checks; it does not meet or ratify the proposed release goals,
complete the missing tests, or establish normative correctness.

## Initial measured baseline

Pinned compiler: `nightly-2026-10-12`; collector: `cargo-llvm-cov 0.9.1`;
component: matching `llvm-tools-preview`. Nightly is used for unstable branch
instrumentation, not for Verus or ghost compilation. The decoder's MSRV and
ordinary native build policy are unchanged.

| Runtime metric | Covered / count | Coverage |
|---|---:|---:|
| Lines | 9,570 / 12,635 | 75.74% |
| Code regions | 9,142 / 12,931 | 70.70% |
| Branch outcomes | 1,686 / 2,636 | 63.96% |

These measurements ran all 202 current correctness tests. The 14 ignored
microbenchmarks were not executed. The initial raw, mixed-source LLVM line
coverage was 79.35%; that includes test code and is not the runtime baseline.

The proposed release goals remain **90% lines and 85% branches**, pending review
and substantially more tests. The CI baseline pass means **no regression below
the initial measured fractions**, not that release coverage is adequate.

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
  pins and hashes of the raw reports. Stale/mismatched inputs fail.
- `target/coverage/failure.json`: explicit collector failure details.

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

1. Symbol/text decoding: 65.25% lines, 47.74% branch outcomes; largest uncovered
   runtime surface. Add valid arithmetic/Huffman/refinement combinations and
   malformed export/placement/aggregation regressions.
2. High-level decode and document lifecycle: 73.90% / 72.66% lines, 58.26% /
   60.74% branches. Exercise reader/sink failures, retained resources, stripes,
   limits and context recovery through public APIs.
3. Typed error formatting/conversions: 19.21% lines. These remain in the
   denominator rather than being excluded to improve the headline percentage.
4. Other coding families: use the uncovered-line artifacts to add exact-output
   and boundary tests, tied to P04 requirement IDs as that ledger is built.

Remaining P06 work includes executable IDs for negative/metamorphic cases,
broader public-API tests, all-limit boundary coverage, more generated valid
streams, targeted mutation testing, and review/enforcement of release and
changed-line thresholds. Coverage is distinct from proof coverage and cannot
close P05's known runtime/proof divergences.

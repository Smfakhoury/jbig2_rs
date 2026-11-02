# Runtime coverage

Coverage measures ordinary executing Rust, not ghost/proof code or normative
requirements. The revision-qualified values and actual accepted run are in
[publication readiness](publication-readiness.md). The unchanged 90% line /
85% branch-outcome goals are met there. Numerical coverage does not establish
complete test adequacy.

The reviewed ratchet is
[`validation/coverage/baseline.json`](../validation/coverage/baseline.json).
CI enforces aggregate and existing per-file non-regression. It never
automatically rewrites that baseline or lowers goals to make a change pass.

## Measurement scope

- Executing `src/` under ordinary cfgs includes error formatting/conversion and
  compiler-generated methods located in that tree.
- Syntax-level exclusion removes inline and separate test helpers, reference oracles,
  microbenchmark drivers and test-only statements/branches inside production
  functions. Their calls still contribute execution to runtime code.
- Positive ghost-only items, separate models/proofs, external drivers, developer
  tools and benchmarks do not enter the runtime denominator. Cfg proof macros
  must not swallow the following real dereference/tuple due to parser ambiguity.
- Integration scenarios reuse exactly `tools/distribution_consumer.rs` and
  `benchmarks/generated.rs`. These first-party fixture mappings are excluded
  even if LLVM supplies a function mapping without a file summary. Their bytes
  and all Rust test/support files remain part of test-source identity.
  Other unknown non-runtime mappings fail closed.
- `src/lib.rs` is explicitly reserved for declarations/re-exports and fails
  if it gains an executing function. Runtime files are derived from the actual
  native function inventory, not every physical `.rs` path. Separate test-only
  files and type/module-only facades are listed as exclusions; every source
  file containing ordinary executing declarations must have line instrumentation.
  Unknown non-runtime LLVM mappings still fail closed.
- Linux instrumentation does not claim coverage of Windows/macOS-only paths.
  Native tests/distribution on those platforms are separate evidence.

## Metrics and gates

| Metric | Definition / limitation |
| --- | --- |
| Lines | Native LLVM LCOV lines restricted to active runtime locations; ambiguous shared runtime/test lines are rejected |
| Regions | Exported LLVM code-region coordinates; generic instantiations at the same coordinates merge, covered if any runtime instance executes |
| Branch outcomes | True/false outcomes of compiler-emitted branch regions, merged by coordinates; not MC/DC or every semantic path |
| Changed lines | Executable runtime changes against exact PR base/previous push commit; uncovered lines reported |

Unstable branch instrumentation does not represent every Rust construct.
Zero per-file branch denominator is unavailable (`null`), never 100%.
Empty aggregate line/region/branch denominators fail.

Fraction comparisons use integer cross multiplication, not rounded percentages.
Missing baseline files, scope changes, unsupported export formats, missing branch
data, malformed/duplicate LCOV records and inconsistent baselines fail closed.
Baseline changes need explicit review and reproducible source/test/tool evidence.
When a layout-only refactor splits files, first compare the measured aggregate
of each original component against both the existing ratchet and the recent
accepted collection. Only after every comparison is nonregressing may the new
physical-file floors be locked to the fresh collection. Source identity,
unchanged region/branch accounting and the 90%/85% goals remain checked; file
splitting is not permission to discard uncovered code.

Changed-line coverage is currently **informational**.
No changed runtime lines is `not_applicable`, not an invented 100%. CI must fetch
the exact comparison commit if absent and fail if it cannot, rather than skip
or substitute a convenient base. Initial pushes without a predecessor still
run all ratchet checks.

## Reproduction

After installing the recorded development dependencies and pinned tools:

```sh
python3 -m pip install -r tools/requirements.txt
rustup component add llvm-tools-preview --toolchain nightly-2026-10-12
cargo +nightly-2026-10-12 install cargo-llvm-cov --version 0.9.1 --locked
python3 -B tools/run_coverage.py --base-ref HEAD
```

Use the meaningful comparison revision for `--base-ref`; it is optional for
local collection. The collector clears inherited proof/compiler overrides,
uses a separate coverage build directory and runs library/integration tests.
Doc tests run in ordinary native CI, not this collection.
Pins live in [`config/toolchain.json`](../config/toolchain.json).

## Artifacts

| Path | Meaning |
| --- | --- |
| `target/coverage/runtime.json` | Authoritative runtime-scoped per-file metrics, counts, uncovered/changed lines and goal status |
| `target/coverage/runtime.md` | Concise job summary |
| `target/coverage/raw.json`, `raw.lcov`, `html/index.html` | Original LLVM diagnostics; raw HTML includes test code and has a different scope from the runtime summary |
| `target/coverage/collection.json` | Source/test identities, actual compiler/collector pins and raw-report/fixture hashes |
| `target/coverage/failure.json` | Explicit collection failure |
| `target/traceability/mutations.json` | Isolated native contract-sensitivity results from the Linux native CI artifact |

Editing tests during/after collection invalidates evidence even if runtime
source is unchanged. Compare coverage unions/fractions on compatible identities;
do not sum historical deltas across changed source coordinates. The reviewed
coverage baseline remains tracked. Superseded implementation/measurement
snapshots are recoverable in Git history and retained CI artifacts, not copied
into a source archive or treated as current measurements.

Reports are GitHub Actions artifacts, not third-party coverage uploads.
The lane has read-only repository permissions and requires no private vectors
or secrets.

## Runtime sensitivity and executable catalogs

```sh
python3 -B tools/check_contract_mutations.py --toolchain nightly-2026-10-12 \
  --output target/traceability/mutations.json
python3 -B tools/check_test_catalogs.py --discover-tests \
  --output target/traceability/test-catalogs.json
```

The existing Linux native CI job uses the pinned coverage nightly for native
mutation controls and retains their report in `traceability-ubuntu-latest`.
The separate coverage job collects LLVM metrics; both jobs must succeed.
This keeps the existing 20-minute native and 15-minute coverage budgets, without
adding jobs, omitting mutants or caching compilation outcomes. The checker
isolates source/fixtures, uses a fresh offline Cargo home, requires the selected
original test to pass, requires each mutant to compile, and requires that exact
test to fail. Ambiguous/missing mutation sites, compilation failure, zero-test
passes and surviving mutants are not detection. Repository runtime files are
not mutated. Selected mutations test sensitivity, not exhaustive correctness.

Negative/metamorphic catalogs require exact Cargo target/name references to
ordinary non-ignored tests. Discovery validates identities; native CI separately
executes tests. A catalog description is not a proof that its assertions are
adequate or its normative interpretation correct.

## Remaining work and meaningful exclusions

Use current uncovered-line artifacts rather than stale per-file percentages
to prioritize symbol/text and high-level lifecycle/resource interactions.
Preserve literal independent output/error oracles; fresh/reused agreement alone
can reproduce the same bug.

Existing tests cover public envelopes/metadata, reader/sink
errors and recovery, headers/retention boundaries, primitive arithmetic/MMR
state and framing, packed storage/operators, numeric quotas and generated
first-party codec fixtures. This does not imply every combination, generated
family or limit has adequate evidence.

Still open:

- Broader valid coding/template/resource interactions and critical error paths,
  including additional refinement, aggregation and custom-Huffman cases.
- Real allocator failures, unrepresentable slice sizes and overflow defenses.
  Direct error conversions are not evidence those states are reachable or
  covered in valid decoding.
- Defensive mode/make-up/span/transition guards excluded by valid
  representation/progress invariants; these remain in the denominator where
  instrumented, not silently marked covered or removed.
- Byte-aligned input cannot end midway through EOFB alignment padding; adjacent
  framing/error tests do not manufacture coverage of that impossible input.
- Broader mutation sensitivity, uncovered-code review, all-limit evidence and
  changed-line threshold enforcement.
- Allocator usable-size and total process-memory accounting. Existing
  post-reservation `Vec::capacity()` checks do not prove allocator/RSS bounds.

Coverage is separate from model correctness, conformance and formal
resource bounds. See the [roadmap](open-source-readiness-plan.md).

# Validation

Validation assets are separated from the published crate.

- `conformance/` is reserved for legally approved normative vectors and their
  expected bitmap hashes.
- `differential/` records pinned independent oracles and every disagreement.
- `negative/` covers truncation, malformed lengths and references, extreme
  values, graph failures, and corrupted coding payloads.
- `metamorphic/` checks equivalent organizations, operator identities, and
  deterministic decoding.
- `performance/` measures scaling, malformed-input worst cases, allocations,
  and peak memory against owner-approved thresholds.

No external vector may be added until `source-provenance.json` contains its
source, revision, hash, license, redistribution status, and approved use.

The official ITU assets remain outside this repository. For an approved local
copy, build `decode_vector` and run:

```text
python3 validation/conformance/run_itu.py \
  --vectors /path/to/extracted-package \
  --decoder target/release/examples/decode_vector \
  --expected-differences validation/conformance/expected-differences.json \
  --output validation/conformance/itu-t88-2018-results.json
```

`--vectors` accepts either the parent extraction directory or the
`JBIG2_ConformanceData-A20180829` directory itself. The runner checks the exact
file set, sizes, and hashes against the committed manifest before decoding.
Use `--archive /path/to/conformance.zip` to check the original archive hash too.
Each decoder invocation has a 60-second timeout, configurable with
`--timeout-seconds`. Zero cases, missing assets, changed expectations and
unresolved failures exit nonzero. Reviewed rejections are reported separately
from exact-output passes and do not establish full conformance.

## Generated base/head performance comparison

```text
python -B tools/run_performance.py --base-ref origin/main \
  --output-dir target/performance-comparison
python -B tools/check_performance_controls.py \
  --output-dir target/performance-controls
```

The same head-side driver, generated fixtures and pinned compiler are linked
against independently built base/head release libraries. Three or more paired
runs alternate execution order on the same CPU. Every returned bitmap byte,
dimension and page number is checked against the generated expectation; work
guard-rejection and late-truncation errors are checked explicitly. Validation, output
hashing and owned-result destruction are outside the measured decode call.
Raw samples reproduce the upper median and nearest-rank p95.

The public corpus currently covers white/transition-heavy MMR, multiple pages,
an exact input-work guard rejection and late malformed input. These are not
normative vectors or a diverse PDF corpus. The shared fixture is also used by
the old smoke example and optional oracle harness; the shared spread formula
retains the existing three-percent stability policy.

Hosted timing is **informational**. Unstable measurements are inconclusive,
never a latency pass. The existing delegated twenty-percent median policy is
reported; p95 remains informational. Local `--enforce-timing` rejects a stable
median regression or an inconclusive comparison, but the hosted workflow
deliberately does not enable it. Proposed five-percent median/ten-percent p95
release thresholds still require owner ratification and a dedicated runner.

The isolated control adds a real decoder delay, checks that timing enforcement
does not accept it, then shortens packed output and requires correctness
validation to fail. A noisy slowdown may be classified inconclusive, not passed.
No production source is modified. Allocation counts, allocated peak bytes,
per-case RSS, arithmetic/text/refinement/halftone cases and representative PDFs
remain unmeasured P09 gaps. No aggregate speed or release-performance claim is
made from this lane. Missing/invalid samples, mismatched outputs and build
failures always fail, with reports/logs uploaded even when validation fails.

The gate regression tests use generated fixtures, not ITU assets:

```text
python -m pip install -r tools/requirements.txt
python -B -m unittest discover -s tools/tests -v
cargo package --locked --list | python tools/audit_package.py
```

The package audit requires a complete nonempty runtime inventory and rejects
unapproved entries; invoking it without a package list is an error.

## Workflow action policy

All 26 action references across the five workflows use full immutable commit
SHAs, with comments retaining their upstream moving-ref names. Revisions were
resolved from the existing official action repositories; a pinned revision is
not an independent approval of upstream code.

Rust setup uses a pinned generic action with explicit compiler inputs.
Version-named upstream action wrappers hard-code their version and do not
accept that input, so they are not used as implicit compiler selectors.
MSRV remains 1.89.0, Verus uses 1.98.1, fuzz/coverage use their existing dated
nightly, and distribution intentionally tests both MSRV and current stable.
All jobs have explicit budgets; native Cargo and Verus now have 20-minute
caps instead of the hosted six-hour default. Existing fuzz, coverage,
distribution and performance budgets are unchanged.

```text
python -m pip install -r tools/requirements.txt
python -B tools/check_workflows.py \
  --output target/traceability/workflow-policy.json
```

The audit uses pinned PyYAML only as a development dependency, preserving
GitHub's `on` key and checking parsed block/flow YAML rather than searching
script text. Duplicate keys, merge keys and custom tags fail explicitly.
It requires remote SHA-pinned actions, explicit Rust versions or the reviewed
stable matrix, read-only permissions and bounded jobs without failure masking.
Local/container actions and reusable jobs need an explicit policy extension;
they are not silently skipped. Failed validation removes prior success output.
Native CI runs the audit and uploads source-bound reports on all three hosts.

The weekly root `github-actions` entry in `.github/dependabot.yml` proposes
action updates. Review the upstream revision and require all checks before
merging; updates do not authorize changing the compiler/profile, granting
write permissions or automatic merging. Configuration does not prove that
Dependabot has delivered a PR. Branch protection, official-asset CI access,
dedicated performance hardware and the complete P10 contract remain open.

## Executable test catalogs

Every negative/metamorphic case now names nonempty exact `{target, name}` test
references. All 29 cases resolve to 38 references and 37 distinct ordinary
tests; file-only `coverage` entries are no longer accepted. Missing, ignored,
duplicated or wrong-target references and empty catalogs fail explicitly.
Failed validation removes any previous output report, preventing stale success
evidence from being mistaken for the current result.

```text
python -B tools/check_test_catalogs.py --discover-tests \
  --output target/traceability/test-catalogs.json
```

The checker validates source identities and actual Cargo test discovery, and
records catalog hashes, test locations and source-file hashes. It does not
execute tests or certify that English statements faithfully describe their
assertions. Native CI separately runs the ordinary tests before checking the
catalogs on Linux, Windows and macOS. The report is uploaded with traceability
artifacts. Normative statement completeness and runtime code coverage remain
separate measurements.

Five focused public regressions strengthen the migrated claims: exact local
duplicate-order and globals/page collision errors, missing-reference fields,
repeated metadata/error results, and every-byte truncation with the one valid
prefix lacking optional EOF. Duplicate local wire numbers are rejected as
`NonIncreasingSegmentNumber`, while the globals/page collision produces
`DuplicateSegmentNumber`; the catalog preserves this existing distinction.

## Proof and normative link audits

Proof audits parse Rust syntax and include production functions after test-only
items, nested modules and path-included runtime modules. Unsupported syntax or
ambiguous module resolution fails closed. Evidence schemas, duplicate-key
checks, exact owner/name matching, verification annotations and source hashes
prevent unrelated methods or stale reviews from satisfying the inventory.

Use the pinned Verus report with:

```text
python tools/audit_production_coverage.py \
  --verus-json verus-production.json --output /tmp/coverage.json \
  --require-complete --require-exact
```

The exact-source release gate currently fails on the 2 explicitly recorded
runtime/proof divergences. Routine CI checks the reviewed inventory, not a claim
that every executing body has been proved. New or changed gaps fail the
inventory checks. `update_proof_dispositions.py --output NEW_FILE` creates
unreviewed proposals only and cannot overwrite the reviewed disposition file.

The five value/operator bridges share exact executing/proof bodies and export
lossless-value or operator-equality postconditions. Three integer conversions
retain `From`; Boolean conversion and enum equality use equivalent explicit
bodies supported by the pinned verifier. The Boolean helper has one narrowly
documented Clippy exception rather than a proof-only implementation or a new
standard-library assumption. All byte/Boolean values and 25 operator pairs are
tested; u32 boundaries and deterministic samples supplement its universal proof.
`tools/check_value_bridge_proofs.py` requires each of five compiling wrong
results to fail its strong contract while the selected weaker control verifies.

`tools/check_mmr_span_proofs.py` similarly checks three invalid/white/empty
span mutations against the exact executing MMR wrapper. Its weaker controls
retain storage and typed-error guarantees but omit only the byte no-op
relation. Runtime tests compare black/white spans with a scalar pixel oracle
across byte/word boundaries, other rows and padding, and check exact invalid
errors with full non-mutation. Formal black-span pixel behavior remains open;
the newly direct proof covers storage/frame and the stated no-op subsets.

`tools/check_region_dispatch_proofs.py` checks the optimized source-offset
composition wrapper with three overflow/zero-area mutations. Each compiles as
ordinary Rust, fails the byte no-op guarantee and verifies when only that
guarantee is removed. Controls retain storage, exact invalid-request errors
and valid zero-area success; mutant-only proof hints expose unchanged field
metadata after the deliberate byte mutation. General composed pixels remain
covered by exhaustive/random scalar-oracle tests rather than these contracts.

`tools/check_text_batch_proofs.py` checks the executing text-placement batch
dispatcher with four compiling mutations. Empty/zero-height byte mutations
fail its no-op contract, and single/multiple pending-entry mutations fail its
successful-clearing contract. Controls remove only the affected relation,
retaining storage/layout, zero-height success and the other semantic guarantee.
The batch kernel proves zero-height success without byte changes. Runtime
tokens are unchanged; general pixel composition and placement order remain
scalar-oracle tested rather than formally derived by these frame contracts.

`tools/check_bitmap_chunk_proofs.py` checks four compiling wrong byte-kernel
implementations: incorrect read/write high-byte shifts, a successful write
changing its prefix, and an overflowing write changing storage. Exact values
or byte frames reject each; controls omit only the affected relation and
retain complete range acceptance, length and unaffected value/frame relations.
Ordinary tests use independent `to_be_bytes`/`from_be_bytes` oracles and compare
all storage around complete, short and overflowing accesses. These are helper
contracts, not a complete composition or release proof.

`validation/differential/run_oracles.py` runs pinned external executables as
black-box processes. It records only versions, binary and normalized
diagnostic hashes, output hashes, and classifications; it never copies oracle
source or vector content into the repository. Pass `--poppler-revision` when
the executable was built from a pinned source revision. The committed
`results.json` records the versioned system comparison, while
`pinned-poppler-results.json` records the exact approved source revision.

## Runtime coverage comparisons

The Linux coverage workflow always enforces the reviewed global and per-file
baseline. Changed-line coverage is informational and compares with the pull
request's base commit or the push event's previous commit. A full checkout
may not contain that previous commit after a branch history rewrite, so the
workflow fetches the exact comparison commit from `origin` when it is missing.
If that fetch fails, the job fails rather than skipping the comparison or
substituting another revision. An initial push without a previous commit still
runs all baseline coverage checks without a changed-line comparison.

## Bounded fuzz campaigns and replay

The Linux runner retains the original seven pinned cargo-fuzz targets and adds
`document_context_structured`, for eight targets in each complete campaign:

```text
python -B tools/run_fuzz.py run --profile smoke \
  --corpus-dir target/fuzz-corpus --output-dir target/fuzz-run
python -B tools/run_fuzz.py replay --targets document_arbitrary \
  --artifact /path/to/reproducer --output-dir target/fuzz-replay
```

Output directories must be fresh, so a prior result cannot be overwritten or
mistaken for the current run. The smoke profile gives each target 60 seconds;
extended runs give each 300 seconds. Both use a five-second per-input timeout,
1024 MiB RSS limit and 16384-byte input bound, with an additional runner
deadline. Only the runner's own newly created process group is terminated if
its deadline expires. Expected parser errors are fuzz inputs, not failures;
target failures, timeouts, build errors and reproducers make the run fail.

The seed example reuses the shared project-generated page fixture and constant
byte patterns. No ITU vectors, third-party samples or private files are read.
Existing generated seed names must match their bytes; changed/symlinked,
oversized or excessive corpus entries fail explicitly. Reports bind tool pins,
source revision/content hashes, bounded commands, logs and reproducer hashes.
Single-file replay uses one execution rather than starting an open campaign.
Every otherwise successful target must emit exactly one valid, positive
libFuzzer final execution counter. Missing, zero, malformed or duplicated
counters fail the run even when the process exits zero; successful replay
additionally requires exactly one execution. Reports include verified
per-target counts and their exact total; the CLI summary includes that total.
Failed targets retain their original exit status, logs and reproducers without requiring final
statistics from a crashed or terminated process.

PR/push runs use smoke budgets without restoring or saving a trusted corpus.
Daily scheduled runs use extended budgets. Manual runs choose a profile.
Only scheduled/manual runs on the repository's default branch may restore or
save the generated corpus cache; every other run remains isolated. Failed jobs
always upload the run/control diagnostics and captured reproducers to GitHub
artifacts, without granting write permissions or secrets to pull requests.

`tools/check_fuzz_controls.py` verifies real failure capture and replay using a
temporary local clone and a project-generated assertion/input. Baseline replay
must pass, the isolated altered target must produce the exact input artifact,
that artifact must reproduce the expected failure, and restoring the original
target must make replay pass. Production sources are never altered. This
control does not count as a production fuzz finding or a conformance test.

Trusted-main validation on 2026-10-20 used immutable revision
`2a621a2702e6e49b946b393ab5ea82e3a9dd9d34`:

- [Smoke run](https://github.com/microsoft/jbig2_rs/actions/runs/39504279479):
  all seven targets passed with 110,251,322 libFuzzer executions.
- [Extended run](https://github.com/microsoft/jbig2_rs/actions/runs/39505509187):
  all seven targets passed with 451,371,561 executions at 300 seconds per target.
- The extended run restored the smoke run's actual main-scoped cache; every
  starting-corpus file/byte count matches the smoke report. It saved a new
  main-scoped cache after completion. Both real capture/replay controls passed.

Those runs used the main runner before execution-accounting enforcement;
counts were independently reconstructed from raw final-stat logs and source
identities checked. Executions are not unique cases or code coverage. These
manual default-branch runs validate the trusted campaign/cache path, not that
the scheduled trigger itself has fired.

The subsequent actual
[scheduled extended run](https://github.com/microsoft/jbig2_rs/actions/runs/39534526812)
on `de2fbf31b8fc37249f33625c6ba9030db6559f66` passed all seven then-current
targets with 566,048,292 executions, real replay controls, cache restoration
and a new cache save. This observes the scheduled trigger on that older
seven-target source, not a scheduled deployment of the later eighth target.

P08 remains incomplete: coverage-driven structured/stateful targets,
regression-corpus review and periodic minimization are still pending. These
bounded campaigns do not replace unit tests, proof checking or normative review.

### Structured context sequences

`document_context_structured` interprets two zero-padded four-byte shape
controls, followed by at most eight opcode/u16 argument triples. Shapes are
bounded to 128 by 16 pixels and three pages, with white or transition-heavy MMR
using the existing project-generated benchmark fixture. Up to 256 further
bytes may be used as arbitrary malformed input. Decoder limits independently
bound streams, dimensions, pages, segments, symbols, references, memory and work.

Operations exercise fresh/reused standalone and embedded decoding, inspection,
empty validated globals, reader/sink output, truncation/bit changes, typed width
rejection, and explicit scratch release. Every operation is followed by a valid
decode and known packed-pixel checks including padding. Fresh/reused errors are
compared by typed variant and all associated fields, not formatted strings.
Nonempty symbol globals and other coding families are not covered by this target.

The same program runs as three ordinary `fuzz_context` tests over all 256 width
controls, every opcode, short/long control inputs and recovery paths. Generated
seeds execute the program before being written. Harness source identities and
trusted cache keys include both the scenario source and shared MMR generator.
The new fixture hash selects a separate corpus namespace; existing cache entries
are not deleted and PR cache isolation is unchanged.

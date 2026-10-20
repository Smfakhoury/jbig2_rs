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

The exact-source release gate currently fails on the 10 explicitly recorded
runtime/proof divergences. Routine CI checks the reviewed inventory, not a claim
that every executing body has been proved. New or changed gaps fail the
inventory checks. `update_proof_dispositions.py --output NEW_FILE` creates
unreviewed proposals only and cannot overwrite the reviewed disposition file.

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

The Linux runner retains all seven pinned cargo-fuzz targets:

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
Failed targets retain
their original exit status, logs and reproducers without requiring final
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

P08 remains incomplete until hosted campaign/artifact behavior is validated
and coverage-driven structured/stateful targets, regression-corpus review and
periodic minimization are completed. These bounded campaigns do not replace
unit tests, proof checking or normative review.

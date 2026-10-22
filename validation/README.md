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

The seven existing official rejections are now `pending_normative_review`, not
reviewed malformation determinations. A pending entry pins the complete typed
error, stream hash and relevant runtime source hashes. It remains an unresolved
failure even when those identities match; changed errors, success, crashes,
unknown fields, stale source identities and invalid JSON fail closed. No
runtime acceptance, marker, retention or recovery policy is relaxed.

`tools/verify_provenance.py` validates the shape and byte identities of these
truthful blocked records; its success is not a conformance execution pass.
`--require-exact-conformance` separately rejects any recorded non-passing case.
The official `run_itu.py` invocation remains nonzero for the seven unresolved
cases. Tests exercise all three outcomes, including strict-gate rejection.
Hash-bound Rust, JSON and Python files have explicit LF checkout rules so the
same source bytes are checked on Linux, Windows and macOS. Approved main
integration requires refreshing the diagnosis identities, not skipping them.

The prior TT1 MMR-truncation explanation was disproved: its initial dictionaries
complete before a text Huffman read exhausts the segment entropy body at file
byte 179. See [the source-bound seven-case review](conformance/seven-rejection-review.json)
for the distinction between observations and unresolved normative causes.
The T.88 7.2.4/7.2.8 retention conflict still requires owner adjudication.

Independent first-party pixel fixtures verify both BMP readers for both palette
orders, top-down/bottom-up orientation, packed-bit padding and four-byte row
padding. An eleven-page fixture checks numeric page pairing. These fixtures
contain no official bytes and found no normalization bug in the covered layouts.
The existing locked Hayro 0.3.1 harness supplies additional rejection evidence,
but its single-image API and FNV64 output summary are not full-byte or
multi-page conformance proofs; an error cannot corroborate an unexposed entropy
position.

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

All 38 action references across the five workflows use full immutable commit
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
references. All 43 cases resolve to 67 references and 62 distinct ordinary
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

Direct typed-error tests cover every generic, symbol/text and Huffman adapter
variant, extreme fields and delegated I/O/allocation/limit/malformed errors.
They construct error enums rather than encoded streams and do not certify
when a decoder must produce each error. Twelve compiling mutation controls
must run exactly one passing baseline and one failing mutant test. The runner
supports explicit `lib` and named integration targets without accepting
zero-test passes or compiler failures as detection. With the owner-merged
reference-role controls the complete runtime mutation set has twelve cases.

The public reference-contract tranche adds six integration tests and two exact
negative-catalog entries. It covers 216 local/global role, cardinality and table
count scenarios plus three eager table-payload error precedence cases. Failure
checks assert precise typed errors, zero page-sink effects, immutable shared
globals and recovery through reused contexts. Positive opaque dictionary and
region payloads assert inspection metadata only, not valid entropy streams or
pixel conformance. No new codec-family payload, fuzz target or seed is added.

Eight isolated runtime mutation controls check I/O-kind preservation,
inclusive stream/table limits, page-sink failure propagation, intermediate-only
refinement references, halftone cardinality, global type classification and
table-error classification. Every mutant must compile and fail its exact
selected test after a passing baseline; empty/duplicate sets, ambiguous sites
and non-test failures are rejected. These are test-sensitivity controls, not
Verus proofs or exhaustive semantic coverage.

The Huffman width regressions independently count significant bits for all
single-bit positions and additional limb patterns at widths 0 through 257.
They also reject oversubscribed canonical trees at 64/128/192-bit boundaries,
check neighboring lengths, and retain decoding of valid complete/incomplete
trees through 255-bit prefixes. Both tests fail on the former zero-offset
shortcut; runtime and project-model expressions are corrected together.
These local regressions are not official conformance vectors or a complete
canonical prefix-freedom theorem.

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

The exact-source release gate currently fails on the remaining explicitly
recorded MMR capacity-accounting divergence. Routine CI checks the reviewed inventory, not a claim
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
errors with full non-mutation. These wrapper checks cover storage/frame and
the stated no-op subsets; the separate primitive theorem below does not by
itself establish complete MMR raster-loop semantics.

`tools/check_black_span_proofs.py` checks six compiling output mutations of
the exact optimized `Bitmap::fill_black_span`: cleared requested head/word
bits, cleared untouched head bits, changed prefix/padding bits and an erroneous
valid-span failure. For valid bitmap storage the strong contract states success,
exact effects on every storage bit, the visible-pixel relation and the
outside-byte frame. Controls omit only the new semantic result or new success
guarantee and retain the old bounds, metadata/length and empty no-op/success
contracts. No storage-validity precondition is added. Fully proved mask and
step lemmas connect the native first-mask/word/byte/tail algorithm to these
postconditions; its erased runtime tokens are unchanged. Exhaustive bounded
scalar comparisons additionally test arbitrary dirty padding and exact bounds
errors, without introducing a new encoded-input family.

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

`tools/check_operator_bit_proofs.py` verifies four compiling wrong constant
byte/word results against the universal per-bit Boolean operator contracts.
The weaker controls remove only that output relation, preserving the
independently proved model lemmas. Normal tests exhaust all 65,536 byte pairs
for each operator and compare every bit against an independent truth table;
boundary-word pairs supplement the universal u64 proof. This is primitive
operator semantics, not complete raster or wire-format conformance.

`tools/check_bitmap_chunk_proofs.py` checks four compiling wrong byte-kernel
implementations: incorrect read/write high-byte shifts, a successful write
changing its prefix, and an overflowing write changing storage. Exact values
or byte frames reject each; controls omit only the affected relation and
retain complete range acceptance, length and unaffected value/frame relations.
Ordinary tests use independent `to_be_bytes`/`from_be_bytes` oracles and compare
all storage around complete, short and overflowing accesses. These are helper
contracts, not a complete composition or release proof.

`tools/check_bitmap_compose_proofs.py` checks the actual optimized ordinary
composition dispatcher. Four out-of-frame byte mutations and a wrong typed
error compile but fail the original frame/success contract; controls remove
only the affected relation while retaining storage/metadata and the other
guarantee. The full proof verifies all aligned/unaligned access arithmetic and
the outside-clipped-byte frame, not complete pixel/operator equivalence.
Expanded independent scalar oracles compare all packed storage across word
edges, clipping, empty dimensions and maximal origins; bit-loop oracles check
complete shifted byte/word/tail availability. Coverage exclusions and the
reviewed floor are unchanged; the separate P06 family work owns floor updates.

`tools/check_arithmetic_decision_proofs.py` checks four compiling wrong
decision/context mutations against the exact arithmetic decision, checked
wrapper and lower/upper exchange contracts. Weaker controls retain the prior
validity/signal guarantees; the exchange controls retain their conditional
decision relation while omitting only the context outputs. Tests independently
exercise all 47 probability rows, both MPS values, boundary routes and context
updates before truncation. The theorem covers successful decisions and context
steps against the project model, not exact renormalized registers, byte
consumption or normative entropy-stream equivalence.

`tools/check_arithmetic_refill_proofs.py` checks six compiling mutations of the
two exact refill helpers. Signal, code, bit count, marker, overflow consumption
and checked-success relations are rejected individually; controls omit only
the affected new relations while preserving original validity/frame
guarantees and other new outputs. A numeric oracle checks all 65,536 byte pairs
at exact code-addition boundaries, EOF and existing-marker/error frames through
both helpers. This proves the project refill step, not normative register
translation, caller timing or complete initialization/renormalization.

`tools/check_arithmetic_initialization_proofs.py` checks six compiling
constructor mutations against exact two-byte acceptance and initialized
code/count/marker/position/interval outputs. Each weaker control removes only
its affected relation and retains old validity plus unaffected new outputs.
Native numeric tests check all 65,536 byte pairs, suffix invariance and exact
empty/single-byte EOF fields. The project initialized-state model composes
the seed and refill; a fully proved bound and shift-constant bridge establish
safe initial scaling. Normative interpretation and later stream decoding
remain separate.

The required manifest now has fifteen groups and 66 contrasts.
`tools/run_semantic_proofs.py` still runs all configured isolated semantic
checkers locally with at most two concurrent children by default. `--only ID`
runs one complete known group with one child; unknown IDs fail. Each checker
retains its pinned verifier, selected-target checks, native compilation and
strong/weaker solver limits. Group IDs and required contrast counts come
solely from `CHECKS`; future groups need no workflow or collector list update.

P05 CI uses three job definitions, each capped at 20 minutes:

* `whole` verifies and compiles the whole crate, checks the exact production
  inventory diff and traceability, seals its outputs, and emits `--matrix`
  directly from the complete current `CHECKS` manifest.
* `semantic` needs `whole` and runs `--only` for each dynamically selected group
  on independent runners, with `fail-fast: false` and `max-parallel: 4`.
  Its Python/Rust/Verus pins and archive checksum match `whole` exactly.
* The stable required job `verify` runs with `if: always()` and needs both
  upstream jobs. A full-SHA-pinned official `actions/download-artifact`
  downloads actual artifacts from the current run/attempt without merging
  their directories. No cross-run token or repository selector is supplied.
  `--aggregate` requires both job results to be `success`, the exact complete
  set of groups and case counts, passing native/strong/weaker/baseline results,
  exact case/target identities, report/log/checker hashes, current complete
  source/tool manifests, whole success/native compilation, exact inventory
  bytes and verifier-backed traceability. Missing, stale, duplicate, skipped,
  cancelled, malformed or failed evidence fails closed.

Partial artifact names include run ID, run attempt and group (`whole` is
reserved). Only the collector uploads `verus-bootstrap-log`, once; its
canonical layout retains `verus-production.json`,
`production-proof-coverage-ci.json` and `target/traceability/` including every
individual report/log and `semantic-proof-summary.json`. Every job uploads
available diagnostics with `always()`; partials remain available on failure.
Rerun the **entire workflow**, not only failed jobs: a new attempt must produce
its own complete evidence set. Local `--aggregate` likewise requires genuine
whole and all-group evidence, never a successful subset. Schema-v1 summaries
predating source/run bindings are intentionally incompatible.

The official download-action v7.0.0 tag resolves to
`37930b1c2abaa49bbe596cd826c3c89aef350131` (official GitHub tag metadata).
The workflow-policy tests protect the stable collector, dynamic matrix,
bounded parallelism, matching tool setup and immutable artifact names.
No branch settings, runtime/proof source, proof dispositions, trust assumptions,
coverage floors or owner-controlled P10 release settings change. The aggregate
is orchestration evidence, not a substitute for individual proofs or a timing
claim.

`validation/differential/run_oracles.py` runs pinned external executables as
black-box processes. It records only versions, binary and normalized
diagnostic hashes, output hashes, and classifications; it never copies oracle
source or vector content into the repository. Pass `--poppler-revision` when
the executable was built from a pinned source revision. The committed
`results.json` records the versioned system comparison, while
`pinned-poppler-results.json` records the exact approved source revision.

## Runtime coverage comparisons

The Huffman limb-boundary correction is measured on immutable source
`d4ff9a688bba5b5574c4c119eb3d95fb74efe455`: 10287/12622 lines,
9900/12933 regions and 1892/2638 branch outcomes. Its upward-only baseline
refresh retains all prior global/per-file fractions. Removing the incorrect
guard removes one region and two branch outcomes; source-line scope,
exclusions and release goals are unchanged. The resulting 81.50% line and
71.72% branch coverage do not meet the proposed 90%/85% release goals.

The Linux coverage workflow always enforces the reviewed global and per-file
baseline. Changed-line coverage is informational and compares with the pull
request's base commit or the push event's previous commit. A full checkout
may not contain that previous commit after a branch history rewrite, so the
workflow fetches the exact comparison commit from `origin` when it is missing.
If that fetch fails, the job fails rather than skipping the comparison or
substituting another revision. An initial push without a previous commit still
runs all baseline coverage checks without a changed-line comparison.

### Public envelope contracts

`tests/public_envelope.rs` adds twenty ordinary public-API tests using
project-generated document structures, blank pages and the existing generated
MMR white-row encoding. It does not extend a codec-family grammar or fuzz target.
The negative/metamorphic catalogs bind every new test to executable Cargo
discovery, not file-only claims.

The assertions cover every public metadata field, ascending metadata order
versus stream-order page delivery, known/unknown declared page counts, zero
dimensions, default-black padding, known/unknown striped heights, profile
placement/supersets, optional/necessary extensions and both comment encodings.
They also check immutable nonempty global-reference chains, last-reference
retention with reordered embedded headers, exact inclusive resource boundaries,
unknown-length MMR envelope termination and second-page sink failure effects.
Typed malformed/unsupported/limit fields and literal packed pixels are the
oracles; fresh/reused agreement and formatted error strings are not.

The original PR42 pinned before/after collection on unchanged production source measures
**135 additional runtime source lines, 113 regions and 62 branch outcomes**.
The freshly measured main-side starting point is 10,069 lines / 9,704 regions /
1,798 branch outcomes, already above the older committed floor of 10,022 /
9,667 / 1,785. The tests bring the measured union to 10,204 / 9,817 / 1,860.
Denominators remain 12,617 / 12,931 / 2,640, with no lost previously covered
coordinates or decreasing per-file fractions. Source-bound identities and
coordinate checks are recorded in
[`coverage/public-envelope.json`](coverage/public-envelope.json).

After normally merging main `d1bf46914c6d7e57177b7c02087cca2555794b52`, a new
independent collection is bound to immutable merge commit
`c95032a826771c66266c00daf0b47a21cb1db206` before its separate floor update.
The runtime source tree and pins are byte-identical to that main; all twenty
public-envelope tests are unchanged. Fresh main/merged-branch coverage is
10,072 / 10,207 lines out of 12,620, 9,710 / 9,823 regions out of 12,932 and
1,799 / 1,861 branch outcomes out of 2,640. These are new collections, not sums
of historical deltas. Main's bitmap source changes the line/region mappings;
the source-coordinate denominators match between the two new runs, not the
older source. Every current-main covered coordinate is retained, and exact
integer comparisons confirm no previous floor global/per-file fraction falls.
The full 312 Rust / 238 Python tests, strict MSRV Clippy/format/docs, provenance
and executable catalogs, verified package and offline downstream consumer pass.
The original evidence remains intact; the refreshed identities, mapping checks
and raw report hashes are in
[`coverage/public-envelope-main-integration.json`](coverage/public-envelope-main-integration.json).

Production sources, specifications, proofs, the normative ledger, dependencies
and coverage exclusions are unchanged relative to the corresponding main.
Allocation-failure/overflow paths and
broader entropy/resource interactions remain genuine gaps; the proposed
90% line / 85% branch release goals are still unmet. These generated contracts
are not normative conformance or release approval.

## Bounded fuzz campaigns and replay

The Linux runner retains the original seven pinned cargo-fuzz targets and adds
`document_context_structured` and `codec_families_structured`, for nine targets
in each complete campaign:

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

P08 remains incomplete: more coverage-driven structured/stateful families,
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

### Structured codec families

`codec_families_structured` uses the shared, first-party
`tests/support/codec_scenarios.rs` encoder, scalar pixel oracle and deterministic
60-entry seed manifest. It creates complete public-decoder streams, not private
primitive calls. No official vectors, third-party codec code or external samples
are used. Its two related collective-bitmap families are:

- MMR pattern dictionaries with one through four patterns; halftone Gray planes
  of depth zero, one or two, each with an explicit EOFB marker. Signed fractional
  grid origins, rotated grids, clipping, all five pattern operators and default
  pixels are checked against independently painted Boolean pixels.
- Huffman symbol dictionaries with raw packed or MMR collective bitmaps;
  Huffman text with one through four instances, all four reference corners,
  transposition, strip sizes 1/2/4/8, same-strip overlap or separate strips,
  all four text operators and default pixels.

The grammar is sixteen zero-padded shape bytes followed by at most four
opcode/byte-argument pairs. Bytes after offset 23 are ignored. The first byte
selects a family modulo two. Page sizes are 1..33 by 1..9, pattern/symbol sizes
1..9 by 1..3, dictionary counts 1..4, and halftone grids 1..9 by 1..5. Origins
are -3..3 before the fixed fractional offset; every encoded length derives from
these bounded structures. Operations 0..7 are reused standalone decoding,
random-access decoding, nonempty reusable globals/embedded decoding, reader,
sink, typed malformed call/recovery, typed limit rejection, and prefix
truncation/recovery. Every input first executes a valid pixel-checked decode.

All fifteen decoder limits are explicitly assigned:

| Limit | Maximum |
|---|---:|
| Width / height / pixels | 36 / 9 / 324 |
| Decoder-managed memory / stream bytes | 1,048,576 / 4,096 |
| Pages / segments / symbols | 1 / 5 / 64 |
| References per segment / total references | 1 / 1 |
| Reference depth / nesting depth | 2 / 2 |
| Work units | 262,144 |
| Cumulative bitmap bytes / retained bytes | 4,096 / 65,536 |

The symbol bound also permits the 35-entry text run-code table. Ordinary
`fuzz_codec_families` tests execute all opcodes, all fourteen exercised limit
kinds, flag/reference/trailing-data faults, every generated-stream prefix,
pattern widths 1..9 and heights 1..3, packed-byte boundaries, seed determinism,
and bounded empty/long control inputs. The nesting limit is set explicitly but
is not claimed as an exercised rejection: these fixtures do not nest codecs.
Every successful API path checks dimensions and complete packed pixels including
padding; sink output is checked directly, not through fresh/reused agreement.

Runner source identities, isolated failure-control copies, seed generation and
trusted corpus cache fingerprints include the exact shared fixture path. No
new runtime coverage exclusion or package content is introduced. PR/push runs
still cannot restore/save the trusted-main cache. Production source, proof
inventory, dispositions and attestations are unchanged.

Local pinned validation on 2026-10-21, bound to immutable source commit
`f338287afe3e6026fe0104436ec4c25c5f726f5a`, passed all nine smoke targets with
55,586,929 verified final-counter executions. The new codec target executed
22,431 inputs; its single-file replay executed exactly once, and real isolated
failure capture/replay/restoration controls passed. Exact per-target counts,
source identities and report hashes are recorded in
[`coverage/structured-families.json`](coverage/structured-families.json).
These local runs did not read/write a trusted cache and do not claim that a
nine-target scheduled or trusted-main deployment has occurred.

This does not close arithmetic symbol/text, aggregate/single-instance refinement,
arithmetic/skip halftone, deeper dictionaries, arbitrary custom tables or corpus
minimization gaps. Those require further first-party valid encoders and
independent oracles; successful bounded fuzz runs are not normative conformance.
## Arithmetic error-context and adapter follow-up

Three direct-state arithmetic unit tests check entry-point context updates and
exact partial EOF registers, unchanged input, every context-byte pair through
restore/reset/default, and all arithmetic error conversion/formatter variants.
No entropy payload, coded family or fuzz seed is added. Seven actual compiling
decision/context proof contrasts include error-only index/MPS corruption and
input replacement; weaker controls retain the other guarantees.
The complete required profile remains sixteen groups across four shards,
now with 75 contrasts and unchanged CI jobs. Actual full pinned Cargo
verification/compilation passes 1172 project obligations with zero errors.
All 352 native tests pass with 14 existing ignored tests; fresh coverage is
82.23% lines, 77.29% regions and 72.40% branch outcomes, with unchanged
denominators and reviewed floors. P04/P05/P06, independent normative review,
the capacity prerequisite and the proposed release coverage goals remain open.

## Complete arithmetic decision-state composition

The next tranche exposes exact signal/acceptance, interval, code, count, input
position/current byte and marker results at both decision entry APIs, including
partial failures. Typed EOF/overflow projection is explicit project policy.
Two opaque standard conversion calls become the identical explicit constructor;
the unchanged reviewed From body and all-signal numeric/prior-path parity tests
support that expansion. All other executing tokens and 656 caller preconditions
are unchanged. Fifteen actual compiling decision contrasts fit the same
sixteen-group/four-shard required profile, now 83 contrasts.

Actual full pinned Cargo verification and compilation pass 1173 project
obligations with zero errors. The complete native collection passes 355 tests
with 14 existing ignored tests. Fresh measured coverage is 82.24% lines,
77.30% regions and 72.40% branches, with unchanged denominators and reviewed
floors. Seven selected E.3.2 rows gain actual functional project-step evidence;
all 249 records remain provisional (73 functional / 150 state-preservation /
26 safety-only), and exhaustive normative review remains incomplete.

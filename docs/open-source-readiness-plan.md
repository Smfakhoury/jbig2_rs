# Open-source readiness gates P01–P12

This is the acceptance contract, not a development diary or publication
authorization. [Publication readiness](publication-readiness.md) records the
revision-qualified snapshot and remaining holds. P01–P03 are complete;
P04–P06 remain incomplete. Operational later lanes and this cleanup do not
automatically complete P07–P12.

## Scope and execution rules

Deliver a lean, reproducibly testable broad bi-level decoder with exact-source
Verus proofs and lawful correctness/conformance/performance validation.
The ordinary crate must work without Verus, `vstd` or repository proof files.
Colour is unsupported; encoder work is deferred. T.88 (08/2018) /
ISO/IEC 14492:2019 and T.6 for MMR are the normative baseline, not text to copy
into the repository. Full Annex F profile 1 conformance is not claimed.

- Preserve public APIs, strict typed errors, resource limits, safe Rust and one
  runtime source tree. A simpler proof-only algorithm is not an exact-source
  proof of the executing implementation.
- Keep independently reviewable changes, splitting semantic work by bounded
  claim when necessary. Bind evidence to immutable source/test/tool identities.
- Inventory consumers before moving/deleting files. Keep legal/security
  notices, necessary models, proof support and test oracles. Preserve Git
  history; do not copy chronological session notes into an archive.
- Store routine generated reports in ignored `target/` or CI artifacts.
  Retain minimal policy, schemas, pins, reviewed dispositions and baselines.
- Missing, empty, stale or incompatible evidence fails closed. Unavailable
  lawful assets are unavailable, not passed.
- Project publication approval, crate publication, asset/oracle redistribution,
  visibility, signing and `publish = false` changes are separate decisions.
- Existing standard-trust, normative/model review and protected fixture holds
  remain in force. Do not add assumptions, local standard/equality
  specifications, termination waivers, pin changes or weakened gates to bypass
  a blocker.

## P01 — Freeze scope, approvals and file disposition

**Dependencies:** none. **Status:** complete.

1. Record the owner's project publication approval in public-safe form and
   reconcile obsolete blocked wording without exposing legal correspondence.
2. Confirm SPDX license and required Microsoft notices; identify official
   vector acquisition/redistribution permissions separately.
3. Inventory every tracked path as keep, relocate, regenerate or remove,
   including its consumers and legal/proof/test role.
4. Record baseline tests, official classifications, package inventory,
   dependencies, production proof accounting and benchmark input/source hashes.
5. Define supported claims and a separate crates.io gate.

**Acceptance:** reviewed inventory covers every tracked path before deletion;
unresolved license/asset decisions explicitly block dependent steps. Historical
classifications do not override the current withdrawn-rejection policy.

## P02 — Make audits and conformance runners fail closed

**Dependencies:** P01. **Status:** complete.

1. Require a valid nonempty real package inventory with required runtime files;
   reject empty, forbidden, malformed and incomplete inventories in tests.
2. Discover documented ITU root layouts deterministically; verify pinned
   archive/per-file hashes and the exact expected case set.
3. Fail on zero cases, missing images, duplicates, unknown classifications,
   changed inputs, bitmap/page mismatches and unresolved errors. Bound subprocess
   time and retain reliable diagnostics.
4. Test runners with small first-party synthetic fixtures, including failures,
   without requiring official downloads.
5. Inventory executing code after test-only items, nested/path modules,
   alternate cfg branches and excluded scopes. Do not truncate at `cfg(test)`
   or conflate unrelated same-named functions.
6. Reject duplicate JSON keys and schema-invalid retained evidence.
   Disposition generators produce proposals, never automatic approval.

**Acceptance:** deliberately broken fixtures exit nonzero; all real executing
functions are accounted for; expected runtime outputs remain unchanged.

## P03 — Establish the proof-free crate boundary

**Dependencies:** P02. **Status:** complete.

1. Retain the single runtime tree and cfg-gated ghost design; do not add a
   second generated or hand-maintained decoder.
2. Create the real `.crate` using `cargo package --locked`, audit its allowlist,
   and extract into an isolated workspace outside the source checkout.
   Build/test offline without Verus, `vstd` or proof directories.
3. Test both the archive and a tiny downstream consumer on MSRV/current stable
   and supported native platforms, including a generated exact-output decode.
4. Reject proof/validation/oracle/benchmark assets and dependencies; prove
   normal downstream builds cannot expose internal validation access.
5. Document inactive source annotations/ghost references as cfg erasure, not
   textual stripping. Internal proof cfgs are unsupported downstream.
6. Include approved license metadata, `rust-version`, description and target
   policy while keeping publication disabled.
7. In the extracted crate require a real unsafe probe to fail and the
   guard-only weaker control to compile; unconditionally restore original
   source before consumer execution.

**Acceptance:** actual archive tests and isolated downstream execution pass
without proof files/dependencies; contents are audited and allowlisted.
An empty-stdin audit is not evidence. Safety controls concern crate-authored
source, not generated/library internals or binary-wide unsafe absence.

## P04 — Complete statement-level normative traceability

**Dependencies:** P01. **Status:** incomplete.

Maintain one versioned ledger and generated human-readable reports. Every
record needs:

```text
requirement_id; standard_id + edition; clause/subclause + table/figure/step;
original paraphrase; applicability (required/unsupported/policy);
runtime symbols; model predicates; production requires/ensures;
proof/theorem symbols; exact test IDs; official-vector IDs where applicable;
proof strength; assumptions/boundaries; review status; source identity
```

1. Account for parsing/organization, arithmetic/integer/IAID, Huffman, MMR/T.6,
   generic/refinement, symbol/text, pattern/halftone, composition, retention and
   globals. Separate project resource policy from normative rules.
2. Start from [`config/support-matrix.json`](../config/support-matrix.json), but
   verify exact locators against the pinned edition; coarse feature citations
   are insufficient.
3. Distinguish safety/bounds, termination, state preservation, functional step
   equivalence and end-to-end output semantics; permit multiple evidence links.
4. Separately trace context geometry, SLTP, typical-row copying, arithmetic
   transitions and output bits. Storage validity does not prove those results.
5. Bind normative parameters/tables to checked models and tests without copying
   standard prose or sample-software source.
6. Reject duplicate IDs, missing/dead symbols/tests, stale evidence and missing
   dispositions. Use stable identities, not line numbers alone.

**Acceptance:** every in-scope normative requirement, including partial/unproved
ones, is accounted for; independent review confirms the map matches actual
contracts. A verified-function count is not spec coverage. The selected
provisional ledger and feature worklists do not establish an exhaustive
denominator; see [traceability](spec-traceability.md).

## P05 — Close semantic proof and conformance gaps

**Dependencies:** P02, P04. **Status:** incomplete.

1. Audit contracts individually: arithmetic decisions/state, MMR mode/run
   consumption and raster output, dictionary export/text placement, then
   page/document semantics.
2. Strengthen storage/bounds-only contracts where the required claim is a
   transformation; prove optimized kernels refine a reviewed independent
   functional model.
3. Retain explicit reviewed limitations, audited forwarders and runtime/proof
   divergences. Bind reasons to actual source and document Rust/Verus/vstd
   trust plus conditional preconditions.
4. Re-evaluate all unresolved official positives using byte/bit traces,
   normative rules and independent oracle evidence. Decoder rejection or an
   oracle warning does not show an official positive is malformed.
5. Fix discovered runtime/model/test errors with exact regression fixtures.
   A normative rejection requires specific independent review. Keep unsupported
   colour separate from supported conformance.
6. Do not close required functionality by relabeling evidence. Prove it or,
   with owner approval, narrow the release claim and leave the stronger claim
   explicitly open.

**Acceptance:** each required claim has appropriate proof strength and executable
tests; supported official positives decode exactly or have reviewed normative
exceptions. Unresolved cases block the corresponding conformance claim.
The exact-source release audit must reject remaining divergences.
See [limitations](verifier-limitations.md) for the reviewed capacity/getter boundary,
and [validation](../validation/README.md) for protected fixture boundaries.

## P06 — Measure coverage and fill test gaps

**Dependencies:** P02; coordinate IDs with P04 and new tests with P05.
**Status:** incomplete; numerical release goals are met at the reference snapshot.

1. Use pinned `cargo-llvm-cov` and matching `llvm-tools-preview` on ordinary
   runtime builds, never ghost code.
2. Publish lines, regions, uncovered locations and measurable branch outcomes.
   Label unsupported metrics explicitly; do not substitute lines for branches.
3. Include runtime unit/integration/API tests; exclude proof-only items,
   tooling and benchmark drivers with documented syntax-scoped exclusions.
   Test validation scripts separately.
4. Require exact executable negative/metamorphic test IDs and public
   slice/reader/sink/globals/context tests.
5. Exercise every resource-limit boundary, malformed/truncated inputs, error
   propagation, clipping/operators/stripes, retention, context reuse and
   optimized/reference output/error parity.
6. Expand generated valid/minimal streams across coding/template/organization
   families only within approved fixture scope; official redistribution is not
   a prerequisite for first-party tests, but held work must not be reconstructed.
7. Exclude ignored microbenchmarks from correctness pass counts; retain useful
   equivalence assertions as ordinary tests.
8. Record baseline first; retain >=90% runtime line and >=85% branch-outcome
   goals where measurable plus evidence for critical parser/limit/error branches.
   Do not silently lower thresholds. Require no unexplained regression,
   changed-line reporting/policy review and targeted mutation sensitivity.

**Acceptance:** reproducible artifacts, reviewed uncovered code/exclusions,
recorded enforced thresholds and explicit remaining policy work. Aggregate and
per-file floors are enforced; changed-line thresholds remain informational.
Meeting percentages alone does not complete test adequacy or P05.
See [runtime coverage](runtime-coverage.md).

## P07 — Automate conformance with lawful assets

**Dependencies:** P02, P04, P05.

1. Choose an authorized hash-pinned fetch, approved redistribution or trusted
   restricted lane under actual asset permissions. Project-source approval
   does not approve official vectors/documents.
2. Provide matching local/CI commands, pinned manifest, exact case counts and
   bitmap hashes. Cache assets only in approved locations.
3. Run generated public regressions on PRs. Run official cases there only if
   lawful without privileged secrets; otherwise require trusted merge/release
   execution and label PR coverage partial. Missing execution is not passed.
4. Keep small reviewed expected differences; new/wider exceptions need normative
   review and tests.
5. Keep third-party oracles external black-box tools with separate
   license/provenance handling.

**Acceptance:** failures reach CI; result categories/exceptions are visible;
release does not rely solely on a stale local report.

## P08 — Harden fuzzing and crash regression

**Dependencies:** P02, P06.

1. Preserve existing targets and compiler/tool pins. Coverage-guided additions
   for symbol/text, refinement, halftone and public-state reuse require the
   applicable approval; the current existing-target lifecycle is documented
   [separately](fuzz-corpus-lifecycle.md).
2. Use bounded PR smoke and longer scheduled campaigns with per-input
   timeout/RSS/work and job limits. Improve useful mutations, not just timeouts.
3. Retain lawful generated corpora between trusted runs; check in only small
   generated/minimized regressions with provenance.
4. Capture logs/reproducers on failure, replay deterministically as normal
   tests, periodically minimize, and keep restricted data out of artifacts.
5. Isolate untrusted PR cache writes and never run unreviewed code on a
   privileged persistent worker.

**Acceptance:** an injected failure proves capture/replay; scheduled execution
is configured; each real finding becomes a regression test.

## P09 — Establish meaningful performance gates

**Dependencies:** P01, P02, P03, P06.

1. Reuse the existing comparison/stability harness; separate base/head
   regressions from optional competitive comparisons.
2. Within approved scope add redistributable deterministic transition-heavy
   MMR, arithmetic/template, refinement, symbol/text, halftone, globals/context,
   many-page and malformed worst-case scenarios. Official TT9/TT10 require P07
   permissions; protected fixture expansion is not implicitly approved here.
3. Validate bitmap/error parity before timing. Use release builds, pinned
   compiler/flags/target, same-host interleaved base/head runs, warmup/repeats,
   per-case median/p95 and variance/confidence.
4. Measure allocations/peak allocated bytes separately from process RSS;
   distinguish parse/decode/output processing so hashing cannot hide costs.
5. Hosted PR timing is informational; use deterministic work/allocation/
   complexity assertions there. Enforce latency only on a stable isolated
   trusted host; unstable timing is inconclusive, not a pass or regression.
6. Ratify proposed dedicated-host limits: no >5% median or >10% p95/peak-memory
   regression across repeated paired runs; zero output/work/complexity
   regression. Reconcile with historical 20%/15% release and 3% optimization
   policies rather than silently replacing them.
7. Do not hide a per-case regression behind a geometric mean. Baseline changes
   require review and linked results.

**Acceptance:** a controlled slowdown is detected, base/head environment/results
are retained, and timing is trustworthy or clearly informational. SIMD/hotpath
experiments are not prerequisites. The existing generated MMR/page/error lane
now checks repeated public work-quota admission boundaries and exact one-below
typed rejections independently of timing, including a real one-unit regression
and restored-library control. Optional Linux RSS observations use a fresh
process per case and sample its own `VmHWM` after one decode/exact validation;
startup, fixture and measurement overhead are included. A real 64 MiB inflation
and restored-library control check observation, not a release-memory threshold.
These are not allocator peak bytes, internal work counters or proofs of
capacity/accounting monotonicity. This lane and its isolated controls are
bounded progress, not complete P09 evidence.

The separate Linux allocator probe now observes tracked allocations, peak live
allocations, total requested payload bytes and peak simultaneously live
requested payload bytes for the same eight approved generated cases. Fixture
preparation happens before measurement; decoding, exact result validation and
drop happen within it. Three paired fresh-process runs per case and real
64 KiB live-allocation inflation/restoration controls are retained as raw JSON.
The timing/RSS and allocator drivers share their exact-output validator.

```sh
python3 -B tools/run_allocation.py --base-ref origin/main \
  --measure-phases \
  --output-dir target/allocation-comparison
```

The isolated, non-publishable tool pins `allocation-counter` 0.8.1 and its
registry checksum in its own Cargo lockfile. The crate declares dual MIT/Apache
licensing. Its thread-local wrapper delegates to the system allocator; it is
an additional measurement-tool trust boundary, not a decoder dependency or
standard-library proof axiom. First-party probe code forbids unsafe code.
Requested `Layout` payloads do not include allocator metadata/usable-size slack,
other threads or process RSS. All measurements remain informational; no
release-memory threshold is ratified here. Broader corpus, internal entropy-only
phase attribution and usable-size/overhead analysis, dedicated-host latency/RSS enforcement and
ratification remain open.

With `--measure-phases`, an additional raw lane separates public decoding,
exact output validation and output destruction. Signed per-window deltas
preserve releases of output allocated by an earlier window. Totals and live
lifetime peaks must reconstruct a separate complete replay and match the
original combined lane. Phase peaks are not summed: each local peak is offset
by the allocations still live at that window's start. A real validation-only
64 KiB allocation must change only its own phase and restore exactly.

Measurement windows are never nested. The pinned counter's nested maximum
aggregation overstates a lifetime peak when one phase frees temporary memory
before the next allocates; the independent flat replay detects that error.
Each phase sample executes decoding twice and public envelope inspection once.
Inspection validates metadata without entropy decoding and is a separate,
overlapping workload, not an internal parsing phase to subtract from decoding.
Public decoding still includes parsing. This closes output-processing
contamination in the measured subset, not internal entropy-only attribution,
usable-size/overhead analysis or the remaining broader/dedicated-host gates.

## P10 — Integrate a documented CI quality contract

**Dependencies:** P03, P04, P06, P07, P08, P09.

| Lane | Execution policy | Required evidence |
| --- | --- | --- |
| Runtime | PR; main/tag pushes; native Linux/Windows/macOS | MSRV/stable policy, build/tests/doc-tests, strict fmt/Clippy/rustdoc, API tests |
| Distribution | Same change validation; native supported hosts | Locked actual archive, package audit, extracted tests/consumer and guard controls |
| Proof | Pinned Verus Linux | Whole verification/build, complete source-bound inventory, dispositions, trust/traceability and every semantic contrast |
| Coverage | Pinned Linux instrumentation | Runtime reports, approved floors, tooling tests and explicit changed-line policy |
| Conformance | Public generated lane plus authorized official lane | Exact output identities and explicit official availability/outcomes |
| Fuzz | PR smoke; scheduled extended | Complete targets, bounded execution, lawful corpus and reproducers |
| Performance | Hosted informational/deterministic; trusted comparisons | Correctness, work/allocation metrics and reliable latency/RSS evidence |
| Release | Owner-approved immutable candidate | All prior gates, lawful notices/assets and clean archive |

1. Keep runtime/MSRV, development, proof and fuzz/coverage pins distinct.
   Declare only tested architectures. Native/emulated ARM64 and 32-bit
   execution are needed if those become supported; cross-compilation alone
   is not equivalent.
2. Pin actions to reviewed SHAs; minimize permissions; bound jobs/caches,
   cancellation and dependency updates. No PR secrets/write tokens.
3. Avoid redundant heavy feature-push plus PR runs; retain stable collectors,
   source/tool/target artifact names and failed-run diagnostics.
4. Configure protection/rulesets after workflows pass. Skips or
   `continue-on-error` must not masquerade as critical release success.

**Acceptance:** a real PR executes all applicable lanes; broken fixtures prove
failure; branch/release rules match the contract. Unavailable runners/permissions
remain blockers. Current workflow operation does not certify these release
conditions; see the reference snapshot for actual accepted execution.

## P11 — Complete cleanup and public documentation

**Dependencies:** P01–P10.

1. Apply reviewed path dispositions in small changes, not wholesale deletion
   by directory name. Keep runtime, models/proofs, test oracles, fuzz targets,
   generators, runners/manifests, executable catalogs and benchmark inputs.
2. Retain minimal authoritative pins, support/provenance/traceability policy,
   reviewed proof dispositions/inventory, compact baselines and necessary
   legal/security files. Remove superseded evidence only after consumers move.
3. Keep imports, package allowlists, coverage/audit tooling and CI references
   correct across moves. Generated reports belong in artifacts, not repeated
   tracked session logs.
4. Publish concise build/API/proof/test guides with honest scope, coverage,
   strength and trust boundaries. Preserve Git history; never import secrets,
   restricted corpora/standards or third-party source during cleanup.
5. Separate optional competitive tooling from core regression gates; remove
   obsolete examples/scripts/case descriptions only after equivalent behavior
   and all consumers are accounted for. Limit ignore rules to actual needs.

**Acceptance:** every retained artifact has a documented role, no dead
references or accidental test/proof loss, full gates pass and package/source
boundaries remain unchanged. See [repository layout](repository-layout.md).
Documentation cleanup alone is not full P11 acceptance.

## P12 — Rehearse release from a fresh checkout

**Dependencies:** P11.

1. Run the complete quality contract on an immutable candidate. Produce an
   evidence index binding source, archive checksum, tools, normative asset
   hashes, coverage, proof-strength ledger, fuzz and performance.
2. Audit/install/use the archive from fresh downstream projects on all declared
   platforms, without proof files or internal cfgs.
3. Review notices/license, claims, official exceptions, residual boundaries
   and unavailable evidence; obtain release-owner sign-off.
4. Record CELA approval only to its actual scope and leave separate decisions
   visible. Enable crates.io only through a separately approved change,
   with restricted trusted publishing if selected.

**Acceptance:** reproducible rehearsal, zero unresolved required gates and no
unsupported full-conformance/complete-functional-proof claim.

## Completion rule

P03, P04 and P06 can progress after their prerequisites; P05 depends on actual
contract review, not verified-function totals. P07 follows normative review;
P08/P09 build on coverage, P10 integrates evidence, P11 finalizes consumers and
P12 rehearses. License/asset scope, supported claims, coverage policy and stable
performance thresholds remain owner decisions where not already recorded.
No gate or document grants permission to publish.

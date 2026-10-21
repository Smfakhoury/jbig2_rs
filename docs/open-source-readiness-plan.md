# Open-source readiness execution plan

Status: P01/P02/P03 completed; P06 is in progress with measured test improvements
and targeted public-contract mutation checks.
The negative/metamorphic catalogs now contain 29 fully executable-linked cases
(38 references to 37 distinct ordinary tests), with no legacy file-only
exceptions. Strict schema, source/ignored-test checks and real Cargo discovery
are enforced in native CI. Five focused regressions strengthen typed duplicate,
missing-reference, truncation and repeat-result claims. Catalog linking is not
semantic or normative approval, and broader P06 coverage goals remain open.
Exact-source proof gaps are exposed and tracked for P05.
P04 is in progress with a provisional 48-record statement-level ledger and
reference-integrity CI; no complete normative decomposition is claimed.
The bounded organization/Huffman/MMR tranche adds 28 selected records, starts
six previously unmapped feature groups, and enforces exact per-feature
requirement membership plus explicit remaining scope. Thirteen of 30 worklists
are partial, 16 not started and one deferred encoder excluded. Five generated
organization regressions supplement 67 distinct linked non-ignored Cargo test
identities. T.88 source hashes were rechecked through the official public
package; no standard assets were committed. T.6 mode/run decomposition, complete
enumeration and owner review remain open. Stricter Huffman partition and MMR
padding rejection policies are explicitly unresolved, not newly approved.
Targeted P05 tranches close the selected batch-reset output-contract gap
and parsed-document count agreement, with compiled semantic counterexamples
and weaker proof controls. Parser result contracts now retain parsed cardinality;
all six slice inspection APIs expose successful metadata shape, and standalone
metadata is linked to the original header-byte fields and known count.
Document cardinality is tied to positive PageInformation records through
collection/finalization, not body-byte parser correctness or complete page
lifecycle semantics. Reader I/O remains a reviewed formal boundary.
The typical-row copy now executes and verifies the same safe native slice-copy
path, with full byte/frame and visible-pixel postconditions; its obsolete
scalar proof substitute is removed. The current selected ledger has 32
functional, three state-preservation and 13 safety-only records; all 48 remain
provisional.
Prefix copying now shares the verified packed-byte implementation; successful
prefix/suffix bytes and unchanged error state are explicit contracts. Blank
zero-storage and exact height-padding bytes are also proved. Six compiled
bitmap counterexamples check those semantic guarantees.
Caller timing/first-row semantics, other semantic gaps and 2 runtime/proof
substitutions remain open.
Five small value/operator bridge substitutions are closed with shared
executing/proof bodies and exact output contracts. Three integer widenings
retain their ordinary `From` calls; Boolean conversion and operator equality
use equivalent explicit `if`/`match` bodies because the pinned standard-trait
contracts are insufficient. Exhaustive small-domain tests, bounded u32
samples and five compiling strong-proof/weaker-control contrasts supplement
the full pinned verification. No caller or normative conformance gap is
inferred closed by these helper-level results.
MMR span dispatch now shares its optimized executing filler with the proof
build, preserving the exact normal-runtime token sequence. Storage/metadata,
invalid-bound errors and invalid/white/empty no-op guarantees are verified
with three compiling semantic contrasts. Black-span pixels remain checked by
bounded exhaustive runtime oracles, not a new formal pixel-correctness claim.
Source-offset region dispatch now shares the optimized chunked implementation
with the proof build. Existing storage guarantees remain; invalid requests
return the exact overflow error without mutation, and valid zero-area requests
succeed unchanged. Three compiling no-op violations fail strong contracts and
verify under storage/error-only controls. The kernel runtime tokens are
unchanged; the wrapper adds only a pure result binding around proof hints.
General composition pixel correctness remains scalar-oracle tested, not newly
proved.
Text-placement batch dispatch now also shares its executing optimized row-batch
call with the proof build. Existing successful storage, metadata, clearing
and batch-validity guarantees remain, with explicit stored length/stride and
empty/zero-height success/no-op guarantees. Four compiling mutations fail the
strong clearing/no-op contracts and verify when only the affected relation is
removed. Both changed declarations have identical erased runtime tokens;
pixel values and placement order remain scalar-oracle tested, not newly proved.
Ordinary bitmap composition and MMR capacity accounting remain the two explicit
source substitutions.
The next P05 prerequisite proves exact big-endian packed-byte read/write
values, complete range acceptance, failed-write non-mutation and outside-span
byte frames. Executing tokens and all existing bitmap models are unchanged;
four compiling semantic contrasts and independent endian/range oracles check
the stronger contracts. This does not reduce the two source substitutions or
prove complete composition. An executing MMR accounting probe identifies
unsupported pinned `Vec::capacity`/`Result::unwrap_or` calls; its edits were
restored. Reviewed library specification/toolchain support is needed, without
substituting length for capacity or introducing project assumptions.
Generated segment-header coverage uncovered a long-retention wire-order and
unused-bit-location mismatch between implementation/model and 7.2.4. Both are
corrected with exact materialization contracts and independent tests. The
conflicting 7.2.8 worked example is explicitly retained as a strict rejection
under the field-rule interpretation; owner normative adjudication remains open.
The initial P08 tranche adds bounded smoke/extended fuzz orchestration,
generated seeds, trusted-only corpus caching, failure diagnostics and exact
single-input replay. An isolated real failure/replay control protects the
workflow contract. Hosted smoke/extended campaigns and failure/replay controls
have been validated on trusted main. Actual cache creation, restoration into
the extended run and saving its updated corpus are verified; all seven
extended starting-corpus file/byte counts match the saved smoke corpus.
The subsequent actual scheduled extended run on the seven-target
`de2fbf3` source also passed, with 566,048,292 executions and actual cache
restore/save. The later eighth-target source has not yet been observed through
a scheduled trigger.
Broader structured/stateful targets, corpus review and minimization remain
pending. This is not completion of P08.
Successful targets now require unambiguous positive final execution counters;
single-input replay requires exactly one execution. Per-target counts and
their exact total are reported, while missing/invalid evidence fails closed.
An eighth structured context target reuses generated MMR shapes and ordinary
regressions to check full packed pixels, fresh/reused results and typed errors,
recovery after malformed/limit calls, globals inspection and scratch release.
It does not close structured symbol/text, refinement or halftone target gaps.
The initial P09 lane adds eight correctness-gated generated base/head cases
with paired raw median/p95 samples and explicit spread classifications. Hosted
informational runs and real slowdown/output-corruption controls have been
validated. Dedicated-host latency gates, broader corpus, allocation/RSS/work
telemetry and threshold ratification remain open.
The P10 integration tranche combines the reviewed PR #19/#20 history from the
former stack base with PR #21 on main, preserving both fuzz and performance
lanes without rewriting history. A merged PR targeting the former stack base
did not by itself deliver those changes to main. This integration preserves
the reviewed runtime/proof bodies and coverage floors; it does not complete
the quality-CI package or approve the provisional normative ledger.
The next P10 preparatory tranche pins all action references to full commits,
separates Rust compiler selection from action ref names, bounds native/proof
jobs, and enforces parsed-YAML policy with a weekly action-update proposal
configuration. Compiler/profile choices, triggers, matrices, cache trust and
read-only permissions are preserved. It does not configure branch rules,
prove upstream action correctness or complete P10's unresolved dependencies.
No broad repository cleanup or release enablement has been performed.
Prepared: 2026-10-19.
Baseline: hosted `main` at `ec6064128d06dfbc3a404b800a718d303c8dad02`,
including the merged fuzz setup fix (PR #5).

Execution baseline, exact file disposition and progress:
[readiness-baseline.md](readiness-baseline.md).

P02 correction: the initial 618-function proof count below was produced by
over-broad identity matching. The current audited inventory is authoritative.
Known runtime/proof substitutions block exact-source release claims; the proof
build passing does not close them.

## Goal and boundaries

Deliver a lean, reproducibly testable JBIG2 decoder repository containing the
runtime, its exact-source Verus proofs, necessary development tooling, and
legally usable correctness/conformance/performance tests. Publish a normal
Rust crate that needs neither Verus nor the repository's proof files.

The owner reports CELA approval for open-source publication. Record that
approval and its scope during execution; do not infer a particular crate
license, permission to redistribute ITU documents/vectors, or permission to
publish third-party oracle assets. Open-source repository publication and
crates.io publication are separate release decisions.

Keep the existing broad bi-level decoder scope. Colour and encoder support
remain excluded. Do not claim full JBIG2/Annex F conformance solely because
Verus verification, code coverage, or the existing vector classifications pass.
The normative baseline is ITU-T T.88 (08/2018), with T.6 for MMR; this plan does
not reproduce either standard's text.

## Findings from the current repository

| Area | Existing evidence | Gap to close |
| --- | --- | --- |
| Native builds | Latest main CI passes on Linux, Windows, macOS with Rust 1.89.0 | Declare/test MSRV and current stable; distinguish native tests from cross-compilation |
| Packaging | Explicit Cargo include list; 23 archive entries; no normal runtime dependencies | No license metadata; no explicit extracted-archive downstream-consumer gate |
| Proof erasure | Normal Cargo builds exclude `verus_keep_ghost` modules; proof directories are not packaged | Demonstrate independence outside the checkout and document what remains in source |
| Verus | 1,136 obligations; 618 verified production functions; 18 reviewed limitations; 21 audited forwarders | Function verification is not a statement-level specification-conformance proof |
| Spec mapping | Feature-to-clause links in `support-matrix.json`; models and theorem files | No complete requirement -> model -> production contract -> proof -> test map |
| Tests | Last measured: 202 passing tests and 14 ignored microbenchmarks | No measured line/region/branch coverage; many manifest references identify files, not exact tests |
| Official vectors | Last measured: 10 cases, 2 exact outputs, 7 reviewed strict rejections, 1 unsupported colour case | Not executed by CI; rejections need normative review, not automatic conformance credit |
| Validation gates | Provenance, package, runtime and proof inventory scripts | Package audit accepts empty stdin; ITU runner can finish successfully with zero cases or unresolved failures |
| Fuzzing | Seven targets; dated nightly and cargo-fuzz pin; hosted smoke now passes | No scheduled campaigns, corpus retention, crash upload/replay gate, or coverage feedback |
| Performance | Generated MMR and Hayro comparison harness; ignored kernel microbenchmarks | No CI regression workflow; public benchmark corpus is too narrow |
| Repository hygiene | Runtime, models, lemmas, tools, reports, approvals and attestations coexist | Separate necessary source/metadata from historic one-off evidence without breaking tooling |

These are measured or inspected facts, not a new claim that the current proof
contracts establish complete pixel-level equivalence to T.88. Current
arithmetic contracts include context/decoder invariants; some raster contracts
preserve storage/context validity. Audit the actual postcondition of each
function before assigning it a functional-correctness claim.

## Execution rules

- One independently reviewable PR per work package below; split P05 further by
  coding family if semantic proof gaps require substantial work.
- Preserve existing public API, strict errors, resource limits, safe Rust, and
  exact runtime/proof source identity. Never substitute a simpler proof-only
  algorithm for the executing one.
- Establish baseline artifacts before deleting or relocating anything.
- Remove files only through an explicit path-by-path disposition reviewed in
  P01. Keep necessary legal/security notices even though they are not codec code.
- Preserve Git history. History rewriting, repo visibility, release signing and
  changing `publish = false` are separate owner decisions.
- Store routine generated reports as CI artifacts, not constantly rewritten
  tracked files. Retain machine-readable expectations, schemas, pins, reviewed
  proof dispositions and compact reviewed baselines as source.
- A gate must fail on missing/empty/incompatible evidence, not silently skip.
  Legally unavailable assets must be reported as unavailable, not as passing.

## Work packages

### P01 - Freeze scope, approvals and file disposition

Dependencies: none.

1. Record the owner's publication approval in a public-safe form; reconcile
   stale publication-blocked wording without exposing internal correspondence.
2. Confirm the exact SPDX license and required Microsoft notices with the
   owner; identify ITU vector acquisition/redistribution permissions separately.
3. Inventory every tracked file as keep, relocate, regenerate, or remove.
   Include its consumers and legal/proof/test role.
4. Record baseline Cargo tests, official classifications, package manifest,
   runtime dependencies, Verus inventory, and benchmark inputs/source hashes.
5. Define public support/claim policy and a separate crates.io release gate.

Acceptance: reviewed inventory covers every tracked path; no deletion yet;
unresolved licensing/assets decisions are explicit blockers for their dependent
steps, not guessed approvals.

### P02 - Make existing audits and conformance runners fail closed

Dependencies: P01.

1. Make package auditing require a nonempty, valid package inventory containing
   required runtime files. Add negative tests for empty, forbidden, malformed
   and incomplete manifests; invoke it using the actual generated archive list.
2. Fix ITU discovery to accept documented root layouts deterministically;
   verify the pinned archive/per-file hashes and exact expected case set.
3. Return failure for zero cases, missing expected images, duplicates, unknown
   classifications, changed inputs, bitmap/page mismatches or unresolved errors.
   Add timeouts and reliable subprocess diagnostics.
4. Test the runners using small synthetic fixtures, including failures, so
   these tests never require ITU downloads.
5. Harden runtime/proof inventory audits against functions after `cfg(test)`
   declarations, nested/path modules, alternate cfg branches, and excluded
   scopes. In particular, do not truncate the production audit at the first
   test-only item and miss later executing functions.
6. Reject duplicate JSON keys and validate retained machine-readable evidence
   against its schema. Regenerated dispositions require human review; do not
   automatically bless newly unverified functions.

Acceptance: deliberately broken fixtures fail with nonzero status; all real
executing functions remain inventoried; current expected output is preserved.

### P03 - Establish and test the proof-free crate boundary

Dependencies: P02.

1. Keep one runtime source tree and the existing `cfg(verus_keep_ghost)` design.
   Do not add a second hand-maintained or generated decoder implementation.
2. Build the `.crate` with `cargo package --locked`; inspect the real archive,
   extract to a temporary directory outside the source checkout, and build/test
   it with no proof directories, Verus toolchain, `vstd`, or network dependency.
3. Create a tiny downstream consumer of that extracted package exercising the
   public API and a generated exact-output decode. Test supported stable/MSRV
   toolchains. Run archive tests as well as compiling the library.
4. Assert absence of proof/validation/oracle/benchmark assets and dependencies.
   Check that normal downstream builds cannot accidentally enable validation
   access. Document that custom internal proof cfgs are unsupported downstream.
5. Explain that inactive proof attributes/references can remain in packaged
   Rust source; they are cfg-elided, not runtime proof code. A textually stripped
   source distribution is unnecessary unless a concrete consumer requires it.
6. Add chosen license files/metadata, `rust-version`, description and supported
   targets after P01 decisions. Keep publication disabled during readiness work.

Acceptance: the isolated consumer and archive tests pass without repository
proof files or proof dependencies; package contents are allowlisted and audited.
Do not call an empty-stdin package audit evidence of proof-free packaging.

### P04 - Build statement-level normative traceability

Dependencies: P01.

Create one versioned machine-readable requirements ledger and a generated
human-readable view, rather than another set of manually synchronized reports.
Each entry contains:

```text
requirement_id; standard_id + edition; clause/subclause + table/figure/step;
short original paraphrase; applicability (required/unsupported/policy);
runtime symbols; model predicates; production requires/ensures;
proof/theorem symbols; exact test IDs; official-vector IDs if applicable;
proof strength; assumptions/boundaries; review status; evidence source identity
```

1. Cover parsing/organization, arithmetic/integer/IAID, Huffman tables, MMR/T.6,
   generic/refinement, symbol/text, pattern/halftone, page composition, retention
   and globals. Classify project resource limits separately from normative rules.
2. Start from `support-matrix.json`, but verify each clause reference against
   the pinned normative edition; coarse feature citations are not enough.
3. Classify proof strength explicitly: safety/bounds, termination, state
   preservation, functional step equivalence, or end-to-end output semantics.
   One requirement may have multiple proof/test links.
4. For example, separately trace a template-0 context geometry rule, its SLTP
   value, typical-row copying, arithmetic transition, and resulting output bits;
   a bitmap-storage-valid postcondition does not prove all these properties.
5. Link normative parameters/tables to checked models and tests, without
   copying standard prose or sample-software source.
6. Add checks for duplicate IDs, missing/dead symbol/test links, stale evidence
   and required requirements without dispositions. Use stable IDs/symbols, not
   line numbers as the only identity.

Acceptance: every in-scope normative requirement is accounted for, including
explicitly partial/unproved ones; review confirms the map represents what the
contracts actually prove. No numerical function count is treated as spec coverage.

Current bounded progress is documented in `docs/spec-traceability.md`.
Feature-local selected requirement IDs and remaining-scope notes are checked
against the records, but those worklists are not an exhaustive normative
denominator. Organization traversal, whole-table canonical assignment, prefix
selection, T.6/MMR raster semantics and caller-policy composition remain open;
the new helper links do not complete this acceptance gate.

### P05 - Close semantic proof and conformance gaps

Dependencies: P02, P04.

1. Audit the ledger contract by contract. Prioritize arithmetic state/decoded
   decisions, MMR mode/run consumption, raster output, dictionary export and
   text placement, then page/document semantics.
2. Strengthen invariants/postconditions where they prove only valid storage or
   bounds rather than the claimed normative transformation. Prove optimized
   kernels refine a reviewed independent functional model.
3. Keep the 18 boundary limitations and 21 forwarders explicit; update outdated
   reasons, bind dispositions to the actual reviewed source, and document the
   compiler/Verus/vstd trust boundary and conditional preconditions.
4. Re-evaluate all seven rejected official vectors using byte/bit traces,
   normative rules and independent oracle evidence. Do not assume that an
   official positive vector is malformed just because this decoder rejects it
   or an oracle emits a warning.
5. Fix decoder/model/test errors discovered, with exact regression fixtures.
   If rejection is normative, retain specific independently reviewed evidence.
   Keep unsupported colour separate from supported conformance.
6. A required functional property cannot be closed by relabeling it verified.
   Close the proof, or explicitly narrow the release claim with owner approval
   and leave the stronger property open.

Acceptance: no unsupported semantic claims; each required claim has the
appropriate proof strength and executable tests. Supported official positives
decode exactly or have a documented normative exception review; unresolved
cases block the corresponding conformance claim.

### P06 - Measure coverage and fill test gaps

Dependencies: P02. Coordinate requirement IDs with P04; P05 adds further tests.

1. Add a pinned coverage tool such as `cargo-llvm-cov` with the matching
   `llvm-tools-preview`, using ordinary runtime builds, not ghost/proof code.
2. Publish line and region coverage, uncovered-code reports, and branch coverage
   where supported by the pinned toolchain. Label unsupported branch metrics
   explicitly; do not silently substitute line coverage.
3. Include runtime unit/integration/API tests; exclude generated tooling,
   proof-only source and benchmark drivers from the runtime denominator.
   Document every exclusion, and test validation scripts separately.
4. Replace file-only negative/metamorphic references with exact executable
   test IDs; add public-API integration tests (slice/reader/sink/globals/context).
5. Test all resource limits at boundary values, malformed/truncated data,
   error propagation, clipping/operators/stripes, retention, context reuse, and
   optimized-versus-reference output/error parity.
6. Add generated valid/minimal streams across all coding/template/organization
   families, so tests are not dependent on redistributing official vectors.
7. Keep the 14 ignored microbenchmarks out of correctness pass counts; retain
   their equivalence assertions as normal tests where needed.
8. Measure baseline first. Proposed release goals: >=90% runtime line and
   >=85% branch coverage where measurable, plus explicit test/proof evidence for
   every critical parser/limit/error branch. Ratify thresholds after the first
   report; do not lower them silently. Add changed-line coverage checks and a
   no-unexplained-regression policy. Use targeted mutation tests for critical
   checks/models to assess test sensitivity, not merely executed lines.

Acceptance: reproducible coverage artifacts; reviewed uncovered code and
justified exclusions; thresholds are recorded and enforced. Test coverage and
proof coverage are reported as different measurements.

### P07 - Automate conformance with lawful asset handling

Dependencies: P02, P04, P05.

1. Decide the official-vector distribution route under P01 permissions:
   authorized hash-pinned fetch at runtime, approved fixture redistribution,
   or a trusted restricted job. Do not commit standards documents or vectors
   merely because the project's own source is approved for open source.
2. Provide the same local command and CI entry point, pinned manifest, exact
   case counts and bitmap hashes. Cache only assets approved for that location.
3. Run generated public conformance/regression tests on every PR. Execute
   official cases on PRs if lawful and available without privileged secrets;
   otherwise run a required trusted merge/release job and report PR coverage as
   partial. Missing official execution must not be marked conformance passed.
4. Preserve a small, reviewed expected-differences file; new or widened
   exceptions require normative review and tests.
5. Keep optional third-party oracles as black-box tools outside the runtime
   and package, with independent license/provenance handling.

Acceptance: failures propagate to CI; official result categories and all
exceptions are visible. A release cannot rely only on a stale local report.

### P08 - Harden fuzzing and crash regression workflows

Dependencies: P02, P06.

1. Preserve seven existing targets and compiler/tool pins; add structured
   symbol/text, refinement, halftone and public-API state/reuse targets where
   the coverage report identifies gaps.
2. Run bounded smoke tests for PRs and longer scheduled campaigns with explicit
   per-input timeout, RSS/work limits and job timeouts. Configure harness limits
   so useful mutations run quickly; do not hide hangs by raising timeouts alone.
3. Persist legally safe seed corpora between trusted campaigns; check in only
   small generated/minimized regression cases with clear provenance.
4. Upload logs and reproducers on failure with `always()` handling; replay
   regressions deterministically as ordinary tests and periodically minimize
   corpora. Keep sensitive third-party data out of public artifacts.
5. Isolate untrusted PR corpus/cache writes; never run unreviewed PR code on a
   privileged persistent worker.

Acceptance: a deliberately failing fixture proves artifact capture/replay;
scheduled runs are configured; every real finding becomes a regression test.

### P09 - Create meaningful performance regression gates

Dependencies: P01, P02, P03, P06.

1. Keep and reuse the existing comparison/stability harness rather than adding
   an unrelated benchmark stack. Separate our base-versus-head regression
   check from optional Hayro competitive comparisons.
2. Add redistributable deterministic cases for transition-heavy MMR, arithmetic
   templates (including TT10-like generated input), refinement, symbol/text,
   halftone, globals/context reuse, many-page graphs and malformed worst cases.
   Retain official TT9/TT10 only under P07 asset permissions.
3. Check bitmap/error parity before timing; benchmark release builds with pinned
   compiler, flags and target. Run base/head interleaved on the same host,
   warm up, repeat and report median/p95 with variance/confidence.
4. Track allocations and peak bytes separately from process RSS; report parse,
   decode and output-processing scope so hashing does not hide decoder costs.
5. On shared hosted PR runners, use deterministic work/allocation/complexity
   assertions and informational timing. For enforceable latency thresholds,
   use a stable isolated, trusted runner; unstable measurements are
   inconclusive, not a pass or a decoder regression.
6. Proposed dedicated-host gate: no >5% per-case median or >10% p95/peak-memory
   regression confirmed across repeated paired runs; zero exact-output,
   work-limit or complexity regressions. Ratify this policy and reconcile it
   with the old 20%/15% release and 3% optimization thresholds.
7. Do not let a fast geometric mean conceal a slow individual case. Baseline
   updates need explicit review and linked results.

Acceptance: a controlled slowdown is detected; base/head results and environment
metadata are uploaded; the quality gate has a trustworthy runner or is clearly
marked informational. SIMD/hotpath experiments are not prerequisites for this gate.

Execution note: an initial generated lane compares identical drivers against
base/head libraries, checks complete generated MMR/page/error outputs, records
paired median/p95 samples and explicitly marks unstable timing inconclusive.
Hosted timing is informational, with real isolated slowdown/output controls.
This is not P09 completion: the broader coding corpus, allocator/RSS telemetry,
dedicated runner and threshold ratification remain open.

### P10 - Consolidate CI into a documented quality contract

Dependencies: P03, P04, P06, P07, P08, P09.

| Lane | Trigger | Required evidence |
| --- | --- | --- |
| Runtime | PR/push; Linux, Windows, macOS | MSRV/current stable policy, build/tests/doc-tests, strict fmt/Clippy/rustdoc, public-API tests |
| Distribution | PR/push; supported native platforms | Locked archive creation/audit, extracted-archive tests and downstream consumer |
| Proofs | PR/push; pinned Verus Linux | Exact production verification, reviewed dispositions, no assumptions/unsafe shortcuts, complete inventory and traceability checks |
| Coverage | PR/push; pinned coverage toolchain | Runtime coverage reports and approved thresholds; script tests |
| Conformance | Public PR lane plus authorized official lane | Generated tests and hash-pinned official cases/explicit availability |
| Fuzz | PR smoke; nightly/weekly extended | All targets, bounded execution, corpus policy and reproducers |
| Performance | PR informational/deterministic; trusted baseline comparisons | Per-case correctness, work/allocation metrics, reliable latency/RSS comparisons |
| Release | Owner-approved tag/manual dispatch | All preceding gates bound to candidate, lawful notices/assets, clean archive |

1. Keep MSRV/runtime, Verus, fuzz and coverage toolchains distinct and pinned;
   declare only architectures actually tested. Add ARM64 native or emulated
   tests and 32-bit execution if those become supported targets; cross-checking
   alone is not equivalent to executing tests.
2. Pin third-party actions to reviewed commit SHAs; minimum permissions,
   timeouts, safe caches, concurrency cancellation and dependency update policy.
   Do not grant secrets/write tokens to untrusted pull requests.
3. Avoid duplicated heavy push/PR runs where practical. Artifact names include
   commit/toolchain/target; upload failed-run diagnostics; use stable job names.
4. Configure branch protection/rulesets using these job names after workflows
   pass. Do not permit skipped critical lanes or `continue-on-error` to appear
   as release success.

Acceptance: a sample PR executes all applicable lanes; deliberately broken
fixtures prove gates fail; protected-branch/release rules match the documented
contract. Unavailable dedicated runners or asset permissions remain explicit.

### P11 - Execute repository cleanup and public documentation

Dependencies: P01 through P10.

Default file disposition (finalize exact paths in P01):

| Files | Default |
| --- | --- |
| `src/`, `specs/`, `proofs/` | Keep; preserve exact-source verification and necessary test oracles |
| `fuzz/`, test generators, conformance runner/manifests, negative/metamorphic tests | Keep |
| Benchmark harness/scenarios and small reviewed regression baselines | Keep; separate optional competitive tooling from core regression lane |
| `toolchain.json`, support profile, proof dispositions, provenance and traceability | Keep minimal authoritative versions |
| `production-proof-coverage.json` | Keep until verifier CI can generate/compare equivalent checked expectations |
| Root historical attestations/approvals and `proof-limitations-plan.json` | Consolidate into current public policy/limitations and generated release evidence; remove only after tool/schema references are migrated |
| Recorded one-run conformance/differential/performance/comparison results | Move to release/CI artifacts; retain required expected results and reviewed exceptions |
| Broad Visual Studio `.gitignore` boilerplate | Reduce to actual project/tool needs, preserving Rust/fuzz/proof/output exclusions |
| `README.md`, `RELEASING.md`, `SECURITY.md`, threat model, license/notices | Keep and refresh; remove stale internal workflow language |
| Unreferenced obsolete examples, scripts or JSON case descriptions | Remove only if functionality is superseded and tests/CI no longer consume them |

1. Apply the reviewed disposition in small PRs; no wholesale deletion based
   solely on directory name or apparent redundancy.
2. Keep relative imports, coverage/tool references, package allowlists and
   CI paths correct after any moves.
3. Publish concise build/decode/proof/test instructions and honest supported
   profile, coverage, proof-strength and remaining-boundary explanations.
4. Preserve old commits; do not import corpora, raw standards, secrets or
   third-party source while cleaning public files.

Acceptance: every retained artifact has a documented role, no dead references,
no accidental test/proof loss, full gates pass and source/package boundaries
remain unchanged. Historical generated evidence need not clutter the source tree.

### P12 - Rehearse release from a fresh checkout

Dependencies: P11.

1. Run the full quality contract on an immutable candidate and produce a
   release evidence index tying source, archive checksum, toolchains, normative
   asset hashes, coverage, proof-strength ledger, fuzz and benchmarks together.
2. Verify archive contents and install/use it from a fresh downstream project
   on all declared platforms without proof files or special cfg flags.
3. Review the final license/notices, claims, official exceptions, residual
   boundaries and any unavailable evidence; obtain release-owner sign-off.
4. Record CELA approval as completed to its actual scope; leave genuinely
   separate pending decisions visible. Enable crates.io publication only in a
   separately approved change, using restricted trusted publishing if chosen.

Acceptance: reproducible release rehearsal; zero unresolved required gates;
no unsupported full-conformance or complete-functional-proof claim.

## Suggested execution order and decisions

Start with P01 and P02. Then P03, P04 and P06 can proceed independently.
P05 is the potentially largest workstream: its size depends on the semantic
contract audit, not today's verified-function count. P07 follows normative
review; P08 and P09 build on test coverage. Integrate lanes in P10, clean in
P11 only after their consumers are established, and rehearse in P12.

Decisions needed during execution (not prerequisites to approving this plan):

- Exact crate license/notices and whether approval includes official vectors.
- Public support target/MSRV policy and ratified coverage thresholds.
- Supported conformance claims after official-vector re-evaluation.
- Stable benchmark runner availability and approved regression thresholds.
- Whether to publish only the repository first or the crate as well.

Definition of complete: the repository is lean without losing verification or
tests; the crate works independently of proofs; normative claims have reviewed
requirement-level evidence; coverage is measured and gated; build, proof,
conformance, fuzz and performance checks run in CI with honest failure and
availability semantics. This plan itself does not authorize publishing.

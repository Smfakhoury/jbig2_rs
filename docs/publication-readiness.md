# Publication readiness

This is a revision-qualified evidence snapshot, not release approval. Current
policy, source bindings and expectations live in the machine records described
in [repository layout](repository-layout.md). Historical runs and implementation
chronology remain in Git; do not treat an older result as a new solver run,
conformance execution or publication permission.

## Publication-layout candidate

The implementation now uses focused Rust child modules, separately packaged
unit tests and external proof helpers; start with the
[responsibility map](repository-layout.md#read-the-implementation-by-responsibility)
or the ordinary-Rust example in the [crate guide](../src/lib.rs).
This changes source organization, not the public decoder API or publication
policy. Inactive contracts and necessary local proof hooks remain in source.

Local acceptance of the integrated layout includes:

- A fresh whole production proof and compiled library: **1,245 verified,
  0 errors**. The increase from 1,244 is one ghost proof helper, not another
  runtime operation.
- All 659 native declarations accounted for, with native token projections,
  attached function/loop contracts, data declarations and mathematical models
  preserved. Production accounting remains 611 directly verified, 24 reviewed
  limitations, 23 audited forwarders and one runtime/proof divergence.
- All 141 existing native mutants compile and are detected. Real operator-bit
  and document-count strong/weaker contrasts also pass the strict collector.
- All 510 tooling tests pass. A real 71-file archive passes its extracted tests,
  doctest and exact downstream-output check on Rust 1.88.0 with a fresh offline
  Cargo home and no normal dependencies.
- Fresh LLVM coverage covers 48 executing files: 11,460/12,716 lines,
  10,974/12,953 regions and 2,245/2,640 branch outcomes. Rebinding the physical
  file baseline preserves both the previous component ratchets and the
  unchanged 90% line / 85% branch goals.
- Actual slowdown and packed-output-corruption controls are still rejected.

These are local results, not current-head hosted acceptance. The complete
34-group / 269-contrast matrix, cross-platform distribution lanes and other
hosted checks must pass on the final candidate before review readiness.
None of these results resolves the release blockers below or grants normative
approval.

## Verified reference revision

Reference: owner-merged [PR #91](https://github.com/microsoft/jbig2_rs/pull/91),
main commit `60786ccb096cc55f9cfe1847f702c0d97b7949d3`, source head
`4d075b9361033bf8c9dd92e4faf7ef1a2e93696b`. The publication-layout cleanup is
not itself covered by those runs.

| Evidence at that revision | Result |
| --- | --- |
| Checks | All 22 passed: 21 workflow jobs plus CLA |
| Whole production proof | 1,232 verified obligations, 0 errors; proof run [40243754827](https://github.com/microsoft/jbig2_rs/actions/runs/40243754827), attempt 1 |
| Production accounting | 610 directly verified native functions, 24 reviewed verifier limitations, 23 audited forwarders, 1 runtime/proof divergence; 0 native assumptions, 0 incomplete functions |
| Proof sensitivity | 29 semantic groups, 236 compiling strong-rejected/weaker-accepted contrasts; six sequential one-worker shards, at most four active |
| Independent reconstruction | 229 source bindings across 66 canonical files; all 51 raw coverage metrics, hashes, source/test/tool identities and actual merge identities checked |
| Runtime sensitivity | 108 real native mutants compiled and were detected by their selected tests |
| Runtime tests | 473 passing; 14 existing ignored microbenchmarks are not correctness passes |
| Tooling tests | 418 passing |
| Coverage | [40243755037](https://github.com/microsoft/jbig2_rs/actions/runs/40243755037): 11,383/12,639 lines (90.062504945%), 10,971/12,950 regions (84.718146718%), 2,245/2,640 branch outcomes (85.037878788%) |
| Native CI/distribution | [40243754851](https://github.com/microsoft/jbig2_rs/actions/runs/40243754851), attempt 1: six native Linux/Windows/macOS × Rust 1.88.0 MSRV/stable 1.99.1 distribution reports |
| Archive controls | 24 package files, fresh offline Cargo home, zero dependencies, exact downstream output, hidden `runtime_access`, unsafe probe rejected and guard-only weaker control compiled; extracted source unconditionally restored |
| Normative ledger | 266 selected records: 257 normative + 9 project-policy; 133 functional, 117 state-preservation, 16 safety; all provisional, denominator not exhaustive |
| Official cases | 2 exact positives, 7 unresolved positives, 1 unsupported; 0 reviewed rejection credit |

No runtime code changed in PR #91. Proof and coverage job caps remained
20 and 15 minutes respectively; the shard layout did not weaken the gates.
Coverage met the unchanged 90% line / 85% branch-outcome goals. This does not
complete P06 or establish normative conformance.

Toolchains remain separate: runtime MSRV 1.88.0, development Rust 1.89.0,
proof Rust 1.98.1 with Verus `0.2026.10.04.426d8b0`, and fuzz/coverage
`nightly-2026-10-12`. Consult
[`config/toolchain.json`](../config/toolchain.json) for pins rather than
assuming one compiler serves every lane.

## Claim and permission boundaries

- The normative baseline is ITU-T T.88 (08/2018) / ISO/IEC 14492:2019,
  with T.6 for MMR. The first milestone is the broad bi-level decoder;
  colour is unsupported, encoder work deferred, and full Annex F profile 1
  conformance is not claimed.
- The owner-reported CELA approval covers project publication to its recorded
  scope. The MIT license does not supply permission to redistribute normative
  documents, official assets or independent oracles.
- The repository remains **private**. Cargo remains `publish = false`,
  version `0.0.1`. Crate publication, visibility changes and release sign-off
  are separate owner decisions, not consequences of passing checks.
- No handwritten unsafe is present and the crate root forbids unsafe code.
  Generated derives, library internals, Rust/LLVM and Verus/vstd are trusted
  boundaries. Proof annotations are cfg-inactive, not removed from source;
  neither binary-wide no-unsafe nor complete no-panic correctness is proved.

## Remaining blockers

1. **Exact source and resource semantics.** One MMR capacity-accounting
   runtime/proof divergence remains. The pinned `Vec::capacity`/getter
   standard-trust decision is held; the separate borrowed-capacity repair
   remains unintegrated. The observed one-byte/eight-byte reservation and
   4,112/4,128-byte managed-total defects are not closed by safety proofs.
   See [verifier limitations](verifier-limitations.md).
2. **Normative/model review.** Independent standard-table, coordinate, caller
   and model review plus complete requirement decomposition remain required.
   Selected project functional contracts are not approved standard translations.
3. **Official positives.** The seven official positive cases remain unresolved;
   an observed typed rejection is not reviewed normative rejection evidence.
   Retention-field/worked-example interpretation and other diagnosis gaps
   remain open. See [validation](../validation/README.md).
4. **Protected fixture expansion.** Generated encoded refinement, aggregation
   and custom-Huffman family expansion remains held. Do not retry or reconstruct
   flagged work. Existing approved raw numeric/metadata tests and approved
   fixtures may be used within their recorded scope; no broader consent follows.
5. **Coverage and performance.** Broader family/all-limit and allocator-failure
   testing, uncovered-path review and changed-line enforcement policy remain.
   Hosted performance is informational; broader corpus, allocator/RSS telemetry,
   dedicated-host evidence and threshold ratification remain open.
6. **Release integration and permissions.** Lawful current official execution,
   final quality-contract/ruleset review, cleanup acceptance and a fresh
   immutable release rehearsal are not automatically complete.

P01, P02 and P03 are complete. **P04, P05 and P06 remain incomplete** despite
their bounded progress. Later P07–P12 gates are not inferred complete from
operational workflows or this documentation cleanup. The complete obligations
and dependencies are in [P01–P12](open-source-readiness-plan.md).

No new external outreach, local standard/equality specifications, assumptions,
termination waivers, pin changes, release enablement or weakened gates are
authorized by this snapshot.

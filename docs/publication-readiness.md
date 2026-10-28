# Publication readiness

This is a revision-qualified evidence snapshot, not release approval. Current
policy, source bindings and expectations live in the machine records described
in [repository layout](repository-layout.md). Historical runs and implementation
chronology remain in Git; do not treat an older result as a new solver run,
conformance execution or publication permission.

## Verified publication-layout revision

Reference: owner-merged [PR #107](https://github.com/microsoft/jbig2_rs/pull/107),
main commit `e145a4532bc0ee4ae0abf237b13e4d755bfd2eee`, source head
`b071519ec681943f483499908117899e897d3948`. Their Git trees are identical.
The results below belong to that source head; this documentation update does
not rerun or relabel its proofs, coverage or conformance observations.

The implementation now uses focused Rust child modules, separately packaged
unit tests and external proof helpers; start with the
[responsibility map](repository-layout.md#read-the-implementation-by-responsibility)
or the ordinary-Rust example in the [crate guide](../src/lib.rs).
This changes source organization, not the public decoder API or publication
policy. Inactive contracts and necessary local proof hooks remain in source.

| Evidence at that revision | Result |
| --- | --- |
| Checks | All 22 passed: 21 workflow jobs plus CLA |
| Whole production proof | 1,245 verified obligations, 0 errors and a compiled library; proof run [40679876981](https://github.com/microsoft/jbig2_rs/actions/runs/40679876981), attempt 1 |
| Production accounting | All 659 native declarations accounted for: 611 directly verified, 24 reviewed verifier limitations, 23 audited forwarders, 1 runtime/proof divergence; 0 native assumptions, 0 incomplete functions |
| Proof sensitivity | 34 semantic groups, 269 compiling strong-rejected/weaker-accepted contrasts; six sequential one-worker shards, at most four active |
| Independent reconstruction | 275 source bindings across 76 canonical files; all 147 raw coverage metrics, hashes, source/test/tool identities and actual merge identities checked |
| Runtime sensitivity | 141 real native mutants compiled and were detected by their selected tests |
| Tooling tests | 511 passing |
| Coverage | [40679877001](https://github.com/microsoft/jbig2_rs/actions/runs/40679877001): 48 executing files; 11,460/12,716 lines (90.122680088%), 10,974/12,953 regions (84.721686096%), 2,245/2,640 branch outcomes (85.037878788%) |
| Native CI/distribution | [40679876967](https://github.com/microsoft/jbig2_rs/actions/runs/40679876967), attempt 1: six native Linux/Windows/macOS × Rust 1.88.0 MSRV/stable 1.99.1 distribution reports |
| Archive controls | 71 package files, fresh offline Cargo home, zero normal dependencies, extracted tests/doctest and exact downstream output, hidden `runtime_access`, unsafe probe rejected and guard-only weaker control compiled; extracted source unconditionally restored |
| Performance and fuzz | Generated performance comparison [40679877015](https://github.com/microsoft/jbig2_rs/actions/runs/40679877015) and bounded existing-target fuzz validation [40679877127](https://github.com/microsoft/jbig2_rs/actions/runs/40679877127) passed; timing remains informational |
| Normative ledger | 266 selected records: 257 normative + 9 project-policy; 133 functional, 117 state-preservation, 16 safety; all provisional, denominator not exhaustive |
| Official cases | Historical observations remain 2 exact positives, 7 unresolved positives, 1 unsupported; 0 reviewed rejection credit. No new official execution or normative approval in this refactor |

Native token projections, attached function/loop contracts, data declarations
and mathematical models were preserved across the source reorganization.
The increase from 1,244 to 1,245 verified obligations is one ghost proof helper,
not another runtime operation. Short contracts and proof hooks remain attached
to the executing Rust.

The whole proof, six semantic shards and final collector passed with unchanged
1,050-second measured guards; the largest measured semantic shard took
948.255 seconds. Coverage met the unchanged 90% line / 85% branch-outcome goals,
with physical-file rebinding checked against both previous component baselines.
These results do not complete P06, establish normative conformance or resolve
the release blockers below. Earlier revision snapshots remain in Git history.

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

# Verification scope and remaining limitations

A successful pinned Verus run proves the submitted contracts under their
preconditions and trusted tool/library interfaces. It is not automatically
complete exact-source verification, normative conformance, complete panic
freedom or an end-to-end CPU/memory bound.

[Publication readiness](publication-readiness.md) is the revision-qualified
numerical snapshot. The current source-bound authorities are
[`verification/proof-dispositions.json`](../verification/proof-dispositions.json),
[`verification/production-proof-coverage.json`](../verification/production-proof-coverage.json)
and [`verification/proof-attestation.json`](../verification/proof-attestation.json).
Do not use a superseded limitation plan or candidate attestation as authority.

## Accounting categories

| Category | Meaning |
| --- | --- |
| Directly verified | Exact qualified native function has accepted proof evidence for its actual contract |
| Reviewed verifier limitation | Complete tested production implementation with an explicit unsupported verifier/library/callback boundary; not a verified body or algorithm stub |
| Audited forwarder | Allowlisted exact conversion/default/collector wrapper around verified operations, separately source-bound; not a generic callback waiver |
| Runtime/proof divergence | Executing body/behavior differs from the proof-selected path and cannot support a full exact-source claim |
| Incomplete | Missing required accounting/evidence; must remain zero rather than being automatically classified |

The inventory follows syntax-level cfg/module scopes, including production
items after test-only declarations. Unknown platform conditions are
conservatively included. Ambiguous paths, unsupported conditional exclusions
and unaccounted macro/local functions fail closed. Same-named functions in
different owners/modules are not interchangeable evidence.

Formatting/error traits, `std::io::Read` and caller-provided sink behavior remain
important reviewed boundaries. Reader/public API runtime regressions do not
turn arbitrary external callbacks into formally verified implementations.
Fallible reservation and deterministic stores have enabled many direct proofs;
do not repeat obsolete broad claims that all allocation or all registries are
unverified. Inspect each current disposition and actual `requires`/`ensures`.

## Blocking capacity/getter decision

The remaining MMR divergence is `prepare_transition_storage` capacity-based
memory accounting. Its executing blocks encounter unsupported `Vec::capacity`
and `Result::unwrap_or` surfaces in the pinned verifier. Vector length is not
capacity; requested storage is not necessarily observed reserved storage.
One-byte requested/eight-byte reserved behavior remains a relevant regression
case. Safety/bounds proofs do not establish the missing managed-memory result.

A separate collector observation accepts a 4,112-byte limit with a 4,128-byte
observed managed total: a 16-byte pointer reservation reports 32 bytes of
capacity. A native-only borrowed-capacity repair has not been integrated into
accepted proof evidence. These are `Vec::capacity()` observations, **not**
allocator usable size, total process memory or RSS measurements.

The owner standard-trust decision is held. An isolated getter probe and closure
of [upstream issue #3304](https://github.com/verus-lang/verus/issues/3304)
do not approve a local trusted getter declaration or close either integration.
The upstream reservation contract in
[PR #3094](https://github.com/verus-lang/verus/pull/3094) does not itself specify
the getter. No local standard/equality specification, assumption, termination
waiver, pin update or further external outreach is authorized to bypass this
boundary.

## Exact-source and trust gates

```sh
python3 -B tools/audit_production_coverage.py \
  --verus-json target/verification/verus-production.json \
  --output target/verification/production-proof-coverage-ci.json \
  --require-complete --require-exact
```

`--require-complete` means every production identity is accounted for and any
non-direct status reviewed. It does **not** mean every body is directly proved.
The release-level `--require-exact` additionally rejects known runtime/proof
divergences and currently must fail on the remaining backlog. Routine inventory
success cannot bypass it.

The proof manifest is `verification/Cargo.toml`, targeting the same `src/lib.rs`
as the dependency-free runtime manifest. Its locked `vstd` is proof-only.
With the pinned bundled Cargo-Verus:

```sh
cargo +1.98.1 verus verify --manifest-path verification/Cargo.toml --locked -- --num-threads 1 --rlimit 30
```

CI uses `cargo verus build` with that manifest to check verification,
compilation, lifetimes and ghost erasure. It rejects empty, dependency-only,
partial, duplicate or wrong-tool reports and flags/environments that disable
these checks. A `--compile` substring in an unrelated option is not evidence.
For local debugging, place `--verus-args-to roots` before `--` and
`--verify-module` after it; partial runs never substitute for whole-crate proof.

The source trust audit rejects project-local trusted standard specifications,
verification exclusions and exec termination waivers in runtime/models/proofs.
Codec algorithms may not use `external_body`, `assume` or `admit` as proof
shortcuts. Enabled exec-loop decreases checks are lightweight and conditional
on callees terminating, not a universal service-availability proof.

Rust/LLVM, pinned Verus/vstd, specified standard operations, generated derives,
and standard-library internals remain trust boundaries. The crate-root unsafe
prohibition and extracted-archive strong/weaker compiler probes check
crate-authored Rust, not a binary-wide absence of unsafe operations.
See [distribution](distribution.md).

## Semantic evidence is separate

The [traceability ledger](spec-traceability.md) classifies actual contract
strength. There are substantial project result relations for arithmetic,
readers, bitmap copying/filling/operators, MMR accepted input/raster histories,
symbol content and container metadata. Their existence does not approve the
normative translation or full acceptance/error/caller/resource composition.
See [the auditor index](verus-auditor-index.md) for exact model entry points.

Every configured semantic group must run. Each contrast needs a pristine
positive baseline covering its exact target, normal Rust compilation, actual
strong postcondition/invariant rejection and success under the selected weaker
contract. Compilation failure or zero selected functions is not detection.
Sharing a pristine baseline across explicit target sets never replaces the
separate mutant/weaker queries.

The stable `verify` collector requires the whole proof/build and same-run shard
receipts, exact membership and individual report/log/checker hashes with matching
source/tool/run/attempt identities. Shards are sequential and worker/job budgets
are unchanged; cached, canceled, partial or merely predicted results are not
acceptance evidence. Current accepted group/contrast counts and runs are in the
snapshot, not repeated as mutable progress totals here.

## Review and release policy

- A changed body invalidates its disposition hash. Do not refresh hashes to
  manufacture approval or let proposal generators overwrite reviewed decisions.
- Close a disposition only with passing pinned proof and updated reviewed
  accounting; maintain zero native assumptions and incomplete functions.
- New native/trusted contracts require architecture review, a named trust
  boundary and an updated source-bound attestation.
- Functional claims require their actual output relation and executable tests,
  not relabeling storage/state proofs.
- Independent normative/model/table/caller review and complete requirements
  decomposition remain open. Seven official positives receive no restored
  rejection credit.
- Protected encoded refinement/aggregation/custom-Huffman fixture expansion
  remains held; approved raw numeric/metadata tests and approved fixtures do
  not grant new family consent.
- Complete P04/P05 review, exact-source release acceptance and the remaining
  [P01–P12 gates](open-source-readiness-plan.md) precede any release claim.

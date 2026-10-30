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
The pin includes formatting-trait precondition specifications, but a focused
checked probe still rejects `Formatter::write_str`. Separate probes reject
the standard `io::ErrorKind` type and even an empty dynamic `Error` source.
Those are concrete missing interfaces, not an assertion that the pin has no
formatting support at all. No local trusted type, function or trait declaration
has been added to bypass them.
Fallible reservation and deterministic stores have enabled many direct proofs;
do not repeat obsolete broad claims that all allocation or all registries are
unverified. Inspect each current disposition and actual `requires`/`ensures`.

## Project-owned typed conversions

The generic-region, Huffman, MMR and symbol/text `From` implementations now
prove their exact error variant and payload mappings through the pinned
verifier's existing trait-contract mechanism, as already used for the public
malformed/limit conversions. These are project error-policy contracts, not
new trusted standard-library declarations or normative T.88 rules. Closed
specification bodies preserve the visibility of the private codec enums.
The executing match bodies are unchanged.

The implementations share a displayed `DecodeError::from` identity but live in
different lexical source scopes. Their contrast evidence therefore binds the
exact file and selection scope and requires separate pristine selected proofs;
it cannot reuse a baseline solely because the displayed name matches. Eight
compiling wrong-variant/field controls are rejected, with their corresponding
conditional-contract weaker controls accepted. Literal Rust tests separately
exercise every local error variant and wrapped public error class.
The new group runs serially in the existing whole-proof job alongside
arithmetic-codec and value-bridge; it does not add a job or load an already
assigned semantic shard.

This closes four directly proved project conversion boundaries. It does not
prove arbitrary I/O, formatting, sinks or orchestration, resolve capacity
accounting, or approve any standard-facing model.

### Owned error wrappers

Twenty formerly audited `From` wrappers now prove their own project contracts.
They preserve wrapped public errors, malformed/limit fields and composition
through the already proved codec conversions. The halftone conversion helper
also has an exact variant/payload result contract. Project policy functions
separate those result equations from trait bridges without cyclic proof
dependencies or new trusted declarations; native bodies are unchanged.

Several implementations share both a displayed method name and a lexical
scope. Evidence therefore additionally binds the actual source trait/type
parameters. A pristine selected baseline must prove every selected peer; each
mutant fails exactly its own implementation while all other peers still prove.
Only that implementation's conditional mapping is weakened for its control,
which must restore proof of the complete selected set. Missing, swapped or
incomplete implementation/peer records fail collection.

The region group runs in the existing serial collector role; symbol/basic
groups run in shards four/five. A complete first candidate run was correctly
rejected when shard five exceeded the 1,050-second guard, despite all queries
passing. A second complete candidate exposed a different overloaded shard.
Combined per-group maxima inform the bounded role plan: retention joins the
whole role; Huffman assignment moves to shard one, operator bits to shard two,
MMR prefix/run to shard three, and bitmap chunks to shard four. Whole-job setup
overhead is included in planning. Rejected-run timings remain planning inputs,
not acceptance credit. Every case/query, the six-shard/max-four policy, eight
producers, solver/job caps and actual 1,050-second guards remain intact.

No audited forwarders remain after the public-default and owned-sink proofs.
This does not close the twenty
reviewed standard/callback/orchestration limitations or the held capacity
divergence, nor does it establish normative model fidelity.

### Public default constructors

`Limits::default` now proves all fifteen approved default resource values.
`Decoder::default` proves the existing project default-limits predicate over
its stored configuration. The executing one-line wrappers and their already
verified constructors are unchanged.

These are checked implementation postconditions under the pinned library's
existing `Default` trait specification, not new trusted standard-library
declarations. The impls use the public `verifier::verify` attribute directly:
the pin's `verus_verify` convenience macro otherwise injects a second method
attribute before the conditional explicit contract has expanded. All
annotations remain inactive for ordinary Rust consumers.

Thirty compiling contrasts independently set each default field to zero.
Each must fail its exact implementation's contract; a control omitting only
that field must prove the other fourteen. Exact file, lexical scope and
`Default` implementation identity are required, including a pristine positive
proof, zero verified bodies/one error for the mutant and one verified
body for the weaker control. A native public-API test separately checks every
constant through both constructors.

The thirty cases are partitioned into ten three-field cohorts. The first
hosted five-field profile reached its added whole-role cohort but exceeded
the unchanged 20-minute whole-job cap; it grants no acceptance credit.
The second profile completed every query, but its final collector correctly
rejected several measured workloads over 1,050 seconds. Its complete
per-group timings now inform an explicit six-shard rebalance. Reset moves
to whole and retention to collector to account for the whole job's measured
setup overhead as well. Those candidate profiles retained eight producers,
six one-worker shards, max-four semantic concurrency and the measured
guards and job caps. They are historical planning inputs, not acceptance credit.

The next complete profile passed every query and all six measured shard
workloads, but its collector measured 1,078.132 seconds and was rejected.
Its complete timings move the first decoder-default cohort to shard three
and the last limits-default cohort from shard five to shard four. Both
preceding profiles remain frozen planning inputs, not acceptance credit.

A fourth complete profile again passed every query but exceeded two shard
guards. The subsequent partition takes each group's maximum from both
complete runs using the identical proof harness, rather than relying on one
run's faster workers. With every value rounded up, whole/collector/shards
initially cost 967/1015/1002/1038/1021/1024/1028/1044 seconds, but the whole
job still exceeded its wall-clock cap. The revised whole assignment costs
900 seconds under the same rounded envelope, leaving 92 seconds beyond the
observed 208-second setup allowance under its unchanged cap. Other roles
cost 1015/1043/1046/1027/1024/1040/1044 seconds. An explicit complete shard
map prevents removed groups from silently shifting round-robin membership.
These are conservative planning inputs only: fresh complete hosted
acceptance remains mandatory.

The existing same-run whole-positive reuse also covers a trait implementation
only when the complete native inventory has exactly one such qualified name,
with matching file, lexical scope and trait, verified marking and no divergence.
The receipt retains the **actual whole-proof count**, not a fabricated selected
count. Missing/stale receipts and ambiguous names still fail closed; colliding
implementations still require their fresh selected peer baselines. Every
mutant compilation and strong/weaker query remains independent and fresh.

Ordinary Rust mutant checks use Cargo's incremental compiler only inside
their temporary workspace, with an explicit workspace-local target directory.
The proof environment is not mutated and no verifier query is cached or
skipped. A real warm-cache control rejected an ill-typed mutant with `E0308`
and accepted the restored source; repeated native checks fell from about
1.0 seconds to 0.33 seconds. This does not disable lifetime, erasure, trait
conflict or semantic checks, and the cache is discarded with the workspace.

The first complete native-cache profile passed every query but measured
1,091.433 seconds on one shard and was correctly rejected. Four small
cohort transfers use its existing measured headroom; rounded whole/collector/
shard costs are 851/972/908/977/934/984/1021/960 seconds. Earlier profiles
remain frozen regression inputs. Actual exact-head acceptance is still
required, rather than treating those planning numbers as a successful run.

### Accepted eight-shard scheduling baseline

The accepted baseline uses **eight one-worker semantic shards**, plus the
existing whole and collector producers. This intentionally revises the former
six-shard/eight-producer scheduling contract: there are ten producer receipts
and 23 workflow jobs, or 24 checks including CLA. The two additional setup
jobs do not raise peak concurrency: at most four semantic jobs run at once.
The 1,050-second per-producer guard, 20-minute whole/collector caps,
30-minute semantic caps, pinned solver/toolchain and every case/query remain.
No canceled, incomplete or over-budget profile is accepted.

Sizing uses each group's rounded maximum from both complete native-cache
profiles. The preceding default profile's whole/collector/eight shards cost
511/703/828/828/887/857/860/829/888/887 seconds under those planning inputs.
Whole handles arithmetic-codec/value-bridge and the owned-sink group below;
collector handles arithmetic-decision/document-count/reset. All other groups
have an explicit eight-shard assignment. Earlier six-shard fixtures remain
frozen, and its frozen tests require all eight exact shard receipts and fail
on any missing one. Actual complete same-head hosted validation is still required.

### Capacity proposal scheduling revision

The review-only capacity proposal measured every one of the 49 groups and
329 contrasts at source `f0b6c5f`. All case checks passed, but the complete
13,132-second workload failed the unchanged 1,050-second producer guard.
Its timings are rejected-profile inputs, not acceptance credit.

The proposal now uses **fourteen one-worker semantic shards**, with the
arithmetic-codec group in whole and arithmetic-decision/reset in collector.
There are sixteen required producer receipts and 29 workflow jobs, or 30
checks including CLA. The existing dynamic matrix adds six setup jobs without
raising the max-four concurrency limit or changing any solver, query, pin,
1,050-second guard, 20-minute whole/collector cap or 30-minute semantic cap.

Balancing all rounded-up actual group costs gives whole/collector costs of
711/638 seconds and shard costs of
850/837/844/844/844/844/843/845/843/847/832/846/842/846 seconds.
Fourteen is the minimum shard count for this workload at the 850-second
target, retaining about 19% headroom. These are measured-input sizing values,
not a fresh accepted profile. Tests preserve the former six/eight-shard
fixtures, require all sixteen current receipts, and reject 1,051 seconds
while permitting exactly 1,050. Fresh complete same-head validation remains
required, and this scheduling decision does not approve the proposed standard
interface or authorize main integration, normative processing or publication.

### Owned page-collector sink

The concrete `PageSink for PageCollector` wrapper now proves its own checked
append/result relation. Its existing `push_page` helper exposes the same
contract as `collect_page`: success appends the exact page and updates the
logical and observed allocated bitmap-byte counters within the named project
limits; errors preserve the page list and both counters.

The old contract is factored into `collector_result` outside the runtime
source. A separately verified definitional bridge lets the implementation
use this closed predicate without exposing private collector field projections
through a public trait method's postcondition. This is a project-result
definition and checked lemma, not a trusted declaration or an approved T.88
rule. Its author navigation classification does not confer normative approval.

Two compiling contrasts reject a successful return that drops the page and
an error returned after an append. Their weaker controls omit only the
corresponding success/error requirement on the concrete sink wrapper; the
constructor and helper contracts remain intact. Evidence binds the exact
file, lexical scope and `PageSink` implementation with a bound pristine proof,
zero verified bodies/one error for each mutant and one body for each control.
The original profile ran this group in whole; the current measured profile
assigns it to a semantic shard without dropping any case.

This says nothing about arbitrary caller-provided sinks, complete error-variant
identity or allocator usable-size/RSS bounds. The executing integration uses
actual observed capacity for its managed counters, as described below. The complete
unfiltered proof must verify the bridge as well as the implementation;
selected-body success alone is insufficient.

## Observed-capacity integration

The prior MMR divergence omitted retained-capacity observation/release checks
from the proof-selected path because the pin lacked a callable capacity getter.
The integration executes the same path in native and proof builds. Checked
and saturating arithmetic replaces the unsupported `Result::unwrap_or` spelling
without substituting requested length for observed capacity. A compile-time
pointer-width guard keeps infallible capacity conversion within the existing
u64 resource-accounting domain.

Bitmap allocation checks actual post-reservation capacity. Borrowed-pointer
storage preserves requested-preflight ordering and then checks the observed
reservation. Owned collection keeps logical decoded bytes separate from
allocated bitmap bytes, retaining the existing error priority and no-mutation
error frame. Native regressions cover a one-byte request/eight-byte reservation,
a 4,112-byte limit/4,128-byte managed total and counter overflow. Removing the
relevant post-reservation checks must make the real regressions fail.
These are `Vec::capacity()` observations, **not**
allocator usable size, total process memory or RSS measurements.

The additional getter signature is explicitly reviewed for private integration
under delegated engineering authority, isolated from the unsafe-forbidden
runtime, and bound by the exact
[standard-interface review](../verification/stdlib/review.json). It asserts no
requires/ensures facts about capacity, length or reservation. Its numeric
boundary helper is verified, not trusted; the strict Cargo stream must validate
the dependency and application reports without dropping zero/failed results.
This is not a new named-human normative adjudication or public-release decision.
An isolated upstream probe alone would not establish these integration facts.
The reservation contract in
[PR #3094](https://github.com/verus-lang/verus/pull/3094) does not itself specify
the getter. No capacity/equality axiom, algorithm assumption, termination waiver
or pin update is introduced.

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
divergences and incomplete functions. CI applies both gates; ordinary complete
accounting cannot bypass the exact-source check or grant normative approval.

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
For local debugging, use Cargo-Verus's focused mode:

```sh
cargo +1.98.1 verus focus --manifest-path verification/Cargo.toml --locked -- \
  --verify-module document --num-threads 1 --rlimit 30 --triggers-mode silent
```

`focus` verifies crate contents without dependencies and forwards selectors to
root crates by default. If overriding forwarding explicitly, the pinned option
is `--fwd-verus-args-to roots` before `--`, not `--verus-args-to`.
Selectors such as `--verify-module` go after `--`. A focused run is partial
and never substitutes for whole-crate verification and compilation.

The source trust audit rejects project-local trusted standard specifications,
verification exclusions and exec termination waivers in runtime/models/proofs.
The one separately reviewed proof-only `Vec::capacity` signature lives under
`verification/stdlib`, not in those codec/model/proof roots. Its typed review
manifest binds the exact package/source hashes and immutable allocator-generic
signature, with no capacity facts and explicit false normative approval.
Unknown declarations, added facts, stale reviews or a changed dependency graph
fail closed. The separately verified numeric boundary helper is not trusted.
This delegated technical review is not a named-human standards adjudication
or authority to publish the crate or change repository visibility.
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

The semantic runner accepts `--whole-baseline PATH` pointing to the sealed whole
artifact directory. It validates the complete source/tool/run/attempt manifest,
whole proof, traceability, inventory, nonempty compiled library and artifact
hashes before reusing only pristine positive baselines covering each exact
qualified target. An explicitly supplied missing, stale or invalid artifact
fails closed; it never falls back to selected verification. Each contrast still
compiles its native mutant and independently executes both verifier queries.

The workflow downloads
the same-run `verus-partial-RUN-ATTEMPT-whole` artifact into a separate directory
in each semantic job, and passes that directory as `--whole-baseline` alongside the
existing shard arguments. Do not place it inside the shard output directory.
The collector revalidates every reused baseline against the whole artifact it
collects. The current proposal preserves fourteen one-worker shards, at most four active semantic jobs,
20-minute whole/collector caps, the owner-merged 30-minute semantic-job caps
and the revision-qualified complete check topology. The measured 1050-second semantic
workload guard remains unchanged.

The existing whole-proof job also executes the complete `arithmetic-codec`
contrast group after sealing its positive evidence.
This uses spare time in an existing job, not additional jobs, solver workers,
larger timeouts or omitted cases. Their reports remain outside the immutable
positive receipt and are separately source/run/hash validated by the collector,
alongside all fourteen shard receipts. Every configured group executes exactly once.
The existing collector job also runs the complete `arithmetic-decision`
and `reset` groups serially before aggregation. Its separately
required `collector` receipt has the same source/tool/run/attempt and individual
checker/report/log checks as every shard. It reuses only the sealed pristine
whole-positive baseline; each negative and weaker query remains fresh.
The same 1050-second complete-measurement guard applies to this serial group
workload and the collector groups as to each shard; each existing 20-minute
whole/collector cap also covers its setup and other work.

PageState's 29 individual contrasts are partitioned into initialization/stripe,
finish and public-metadata groups (13/7/9), without adding jobs or changing
queries. The historical document-partition regression retains its original
30-group, 236-case, 42-case-per-shard snapshot and 1000-second estimate guard.
It is not a global case ceiling. Complete current-profile collection instead
checks every configured group's actual successful elapsed time and rejects a
producer total above the unchanged 1050-second measurement guard.

The stable `verify` collector requires the whole proof/build, its assigned
contrast receipt and same-run shard
receipts, exact membership and individual report/log/checker hashes with matching
source/tool/run/attempt identities. Shards are sequential, with unchanged peak
concurrency and per-job budgets; old verification results, canceled/partial collections or merely
predicted timings are not acceptance evidence. Current accepted group/contrast counts and runs are in the
snapshot, not repeated as mutable progress totals here.

Bounded in-process caches store only deterministic text normalization and native
declaration metadata keyed by exact immutable source bytes and mode. Changed
inputs are reparsed, mutable records are copied, and neither compiler outcomes
nor strong/weaker verification queries are cached.

## What remains before a complete proof/specification claim

There are two different backlogs; closing one does not automatically close the
other.

| Backlog | Required closure | Authoritative detail |
| --- | --- | --- |
| Exact executing-source correspondence | Retain actual observed-capacity checks, verified current-source contracts and exact reviewed standard-interface identities; do not replace observations with requested size | [Dispositions](../verification/proof-dispositions.json) and the capacity section above |
| Reviewed native/standard/callback boundaries | Directly prove the relevant body or retain an explicitly justified conditional trust contract; arbitrary caller I/O/sink behavior is not proved by testing a wrapper | Every named [disposition](../verification/proof-dispositions.json) |
| Observable algorithm and caller semantics | Strengthen required bounds/state-only results to actual input/output, error and resource relations, including Huffman canonical/prefix behavior, page/stripe lifecycle, symbol/text/refinement/halftone composition, references/retention and globals | Each statement's `gaps` and each feature's `remaining_scope` in the [ledger](../validation/traceability/requirements.json) |
| Exhaustive standard coverage and model fidelity | Complete the in-scope statement decomposition and independently review standard-to-model, table and caller interpretation; selected records are not an exhaustive denominator | [Requirements](../validation/traceability/requirements.json) and [traceability guide](spec-traceability.md) |
| Executable conformance | Resolve supported official positives with actual output evidence or an independently authorized disposition; do not infer normative malformation from a decoder rejection | Current [observations](../validation/conformance/itu-t88-2018-results.json), [expectations](../validation/conformance/expected-differences.json) and [review](../validation/conformance/seven-rejection-review.json) |

The missing semantic properties are documented even for directly verified
functions: direct verification proves their current contracts, not every
property those functions ought eventually to expose. A successful Cargo run,
complete accounting, semantic sensitivity and standards-conformance review
must therefore be interpreted separately.

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

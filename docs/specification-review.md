# Independent specification review

## Two claims, two reviews

An implementation proof establishes its submitted contract under its
preconditions and trusted interfaces. An independent source-to-model review
asks whether that contract/model expresses the supported standard.
Neither green CI nor internal model theorems establish the second claim.

This workflow starts from owner integration
`687035de52b660cbdc80dee8e69e7039840bc601`, including the bounded PageState and
raw Huffman proof tranches. Those models were not universally approved and
frozen before implementation proof work. This document does not retroactively
approve them. Start reading at the [model guide](../specs/README.md).

## Current gates

| Gate | State |
|---|---|
| Project source/code-only review and navigation | Available |
| Authenticated human source-snapshot disposition | Checker available; owner-controlled authority not configured |
| Exact standard identities and official acquisition links | Recorded in [source provenance](../validation/source-provenance.json) |
| Permission to redistribute PDFs or upload them to agents | Not granted by this workflow; existing prohibitions remain |
| Named human adjudicator for normative findings | Not yet assigned |
| Independently approved normative model freeze | Not yet established |
| Exhaustive supported-standard decomposition | Incomplete |

An unanswered request, a reviewer profile or an agent's self-reported approval
does not close a gate. Source-access authorization and human assignment must
be recorded by the responsible owner. Until then, source-to-model audits,
normative status promotion and subsequent approved-model refinement remain
blocked; project consistency work must be labeled as such.

## Review a small slice first

The first proposed slice is arithmetic BYTEIN/initialization and probability
table interpretation. Use the existing requirement IDs and locators, not
copied standard prose or a new tracking database.

Separate the author, two read-only auditors, implementer and human adjudicator.
Auditors produce their initial findings without seeing one another's reports.
For the source-to-model pass they must not treat the implementation as the
normative answer. An appropriately isolated, authorized review environment is
required; tool instructions alone are not a filesystem isolation guarantee.

Keep the requested GPT-6.1 Sol routing and have the coordinator confirm actual
provider session metadata. Independent contexts using the same model can share
blind spots. Human adjudication remains necessary. The
[auditor profile](../.github/agents/specification-auditor.agent.md) grants no
standard-processing permission and cannot approve its own conclusions.

Each auditor must challenge:

- Preconditions that exclude legitimate supported inputs.
- Vacuous or success-only claims that allow rejecting every valid input.
- Independent table fidelity versus implementation-array membership.
- Bit/coordinate order, marker and initial/final boundary interpretation.
- Caller obligations and project resource/error policy versus normative rules.

The exact reviewed capacity-interface trust boundary and existing encoded-fixture
and official-positive interpretation holds remain in force. Do not reconstruct
held fixtures to satisfy this list.

## Generate an explicitly unreviewed packet

The existing traceability command checks references and can produce a bounded
snapshot without reading or redistributing standard text:

```sh
python3 -B tools/check_traceability.py \
  --audit-requirement T88-ARITHMETIC-BYTEIN-BRANCH \
  --audit-requirement T88-ARITHMETIC-INITDEC-BYTEIN \
  --audit-packet target/traceability/bytein-audit-packet.json \
  --output target/traceability/reference-report.json
```

The packet binds the selected rows, definitions, standard hashes and a
conservative source/configuration snapshot. Its state is always `unreviewed`;
document-processing authorization and normative approval are false.
It is **not** an approved normative-only model freeze. Its broad source binding
is deliberately conservative and includes implementation dependencies.
Local proof-only standard-interface sources, Cargo manifests/locks, review
records and controls under `verification/stdlib/` are included. Changing, adding
or removing one invalidates the pinned snapshot, even if runtime/model files
are unchanged. Generated build/cache files are excluded, symlinks rejected and
no standard text or new approval is introduced by this source binding.

Supply the packet SHA-256 through the coordinator's separate assignment, then
validate it before a read-only review:

```sh
python3 -B tools/check_traceability.py \
  --check-audit-packet target/traceability/bytein-audit-packet.json \
  --expected-audit-sha256 COORDINATOR_SUPPLIED_SHA256 \
  --output target/traceability/packet-check.json
```

Do not take the expected pin from an untrusted replacement packet. Changes to
bound inputs, selected scope or asserted approval fields fail validation.
Regenerating a packet does not renew a review or create an approval.
Creation and report outputs stay under the local `target/` directory and cannot
alias input evidence. Hashes identify exact checkout bytes; different line
endings are different inputs, not silently normalized source identities.

### Authenticate an optional human source-snapshot disposition

The same command can authenticate an explicit human review of that exact source
snapshot. This is **not normative approval**, permission to process standards,
an independent dependency-closure audit, or the normative-only model freeze
required below. It does not modify the unreviewed packet or provisional ledger.
Ordinary approval of a code PR is not implicitly a snapshot disposition.

The responsible owner must first record an agreed human's numeric GitHub user ID
under `policy.source_review_authority` in
[source provenance](../validation/source-provenance.json), through the normal
reviewed default-branch process. The optional authority is absent today, so the
check fails closed. Its disabled shape is:

```json
{"enabled": false, "human_user_ids": []}
```

Activation requires an explicit `enabled: true` and a nonempty, duplicate-free
list of agreed human user IDs. This source-review authority neither changes nor
overrides any source-access, upload or redistribution restriction.

The authorized human submits an `APPROVED` GitHub review whose entire body is
the following single line, using the separately supplied packet pin:

```text
JBIG2-SOURCE-REVIEW-SHA256: COORDINATOR_SUPPLIED_SHA256
```

Then run from a clean, committed checkout of the exact open PR head:

```sh
python3 -B tools/check_traceability.py \
  --check-audit-packet target/traceability/bytein-audit-packet.json \
  --expected-audit-sha256 COORDINATOR_SUPPLIED_SHA256 \
  --source-review-pr PR_NUMBER --source-review-id REVIEW_ID \
  --output target/traceability/source-disposition.json
```

This optional operation needs authenticated read access through `gh`; normal
packet validation and Cargo verification do not. It reads the configured
default branch and exact current base revision from GitHub, not authority
assertions supplied by the packet or candidate branch. The candidate must
target that current default-branch revision within this repository. Bot reviews,
PR-author self-review, wrong heads/pins, dismissed or superseded decisions,
ambiguous submission ordering and incomplete review-history retrieval fail.
The history bound is 2,000 reviews; exceeding it fails rather than accepting
a truncated history. Source changes require a fresh packet and explicit review.

The JSON and Markdown outputs label this as a source-snapshot disposition only;
normative approval and document-processing authorization remain false. This is
a point-in-time read-only check, not a timeless approval token. It establishes
the recorded account's disposition, not all contributors' independence or
semantic correctness. Normative review still requires the separate role,
authorization, source-review and dependency-closure gates below.

## Source-only navigation classification

The existing ledger's `model_classifications` is the sole authored purpose
registry. Each family lists exact `kind::name` members in one physical file;
there are no suffix/path wildcards. The inventory includes every registered
definition plus every spec function discovered in `specs/` and
`proofs/model_support/`. Unregistered declarations elsewhere (for example
additional shared views and property helpers) are outside this bounded scope,
not silently claimed as classified. All Rust source is nevertheless bound as
conservative dependency evidence.

Purpose labels are `project-result-model`, `project-policy`,
`representation-invariant`, `proof-support` and
`candidate-standard-facing-model`. They are not strength rankings. In
particular, a candidate is not independently approved normative evidence.
`classification-pending` requires a visible reason; a pending family records
membership, not a completed semantic review. Every label carries the explicit
`author-navigation-only-not-independent-normative-approval` review marker.

Authored families inspect bodies, dependencies and use sites across arithmetic,
MMR/Huffman, document/headers, generic regions, symbols/text, decode, halftone
and reader/bitmap support. These are author navigation judgments, not independent
normative reviews. For example, numeric bit results differ from typed error
policy; bitmap content/frame results differ from storage invariants and
projection/addressing vocabulary. `prefix_copy_result` is an output relation,
whereas `black_fill_prefix` is internal loop support.

The JSON report lists unresolved **purpose** ambiguity even when an enum has
been selected. Such ambiguity blocks classification completion. Separate
`caveats` describe partial scope, multiple uses or unreviewed normative fidelity;
they never grant approval, but do not by themselves make a known source purpose
unknown. Counts and completion come from the current report, not this guide.

Original roles, requirement IDs and locators are preserved. Unregistered
helpers have empty requirement links; recording them invents no normative
obligation. The implementation-constant scan is lexical, not a complete
dependency proof. Imported implementation tables/constants still need
independent review and are never normative ground truth.

The checker rejects duplicate identities/family members, unknown or missing
members and ambiguous repeated declarations. New, removed or renamed scoped
declarations must be explicitly reconciled. A registry digest binds the
original definition identities/roles/module paths. A project-source digest
binds the sorted mapping of relative paths to exact byte SHA-256 values for
all Rust files under `src/`, `specs/` and `proofs/`, using canonical JSON
(sorted keys, compact separators). This conservatively detects changed bodies,
module bindings, views and implementation tables, including source-set changes.
Updating a hash is not a classification review: re-read affected purposes and
dependencies, keep ambiguity visible, then rerun the checker and tooling tests.
No standard processing or normative approval follows from refreshing evidence.

### Inline structural navigation

Adjacent comments cover the same declared inventory, including registered
support outside `specs/` and `proofs/model_support/`. Each names the exact
`kind::name`, current purpose label and a short explanation. Registered members
also retain their exact ledger `Audit` ID. One existing linked row supplies
`REQUIREMENT CONTEXT` with its exact ID, standard and inherited clause string;
the complete set remains in the ledger/report. This is reading context, not a
new direct normative equation or an independently checked citation. Policy,
representation and proof helpers are not promoted to standard rules.
Unregistered candidates explicitly expose missing requirement/locator mapping.
No PDF-page offsets are inferred or newly verified.

The checker scans real parser comment nodes in **all** project Rust source,
masking strings and excluded code through the existing source helpers.
Every scoped declaration and registered reference must be covered exactly once.
Markers must be immediately before the named declaration, separated only by
whitespace/comments; intervening attributes, items or unsupported placement
fail explicitly rather than inheriting a nearby reference. Missing, duplicate,
unknown, wrong-purpose, wrong-file and misplaced markers fail. New scoped
declarations require authored classification and inline navigation, not a
hardcoded inventory total. `inline_navigation` reports structural coverage
separately from independently verified citations, normative approval and
document-processing authorization; all three latter flags remain false.

For comment-only identity refreshes, first compare the complete `src/`, `specs/`
and `proofs/` source set with the assigned exact base. Parse both versions and
compare all non-comment token leaves, retaining literal contents, rather than
masking away possible string/table changes. Only after code equivalence is
demonstrated may the provisional raw source bindings and source-purpose digest
be refreshed. This does not renew reviews, freeze a normative model or change
any selected requirement's provisional status.

Preserve token boundaries, literal whitespace, doc comments and every test/proof
cfg branch in that comparison. A comment masker that also removes strings,
test-only code or doctest-bearing comments cannot establish this equivalence.

Generate the inventory without providing any standard document:

```sh
python3 -B tools/check_traceability.py \
  --output target/traceability/reference-report.json
```

Inspect `model_audit_inventory` in that report for exact membership, purpose
counts, evidence of purpose and unresolved declarations. Counts are current
observations, not a fixed denominator or coverage percentage. The selected
requirement records remain provisional; snapshot state and document-processing/
normative-approval flags are unchanged.

## After authorization: review, freeze, then implement

1. Verify authorized source identity and scope. Provide only permitted inputs.
2. Obtain two source-to-model reports, listing findings, source/model hashes,
   reviewer identity and actual agent session/model receipts where applicable.
3. Have the named human adjudicate disagreements. Keep unresolved claims
   provisional. No author or implementer self-approval.
4. Record the approved normative model and its actual dependency closure,
   including tables, guards and assumptions. An implementation-dependent
   operational model may need a distinct independent normative layer.
5. Freeze that reviewed closure through an owner-controlled approval channel.
   The unreviewed packet checker above does not implement or authenticate this
   still-unconfigured approval channel.
6. Prove native Rust refines the frozen model. Implementers cannot silently
   change the target. Model corrections require separately source-cited
   re-review and invalidate affected prior evidence.
7. Re-run every required current-head proof, sensitivity, runtime, coverage and
   distribution gate. Do not reuse stale success for a changed head.

An immutable reviewed snapshot need not be merged as a broken specification
first; the final implementation PR can carry its unchanged reviewed model.
Audited slices do not approve unrelated rows or close global P04/P05/P06.

After the pilot, expand to MMR, the recorded retention interpretation conflict,
raw Huffman semantics, page/container lifecycle and remaining families.
Give humans short immutable spot-audit packets and let them choose additional
samples. Full conformance still needs exhaustive decomposition and the other
[readiness gates](open-source-readiness-plan.md).

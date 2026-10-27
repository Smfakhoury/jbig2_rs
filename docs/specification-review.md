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

Existing encoded-fixture, capacity/trust and official-positive interpretation
holds remain in force. Do not reconstruct held fixtures to satisfy this list.

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

The authored slice reads reader/bitmap bodies, their production callers and
selected support: numeric bit results versus typed error policy; bitmap content
and frame results versus storage invariants, machine/resource guards and
projection/addressing vocabulary. `prefix_copy_result` is a result, whereas
`black_fill_prefix` is internal loop support. Other families remain pending
because individual body/dependency/use-site review is unfinished, not because
their names prove their purpose. This is an honest partial classification.
The JSON report lists every unresolved declaration and its reason and cannot
report classification complete while any remain.

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

# Provisional statement-level T.88 traceability

P04 is in progress. The first ledger maps **16 selected requirement records**
to source contracts, project models, proof support and exact test identities.
It also inventories all **30 support-matrix features** as a decomposition
worklist. That worklist is not a count of all normative obligations.

The source is ITU-T T.88 (08/2018), edition 2.0, identified by the document hash
already in `source-provenance.json`. The approved document was consulted locally.
No standards text, diagrams, sample software or official vectors are added to
the repository. Ledger titles and scope notes are original paraphrases.

## Authoritative inputs and generated views

- [Requirements ledger](../validation/traceability/requirements.json): stable
  requirement IDs, exact clause/step references, scope, assumptions, gaps,
  definitions, contract evidence, tests and source bindings.
- `support-matrix.json`: authoritative feature/profile policy. Deferred encoder
  work can be excluded; required behavior, unsupported-feature rejection and
  prohibited substitutions cannot silently disappear from the worklist.
- `production-proof-coverage.json`: accounted-for runtime identities and current
  proof/disposition status, not a normative-conformance percentage.
- `target/traceability/*.json` and matching Markdown: generated reports with
  source links, model/proof support, exact tests, assumptions and unresolved gaps.
  CI uploads these instead of committing constantly regenerated reports.

All 16 initial records are **provisional**, pending owner normative/model review.
Five feature groups have partial requirement records; 24 remain not started,
one deferred feature is excluded, and none has an approved complete normative
decomposition. This tranche does not complete P04.

## Evidence strength is not inferred from a function count

| Classification | Initial records | What the record establishes |
|---|---:|---|
| `functional_contract` | 13 | Explicit postcondition evidence within the stated narrow property scope |
| `state_preservation` | 1 | Decoder/context invariants, not the requested semantic transformation |
| `safety_only` | 1 | A verified body without the claimed functional result postcondition |
| `runtime_divergence` | 1 | An executing/proof-body substitution already tracked by P05 |

The mapped areas are packed-bit/word order, standalone header fields and
rejection behavior, selected generic-template parameters, arithmetic reset and
decision contracts, and typical-row copying.

Important examples:

- `inspect_file_header` has an explicit relation to its local syntax/field
  models, including validity rejection and optional count interpretation.
  This does not prove complete-document page-count agreement.
- `build_document_info` checks page counts in its implementation and has a
  regression test, but its current verification annotation does not expose a
  functional return-value postcondition for that obligation.
- `ArithmeticContext::reset` explicitly promises zero index and MPS fields.
  The initial batch `reset_contexts` contract promised validity only. The P05
  reset tranche now proves all selected output indices and MPS fields are zero,
  preserves slice length, and retains the validity guarantee without adding an
  input-validity precondition. Reset call-site timing is still outside this
  helper-level record, and normative review remains provisional.
- Arithmetic decision decoding preserves bounded, valid state. Its exposed
  contract is not a complete relation between old state, decoded decision and
  the prescribed next state.
- Generic SLTP indices match the project's lookup model. Context-gathering
  order is implementation-defined; those numeric indices are not universal
  normative constants. The diagram-to-gather-order bridge and caller timing
  need separate evidence.
- A row-copy proof/runtime substitution cannot be credited as exact-source
  functional verification merely because scripted-entropy row tests pass.

The ledger records these distinctions rather than upgrading weaker evidence.
Definitions are not independent correctness theorems, and a model matching
implementation data is not by itself proof of a faithful standards translation.

## Checks and limits

The checker validates:

1. Schema, exact support-matrix worklist, normative edition/hash identity,
   unique requirement IDs and explicit partial/provisional states.
2. Hash bindings for every referenced source and module-declaration file.
   A source change invalidates the affected review; no automatic reapproval
   or hash-refresh workflow is provided.
3. Runtime IDs against the syntax inventory and source-bound proof accounting.
   Unverified/divergent bodies cannot back a direct functional claim.
4. Model namespaces by following actual Rust module declarations, then matching
   named spec/proof definitions. With a verifier report, exact qualified symbols
   must be present; same-named definitions in other modules do not suffice.
5. Proof-macro definitions and lexical calls from the recorded runtime body.
6. Recorded contract fragments in their declared `requires`/`ensures` section.
   Preconditions cannot satisfy an output-equivalence claim.
7. Test source identities, ordinary test cfgs and non-ignored status.
   `--discover-tests` additionally compiles/lists the exact Cargo targets and
   verifies the names actually exist in compiler discovery.

These checks establish reference integrity and preserve a reviewed strength
assessment. They do **not** decide semantic truth, infer complete call graphs,
prove test adequacy, establish that a requirement was tested merely from its
name, or certify that a model faithfully translates a clause. Owner review
and the actual proofs/tests remain essential.

The source scanner rejects unresolved/ambiguous paths and unsupported bindings
rather than guessing. The initial definition resolver supports external module
chains; inline model-module bindings require an explicit extension.

## Local and CI use

Prepare the existing development dependencies, then:

```text
python -m pip install -r tools/requirements.txt
python -B tools/check_traceability.py --discover-tests \
  --output target/traceability/reference-report.json
python -B tools/check_traceability.py --verus-json verus-production.json \
  --discover-tests --output target/traceability/combined-report.json
```

An approved local normative document can additionally be hash-checked with
`--standard-pdf /path/to/approved-document.pdf`. Its absence in CI is reported
explicitly; provenance identity is still checked. The checker does not acquire
or redistribute documents.

A supplied successful Verus report is not inherently fresh just because it was
loaded: the caller must run the pinned verifier on the same source. Verus CI
does this immediately before the checker. Its report marks verifier-report
checking separately from native CI's Cargo test discovery; listing tests is
not the same thing as executing them.

`--require-complete` needs both verifier evidence and test discovery and currently
fails because decomposition, review and semantic gaps remain incomplete. A normal
reference-check pass is not a release/conformance pass.

## Next work

Decompose the remaining coding families, organizations and page/resource rules;
extend the ledger with exact table/step mappings and model/contract assessments.
Keep T.6/MMR obligations and project-only resource policy distinct. Review
model adequacy, caller obligations and official-vector disagreements, then close
the semantic and exact-source gaps under P05. P06 coverage/mutation evidence
can strengthen testing but cannot substitute for those proofs.

## Batch-reset semantic sensitivity

`tools/check_reset_proof.py` validates that the stronger reset proof is not a
label-only promotion. In isolated copies it replaces the zeroing assignment with
valid but nonzero contexts `(I=1, MPS=0)` and `(I=0, MPS=1)`. Both compile as
ordinary Rust, fail the selected strong Verus reset proof, and pass the control
that removes only the zero-result postcondition/invariant while retaining
length and context validity. Thus validity alone does not prove E.3.7's selected
reset transformation.

```text
python -B tools/check_reset_proof.py \
  --verus-executable /path/to/pinned/verus \
  --output target/traceability/reset-proof-sensitivity.json
```

Verus CI runs this after normal whole-crate verification. The tool's selected
function report is explicitly partial and cannot replace the whole-crate gate.
No mutation or weakened contract is written to the repository's source tree.
Runtime regressions also cover all 65,536 stored byte pairs, idempotence, empty
slices and every subslice of a distinct-state array with unchanged neighbours.

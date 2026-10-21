# Provisional statement-level T.88 traceability

P04 is in progress. The ledger maps **48 selected requirement records**
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

All 48 records are **provisional**, pending owner normative/model review.
Thirteen feature groups have partial requirement records; 16 remain not started,
one deferred feature is excluded, and none has an approved complete normative
decomposition. This tranche does not complete P04.

## Evidence strength is not inferred from a function count

| Classification | Selected records | What the record establishes |
|---|---:|---|
| `functional_contract` | 32 | Explicit postcondition evidence within the stated narrow property scope |
| `state_preservation` | 3 | Decoder/context/storage invariants, not the requested semantic transformation |
| `safety_only` | 13 | A verified body without the claimed functional result postcondition |
| `runtime_divergence` | 0 | An executing/proof-body substitution already tracked by P05 |

The mapped areas are packed-bit/word order, selected file/segment organization
and header fields, Huffman construction/decoding helpers, JBIG2-specific MMR
exceptions, selected generic-template parameters, arithmetic reset and decision
contracts, and typical-row copying.

Important examples:

- `inspect_file_header` has an explicit relation to its local syntax/field
  models, including validity rejection and optional count interpretation.
  This does not prove complete-document page-count agreement.
- `build_document_info` now promises that accepted page-vector length equals
  both the positive-page-associated PageInformation count in its parsed-segment
  input and any declared count. Collection maintains prefix cardinality and
  finalization preserves order length. Both parser results now retain that
  relation to their returned parsed segments. `Decoder::inspect` and
  `DecoderContext::inspect` expose successful metadata's organization and count
  relation to the original standalone header bytes, including known-count
  agreement. This does not prove body-byte parsing, page identity or complete
  lifecycle semantics, rejection completeness or decoded output-page contracts.
  Embedded inspection wrappers also preserve the project's sequential/unknown-
  declaration metadata shape; this auxiliary API invariant is not an
  independent interpretation of D.4.3.
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
- The row-copy substitution is now removed. The actual native slice-copy path
  preserves every non-destination byte, copies the complete stored row and
  explicitly relates each visible destination pixel to the old source pixel.
  Typical-prediction timing, caller-loop semantics and the first-row zero edge
  remain separate gaps. Blank construction now also proves zeroed storage as a
  foundation for, not a complete proof of, the first-row caller rule.
  Two runtime/proof divergences remain in the
  full inventory outside this selected ledger.

The ledger records these distinctions rather than upgrading weaker evidence.
Definitions are not independent correctness theorems, and a model matching
implementation data is not by itself proof of a faithful standards translation.

## Bounded organization, Huffman and MMR decomposition

The next P04 tranche adds **28 records** without changing runtime, models,
proofs or production-proof accounting. It starts six previously unmapped
feature worklists: sequential, random-access and embedded organization, standard
and custom Huffman tables, and MMR. It also adds five selected segment-header
records under the decoder worklist.

| Selected area | New records | Evidence boundary |
|---|---:|---|
| D.1/D.2/D.3 organization | 6 | Verified parser bodies, not input-to-returned-segment layout/order theorems |
| 7.2.3/7.2.6/7.2.7 segment fields | 5 | Four narrow type/page/length helper contracts; flag extraction remains safety-only |
| B.2/B.3 custom tables | 8 | Three exact flag-field contracts; bound/line parsing and complete canonical assignment remain safety-only |
| B.4/B.5 shared decoding and predefined tables | 5 | Selected-line integer/OOB contracts and reader validity; no complete prefix-selection or independently checked table theorem |
| 6.2.6 JBIG2 MMR exceptions | 4 | Finishing/alignment helper contracts and bitmap storage validity; no complete raster/polarity result |

All references in this tranche were checked against the hash-pinned T.88
document, reacquired from its official public recommendation page through the
[official package link](https://www.itu.int/rec/dologin_pub.asp?lang=e&id=T-REC-T.88-201808-I!!SOFT-ZST-E&type=items).
The package and PDF matched the existing provenance hashes. Only the PDF was
extracted for consultation; no sample software or conformance payload was used
or added. No T.6 document was consulted in this tranche, so it does not invent
T.6 mode/run clause decomposition.

Five new ordinary Cargo tests in `tests/traceability_organization.rs` use
project-generated segment/page fields to check distinct sequential and
random-access two-page layouts, missing random-access EOF, descending standalone
numbers and headerless/non-monotonic embedded input. Existing exact test
identities back the other rows; the ledger references **67 distinct non-ignored
Cargo tests** in total. Discovery lists names; executing these tests is a
separate validation step. Neither operation establishes test adequacy.

The ledger explicitly leaves two newly exposed policy questions open:
custom-table range-partition overshoot rejection and MMR zero-alignment-padding
rejection may be stricter than the cited T.88 rule. Existing behavior is not
changed or granted normative approval here. The prior retention example
conflict also remains unresolved. B.5 is only a selected family record:
per-table normative review and complete enumeration are still required.

Every feature now records its exact selected `requirement_ids` and an explicit
`remaining_scope` worklist. The checker cross-checks membership in both
directions, disallows records under `not_started`/`excluded`, and disallows empty
`partial` features. Incomplete features must retain nonblank remaining scope;
the excluded encoder retains an explicit policy boundary. A `complete` feature
would require nonempty, approved, gap-free functional records and no remaining
scope, but that is still a review declaration, not a proof of exhaustive
enumeration. No actual feature is marked complete.

## Checks and limits

The checker validates:

1. Schema, exact support-matrix worklist, normative edition/hash identity,
   unique requirement IDs, exact per-feature requirement membership, explicit
   remaining scope and consistent decomposition/review states.
2. Hash bindings for every referenced source and module-declaration file.
   A source change invalidates the affected review; no automatic reapproval
   or hash-refresh workflow is provided.
3. Runtime IDs against the syntax inventory and source-bound proof accounting.
   Unverified/divergent bodies cannot back a direct functional claim.
4. Model namespaces by following actual Rust module declarations, then matching
   named spec/proof definitions. With a verifier report, exact qualified symbols
   must be present; same-named definitions in other modules do not suffice.
5. Proof-macro definitions and lexical calls from the recorded runtime body.
6. Recorded contract fragments in their declared section. Preconditions cannot
   satisfy output-equivalence or state-preservation claims. Functional evidence
   cannot be hidden under a weaker strength label. Duplicate runtime, model,
   test, claim and whitespace-equivalent token evidence is rejected, as are
   unused or aliased duplicate model/proof definitions.
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

Complete enumeration of the partially mapped organizations/coding families and
decompose the remaining coding families and page/resource rules;
extend the ledger with exact table/step mappings and model/contract assessments.
Keep T.6/MMR obligations and project-only resource policy distinct. Review
model adequacy, caller obligations and official-vector disagreements, then close
the semantic and exact-source gaps under P05. P06 coverage/mutation evidence
can strengthen testing but cannot substitute for those proofs.

## Document-count semantic sensitivity

`tools/check_document_count_proof.py` runs five isolated counterexamples:
accepting an inconsistent declared count, dropping a finalized page, and
collecting a PageInformation payload with a non-PageInformation header, plus
discarding the standalone header count and inventing an embedded declaration. Each
compiles as ordinary Rust, fails its selected strong contract and passes a
control removing only the relevant cardinality clauses. Other metadata and
collection length-agreement guarantees remain in those controls. The
non-PageInformation-header case deliberately separates parsed header/data consistency from counting;
it is not evidence that such input can pass the raw-byte parser.

The parser controls remove only the corresponding declaration relation in the
isolated metadata model, retaining header validity, organization and known-count
agreement. The proof-only file-header model module is crate-visible so the
document predicate reuses the already verified field interpretation rather
than duplicating header parsing. All runtime statements are unchanged in this
public-contract propagation tranche. Generated tests exercise all six slice
inspection APIs, reuse after failures, both organizations, known/unknown counts,
extended flags, embedded globals and colour-header rejection. Reader results
are tested but `std::io::Read` remains a reviewed formal boundary.

The collection guard uses an equivalent `matches!` pattern instead of generated
enum `PartialEq`: the latter did not expose the structural equality needed by
the proof. Mutable keyed access now exports preservation of slice length.
There are no new assumptions or runtime/proof substitutions. The result is
still provisional, scoped evidence for D.4.3, not full conformance.

## Executing typical-row copy

`Bitmap::copy_row` now uses safe disjoint slices and `copy_from_slice`, not a
word loop paired with a different ghost scalar helper. Bounds and disjointness
follow from valid storage and in-range row numbers. Runtime checks retain typed
errors for invalid ranges; self-copy and zero-stride rows remain no-ops.
`copy_bitmap_row` calls the same implementation in both builds. The obsolete
proof-only scalar row copier is deleted.

The shared packed-byte helper proves complete slice equality, including its short
0-2 byte and 65-128 byte fixed-block paths. Those paths avoid measured small-row
library-call overhead without architecture-specific unsafe code. Kernel timing
is informational on a shared host, not a substitute for P09's dedicated gate.

The functional byte model specifies the entire output buffer, including
unchanged bytes outside the destination row. A proved bitmap-layout bridge
establishes the wrapper's explicit visible-pixel equality. Pinned vstd's
`split_at_mut`/`copy_from_slice` specifications, Rust's standard library and the
compiler are part of the trust boundary; this does not prove their internals.

`tools/check_bitmap_copy_proofs.py` skips the row copy or corrupts a different row in
isolated copies. Both mutants compile normally, fail the strong byte-result
contract and pass the control removing only that result while retaining
storage validity, metadata, length and success. CI uploads the selected
semantic evidence separately from its required whole-crate proof. Exact tests
cover arbitrary stored bytes, padding, both row orders, self-copy, zero width,
large rows and invalid ranges; existing scalar-oracle comparisons remain.

This closes the helper-level row-content and exact-execution gap, not SLTP
state/timing, caller-loop output semantics, first-row caller zero-edge semantics or
full generic-region conformance. All requirement reviews remain provisional.

## Segment-header retention field interpretation

The approved edition's explicit field definitions in 7.2.4 assign flag bits
from the first wire byte onward, least-significant bit first within each byte.
Unused positions are in the final long-form flag byte. The previous runtime
and model traversed bytes in reverse and checked padding in the first byte.
Generated independent tests failed before correcting both sides.

There is an **unresolved inconsistency with the worked example in 7.2.8**:
its existing two-byte fixture has the opposite order to the field definitions
and its accompanying interpretation. This branch follows 7.2.4, preserves the
existing fixture as an explicit strict rejection, then tests the field-order
correction. It does not add a guessed dual-order acceptance heuristic.
Owner normative adjudication remains a recorded gap, not an assumed approval.

This intentionally changes multi-byte long-form retention behavior. Short
fields and one-byte long fields retain their interpretation. Successful
materialization now proves the exact count and wire-bit relation for every
flag, rather than just vector bounds. The private helper's empty-vector
precondition is discharged by its verified constructor.

Four provisional records cover flag order/materialization, unused bits,
reference-width boundaries and strictly lower reference numbers. They are
helper-level results: raw header-window correspondence, reference graph
validity and retention lifecycle/caller rules still need separate work.
Generated tests cover all flag/count bytes, all 65,536 two-byte flag patterns,
field-width boundaries, page forms, truncations and exact errors. The three
compiled controls in `tools/check_retention_proofs.py` reject reversed order,
first-byte padding checks and inverted materialized flags; weaker controls
retain bounds/length safety without those semantic guarantees.

The approved official-vector classification was re-executed locally and remains
two exact outputs, seven reviewed strict rejections and one unsupported colour
case. That unchanged classification does not adjudicate this worked-example
conflict or establish full conformance. No standards text or vector assets
are added to the repository.

## Exact prefix and zero-padding implementation contracts

The same packed-byte helper now serves `Bitmap::copy_prefix_from`. The old
normal chunked helper and proof-only scalar helper are deleted. Accepted copies
promise the exact source prefix and unchanged destination suffix; rejected
counts promise the exact error and no destination mutation. `Bitmap::blank`
and false-filled construction promise zero stored bytes, and height padding
preserves decoded bytes while zeroing the newly added storage.
An eight-byte prefix specialization uses a fixed-size standard slice copy;
the existing row-copy implementation and shared helper bodies are unchanged.

The generalized bitmap sensitivity tool runs six compiled counterexamples:
skipping a row, corrupting another row, skipping a prefix, corrupting an
uncopied suffix, creating a nonzero blank bitmap, and discarding decoded rows
during padding. Each fails its strong selected proof and passes a control
removing only the corresponding output clause, retaining storage/dimensions
and, where applicable, exact error/non-mutation guarantees. The report is
`bitmap-copy-proof-sensitivity.json` in Verus CI artifacts; it is explicitly
partial and cannot replace normal whole-crate verification.

These prefix/padding statements are project implementation contracts, not
newly approved T.88 requirement records. The existing typical-row record links
zero initialization as supporting evidence while retaining first-row caller
and SLTP timing/loop gaps. Its classification and all provisional owner-review
states remain unchanged. The tool replaces the former row-only checker; no
ordinary crate API changes.

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

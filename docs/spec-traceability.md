# Provisional statement-level T.88 and T.6 traceability

P04 is in progress. The ledger maps **266 selected requirement records**
to source contracts, project models, proof support and exact test identities.
These comprise **257 selected normative-rule records and nine explicit project-
profile-policy records**. A policy row uses the standard only as context and
does not increase the normative-rule count.
It also inventories all **30 support-matrix features** as a decomposition
worklist. That worklist is not a count of all normative obligations.

The sources are ITU-T T.88 (08/2018), edition 2.0, and the separately recorded
ITU-T T.6 (1988) normative reference. Their distinct document hashes are already
in `source-provenance.json`. T.6's reference has no separate edition field;
the ledger records `edition: null`, not an invented edition or a reused T.88
value. Both approved documents were consulted locally.
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

All 266 records are **provisional**, pending owner normative/model review.
Twenty-nine feature groups now have partial records, none is unstarted, one
deferred feature is excluded, and none has an approved complete normative
decomposition. Starting every coarse feature worklist does **not** mean that
every normative requirement has been enumerated. This work does not complete
P04. Three worklists contain only project-policy records, not a completed
normative decomposition.

## Evidence strength is not inferred from a function count

| Classification | Selected records | What the record establishes |
|---|---:|---|
| `functional_contract` | 113 | Explicit postcondition evidence within the stated narrow property scope |
| `state_preservation` | 127 | Decoder/context/storage invariants, not the requested semantic transformation |
| `safety_only` | 26 | A verified body without the claimed functional result postcondition |
| `runtime_divergence` | 0 | An executing/proof-body substitution already tracked by P05 |

The mapped areas are packed-bit/word order, selected file/segment organization
and header fields, Huffman construction/decoding helpers, selected T.6 mode/run/
framing rules and JBIG2-specific MMR
exceptions, selected generic-template parameters, arithmetic reset contracts,
selected Annex A/E algorithm and table relations, and typical-row copying.

Seventeen additional selected pattern/halftone header-field records now link
exact project acceptance predicates, successful raw fields and seven/twenty-one
byte cursor advancement. Their locators were checked locally on 2026-10-23
against the unchanged approved T.88 hash above: 7.4.4.1.1-7.4.4.1.4 and
7.4.5.1.1-7.4.5.1.3.2, with Figures 41-46's field layouts/captions.
Only original paraphrases, identifiers and locators are recorded, not source
prose, diagrams or vectors. The halftone-specific helper starts after region
information; signed origins and unsigned vectors retain their encoded
256-scaled representation. Native parser bodies/preconditions are unchanged.
Nine real compiling contrasts include deliberately rejecting otherwise valid
headers, so accepted-field contracts are not the only success guarantee.
Typed partial-error/cursor contracts, full segment/caller/resource and raster
composition, exhaustive decomposition and independent normative review remain
open. Both feature worklists remain partial, and all new records provisional.

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
- Arithmetic decision decoding now relates the successful bit and next
  probability-context index/MPS to the project Qe-table model and entry
  interval/code. Exchange helpers additionally specify context updates on
  error. Exact renormalized registers, byte consumption, initialization/refill
  and normative translation are not yet proved. The aggregate record retains
  state-preservation strength rather than claiming the complete transition.
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
  See the current audited inventory for remaining runtime/proof substitutions.
  A caller's storage contract is not independent exact-source evidence for
  every transitive kernel it invokes.
- The executing optimized `Bitmap::fill_black_span` now proves valid-storage
  success, exact black requested pixels, and preservation of all other stored
  bits, including dirty padding, with a derived outside-byte frame. This
  auxiliary primitive result is not yet a complete MMR raster-loop/polarity
  theorem: the wrapper's weaker storage predicate and the decoder's caller
  composition remain separate obligations. The polarity record remains
  `state_preservation`, and all normative records remain provisional.

The ledger records these distinctions rather than upgrading weaker evidence.
Definitions are not independent correctness theorems, and a model matching
implementation data is not by itself proof of a faithful standards translation.

The arithmetic refill tranche adds an exact project-step relation for
`byte_in_signal` and its checked wrapper: return classification and all
position/current-byte/code/count/marker outputs, including EOF and partial
overflow consumption. The BYTEIN interface record links this shared model and
the all-byte-pair/boundary tests. Its ten leaf records now distinguish these
project guarantees from remaining normative/caller gaps instead of saying the
postconditions are absent. All eleven retain provisional state-preservation
strength; no requirement count, review approval or full-stream claim changes.

The initialization tranche composes the project seed/refill transition and
exposes exact constructor acceptance and returned state for every supplied
slice. A proved arithmetic bound excludes code-scaling overflow for all
two-byte prefixes. The six INITDEC records now distinguish this complete
project-constructor relation from still-unreviewed normative register
interpretation, caller slice selection and later decoding. They retain their
prior strengths and provisional reviews; the new model and exhaustive prefix
tests do not constitute six independent normative proofs.

## Selected Annex A/E arithmetic enumeration

Three selected Annex A integer-history/context records now link exact
single-step output contracts: the returned bit and full decoder/error state,
selected-context update and unselected-bank frame, growing/rolling history,
and unchanged history on error. Nine actual compiling contrasts and 144,384
numeric/frame scenarios supplement the proofs. The records remain provisional;
independent normative interpretation stays open.
The complete signed-integer follow-up now composes the seed/sign, all six
leaf counts/offsets, MSB-first payload and signed value/OOB against
`integer_step_state`, `integer_selection_state`, `integer_bits_run` and
`integer_state`. The entry point exposes exact result/full-decoder/full-bank
equality; the payload helper additionally exposes its complete history.
Twelve selected algorithm rows gain actual functional project evidence.
Procedure-bank/storage and all fourteen Table A.1 rows deliberately remain
state-only: complete project composition is not an independent caller or
representation theorem. Nine additional compiling contrasts and 31,080
numeric/state/prior-procedure comparisons supplement the contracts.
IAID preflight rejections now have exact project classification and state frames,
and the complete IAID follow-up proves the positive prefix/value fold and all
decoder/context outputs against `iaid_run` and `iaid_state`. Six selected rows
now link the production result/decoder/bank equalities, covering seed, context,
history, iteration count, sentinel removal and result. Six additional compiling
contrasts and 3,600 numeric/state/prior-procedure comparisons supplement these
contracts. The 31-bit cap is not an Annex A limit; the SBSYMCODELEN caller and
normative address-space obligations remain open. The two LENGTH/STORAGE rows
are deliberately **not** upgraded.

The concrete arithmetic codec now exposes exact project value/error forwarding,
full decoder/selected-bank relations and all other bank/refinement/work frames.
Its field slot equals the project ordinal, and the arithmetic error adapter
has an exact field-preserving model. Eight compiling contrasts and 12,648 direct
caller cases supplement these contracts. This still does not prove that every
dictionary/text call supplies the normative field or that symbol counts imply
the supplied SBSYMCODELEN and bank address space. The PROCEDURE-BANKS and two
IAID LENGTH/STORAGE rows therefore gain links but **retain** state-only strength
and provisional review. No selected count, normative approval or decomposition
status changes.

The complete decision-state follow-up gives seven selected E.3.2 records
functional project-step evidence: interval subtraction, strict code partition,
upper code subtraction, fast MPS, both exchange/normalization paths and the
returned decision. Production contracts expose the full signal/register/history
tuple against the closed `decision_state` model and its scalar
`decision_registers`/`exchange_normalization` composition. Context and input
frames remain, and checked errors have a typed project projection.
Fifteen actual compiling decision contrasts and wide numeric state oracles
supplement these theorems. All seven records remain provisional: independent
Qe/register/pointer interpretation and complete stream/caller semantics remain
open. The ledger is still a selected, nonexhaustive denominator.

The normalization follow-up gives six selected records explicit functional
evidence against the project `normalized_state` model: the low-input register
row and all five `RENORM-*` rows. Both signal and checked helpers expose exact
signal/acceptance, interval, consumed position/current byte, code, counter and
marker state, including EOF/refill overflow and interval-before-code-overflow
ordering. Bytes remain unchanged, and existing preconditions are not
strengthened. The decreasing measure remains termination support; the new
recursive output model supplies the register recurrence.

Two independent numeric tests use a closed-form leading-zero shift count and
wide arithmetic across every byte pair, normalized no-op states, shift limits,
real marker fills, stuffing, EOF and partial overflow. Six native-compiling
mutants violate individual result relations and pass when only the affected
postconditions are omitted; input framing, state validity and loop recurrence
remain in the controls. These results do not approve normative register/pointer
interpretation, caller sequencing or full arithmetic-stream conformance.
The selected arithmetic membership and its provisional review states are unchanged.

This tranche adds **138 provisional normative records**, bringing the arithmetic
worklist from three records to **141**, without changing runtime, models, proofs,
coverage floors or trust/attestation metadata. The two existing reset records
retain their narrow functional classification; the aggregate decision record
retains state-preservation strength. At initial enumeration all 138 additions
were **state-preservation** records with explicit missing functional relations.
Thirty-four additions now have narrow functional project evidence across
normalization, decision composition, integer history/procedure and IAID.
The remaining 104 retain their original state-preservation classification.
The original 111 IDs and review states are preserved.

| Consulted source / selected membership | Added records | Accounting boundary |
|---|---:|---|
| A.1/A.2 procedure namespaces, history, sign, six magnitude leaves, payload order, three result cases | 17 | Fifteen now link functional project history/procedure contracts; caller namespace/storage obligations remain state-only and unapproved |
| A.2 Table A.1, each of its 14 representation rows | 14 | Separate row locators, including negative unit and OOB; no copied encodings or independent table-to-output proof |
| A.3 fixed length, seed, context, recurrence, iteration count, sentinel removal, return and storage | 8 | Six rows now link complete project result/decoder/bank contracts; normative caller length and address-space sufficiency remain state-only and unapproved |
| E.3/E.3.1 interface, stream/context loop and register conventions | 6 | Caller sequence/stream agreement and logical code-register interpretation remain gaps |
| E.3.2 Figure E.15 decision steps | 8 | Lookup, interval subtraction, strict partition, high-code subtraction, fast MPS, both exchange/renormalization paths and return |
| E.3.2 Figures E.16/E.17 exchange branches | 11 | Both comparisons, old-sense decisions, NMPS/NLPS, switch/no-switch and lower-path interval replacement |
| E.3.3 Figure E.18 normalization steps | 5 | Exact project refill/shift/counter/exit recurrence now linked; normative register interpretation and caller composition remain open |
| E.3.4 Figure E.19 input branches | 11 | FF/marker tests and each branch's pointer, code alignment/fill and exact bit count |
| E.3.5 Figure E.20 initialization steps | 6 | Pointer, code seed, BYTEIN, alignment shift, counter adjustment and initial interval |
| Decoder-shared E.2.5 estimator rules and each Table E.1 index 0–46 | 52 | Five rule records plus 47 four-field row relations; bounded transitions are not table fidelity |

`T88-ARITHMETIC-INTEGER-*`, `IAID-*`, `DECODE-*`, `EXCHANGE-*`,
`RENORM-*`, `BYTEIN-*`, `INITDEC-*` and `QE-*` are stable selected membership
under the arithmetic prefix, not new feature groups or an exhaustive denominator.
Exact table locators and finite selected membership are regression-tested.
The ordinary reference checker validates every runtime/model/macro definition,
postcondition token, source binding and test identity; Cargo discovery and a
fresh pinned-Verus report validate executable/proof membership. No unchecked
parallel requirements format or completion flag is introduced.

**Source consultation, 2026-10-21:** the approved shared local T.88 PDF was
read without modifying the shared assets. Its SHA-256 was
`dff16153d2b959d78e0018bfc0a054bd4ba136b0a156021b03e4743bfa6af2d6`;
the recorded official package also independently hashed to
`e2a3c458c4d28d5a835c0815fd9b23e197dbc1b83fe81d8e8d8b2ac413d8f22f`.
Acquisition provenance remains the ledger's
[official ITU package URL](https://www.itu.int/rec/dologin_pub.asp?lang=e&id=T-REC-T.88-201808-I!!SOFT-ZST-E&type=items),
not a new download or a third-party source. Consultation used local text for
A.1–A.3 (printed pages 89–92), decoder-shared E.2.5/Table E.1 (115–116),
E.3–E.3.5 (117–122) and E.3.7 (123). Figure A.1 and Figures E.13–E.20
were also rendered and visually read from PDF pages 102 and 130–134;
their flowchart branches were **not** inferred from missing extracted text.
Every new row's exact clause/step/table locator resolves through its standard
identity to this hash. T.6 was not freshly consulted for this tranche;
its existing consultation history and records are unchanged. This consultation
is not owner approval or an independent formal standard-translation review.

The existing integer/IAID scripted tests run callback reference procedures.
The concrete comparison test runs both production helpers on an existing
stream. The complete IAID test additionally compares 3,600 direct states to
an independently accumulated numeric value/context-address oracle and the
prior checked procedure, including zero length, success and partial errors.
The signed-integer follow-up adds 11,280 complete-procedure and 19,800
payload-fold comparisons, exercising every leaf and positive/negative/zero/OOB/
EOF/overflow outcomes. These direct states are not claimed as valid encoded
streams or exhaustive representation-range tests. The randomized
decision oracle repeats the implementation algorithm; the table test repeats a
project-local constant list; neither is an independent normative specification.
The older IAID pure-model lemmas start from an assumed decision sequence.
The new production postconditions instead equal the complete `iaid_state`
model, which composes the exact decision state with each context update.
Neither this composition nor shared Qe data independently approves normative
bitstream interpretation. Records distinguish these claims rather than
treating test names or bounds lemmas as functional proof.

Selected-section accounting is now substantially finer grained, but remains
**provisional and partial**. Owner review must check for omissions and confirm
the paraphrases and contract strengths. Procedure-bank/caller identity,
initialization/reset timing and cross-clause composition remain open.
E.3.6 recovery (explicitly not standardized), E.3.8 GB/GR save/restore,
encoder algorithms and non-normative Annex G are outside this tranche, not
newly completed features. Other P04 feature worklists retain their existing
undecomposed scope. The owner PR #47 separation is preserved:
`--require-reviewed-decomposition` still fails for these unreviewed partial
records; `--require-complete` still additionally requires functional semantic
closure. Neither P04 completion nor full P05/conformance is claimed.

## Initial bounded organization, Huffman and MMR decomposition

The initial bounded P04 tranche added **28 records** without changing runtime, models,
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

Five ordinary Cargo tests added in `tests/traceability_organization.rs` use
project-generated segment/page fields to check distinct sequential and
random-access two-page layouts, missing random-access EOF, descending standalone
numbers and headerless/non-monotonic embedded input. Existing exact test
identities backed the other rows; that first tranche referenced 67 distinct
non-ignored Cargo tests. The current ledger now references **134**. Discovery
lists names; executing these tests is a
separate validation step. Neither operation establishes test adequacy.

The ledger explicitly leaves custom-table range-partition overshoot rejection
and T.6/T.88 padding applicability open. The later approved T.6 consultation
establishes that its post-EOFB pad format uses zero bits; whether every T.88
prefix/known-extent case inherits that rule still needs review, as does fixed-
block versus octet-only framing. Existing behavior is not changed or granted
normative approval here. The prior retention example
conflict also remains unresolved. B.5 is only a selected family record:
per-table normative review and complete enumeration are still required.

Every feature now records its exact selected `requirement_ids` and an explicit
`remaining_scope` worklist. The checker cross-checks membership in both
directions, disallows records under `not_started`/`excluded`, and disallows empty
`partial` features. Incomplete features must retain nonblank remaining scope;
the excluded encoder retains an explicit policy boundary. A `complete`
decomposition requires nonempty, reviewed records and no remaining enumeration
scope. It can account explicitly for known semantic gaps and weaker proofs:
P04's map must represent those honestly rather than require P05 to finish
before P04 can complete. This is still a review declaration, not a proof of
exhaustive enumeration. No actual feature is marked complete or newly approved.

The separate `semantic_claims_complete` gate additionally requires every
record to have functional evidence, no semantic gaps and approved review.
The existing `--require-complete`, overall report status and legacy
`normative_decomposition_complete` field retain that stronger meaning.
`decomposition_review_complete` and `--require-reviewed-decomposition` expose
the narrower P04 milestone without closing any P05 obligation. A reviewed map
with known unproved transformations can pass the latter but must fail the
full gate and retain `semantic_claims_review_required` as its release state.

## Continued family-specific mapping

The second bounded tranche added **46 records** across the 16 worklists that previously had
no records. They are not feature-label placeholders: each has a clause/context,
exact runtime/test identities, a scoped contract assessment, assumptions and
unresolved obligations.

| Worklist | Added records | Selected obligations and boundary |
|---|---:|---|
| Known-height pages | 3 | Dimensions/resolutions remain safety-only; default-pixel flag interpretation is proved, initialization pixels are not |
| Unknown-height pages | 2 | Sentinel interpretation remains unproved; successful unknown-height metadata requires striping |
| Striped pages | 3 | Exact end-row field helper; row progression/max-size and final-height lifecycle results remain unproved |
| Generic refinement | 4 | Context-size/SLTP lookups; adaptive geometry and reference-neighbourhood prediction remain open |
| Symbol dictionaries | 4 | Selected flags/export count, bounded class state and export assembly; complete run/order/pixel semantics remain open |
| Text regions | 4 | Selected flags/current-state placement/checked ID; clipped raster and full strip recurrences remain open |
| Pattern dictionaries | 3 | Count and collective-width helpers; per-pattern output pixels/order remain unproved |
| Halftone regions | 5 | Grid/depth/nonintersection helpers; executing Gray-plane reconstruction and rendering have storage-only contracts |
| Immediate regions | 3 | Caller storage plus sealed-stripe guard; not a kernel-equivalence or composed-pixel theorem |
| Intermediate regions | 2 | Auxiliary storage validity and permission checking; exact buffer identity/lifetime remain open |
| Refinement aggregation | 2 | Mode flag and shared multi-instance text-refinement geometry; count-one dispatch/parameters and aggregate pixels remain open |
| Global segments | 2 | Page association and reference scope checks; no complete global-resource/lifetime theorem |
| Combination operators | 3 | Boolean operator, wire-code helper and page override guard; packed/caller pixel semantics remain separate |
| Colour profile rejection | 2 policy | Typed page/palette rejection, not colour decoding conformance |
| No lossy decoder substitution | 2 policy | Exact checked-ID and bitmap-clone helper results, not end-to-end symbol selection/rendering |
| Globals-backed embedded API | 2 policy | Reusable resource API and embedded metadata shape, not PDF container semantics |

Thirteen additional generated public-API tests in
`tests/traceability_semantics.rs` check page fields/default pixels, all operator
codes against a Boolean oracle, override permission, clipped page effects,
intermediate non-page effects/permissions, stripe errors/final height, global
scope and typed colour exclusion. At that stage the ledger linked 90 library tests,
nine `spec_header`, one `catalog_contracts`, five `traceability_organization`
and 13 `traceability_semantics` tests at that 94-record stage, all ordinary and
non-ignored. The T.6 extension below adds 16 existing library-test identities.

The checker now distinguishes `normative_rule` from `project_profile_policy`.
Policy evidence must name an existing explicit support-matrix policy and match
its hash. The six feature policies bind their exact feature rows; the three
T.6-context typed-error policies explicitly select and hash the approved
`public_api` section containing the typed-errors requirement. Changing the
selected policy note, failure policy, approval or scope
invalidates the policy evidence. Policy rows remain visible in both generated
views, separately counted from normative rules. Informative 0.2 is not promoted
to a new normative prohibition, and a reusable globals API is not presented as
a PDF-standard requirement.

`source_consultations` retains the exact official acquisition URL, approved
package/PDF hashes and consulted clause families even after local source
intermediates are removed. The checker rejects missing/duplicate consultations,
stale identities, nonofficial URLs and URLs naming another publication/package.
It checks recorded identity, not whether a human translation is correct or
every clause has been enumerated. No document contents are stored in the ledger.

Immediate-region records deliberately reference unchanged caller storage
contracts, not a scalar composition substitute or a claim that all transitive
kernels are source-identical. See the current audited inventory for remaining
substitutions. The new mappings do not use `compose_scalar_verified` as evidence
and do not infer visible-pixel correctness from storage preservation.

Owner adjudication is still needed for the retention worked-example conflict,
custom Huffman overshoot rejection and T.6/T.88 padding applicability.
Independent model/diagram/table review, complete clause enumeration and
functional caller/raster composition remain unresolved. T.6 now has separately
consulted approved source evidence and selected records, but its complete
decomposition remains open. Any actual PDF-container normative claim still
needs its own approved source. These are real boundaries, not closed by having
started every feature worklist.

## Approved T.6 MMR source and bounded decomposition

The concrete T.6 prerequisite used **only the recorded official acquisition**:

```text
https://www.itu.int/rec/dologin_pub.asp?lang=e&id=T-REC-T.6-198811-I!!PDF-E&type=items
SHA-256: 8ccbe2d203e03ee2ac37478d50d2d946bab49dfb45376eb0f773009d183a7767
```

It returned HTTP 200 at that exact URL and matched the approved
`normative_references` document hash. No fallback or new permission was used.
No package identity is invented for this direct PDF, and no T.88 hash or edition
is reused. The checker resolves the explicitly selected provenance collection,
rejects ambiguous/missing identities, and treats an unrecorded edition as
explicit `null`. Generated rows now identify their standard so T.6 clause 2.x
cannot be mistaken for T.88.

Seventeen records are added: **14 selected T.6 rules and three separately
hash-bound project typed-error policies**. Consulted families are 2.2.1–2.2.2,
2.2.3.1–2.2.3.3/Table 1, 2.2.4/Tables 2–3, 2.2.5.1–2.2.5.2, 2.3.1 (optional-
mode context only) and 2.4.1.1–2.4.1.3. No table entries, diagrams, bit vectors
or standards prose are added.

| Selected obligation | Boundary actually established |
|---|---|
| Initial white reference and previous-row reuse | Raster storage/reader invariants, not reference pixel equivalence |
| Changing elements and pass/vertical/horizontal transitions | Bounded reference positions and safe raster state, not an exact line-transition theorem |
| Mode prefix dispatch | Exact old-input bit-prefix, implementation-table mode and consumed-length witness; independent table fidelity and complete acceptance/errors remain open |
| White/black run lookup | Matching supplied key and membership in implementation arrays; no independent T.6 table-translation proof |
| Make-up/terminating accumulation and initial imaginary white run | Reader/row bounds and storage, not run summation or initial-position equivalence |
| Terminal reference sentinels | Valid width-sentinel representation and two added entries, not preservation of all earlier transitions or full row pixels |
| Required EOFB | Exact local marker predicate on successful required framing; T.88 omission exceptions remain explicit |
| Post-EOFB padding | Reader/framing invariants; zero-padding values and T.88/fixed-block applicability remain open |
| Truncated runs, row overshoot and required-marker truncation | Executable private typed-error regressions and bounded reader state, not T.6-mandated Rust diagnostics or a proved public-error mapping |

The names `mode_code_in_implementation_table` and
`run_code_in_implementation_tables` explicitly denote project array-membership
predicates. Naming does not make those arrays independently
faithful to T.6. Ordinary table-driven tests can share the implementation tables;
the ledger records that limitation rather than promoting them to a normative
oracle. All new records remain provisional with explicit gaps.

There are now **134 linked ordinary non-ignored tests**: 106 library, nine
`spec_header`, one `catalog_contracts`, five `traceability_organization` and
13 `traceability_semantics`. The T.6 tranche adds no production source, proof,
table data or official test assets.

## Checks and limits

The checker validates:

1. Schema, exact support-matrix worklist, normative edition/hash identity,
   unique requirement IDs, exact per-feature requirement membership, explicit
   remaining scope and consistent decomposition/review states; source
   consultation identity and separate hash-bound policy accounting.
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
   unused or aliased duplicate model/proof definitions. Commented-out clauses
   and bare `true` cannot replace genuine functional/state evidence after
   source hashes are refreshed. Evidence must come from the actual
   ghost-conditioned `verus_spec` attribute, not matching text in documentation.
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

Approved local normative documents can additionally be hash-checked by explicit
identity, for example:

```text
python -B tools/check_traceability.py --discover-tests \
  --standard-document 'ITU-T T.6 (1988)=/path/to/approved-t6.pdf' \
  --output target/traceability/t6-report.json
```

Repeat `--standard-document STANDARD_ID=PATH` for each available document.
`--standard-pdf /path/to/approved-document.pdf` retains its legacy meaning:
the first declared standard, not an inferred PDF identity. Unknown/duplicate
selections and cross-standard hash substitutions fail. Reports expose
`normative_document_hash_checks` separately for each standard; the aggregate
`normative_document_hash_checked` is true only when **all** declared documents
were checked in that invocation. A T.6-only check does not claim the deleted
T.88 local copy was rechecked. Absence in CI is explicit; provenance identity
is still checked. The checker does not acquire or redistribute documents.

A supplied successful Verus report is not inherently fresh just because it was
loaded: the caller must run the pinned verifier on the same source. Verus CI
does this immediately before the checker. Its report marks verifier-report
checking separately from native CI's Cargo test discovery; listing tests is
not the same thing as executing them.

All twenty-four semantic groups execute; four deterministic shards reduce
repeated setup without dropping any of the 141 required strong/weaker compiling
contrasts. Each shard executes sequentially, and the final collector requires
every configured group exactly once, its selected-case baselines and native
compilation results, individual report/log/checker hashes, and matching
whole-crate source/tool/run/attempt identities. The source-bound receipt
includes the complete shard assignment. The stable `verify` check fails on a
missing, failed, duplicate, stale or wrong-shard result.

The bit/word records additionally link exact auxiliary project reader failures.
Successful MSB-first value semantics and exhaustive bit tests already existed.
All byte methods now expose EOF payloads; bit reads preserve invalid-count
priority/value, exact EOF and absence of spurious overflow or valid-width
rejection. Internal overflow-bound support is separately source-bound under
proofs, not presented as a normative theorem. No requirement, strength or review
count changes at that reader tranche: all266remain provisional111/129/26. Project error payloads do
not establish normative rejection/marker-fill rules or downstream conformance.

The subsequent MMR prefix tranche closes a distinct table-witness gap:
mode and run-code success now prove that the implementation key equals the
actual old input's consumed `bits_value`, not merely some same-length table
entry. Four compiling wrong-key contrasts retain old membership/length/reader
guarantees under their weaker controls. One mode record gains functional
project input-to-code evidence; the truncation-policy row only gains an
auxiliary success relation and retains state-only strength. All266remain
provisional112/128/26. Shared-table scalar windows are not independent T.6
table approval, complete mode/error/run-total or row/raster/caller theorems.

The run-sequence follow-up proves that each successful `decode_run` has a
nonempty input-bound implementation-table sequence: preceding codes are
make-up, the final code is terminating, and its value sum and bit-length sum
equal the returned total and consumed extent. Four compiling contrasts detect
lost totals, discarded make-up values, premature make-up termination and
empty zero-run acceptance. Numeric tests exercise 87,040 colour/alignment/
make-up/terminal scenarios with exact and spare row remainders, every nonzero
scenario's first overshoot, and eight exact missing-terminal EOF cases.
They share implementation tables, not independently translated normative data.
Only `T6-RUN-MAKEUP-SUM` gains functional strength: all 266 records remain
provisional, now 113 functional / 127 state / 26 safety. Full acceptance/typed
errors, horizontal-pair/row/raster/caller composition and table review remain
separate. All native code, caller preconditions and trust boundaries are
unchanged; this theorem does not close the complete-scope gate.

`--require-complete` needs both verifier evidence and test discovery and currently
retains the full reviewed-functional, gap-free requirement. The additional
`--require-reviewed-decomposition` also requires both inputs, but checks only
the reviewed, exhaustive-enumeration declaration. Neither flag passes for
the actual provisional ledger today. Known semantic gaps may be accounted
for by a fully reviewed P04 map, but cannot satisfy the full flag or close
the corresponding P05 claims.

The current complete-scope gate still
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

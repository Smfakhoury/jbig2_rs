# Provisional T.88/T.6 statement traceability

The [requirements ledger](../validation/traceability/requirements.json) maps
selected requirements to exact runtime contracts, models, proof support and
executable tests. **P04 is incomplete.** All records are provisional; the
selected denominator is not exhaustive and no feature has an approved complete
normative decomposition. See [publication readiness](publication-readiness.md)
for revision-qualified counts and accepted runs.

## Authorities and scope

- [`config/support-matrix.json`](../config/support-matrix.json) defines required,
  unsupported and deferred profile behavior. A coarse feature worklist is not
  an enumeration of every normative obligation.
- [`validation/source-provenance.json`](../validation/source-provenance.json)
  identifies approved sources and hashes: T.88 (08/2018), edition 2.0 /
  ISO/IEC 14492:2019, and the distinct T.6 (1988) reference for MMR.
  T.6 has no separately recorded edition: `null` is intentional, not a copied
  T.88 edition. Document identities and permissions are not interchangeable.
- [`verification/production-proof-coverage.json`](../verification/production-proof-coverage.json)
  accounts for qualified runtime identities. It is not a normative percentage.
- The ledger records stable IDs, exact clause/figure/table/step locators,
  original paraphrases, applicability, assumptions, gaps, contracts,
  models/proofs, exact tests, review state and source bindings.
- Reports under `target/traceability/` are generated views, not another
  independently maintained requirements database.

Project resource limits and typed Rust error policies are explicitly separate
from normative rules; a policy row does not increase normative coverage.
The first milestone is broad bi-level decoding, not colour or encoding, and
does not claim full Annex F profile 1 conformance. Standards prose, diagrams,
table contents, sample software and official vectors are not reproduced.

## What an evidence label means

| Evidence | Establishes | Does not establish |
| --- | --- | --- |
| Safety/bounds | Valid storage, ranges, checked arithmetic under stated preconditions | Intended decoded value or complete rejection behavior |
| State preservation | Selected invariants/frames across an operation | The required transformation merely because state stays valid |
| Termination | A local decreasing measure, conditional on callees terminating | Aggregate CPU/work bound or conformance |
| Functional project contract | Exact relation between input/state and the specified result | Independent fidelity of that model to the standard |
| Runtime test | Assertions on executed first-party/approved cases | Universal proof, exhaustive requirements or normative approval |
| Strong/weaker contrast | A compiling counterexample is rejected by a selected strong proof but accepted with only the selected guarantee removed | Correctness of the standard translation or fresh whole-crate proof |
| Reference/hash audit | Bound definitions, contracts, test names and source identities | Semantic truth, test adequacy or execution |

Functional evidence must come from actual production `ensures` and proved
support, not a model's name, a precondition, `true`, or an unrelated same-named
function. A function may verify storage safety while a selected requirement
still lacks output semantics. Never promote a record solely because all its
functions appear in a successful Verus report.

## Current model surface and its limits

The [auditor index](verus-auditor-index.md) gives parameter/notation bridges and
source entry points. The following summarizes consequential distinctions rather
than a chronology of contract changes.

### Source-only purpose inventory

The ledger's `model_classifications` assigns exact declaration families one of
`project-result-model`, `project-policy`, `representation-invariant`,
`proof-support`, `candidate-standard-facing-model` or `classification-pending`.
The bounded inventory covers registered definitions plus every spec function
in `specs/` and `proofs/model_support/`; it does not claim every unregistered
proof/view declaration. Original roles and requirement links remain separate.
Body/dependency justifications describe primary project purpose, not proof
strength, native caller coverage or normative fidelity.

`ambiguity` is reserved for **unresolved source purpose**. Any nonblank ambiguity
appears in `unresolved_declarations` and makes `classification_complete` false,
even with a non-pending category. Pending entries must explain their ambiguity.
The optional `caveats` field preserves partial-output, caller, trust and
unreviewed normative limitations without turning those into purpose gaps.
Neither field is inferred from declaration names or free-text keyword matching.

All 417 declarations in this bounded source inventory have authored purposes:
147 project results, 53 representation invariants, 142 proof-support declarations,
29 project policies and 46 candidate standard-facing models. No source-purpose
ambiguities remain. These counts are not normative coverage percentages or
approval: all 266 requirement reviews remain provisional. For example,
`decoded_pixels` merely truncates supplied decisions, and abstract `page_step`
must not be equated to the narrower production stripe API. Their purposes can
be established while their output/caller/normative gaps remain open.

The inventory validates exact membership and conservatively binds all project
Rust plus the unchanged definition registry. New, removed, duplicated,
ambiguous or stale declarations/bindings fail explicitly; there is no automatic
purpose inference or source-binding refresh. Reports always keep
`normative_approval` false. Candidate status grants neither independent source
approval, source-processing authorization, human assignment nor release
readiness. Capacity/trust, retention and protected fixture holds are unchanged.

### Arithmetic, integer and IAID

Project contracts cover initialization, BYTEIN, normalization and complete
decision register/context state, including partial errors and unchanged input.
`entry_*` models use the runtime Qe arrays: consistency with implementation data
is **not** independent Table E.1 fidelity.

`integer_step_state`, `integer_selection_state`, `integer_bits_run` and
`integer_state` compose sign/selection/payload/history/value or OOB.
`iaid_run` and `iaid_state` compose the prefix/value fold, decoder and full
context bank. Concrete codec callers preserve selected and unselected banks,
refinement/work state and exact project error fields. Reset states every
selected context index and MPS becomes zero; validity alone would be weaker.

Independent register/pointer/table interpretation, procedure-bank placement,
Table A.1 representations, caller field selection, SBSYMCODELEN/address-space
sufficiency and complete stream composition remain open. The IAID 31-bit cap
is project policy, not an invented Annex A limit.

### Readers, bitmap and raster primitives

Byte/bit reader contracts preserve exact input, cursor, value and project
failure payload/priority. Bit accumulation lemmas do not remove native checked
operations or establish normative rejection rules.

Packed row/prefix copying uses the same native safe slice path in runtime and
proof builds. Contracts describe complete copied contents and untouched
frames; blank/fill/padding results describe stored bytes, including partial-byte
padding. Black spans have exact bit/pixel/frame results; white spans are no-ops,
not claims that arbitrary old white pixels were already zero. Byte/word
operators relate every result bit to the Boolean combination model.

These are helper semantics. SLTP timing, first-row caller edges, clipping,
placement, masks and full generic/refinement/page composition need their own
proof and independent normative review. Standard slice-copy contracts and the
compiler/library remain trusted boundaries.

### MMR/T.6

Mode/run lookup binds consumed old-input bits and length to the implementation
tables. `complete_run` additionally states actual prefix, bit extent,
make-up/terminating structure and sum; `horizontal_pair` composes opposite
colour runs. Reference search models expose changing-element parity,
initial-position equality and width-sentinel fallback.

`row_cache_matches` connects actual previous-row pixels to transitions.
Finite `RasterState`/`RasterStep`/`RasterRow` result histories connect input,
mode/run/reference/cache state and completed visible rows; they are observable
result semantics, not just accumulator invariants. Required framing describes
the immediate EOFB and zero alignment bits.

Independent table/coordinate fidelity, complete valid-input acceptance and
errors, T.88 omission/known-extent/fixed-block padding applicability and broader
caller/resource composition remain open. Names such as
`mode_code_in_implementation_table` deliberately avoid claiming normative
table certification. Tests sharing implementation arrays are not independent
normative oracles.

### Organization, metadata and lifecycle

Sequential relations connect adjacent original header/payload windows and
complete body extent. Standalone results preserve file-header organization,
body origin, increasing numbers and declared/page-information cardinality;
embedded results preserve supplied order without importing standalone ordering.
Random-access relations connect separate header and payload sections,
deterministic offsets, increasing numbers and a final EOF header.

Typed metadata relates accepted page dimensions, height encoding, resolutions,
flags and ordered profile IDs to original bytes and returned variants.
Opaque table/entropy `Other` payloads are not codec-validity theorems.
Unknown-length termination, complete acceptance/errors/resources, full
page/stripe/profile interpretation and lifecycle/decoded output remain separate.

Retention uses forward wire-byte order, least-significant bit first within each
byte, and unused bits in the final long-form byte under the recorded 7.2.4
interpretation. The conflicting 7.2.8 worked-example fixture remains an explicit
strict rejection, not a dual-order heuristic or an approved normative exception.
Owner adjudication and full retention/caller semantics remain open.

### Huffman, symbol/text and halftone

Selected-line value/OOB and implementation-table membership do not prove
complete canonical assignment, prefix selection, custom-table range partition
or independent predefined-table translation.

Symbol lookup/export content relations bind selected geometry, stride and
packed bytes, including ordered/repeated supplied selections. They do not
identify encoded export-run flags, physical capacity, complete dictionary
refinement/aggregation or text placement. Pattern/halftone header models bind
raw fields and exact parser extent, not full grid/raster/caller interpretation.
Protected encoded-family fixture expansion remains held.

## Checker contract

`tools/check_traceability.py` validates:

1. Schema, unique IDs, exact support-feature membership, standards editions/
   hashes, consultation identities and separate policy accounting.
2. Bidirectional feature worklists and nonblank remaining scope for partial
   features; excluded/deferred features cannot hide required behavior.
3. Hashes of referenced source and module-declaration files. Changes invalidate
   review; there is no automatic reapproval.
4. Runtime IDs against syntax-level inventory and proof accounting.
   Unverified/divergent bodies cannot support a direct functional claim.
5. Actual module namespaces and qualified model/proof symbols; supplied
   verifier reports must contain those exact identities.
6. Proof-macro definitions and lexical calls from the recorded runtime body.
7. Contract evidence in the declared `requires`/`ensures` section of actual
   ghost-conditioned attributes. It rejects duplicate/aliased evidence,
   whitespace-equivalent duplicates, commented-out clauses, vacuous replacements,
   functional evidence mislabeled weaker and unused definitions.
8. Exact non-ignored ordinary-test source identities and, with
   `--discover-tests`, compiled Cargo target/name discovery.

Unresolved/ambiguous module paths and unsupported bindings fail closed instead
of being guessed. Reference checking does not infer complete call graphs, prove
test adequacy, execute tests or approve standard translations.

`decomposition_review_complete` / `--require-reviewed-decomposition` express
the narrower P04 map-review gate: a reviewed exhaustive map may explicitly
contain unproved transformations. `semantic_claims_complete`, `--require-complete`
and the legacy `normative_decomposition_complete` retain the stronger meaning:
functional evidence, approved review and no semantic gaps. Passing the former
must not silently close P05 or the latter.

## Reproduction

```sh
python3 -m pip install -r tools/requirements.txt
python3 -B tools/check_traceability.py --discover-tests \
  --output target/traceability/reference-report.json
python3 -B tools/check_traceability.py --verus-json target/verification/verus-production.json \
  --discover-tests --output target/traceability/combined-report.json
```

A supplied report must come from the pinned whole verifier on the same source;
loading a successful JSON file is not a fresh proof. CI separately records proof
checking, test discovery and test execution.

For an already approved local document, explicit identity can also be checked:

```sh
python3 -B tools/check_traceability.py --discover-tests \
  --standard-document 'ITU-T T.6 (1988)=/path/to/approved-t6.pdf' \
  --output target/traceability/t6-report.json
```

Repeat `--standard-document STANDARD_ID=PATH` for available documents.
Legacy `--standard-pdf` means the first declared standard, not inferred identity.
Unknown/duplicate selections and cross-standard substitutions fail. Per-standard
hash checks are separate; the aggregate flag is true only when every declared
document was checked in that invocation. CI absence is explicit; provenance
checking does not claim to reacquire documents or grant redistribution.

The required semantic groups execute in same-source shards with selected
positive baselines and separately executed compiling strong/weaker controls.
The collector rejects missing, stale, duplicate, misrouted or wrong-tool/run/
attempt evidence; no partial report replaces whole proof and compilation.
See [limitations](verifier-limitations.md) and the [snapshot](publication-readiness.md).

# Verifier limitation roadmap

Pinned Verus verifies 1136 obligations with zero errors and zero native
assumptions in its proof build. The syntax-audited production inventory contains
606 directly verified functions, 24 reviewed verifier limitations, 23 audited
forwarders and 3 explicitly reviewed runtime/proof divergences.

The readiness audit corrected earlier same-name proof matching and exclusion
of production code after test-only items. The old count of 618 was not reliable
evidence of exact-source coverage. Reusable-context paths and their real
scratch handling are now directly verified; a mutable closure was extracted
into an ordinary helper with the same success/error cleanup behavior.

The remaining divergences include bitmap composition dispatch, MMR
capacity-based memory accounting, and batched text placement.
The five primitive value/operator bridges now share their verified executing
bodies. Even where an optimized helper separately
verifies, a different ghost wrapper is not an exact-source proof of the executing
wrapper. See the individual IDs and reasons in `proof-dispositions.json`.
They must be closed in P05, not relabeled as verified.

MMR span dispatch now executes and verifies the same optimized bitmap filler;
the scalar proof-only loop is removed. The filler additionally preserves
stored length and proves successful empty spans are no-ops. The wrapper proves
storage/metadata preservation, exact invalid-bound errors and unchanged bytes
for invalid, white and empty spans. Three compiling mutations violate those
no-op guarantees and fail the stronger proof while storage/error-only controls
verify. Exhaustive bounded pixel/storage oracle tests check black spans, but
this tranche does not formally establish their pixel relation, complete MMR
decoding or T.88 conformance. The changed obligation count reflects removal of
the obsolete proof path, not a completeness metric.

Source-offset region composition now also verifies the executing chunked
dispatcher rather than a scalar proof substitute. Its existing storage,
metadata and stored-length guarantees are preserved, with exact invalid-request
errors and unchanged bytes for invalid or zero-area requests. Valid zero-area
requests succeed. The optimized kernel's runtime tokens are unchanged; the
wrapper only names and returns its call result around proof-only field reveals.
Three compiling wrong-result mutations fail the no-op contract while controls
retaining storage/error guarantees verify. Existing scalar-oracle tests cover
composition pixels, which are not formally derived by these frame contracts.
The deleted scalar substitute accounts for the lower obligation count.

`--require-complete` checks that every function is accounted for and reviewed;
it does not imply that every function is directly proved. The release-level
`--require-exact` additionally rejects the runtime/proof divergence backlog.
It currently fails, as intended, and `proof-attestation.json` blocks release.

The inventory scanner follows syntax-level cfg/module scopes. Unknown platform
conditions are conservatively included; unsupported conditional exclusions,
unresolved/ambiguous modules and macro-generated/local function declarations
fail closed rather than disappearing. Pinned tools are in `tools/requirements.txt`
and are not runtime crate dependencies.

## Historical development notes

The notes below describe proof development and kernel contracts, not a blanket
claim that the executing decoder's every algorithm or T.88 output rule is proved.
Generic bitmap row copying now has directly verified row-span bounds, metadata
preservation, and byte-wise mutation safety.
Immediate page composition, stripe sealing, and auxiliary-buffer policy are
verified as concrete page-state kernels. Stripe sealing and auxiliary lookup
also verify through the deterministic registry; immediate composition still
requires a registry-wide bitmap invariant.
The runtime page registry now uses deterministic vector storage with verified
index lookup, immutable/mutable access, insertion/replacement, stripe sealing,
auxiliary-page lookup, and immediate composition under a registry-wide bitmap
invariant. Other resource stores remain pending.
The intermediate-region buffer registry now also uses deterministic vector
storage with verified index and immutable lookup; insertion/removal retain
their explicit fallible/retention accounting boundaries.
The Huffman-table registry uses deterministic vector storage with verified
index lookup, immutable access, and duplicate-checked insertion.
The pattern-dictionary registry uses deterministic vector storage with
verified index lookup, immutable access, and duplicate-checked insertion.
The symbol-dictionary registry uses deterministic vector storage with verified
index lookup, immutable access, and duplicate-checked insertion while retaining
exported bitmap order and optional arithmetic contexts.
Document segment indexes, global type/depth metadata, page lifecycle state,
profile tracking, and retention last-reference state now also use deterministic
vector storage with verified generic keyed lookup/get/mutable-get/insertion.
Production runtime code no longer uses `HashMap`.
Segment reference-type validation now uses explicit verified classification
loops over deterministic local/global type stores.
Retention-flag policy validation is separated from scratch allocation and uses
directly verified bounded loops over last-reference metadata.
Reference-depth computation is separated from ordered/depth vector allocation
and directly verifies local/global depth resolution, overflow, duplicate, and
configured-depth checks.
Page-profile uniqueness and global-profile compatibility are separated from
scratch allocation and directly verified with bounded membership scans.
Page-state collection is separated from vector reservation and directly
verifies page association, first-information, page-count, stripe, duplicate,
and end-of-page lifecycle rules.
Total-reference accounting and local/global reference-link resolution are
separated from index allocation and directly verify missing targets and
cross-page association rules.
Finalized public page-number continuity is separated from output allocation and
sorting and directly verified.
Retention-bit expansion and referred-segment reading are separated from vector
reservation and directly verified as fail-closed bounded materialization loops.
Profile-list entry decoding is separated from vector reservation and directly
verified through the bounded byte reader.
Fallible allocation now uses pinned-vstd's specified `Vec::try_reserve`
primitive. Bitmap construction, profiles, segment references, context
snapshots, MMR transition storage, document graph materialization, and several
symbol/halftone raster materializers verify without project assumptions.
The complete MMR bitmap, bounded-block, and concatenated-prefix entry points
now verify through bitmap layout, reference-transition, work, and reader
invariants.
Verus `0.2026.10.04.426d8b0` removes the former composed-parser compiler
panic. Indexed adaptive-template parsing now directly verifies both generic
and refinement region headers.
Generic and refinement arithmetic context assembly now uses verifier-visible
constant tables and bounded indexed selection instead of slice coercions.
Generic and refinement probability decoding now uses concrete decoder/context
state, and their raster loops, reusable sessions, known/unknown framing, and
complete segment dispatch paths verify end-to-end.
Verified performance kernels include incremental and monotone MMR transition
tracking, safe chunked bitmap composition, specialized arithmetic context
assembly, Huffman prefix lookup, borrowed symbol state, batched text placement,
and safe auto-vectorized gray-plane operations.
Nominal template-0 arithmetic generic regions additionally use directly
verified rolling word rows with explicit peak-memory accounting and immediate
flushes to the public packed-byte bitmap. Compatible pristine page regions
decode directly into the destination page.
Huffman construction, symbol dictionaries, text regions, patterns, halftones,
resource retention, page lifecycle, and the concrete high-level decode
orchestration now verify end-to-end. Remaining reviewed items are limited to
formatting/error traits, `std::io::Read`, and generic caller-provided page-sink
callbacks in addition to the explicit runtime/proof divergence backlog above.
Statement-level functional conformance is a separate P04/P05 review.

## Selected semantic contract closures

The P04 ledger found that batch context reset exported validity but not the
zeroed output required by the selected E.3.7 transformation. Its P05 contract
now quantifies zero state index and MPS over the complete selected slice,
preserves slice length, and requires no valid incoming context values. The
loop's runtime statements are unchanged. Two valid nonzero counterexamples
compile normally, are rejected by the stronger selected proof, and verify under
the validity-only control. This closes that helper-level semantic gap, not the
remaining arithmetic-decision relation, reset call-site timing, or any of the
remaining runtime/proof divergences.

Document count agreement now relates accepted metadata to both the declared
count and the cardinality of positive-page-associated PageInformation records
in the parsed input. Collection maintains prefix cardinality from empty
vectors, finalization preserves order length, and metadata field views expose
the result. An equivalent pattern-matching guard replaces an enum `PartialEq`
comparison whose generated contract did not establish structural equality.
Five compiling semantic mutants fail the strong selected proofs and pass
weaker controls. The standalone and embedded parsers now export those
cardinality guarantees against their returned segment sequences. All six
slice-based `Decoder`/`DecoderContext` inspection methods expose successful
metadata shape; standalone results preserve header-byte organization/count
and known-count agreement. Existing runtime statements are unchanged.
Body-byte parser correspondence, page identity/lifecycle semantics, decoded
output contracts, rejection completeness and normative review remain open.
Reader I/O remains a reviewed boundary rather than a newly verified body.

The generic typical-row copying wrapper now calls the same native slice-copy
path in normal and proof builds. The obsolete ghost scalar copier and manual
word-copy loop are removed. The new contract copies the old source row,
preserves all other bytes and metadata, and explicitly proves corresponding
visible pixels equal. Disjointness/bounds proofs justify safe
`split_at_mut`/`copy_from_slice` use under the pinned vstd contracts; no new
project assumption or proof-only algorithm is introduced. Two compiling
semantic mutants distinguish byte contents/frame preservation from storage
safety alone. Fewer obligations reflect removed loops/duplicate proof code,
not weaker production accounting. Caller SLTP timing/loop semantics, the
first-row zero edge and normative review remain open.

Bitmap prefix copying now also executes the same verified packed-byte helper,
with both the ghost scalar and manual chunked prefix helpers deleted. On
success, every prefix byte equals the source and the destination suffix is
unchanged. Out-of-range counts return the exact overflow error without mutating
the destination. This is an implementation contract, not a newly reviewed
standalone T.88 requirement.

Blank bitmap construction and false-filled construction now prove zeroed
storage. Height padding propagates the exact decoded prefix and zeroes all
added stored bytes. These statements are proved, not inferred from storage
validity, and are tested with nonzero input, differing dimensions, zero sizes,
partial rows, limit errors and invalid counts. The generalized bitmap proof
sensitivity harness checks six compiled counterexamples with weaker controls.
Zero initialization remains a foundation for, not a completed proof of,
first-row caller/SLTP behavior.

Segment retention now uses forward wire-byte order and validates unused bits
in the last flag byte, following the explicit 7.2.4 field definitions.
Flag construction exports exact length and the complete supplied-byte relation;
the materializer's verified prefix invariant connects each appended flag to
the model. The previous reverse traversal and model have been corrected
together. Three compiled semantic controls distinguish those guarantees from
bounds/length preservation alone.

The 7.2.8 worked example conflicts with these field definitions. Its original
fixture is preserved as a documented strict rejection under this interpretation,
not silently rewritten or accepted through an ambiguous fallback. Normative
owner adjudication, raw parser correspondence and full retention lifecycle
proofs remain open; no full conformance claim is added.

The Annex A integer and IAID callback limitation is closed: production now
uses concrete decoder-state methods with verified context, cursor, and
probability-state preservation contracts.
Default arithmetic-context construction is also directly verified.
The inventory excludes item-level `#[cfg(test)]` helpers as well as test
modules, so reviewed counts now describe executable production code only.
Pure halftone adaptive-pixel, header/dimension validation, bitmap-size,
resource-limit, and pixel-offset helpers are directly verified; remaining
halftone items require callbacks, allocation, bitmap mutation, or composition.
Generic/refinement context counts and layouts, bitmap-size arithmetic,
work accounting, and input/resource limit guards are directly verified.
Signed bitmap/reference sampling, observed-marker validation, retained-state
and managed-memory guards, and EOF construction are directly verified.
Generic/refinement context reset and arithmetic-session marker/offset accessors
are directly verified.
Adaptive-pixel validation and bounded arithmetic-terminator scanning use
explicit verified loops with checked indexing and termination measures.
Signed adaptive offsets are decoded through an explicit verified two's-
complement mapping with reader-position preservation.
Retained generic/refinement probability states are validated and restored with
explicit verified loops; snapshot allocation remains fallible and tracked.
Symbol/text integer-context slot selection and owned/borrowed Huffman table
dispatch are directly verified.
Text-region header parsing is directly verified through the shared bounded
region-reader contract.
Decode-side page metadata lookup and structural-work accounting use explicit
verified scans over modeled parsed segments.
Document stream, segment, page, input-work, and aggregate structural-work
resource guards are directly verified.
Striped-page state progression, page completion, and public metadata
finalization are directly verified.
Intermediate-region classification and segment page-association rules are
directly verified.
Unknown-length generic framing discovery, explicit big-endian row-count reads,
and runtime page-write stripe tracking are directly verified.
Generic and refinement header data types and their composed header parsers are
directly verified. The former `rustc_middle::ty::generic_args` crash is resolved
by the updated pinned toolchain and indexed parser-state loops.

Audited forwarders are limited to exact allowlisted standard conversion/default
wrappers around verified typed constructors/conversions and the concrete
`PageCollector` sink wrapper around its verified collection method. Generic
callback wrappers are excluded. Forwarders are not native assumptions, and any
source-file change invalidates their disposition.
Known- and unknown-length segment payload reads now compose the verified
bounded reader atomically; the obsolete standalone cursor-advance helper was
removed.
Halftone peak-memory accounting over retained patterns, gray planes, skips,
values, and output storage is directly verified.

The limitations are not algorithm stubs. They are complete, tested production
functions whose direct proofs are blocked by unsupported pinned-Verus/vstd
surfaces: fallible `Vec` allocation, `HashMap`, `RefCell`, trait callbacks,
slice coercions, formatting, and `std::io`.

## Mitigation order

1. **Fallible allocation foundation**
   - Completed for bitmap, reference/profile, context, document, symbol-copy,
     and MMR allocation paths with pinned-vstd's `Vec::try_reserve` contract.
   - Remaining Huffman and orchestration items are blocked by construction,
     callback, or slice-coercion semantics rather than raw reservation.
2. **Deterministic verified stores**
   - Replace proof-facing `HashMap` operations with sorted/indexed vectors.
   - Separate `Read` acquisition from verified slice parsing.
3. **Concrete entropy and raster state**
   - Replace `RefCell` and dynamic callbacks with concrete decoder-state enums
     and methods.
   - Reuse the verified arithmetic, Huffman, MMR, and bitmap kernels.
4. **End-to-end raster proofs**
   - Close generic/refinement and halftone entry points after bitmap allocation
     is verified.
5. **Dictionary/text proofs**
   - Close continuous arithmetic, refinement aggregation, Huffman alignment,
     placement, and composition end-to-end.
6. **Non-algorithm boundaries**
   - Keep formatting and platform I/O outside codec correctness claims, or add
     narrowly reviewed native contracts when vstd support exists.

## Governance

- No disposition may be removed without a passing pinned-Verus run.
- Any body change invalidates its disposition hash and fails CI.
- New native contracts require architecture review, a named trust-boundary
  entry, and an updated proof attestation.
- `external_body`, `assume`, and `admit` remain prohibited for codec
  algorithms.
- Progress is measured by reducing `reviewed-verifier-limitation` while keeping
  `incomplete` and native assumptions at zero.

The machine-readable workstreams, counts, priorities, and exit criteria are in
`proof-limitations-plan.json`.

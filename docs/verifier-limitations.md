# Verifier limitation roadmap

Pinned Verus verifies 1165 obligations with zero errors and zero native
assumptions in its proof build. The syntax-audited production inventory contains
608 directly verified functions, 24 reviewed verifier limitations, 23 audited
forwarders and 1 explicitly reviewed runtime/proof divergence.

The readiness audit corrected earlier same-name proof matching and exclusion
of production code after test-only items. The old count of 618 was not reliable
evidence of exact-source coverage. Reusable-context paths and their real
scratch handling are now directly verified; a mutable closure was extracted
into an ordinary helper with the same success/error cleanup behavior.

The remaining divergence is MMR capacity-based memory accounting.
Including its executing blocks is rejected by the pinned verifier
on `Vec::capacity` and `Result::unwrap_or`. This is a standard-library
specification prerequisite, not a reason to count vector length as capacity,
add project assumptions or declare the source gap closed. Its runtime body
and accounting policy remain unchanged.
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
verify. The later primitive algebra closure described below establishes the
black-span pixel relation, not complete MMR decoding or T.88 conformance.
The changed obligation count in the dispatch tranche reflected removal of
the obsolete proof path, not a completeness metric.

`Bitmap::fill_black_span` now proves valid-storage success and an exact relation
for every bit of every stored byte: requested row bits become one; every other
bit is preserved, including other rows, unused bits in boundary bytes and
arbitrary pre-existing padding. Derived postconditions expose all visible
pixels and exact unchanged bytes outside the affected byte interval. Existing
bounds preconditions and unconditional metadata/length/empty-span guarantees
are retained; storage validity is a condition on the new guarantees, not a
stronger precondition.

Five fully proved helpers establish partial-byte masks, aligned byte/word
steps, byte extensionality and the storage-to-visible-pixel bridge. The
executing first-mask/word/byte/tail algorithm and its native tokens are
unchanged. Six compiling output mutations distinguish requested head/word
bits, untouched boundary/prefix bits, dirty padding and valid-span success
from weaker controls retaining all prior guarantees. The mandatory semantic
runner includes these contrasts. Independent scalar storage oracles now also
exercise arbitrary dirty padding. No MMR caller/polarity row is upgraded:
the wrapper's weaker storage predicate and complete raster-loop composition
remain separate obligations, as do general bitmap composition pixel
semantics and the unchanged capacity-accounting source gap. Primitive
byte/word operator semantics are proved independently below.

Before the black-span source-binding refresh, all 656 production declarations were
compared with base `d1bf46914c6d7e57177b7c02087cca2555794b52`: only
`Bitmap::fill_black_span` changes, with identical erased native tokens; the
other 655 bodies/attributes are byte-identical. All 27 existing bitmap model
definitions, 11 existing shared proof helpers and existing production proof
macros are unchanged. There are no bitmap dispositions to relabel; disposition
identities, reasons and statuses remain unchanged. The three existing bitmap
ledger file bindings are refreshed only after this review; no normative
strength/review classification changes.

Merging the later operator-bit proofs retains both sets of bitmap models,
helpers and production contracts. The combined source verifies 1164 obligations;
all mandatory semantic checkers retain their complete configured contrasts. Bitmap source
bindings are refreshed for this combined source without relabeling dispositions
or upgrading normative evidence.

Arithmetic decision decoding additionally proves the successful returned bit
and exact next probability-context index/MPS against the project Qe-table
model, relative to the entry interval and code. Both the signal helper and its
checked wrapper expose this relation. The lower/upper exchange helpers also
prove their context update on error, preserving the distinction between their
supplied old MPS and the context's prior MPS. No precondition is strengthened.

Seven pure model functions and one proved equation bridge expose these
relations without changing existing model definitions or native tokens. All
656 production declarations were compared before refreshing source bindings:
only the four decision/exchange contracts and proof blocks change; the other
652 bodies/attributes are byte-identical. The reviewed arithmetic dispositions
retain their identities, statuses and reasons.

Four compiling mutations distinguish wrong successful decisions and wrong
error-path context updates from weaker controls retaining the prior guarantees.
Normal tests independently check all 47 probability rows, both MPS values,
boundary decision routes and context updates before truncation. The combined
whole-crate proof verifies 1165 obligations; thirteen mandatory semantic
checkers cover 54 contrasts. Exact renormalized registers, input consumption,
initialization/refill and faithful normative translation remain separate
obligations. The aggregate arithmetic ledger record therefore retains its
state-preservation classification and provisional review.

P05 CI scheduling separates whole-crate verification from an independently
bounded job per semantic group (at most four concurrent groups). The stable
`verify` collector fails unless whole verification and every current group
succeed with matching source, tool and run/attempt identities. The local
default remains all groups with two workers. This changes orchestration only:
whole-crate obligations, native behavior, project assumptions, the remaining
MMR capacity gap and provisional normative classifications are not upgraded.

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

Text-placement batch dispatch now verifies the same optimized row-batch call
as normal Rust; its proof-only repeated-region loop is removed. Existing
successful storage/metadata, batch clearing and batch-validity guarantees
remain, with explicit stored-length/stride preservation and empty/zero-height
success/no-op contracts. Four compiling mutations distinguish those guarantees
from weaker controls retaining storage/layout and the unaffected relation.
Both changed production declarations have identical erased runtime tokens;
all other 654 production bodies/attributes and all bitmap/text models are
unchanged. General pixel values and overlapping-placement order remain checked
by the retained scalar oracles, not formally derived here. One obsolete loop
obligation disappears; this is not a completeness metric.

Packed-byte reads and writes now export exact big-endian values, acceptance
of exactly the complete eight-byte ranges, unchanged length, and unchanged
bytes outside successful writes or throughout failed writes. Two pure chunk
models are added without changing existing bitmap models or executing tokens.
Four compiling value/frame mutations fail strong contracts and verify when
only the affected relation is omitted; range, length and other relations stay
in the controls. Independent standard-library endian oracles cover all 256
byte values in each of eight positions, plus complete/short/overflowing ranges.
These contracts are prerequisites for the ordinary composition proof, not a
closure of that dispatch substitution or its clipping/pixel relation.

Ordinary bitmap composition now verifies the optimized chunked path with the
original unconditional success, storage/metadata and outside-clipped-byte
frame contract. Its obsolete scalar proof substitute is removed. Four proved
arithmetic/frame lemmas establish complete row accesses and witnesses for
every byte that may change; shifted readers export sufficient availability
guarantees for the real word/byte/tail paths. Five compiling success/frame
mutations fail strong contracts and verify when only the affected relation is
removed. General operator/pixel equivalence is still scalar-oracle tested,
not newly derived by this byte-frame proof.

Two equivalent native syntax changes expose facts hidden behind the pinned
callback interface: the row's checked multiply and checked add are explicitly
sequenced instead of using `and_then`, preserving their order and exact error;
the wrapper names and returns its call result around proof-only field reveals.
The three shifted-reader runtime bodies are unchanged. All other 651
production bodies/attributes are byte-identical. Full proof verification
includes the added lemmas, not just assumed helper contracts.

Byte and word combination operators now additionally prove that every output
bit agrees with the existing Boolean combination model for all five operators,
all u8/u64 input pairs and all valid bit positions. Two proved bit-vector
lemmas connect pure byte/word definitions to that Boolean relation. Native
operator expressions are unchanged; pure result bindings expose their values
to erased proof code. Exhaustive byte-pair truth tables and boundary-word
pairings are independent runtime oracles. Four compiling constant-output
violations fail the per-bit contracts while the original `ensures true`
controls verify. Placement, shifts/masks, wire-value mapping and the normative
translation remain separate obligations; this is not full raster correctness.

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

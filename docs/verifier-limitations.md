# Verifier limitation roadmap

Pinned Verus verifies 1145 obligations with zero errors and zero native
assumptions in its proof build. The syntax-audited production inventory contains
597 directly verified functions, 24 reviewed verifier limitations, 23 audited
forwarders and 12 explicitly reviewed runtime/proof divergences.

The readiness audit corrected earlier same-name proof matching and exclusion
of production code after test-only items. The old count of 618 was not reliable
evidence of exact-source coverage. Reusable-context paths and their real
scratch handling are now directly verified; a mutable closure was extracted
into an ordinary helper with the same success/error cleanup behavior.

The remaining divergences include bitmap copy/composition dispatch, MMR span
filling and capacity-based memory accounting, batched text placement, and small
conversion/operator bridges. Even where an optimized helper separately
verifies, a different ghost wrapper is not an exact-source proof of the executing
wrapper. See the individual IDs and reasons in `proof-dispositions.json`.
They must be closed in P05, not relabeled as verified.

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

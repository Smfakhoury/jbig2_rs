# Verifier limitation roadmap

Pinned Verus verifies 629 obligations with zero errors and zero native
assumptions. The production inventory contains 326 directly verified functions
and 176 reviewed verifier limitations. Each limitation is bound to the exact
source-file hash in `proof-dispositions.json`.

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

The limitations are not algorithm stubs. They are complete, tested production
functions whose direct proofs are blocked by unsupported pinned-Verus/vstd
surfaces: fallible `Vec` allocation, `HashMap`, `RefCell`, trait callbacks,
slice coercions, formatting, and `std::io`.

## Mitigation order

1. **Fallible allocation foundation**
   - Upstream or locally audit a minimal `try_reserve_exact` contract.
   - Wrap allocation in one reusable verified abstraction.
   - Close bitmap constructors first, then reference/profile/Huffman/MMR
     allocations.
   - Current pinned-Verus probing confirms `Vec::try_reserve_exact` is
     unsupported and only offers `assume_specification`; that assumption is
     not permitted by the project proof policy.
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

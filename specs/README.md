# A short guide to auditing the JBIG2 models

These are **project models, not an independently approved transcription of
T.88/T.6**. Recent proofs and models were developed together. A proof that Rust
matches a model is useful, but a second review must establish that the model
expresses the intended standard. All selected requirement records remain
provisional; their number is not an exhaustive specification denominator.

## Start here: one question, about fifteen minutes

1. Pick one small question, such as BYTEIN's marker branch. Find
   `T88-ARITHMETIC-BYTEIN-BRANCH` in the
   [requirements ledger](../validation/traceability/requirements.json).
2. Obtain the exact standard through the official links below. Check its
   identity before trusting a page reference.
3. Read the row's clause, assumptions and gaps, then its linked model.
   The [auditor index](../docs/verus-auditor-index.md) translates the standard's
   notation to project parameters.
4. Compare the equation with the standard **before** using the implementation
   as an explanation of what the standard means.
5. Follow the row to the actual production contract and tests. Ask whether
   legitimate inputs are excluded, success is guaranteed where claimed, and
   expected values come from an independent source.
6. Record a discrepancy or a narrowly scoped conclusion. Do not approve a
   whole file because one equation is correct.

The [review procedure](../docs/specification-review.md) explains independent
review packets, snapshot checks and the approval gates that remain open.

## Obtain the right documents, not a convenient edition

| Source | Official acquisition | Pinned identity |
|---|---|---|
| T.88 (08/2018), edition 2.0 | [ITU](https://www.itu.int/rec/T-REC-T.88-201808-I/en) | [Source provenance](../validation/source-provenance.json) |
| T.6 (1988), MMR reference | [ITU](https://www.itu.int/rec/T-REC-T.6-198811-I/en) | [Source provenance](../validation/source-provenance.json); no separate edition is recorded |

Use the recorded document SHA-256, not just a filename or ZIP hash. Existing
ledger page references are inherited consultation locators; printed page
numbers and a viewer's PDF page index are not interchangeable. Do not invent
deep links to unverified pages.

The current policy prohibits redistribution and third-party uploads. Obtain
documents directly or through an approved licensed channel. Do not commit PDFs
or send them to a cloud agent merely because an official download is free.
Hash-checking a local document does not approve its interpretation or processing.

## What to open for each component

| Component | Model entry | Audit focus |
|---|---|---|
| Arithmetic, integers, IAID | [arithmetic](arithmetic.rs) | Registers, refill/initialization, decisions, context meaning and caller interpretation |
| MMR | [mmr](mmr.rs) | Tables, changing elements, coordinate boundaries, raster histories and framing |
| Headers and references | [segments](segments.rs), [file header](file_header.rs) | Wire fields, retention order and original input extents |
| Containers and pages | [document](document.rs) | Organization, metadata, accepted state transitions and their limits |
| Huffman | [huffman](huffman.rs) | Raw-line ordering/assignment versus complete parsing, prefix freedom and entropy decoding |
| Generic/refinement regions | [generic region](generic_region.rs) | Geometry/context interpretation versus representation and work bounds |
| Symbols and text | [symbol/text](symbol_text.rs), [decode](decode.rs) | Content/order, placement, caller state and composed output |
| Halftone | [halftone](halftone.rs) | Grids, headers, pattern placement and caller semantics |
| Reader/bitmap primitives | [readers](readers.rs), [bitmap](bitmap.rs) | Exact bit/pixel results, frames and the scope of helper contracts |

## Do not confuse these layers

- **Normative candidates** need an independent comparison with the standard.
- **Project result models** describe observable project behavior; that alone
  does not make the behavior normative.
- **Project policies** cover quotas and Rust errors, not invented ITU rules.
- **Representation invariants** establish storage/state conditions, not
  necessarily the required transformation.
- **Proof support** supplies access, accumulators and lemmas.

For example, arithmetic and MMR models refer to runtime tables. Membership or
consistency with those arrays is not independent table fidelity. Model theorems
and strong/weaker controls do not remove this review obligation.

The generated traceability report includes an authored, source-only purpose
inventory. The requirements ledger's `model_classifications` families own exact
members, justifications and unresolved ambiguity, separately from normative
requirement links. The current bounded authoring slice covers reader/bitmap
models and selected support; other declarations remain explicitly pending.
Original roles/locators and unregistered helper status are preserved. Lexical
implementation-constant references are dependencies to review, not normative
ground truth. Neither these navigation labels nor structural checks approve a
model. See the [inventory scope and freshness rules](../docs/specification-review.md#source-only-navigation-classification).

For exact source/trust limits read [verifier limitations](../docs/verifier-limitations.md).
For wider scope and evidence-strength distinctions read
[traceability](../docs/spec-traceability.md). The detailed navigation lives in
the [auditor index](../docs/verus-auditor-index.md), not in this introduction.

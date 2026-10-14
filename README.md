# jbig2_rs

`jbig2_rs` is a production-oriented, memory-safe Rust implementation of a
strict JBIG2 decoder. It is under active development and is not yet suitable
for decoding untrusted production documents.

The approved first milestone is decoder-only. Its exact feature and resource
contract is recorded in `support-matrix.json`. Publication is disabled, and
the project does not currently claim full JBIG2 or Annex F profile
conformance.

## Architecture

- `src/` is the sole executable implementation and Cargo source of truth.
- `specs/` contains functional models used by Verus.
- `proofs/shared/` contains abstract views and representation invariants.
- `proofs/spec_properties/` contains implementation-independent theorems.
- `proofs/production_impl/` contains proof bodies and proof macros attached to
  the exact implementation under `src/`.
- `validation/` contains conformance, differential, negative, metamorphic, and
  performance validation.
- `fuzz/` contains arbitrary-input and structured-input fuzz targets.

Ordinary Cargo builds and packages exclude all proof bodies and validation
assets. The pinned Verus workflow verifies `src/lib.rs`, the same crate root
that Cargo builds.

## Current status

The decoder implements:

- standalone sequential and random-access organizations;
- contiguous embedded/PDF page streams and reusable validated globals;
- known-height, unknown-height, multi-page, and striped pages;
- arithmetic coding, arithmetic integers, IAID, standard/custom Huffman, and
  MMR;
- generic and generic-refinement regions;
- symbol dictionaries and text regions, including Huffman/arithmetic variants
  and refinement aggregation;
- pattern dictionaries, Gray reconstruction, and halftone regions;
- immediate/intermediate region buffers, retention, clipping, and all
  combination operators;
- owned, bounded-reader, and page-sink APIs.

Publication and conformance claims remain blocked. CELA-approved private
validation has executed the official ITU vectors and two independent
black-box oracles; the resulting strict-rejection and oracle differences are
tracked under `validation/`. The production proof inventory is complete with
body-hash-bound reviewed verifier limitations. Crate licensing, native CI,
release-owner approval, and public-repository review remain open gates.

## Local checks

```text
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo doc --no-deps
cargo package
```

Do not use standards documents, sample software, or conformance assets unless
their entry in `source-provenance.json` records the required legal approval.

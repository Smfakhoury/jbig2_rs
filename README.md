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

The owner has reported CELA approval for open-source project publication, and
the repository includes the MIT license. Crates.io publication remains disabled.
Official ITU assets stay external under their separately recorded permissions.
Native Linux, Windows and macOS CI, Verus and fuzz smoke checks are operational.

The readiness audit found that earlier proof coverage overstated exact-source
verification: some same-named functions were conflated and several cfg-selected
runtime paths differed from proof paths. These are now explicit, hash-bound
limitations. The current inventory has 597 directly verified functions, 24
reviewed boundaries, 23 audited forwarders and 12 runtime/proof divergences.
The latter block a full exact-source proof claim; passing Verus does not alone
establish normative conformance. See [verifier limitations](docs/verifier-limitations.md).

Approved local validation has executed official ITU vectors and independent
black-box oracles. The reviewed rejections and oracle differences remain under
`validation/` and require normative re-evaluation before stronger claims.
The [readiness plan](docs/open-source-readiness-plan.md) tracks the remaining work.

## Local checks

```text
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo doc --no-deps
cargo package
```

Audit tooling additionally requires the pinned development-only dependencies:

```text
python -m pip install -r tools/requirements.txt
python -B -m unittest discover -s tools/tests -v
```

Do not use standards documents, sample software, or conformance assets unless
their entry in `source-provenance.json` records the required legal approval.

## Runtime coverage

The pinned Linux coverage workflow measures ordinary runtime code, excluding
inline tests/oracles and ghost items. Its initial baseline is **75.74% lines,
70.70% regions and 63.96% branch outcomes**. CI checks aggregate/per-file
non-regression; the proposed 90% line / 85% branch release goals remain unmet.
See [runtime coverage](docs/runtime-coverage.md) for reproduction, scope,
uncovered-code artifacts and the remaining P06 test-improvement work.

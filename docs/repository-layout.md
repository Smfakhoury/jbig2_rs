# Repository layout and evidence roles

Keep source, policy and reviewed expectations in Git. Generate routine reports
under ignored `target/` directories and upload them as CI artifacts; do not
commit a new chronological progress log or refresh historical reports to look
current. Every source-bound record speaks only for its recorded revision.

## Runtime and proof source

| Path | Role |
| --- | --- |
| `src/`, `Cargo.toml`, `Cargo.lock` | Sole executable decoder, runtime package policy and lockfile |
| `tests/`, `examples/` | First-party public-API regressions, fixture/validation entry points; not a second decoder |
| `specs/` | Candidate functional models and observable result relations |
| `proofs/shared/` | Abstract views, representation invariants and shared lemmas |
| `proofs/model_support/` | Proved model-access and implementation accumulator support, not additional normative rules |
| `proofs/production_impl/` | Ghost bridges/proof bodies attached to the actual runtime |
| `proofs/spec_properties/` | Implementation-independent properties of project models |
| `verification/Cargo.toml`, `verification/Cargo.lock` | Isolated pinned proof dependency graph targeting `src/lib.rs`; not runtime dependencies |
| `tools/`, `tools/tests/`, `tools/requirements.txt` | Audits, collectors, generators and their tests/development dependencies |
| `fuzz/`, `benchmarks/` | Bounded fuzz targets and generated performance/optional comparison harnesses |

The Cargo include list is the distribution authority, checked against a real
archive. Separate proof, validation, fuzz, tool and benchmark assets are not
packaged. Inactive inline proof annotations remain in runtime source; see
[distribution](distribution.md).

### Read the implementation by responsibility

The top-level component files are facades, not concatenated source fragments.
They retain the established types and entry points while ordinary Rust child
modules hold focused implementation responsibilities.

| Start here | Focused implementation |
|---|---|
| [`src/bitmap.rs`](../src/bitmap.rs) | [`bitmap/`](../src/bitmap): storage, access, operators, chunks, composition and region updates |
| [`src/document.rs`](../src/document.rs) | [`document/`](../src/document): bounded input, parsing, references and page metadata |
| [`src/decode.rs`](../src/decode.rs) | [`decode/`](../src/decode): public operations, resources, dispatch, orchestration and rendering |
| [`src/symbol_text.rs`](../src/symbol_text.rs) | [`symbol_text/`](../src/symbol_text): codecs, headers, tables, dictionaries, placement, text and bitmap support |
| [`src/generic_region.rs`](../src/generic_region.rs) | [`generic_region/`](../src/generic_region): contexts, headers, generic/refinement engines and segment wiring |
| [`src/halftone.rs`](../src/halftone.rs) | [`halftone/`](../src/halftone): headers, resources, planes and rendering |

Unit tests live in each component's `tests.rs` rather than interrupting its
implementation. They remain packaged and keep their logical Cargo test names.
Their file-local `cfg(test)` guards also let standalone source/coverage audits
identify them as test-only; the narrow duplicate-attribute lint allowances are
intentional. Internal fuzz/validation helpers retain their existing cfgs and
are not moved into test-only modules.

Detailed proof reasoning stays in the corresponding proof files. Short
contracts, loop invariants, local snapshots and calls to those helpers remain
attached to the actual Rust operations. This is one implementation, not a
generated proof-free fork. Function/loop guarantees and mathematical models
are not weakened to make relocation verify.

Semantic and native mutation checks bind logical component source groups to
the actual module graph. Their temporary edits are written back to individual
physical files; the combined inspection text is never compiled as a substitute
implementation. Ordinary function identities and physical verifier-selection
scopes are checked separately when an impl lives below its owning type.

## Policy, baselines and source-bound records

| Retained path | Purpose and interpretation |
| --- | --- |
| [`config/toolchain.json`](../config/toolchain.json) | Tool/compiler pins and execution policy; runtime, development, proof and nightly lanes remain distinct |
| [`config/support-matrix.json`](../config/support-matrix.json) | Feature/profile, unsupported/deferred behavior and resource policy; not conformance certification |
| [`validation/source-provenance.json`](../validation/source-provenance.json) | Source/license/hash/approved-use identities for standards, fixtures and tools; acquisition and redistribution permissions are distinct |
| [`verification/proof-dispositions.json`](../verification/proof-dispositions.json) | Reviewed qualified production identities, boundary reasons and source hashes; generated proposals are not approval |
| [`verification/production-proof-coverage.json`](../verification/production-proof-coverage.json) | Checked production inventory/accounting baseline; not a normative coverage percentage |
| [`verification/proof-attestation.json`](../verification/proof-attestation.json) | Revision-bound proof result/limitations and release status; not a substitute for a fresh candidate run |
| [`validation/traceability/requirements.json`](../validation/traceability/requirements.json) | Selected requirement IDs, source locators, contracts/models/proofs/tests, strength, gaps and provisional review |
| [`validation/coverage/baseline.json`](../validation/coverage/baseline.json) | Reviewed aggregate/per-file runtime coverage ratchet; never updated automatically |
| [`validation/conformance/attestation.json`](../validation/conformance/attestation.json) | Revision-qualified conformance summary with honest unresolved/unsupported outcomes |
| [`validation/conformance/itu-t88-2018-manifest.json`](../validation/conformance/itu-t88-2018-manifest.json) | Approved official archive/file identities and exact expected case set; contains no permission to redistribute assets |
| `validation/conformance/expected-differences.json`, `seven-rejection-review.json` | Pinned official disagreements, typed-error/source identities and unresolved diagnoses; not rejection credit |
| `validation/differential/oracles.json`, `expected-differences.json` | Oracle versions/provenance and reviewed comparison expectations; external black-box tools stay outside the runtime |
| `validation/negative/cases.json`, `validation/metamorphic/cases.json` | Exact executable Cargo test catalogs, not tests by themselves |
| `validation/performance/scenarios.json` | Benchmark scenarios and policy expectations; timing availability must be explicit |
| [`validation/schemas/release-attestation.schema.json`](../validation/schemas/release-attestation.schema.json) | Structure for candidate evidence; a valid document is not release-owner approval |

The coverage baseline and performance scenarios remain live reviewed inputs.
Conformance/differential observations, expectations and provenance records are
retained for their validation roles. Consult each record's source and status;
passing a provenance/schema audit does not make it latest-main execution.

Superseded coverage tranches, implementation tracking and single-run
performance/comparison measurements are recoverable in Git history and retained
CI artifacts, not copied into a source archive. They are not live baselines,
contracts or current-evidence dependencies. Keep the harnesses, fixtures, tests
and actual policy inputs; generate fresh measurements under `target/`.

Obsolete smoke snapshots, superseded limitation plans, withdrawn-credit
candidates and old candidate-approval tracking are not current authorities.
Use the records above, [publication readiness](publication-readiness.md) and
[release gates](../RELEASING.md), not a copied archive of session notes.

## Generated evidence

- `target/verification/`: local whole-proof `verus-production.json` and
  regenerated `production-proof-coverage-ci.json`; canonical CI artifact
  filenames remain unchanged.
- `target/traceability/`: source/reference/test/workflow audits and semantic
  contrast reports.
- `target/coverage/`: runtime summary, raw LLVM JSON/LCOV/HTML, collection
  identities and native mutation results.
- `target/distribution/`: actual archive, toolchain and downstream-check reports.
- `target/fuzz-*`, `target/generated-corpus-validation/`: bounded campaign,
  replay, minimization and corpus provenance diagnostics.
- `target/performance-*`: paired base/head measurements and failure controls.

CI artifacts must retain source/test/tool identities, raw report/log hashes,
run/attempt identities and failure diagnostics. Missing or stale evidence
fails closed; previous success cannot stand in for current execution.
Normative documents, official vectors, private corpora and native oracle
binaries remain external unless their separately recorded permissions allow
the particular use and location.

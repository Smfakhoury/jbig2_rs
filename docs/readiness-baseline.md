# Readiness baseline and file disposition

Recorded 2026-10-19 for P01 of the [execution plan](open-source-readiness-plan.md).
Source baseline: `ec6064128d06dfbc3a404b800a718d303c8dad02` (133 tracked files).
This document records the current state, not release approval or a new
conformance claim. No file removal is authorized by this inventory.

## Decisions and unresolved approvals

- The owner reported CELA approval for open-source project publication on
  2026-10-19. This records that statement, not internal legal correspondence.
- MIT licensing was proposed separately at the baseline in
  [PR #6](https://github.com/microsoft/jbig2_rs/pull/6), addressing
  [issue #2](https://github.com/microsoft/jbig2_rs/issues/2), and has since merged.
  Package license metadata is still open for P03.
- No new ITU vector or standards-document redistribution approval was supplied.
  Keep those assets external under the existing approved internal-use policy.
- Crates.io remains disabled. Public-source readiness is not permission to
  publish a package or broaden supported-profile claims.
- Existing root attestations describe historical revisions; do not edit them
  into apparent attestations of the readiness branch. Generate fresh release
  evidence at P12 instead.

## Frozen baseline

| Evidence | Result and provenance |
| --- | --- |
| Runtime tests | Re-ran `cargo test --all-features`: 202 passed, 14 ignored microbenchmarks, 0 failures |
| Package contents | Re-ran `cargo package --locked --list`: 23 entries, proof/validation assets excluded |
| Normal dependencies | Re-ran `cargo tree --edges normal`: no runtime dependencies |
| Verus inventory | Replayed the existing successful exact-source Verus JSON through `--require-complete`; byte-identical to committed coverage: 618 verified, 18 reviewed limitations, 21 forwarders, 0 incomplete |
| Verus obligations | Existing pinned report: 1,136 verified, 0 errors; replaying the inventory is not a new solver run |
| Official vectors | Hardened runner re-executed the approved local archive: 10 cases, 2 exact outputs, 7 reviewed rejections, 1 unsupported colour case, 0 unresolved |
| Official assets | External archive SHA-256 `fe3de9056d94b20db3a6ab17032c621a7f05b6ecb1db84b4b58d0825fd6b7033`; exact per-file manifest remains authoritative |
| Native CI | Baseline main runs: [Cargo](https://github.com/microsoft/jbig2_rs/actions/runs/39328623320), [Verus](https://github.com/microsoft/jbig2_rs/actions/runs/39328623335), [fuzz](https://github.com/microsoft/jbig2_rs/actions/runs/39328623319) all succeeded |
| Benchmarks | Retain the input definitions, source identity and measurements in `validation/comparison/hayro-0.3.1-results.json`; no new performance claim or timing baseline is introduced |

Unmeasured: executable line/region/branch coverage. Unresolved: statement-level
proof completeness and the normative validity of the seven reviewed official
rejections. Neither function counts nor the classification baseline closes
those questions.

P02 subsequently found the old proof inventory was over-broad: same-named
functions could match unrelated evidence and cfg-selected proof bodies could
differ from runtime bodies. The table above is a historical baseline replay,
not proof that 618 executing functions were directly verified. The current
authoritative counts are in `proof-attestation.json`; historical candidate
attestations are marked superseded.

## Exact path inventory

Paths below are relative to the repository root at the baseline commit. Every
tracked path belongs to exactly one group; action and consumer descriptions
apply individually to each listed path. New readiness documents/tests are
listed separately at the end.

### Keep: executable runtime and its unit-test/reference code

Consumers: Cargo, Verus, public API, fuzz/validation adapters and proof
inventories. Do not remove test oracles as "nonproduction clutter."

```text
src/arithmetic.rs
src/bitmap.rs
src/decode.rs
src/document.rs
src/error.rs
src/file_header.rs
src/generic_region.rs
src/generic_row_ring.rs
src/halftone.rs
src/huffman.rs
src/lib.rs
src/limits.rs
src/mmr.rs
src/reader.rs
src/segment_data.rs
src/segment_header.rs
src/symbol_text.rs
```

### Keep: models, representation invariants and exact-source proof bodies

Consumers: cfg-gated runtime imports, pinned Verus and proof-trust audits.
These files are repository-only and must remain outside the runtime crate.

```text
specs/arithmetic.rs
specs/bitmap.rs
specs/decode.rs
specs/document.rs
specs/file_header.rs
specs/generic_region.rs
specs/halftone.rs
specs/huffman.rs
specs/mmr.rs
specs/mod.rs
specs/readers.rs
specs/segments.rs
specs/symbol_text.rs
proofs/production_impl/arithmetic.rs
proofs/production_impl/bitmap.rs
proofs/production_impl/decode.rs
proofs/production_impl/document.rs
proofs/production_impl/file_header.rs
proofs/production_impl/generic_region.rs
proofs/production_impl/halftone.rs
proofs/production_impl/huffman.rs
proofs/production_impl/mmr.rs
proofs/production_impl/readers.rs
proofs/production_impl/segment_data.rs
proofs/production_impl/segment_header.rs
proofs/production_impl/symbol_text.rs
proofs/shared/arithmetic.rs
proofs/shared/bitmap.rs
proofs/shared/decode.rs
proofs/shared/document.rs
proofs/shared/generic_region.rs
proofs/shared/halftone.rs
proofs/shared/huffman.rs
proofs/shared/invariants.rs
proofs/shared/mmr.rs
proofs/shared/segment_data.rs
proofs/shared/segment_header.rs
proofs/shared/symbol_text.rs
proofs/shared/views.rs
proofs/spec_properties/arithmetic.rs
proofs/spec_properties/bitmap.rs
proofs/spec_properties/decode.rs
proofs/spec_properties/document.rs
proofs/spec_properties/generic_region.rs
proofs/spec_properties/halftone.rs
proofs/spec_properties/huffman.rs
proofs/spec_properties/mmr.rs
proofs/spec_properties/readers.rs
proofs/spec_properties/segment_data.rs
proofs/spec_properties/segment_header.rs
proofs/spec_properties/symbol_text.rs
```

### Keep and improve: build, CI, policy and correctness tooling

Consumers: developers, Cargo packaging, native CI, Verus workflow and legal
asset validation. Update stale claims and approvals without fabricating new
evidence. Review the disposition generator before using it again; its broad
file-level reasons must not automatically approve new unverified functions.

```text
.github/workflows/ci.yml
.github/workflows/fuzz.yml
.github/workflows/verus.yml
Cargo.lock
Cargo.toml
README.md
RELEASING.md
SECURITY.md
docs/threat-model.md
docs/verifier-limitations.md
production-proof-coverage.json
proof-dispositions.json
source-provenance.json
support-matrix.json
toolchain.json
tools/audit_package.py
tools/audit_production_coverage.py
tools/audit_runtime.py
tools/prepare_itu_manifest.py
tools/update_proof_dispositions.py
tools/verify_provenance.py
```

### Keep and improve: fuzzing, conformance and other executable validation

Consumers: fuzz workflow, local/authorized vector runner, differential runner
and internal cfg-gated runtime adapter. Preserve expected-result manifests and
reviewed disagreements even when one-run reports move out of the source tree.
Convert file-only case references into executable test IDs in P06.

```text
examples/decode_vector.rs
fuzz/Cargo.lock
fuzz/Cargo.toml
fuzz/fuzz_targets/arithmetic.rs
fuzz/fuzz_targets/document_arbitrary.rs
fuzz/fuzz_targets/file_header_arbitrary.rs
fuzz/fuzz_targets/file_header_structured.rs
fuzz/fuzz_targets/generic_region.rs
fuzz/fuzz_targets/huffman.rs
fuzz/fuzz_targets/mmr.rs
validation/README.md
validation/conformance/expected-differences.json
validation/conformance/itu-t88-2018-manifest.json
validation/conformance/run_itu.py
validation/differential/expected-differences.json
validation/differential/oracles.json
validation/differential/run_oracles.py
validation/metamorphic/cases.json
validation/negative/cases.json
validation/runtime_access.rs
```

### Keep, separate optional comparisons: performance inputs and harness

Consumers: local benchmarks and future base/head CI. Do not require a competitor
dependency or privileged hardware counters for ordinary decoder correctness.
Retain seeds/input definitions before replacing the narrow generated corpus.

```text
benchmarks/hayro-comparison/.gitignore
benchmarks/hayro-comparison/Cargo.lock
benchmarks/hayro-comparison/Cargo.toml
benchmarks/hayro-comparison/capture-counters.sh
benchmarks/hayro-comparison/check-stability.py
benchmarks/hayro-comparison/run-comparison.sh
benchmarks/hayro-comparison/run-stability.sh
benchmarks/hayro-comparison/src/main.rs
examples/perf_smoke.rs
validation/comparison/README.md
validation/performance/scenarios.json
```

### Consolidate later: historical evidence, not immediate deletion

Consumers: provenance script's required JSON list, release schemas, candidate
hash bindings, README/release instructions, comparison documentation. P11 must
migrate those references and provide replacement CI/release evidence first.
Retain explicit reviewed limitations and owner decisions in a current minimal
public document, not only in an expired CI artifact.

```text
conformance-attestation.json
fuzz-attestation.json
proof-attestation.json
proof-limitations-plan.json
release-approvals.json
release-attestation.schema.json
release-candidate-attestation.json
validation/comparison/hayro-0.3.1-results.json
validation/conformance/itu-t88-2018-results.json
validation/differential/pinned-poppler-results.json
validation/differential/results.json
validation/performance/results.json
```

### Reduce later: unrelated ignore boilerplate

Consumer: Git/developer working trees. P11 may remove unrelated Visual Studio
rules while preserving exclusions for Rust, proof outputs, corpora and artifacts.

```text
.gitignore
```

### New execution deliverables

Keep while executing readiness; consolidate the dated baseline/plan into the
release record once P12 completes. Gate tests remain permanent.

```text
docs/open-source-readiness-plan.md
docs/readiness-baseline.md
tools/tests/test_validation_gates.py
tools/tests/test_proof_audits.py
tools/rust_source.py
tools/evidence.py
tools/evidence.schema.json
tools/requirements.txt
.gitattributes
```

## Progress and next entry point

- P01 baseline and complete path inventory captured; unresolved license/vector
  decisions are isolated above. No file is designated for immediate deletion.
- P02 completed: package/ITU gates, syntax-based production cfg/module
  inventory, exact proof-identity matching, strict JSON/schema validation,
  source-bound review checks and proposal-only disposition generation.
  The gate tests now contain 45 generated cases and use the pinned
  development dependencies in `tools/requirements.txt`.
- The reusable-context paths and real scratch handling now verify directly.
  Twelve remaining runtime/proof divergences are explicit P05 release blockers,
  not counted as directly verified functions. The exact-source release command
  fails until those substitutions are removed and verified.
- PR #6 has now merged the MIT license file. P03 will add crate license
  metadata and validate the isolated package; crates.io remains disabled.
- Next: P03 isolated crate verification, P04 normative traceability and P06
  runtime coverage. P05 must close the now-identified exact-source backlog.

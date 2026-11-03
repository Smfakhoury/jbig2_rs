# Development roadmap

`main` develops the unpublished 0.2 line. The protected `release/0.1` branch
maintains 0.1.x; relevant decoder correctness fixes are assessed individually
for cherry-picking with their tests. Proof-only progress stays on `main` unless
it is needed to validate a selected functional maintenance fix.

Version 0.1 provides a broad bi-level decoder with partial Verus coverage.
The [support matrix](../config/support-matrix.json) defines supported,
unsupported and deferred behavior. The [release validation](publication-readiness.md)
page describes measured results and their limits.

## Requirement and functional-proof coverage

The [ledger](../validation/traceability/requirements.json) connects selected
T.88/T.6 requirements to source, contracts, models, proofs and exact tests.
It is not an exhaustive standards denominator.

Further work includes:

- Complete arithmetic integer/IAID and Huffman/MMR caller/output composition.
- Generic/refinement template, prediction, reference alignment and raster
  semantics.
- Dictionary imports/exports, aggregation, text placement, pattern/halftone
  outputs and page composition.
- Globals, reference resolution, retention/discard, intermediate-region
  lifetime, multipage and stripe lifecycle guarantees.
- Exhaustive enumeration of the supported scope, including acceptance,
  failure, resource and cross-component obligations.

Progress requires stronger source-bound contracts and executable evidence,
not merely more verified-function counts. Existing assumptions, preconditions
and standard-library/callback boundaries must remain explicit. Deferred
verification work is tracked in
[#135](https://github.com/microsoft/jbig2_rs/issues/135).

## Test and resource coverage

- Expand independently checked valid/minimal examples across coding modes,
  templates, page organization and resource interactions.
- Exercise critical malformed/truncated-input and allocation-failure paths.
- Review uncovered runtime paths and extend mutation sensitivity.
- Distinguish impossible or invariant-excluded branches from tested branches;
  do not remove them from reported denominators without explanation.
- Keep aggregate/per-file coverage floors and report changed-line coverage.

Generated test inputs need clear provenance and independent expected results.
Agreement between two calls to the same implementation is not an output oracle.
See [runtime coverage](runtime-coverage.md) and
[validation](../validation/README.md).

## Performance and memory characterization

Current gates check exact output, deterministic quota boundaries and isolated
allocation controls. Timing and RSS on shared CI hosts are observations, not
latency or memory guarantees.

Further work includes broader workloads, stable paired measurements on isolated
hosts, internal-phase attribution and allocator overhead/usable-size analysis.
Keep requested allocation payloads, vector capacity and process RSS distinct.
See [performance reproduction](../validation/README.md#basehead-performance).

## Maintenance requirements

- Keep a single executing Rust source tree for native and proof builds.
- Preserve typed errors, fallible allocation and configured resource guards.
- Enforce the 800-line budget per handwritten Rust source, model, proof and
  test file through responsibility-based modules.
- Keep runtime dependencies empty and proof/development tools out of the crate.
- Require fresh source/test/tool-bound verification and regression evidence.
- Retain reproducible first-party fuzz inputs and failure replay.
- Keep benchmark and conformance expectations separate from runtime behavior;
  do not add recovery heuristics solely to match an external decoder.

## Releases

Each release needs current quality checks, an audited actual archive, native
platform/toolchain tests, exact downstream behavior and accurate claims.
Immutable registry artifacts and source tags must remain tied to their
checksums. Release instructions are in [RELEASING.md](../RELEASING.md).

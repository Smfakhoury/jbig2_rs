# Releasing

Publication remains disabled until all release gates are complete.

A release requires:

1. owner approval of the complete support matrix and resource limits;
2. CELA/legal approval for crate publication and every redistributed vector;
3. zero incomplete production functions and a reviewed proof disposition for
   every production function;
4. pinned Verus verification of the exact `src/lib.rs` built by Cargo;
5. passing Cargo, Clippy, test, documentation, package, provenance, fuzz,
   conformance, differential, negative, and performance gates;
6. reviewed and pinned differential disagreements;
7. an audited package containing no proof bodies, standards documents,
   non-redistributable vectors, native oracles, adapters, benchmarks, or build
   output;
8. completed attestations bound to one immutable source revision.

The readiness proof audit found runtime/proof substitutions. Before a release,
run the coverage audit with both `--require-complete` and `--require-exact`.
The first checks inventory accounting; the second rejects known substitutions.
Currently the exact-source gate is blocked, and the historical candidate
attestation is superseded. A passing Verus build or an accounted-for inventory
is not permission to bypass this gate.

Changing `publish = false` requires explicit release-owner approval.

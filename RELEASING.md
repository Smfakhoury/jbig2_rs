# Release requirements

**Publication is disabled.** The repository remains private; Cargo is
`publish = false`, version `0.0.1`. Owner-reported CELA project publication
approval is recorded, but crate publication, official-asset/oracle redistribution
and visibility changes are separate decisions. No green workflow or document
authorizes them.

A release requires:

1. Owner approval of the complete
   [support matrix](config/support-matrix.json), resource limits and actual
   supported claims. Colour remains unsupported, encoder deferred, and full
   Annex F profile 1 conformance is not claimed.
2. Legal approval for crate publication and every redistributed asset, with
   correct license/notices and [provenance](validation/source-provenance.json).
3. Zero incomplete production functions, source-bound reviewed dispositions
   for every non-direct status, and closure of required runtime/proof gaps.
4. Pinned Verus verification **and compilation** of the same `src/lib.rs` built
   by Cargo, complete production accounting and all semantic contrast groups.
5. Passing Cargo, strict formatting/Clippy/rustdoc, tests/doc-tests, package,
   provenance, coverage, fuzz, conformance, differential, negative/metamorphic
   and performance gates with honest availability semantics.
6. Independently reviewed normative/model claims and differential disagreements.
   Unresolved official positives are blockers, not reviewed rejection credit.
7. A real allowlisted archive excluding separate proof/model files, restricted
   standards/vectors, oracle binaries/adapters, benchmarks and build output.
   Inline proof attributes/ghost code may remain cfg-inactive source; this is
   not textual proof stripping.
8. Extracted archive tests and a fresh offline downstream exact-output consumer
   on every declared platform/toolchain, without proof files/dependencies or
   internal cfgs. An unsafe probe must fail, its guard-only weaker control must
   compile, and extracted source must be unconditionally restored.
9. A fresh immutable-candidate rehearsal, complete evidence index and explicit
   release-owner sign-off. Evidence must bind source, archive checksum,
   toolchains, tests, normative asset hashes, proof strength, coverage, fuzz and
   performance under the retained
   [release schema](validation/schemas/release-attestation.schema.json).

The release-level proof audit must use **both** completeness and exactness:

```sh
python3 -B tools/audit_production_coverage.py \
  --verus-json target/verification/verus-production.json \
  --output target/verification/production-proof-coverage-ci.json \
  --require-complete --require-exact
```

Completeness means accounted-for inventory, not universally verified bodies.
Exactness currently fails on the disclosed capacity-accounting divergence.
The standard-trust decision and protected encoded-family fixture holds remain
in force; no assumptions, local std/equality specifications, termination waivers,
pin changes or lower gates may bypass them.

See [publication readiness](docs/publication-readiness.md) for the qualified
snapshot, [P01–P12](docs/open-source-readiness-plan.md) for complete acceptance
criteria, and [limitations](docs/verifier-limitations.md) for trust boundaries.
Later cleanup, CI integration and release rehearsal are not automatically
complete. Changing `publish = false` requires a separately approved owner change.

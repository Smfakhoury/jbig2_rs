# Release requirements for 0.1

The repository is public; Cargo version `0.1.0` is restricted to
`publish = ["crates-io"]`. The owner authorized the actual publication step on
2026-11-02 after configuring the Cargo API key; the
[authorization record](https://github.com/microsoft/jbig2_rs/issues/125#issuecomment-6580927454)
contains no credential material. Official-asset/oracle redistribution remains
separate. No green workflow alone authorizes an upload.

The owner approved an initial **0.1 release with partial Verus coverage** on
2026-11-02. Exhaustive requirement enumeration and complete functional proofs
are deferred to [future main work](https://github.com/microsoft/jbig2_rs/issues/135).
The [source-bound release scope](validation/traceability/requirements.json)
records the decision and deliberately leaves actual proof gaps unchanged.
No full-spec, full Annex F profile 1 or complete-formal-verification claim is made.

The 0.1 release still requires:

1. Owner approval of the complete
   [support matrix](config/support-matrix.json), resource limits and actual
   supported claims. Colour remains unsupported, encoder deferred, and full
   Annex F profile 1 conformance is not claimed.
2. Legal approval for crate publication and every redistributed asset, with
   correct license/notices and [provenance](validation/source-provenance.json).
3. Zero incomplete production functions, source-bound reviewed dispositions
   for every non-direct status. Existing exact-source proofs and quality gates
   must continue to pass; missing end-to-end functional proofs are disclosed
   deferred coverage, not newly proved results.
4. Pinned Verus verification **and compilation** of the same `src/lib.rs` built
   by Cargo, complete production accounting and all semantic contrast groups.
5. Passing Cargo, strict formatting/Clippy/rustdoc, tests/doc-tests, package,
   provenance, coverage, fuzz, conformance, differential, negative/metamorphic
   and performance gates with honest availability semantics.
6. Independently reviewed normative/model claims and differential disagreements.
   Unresolved official positives are blockers, not reviewed rejection credit.
   The owner explicitly confirmed validation of normative/model review on
   2026-11-02. The source-bound confirmation is recorded separately in the
   traceability ledger; no further generic review authorization is missing.
   That confirmation does not supply the ledger's still-missing functional
   guarantees or exhaustive requirement coverage. The seven official execution
   disagreements have accepted strict-rejection dispositions after #133; they
   are not bitmap passes or a full-spec conformance certificate.
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

## Publication dry run without uploading

```sh
python3 -B tools/check_distribution.py --toolchain 1.88.0 \
  --publish-dry-run --output target/distribution/publication-dry-run.json
```

This tests the real archive and downstream consumer, then executes the selected
Cargo's actual `publish --dry-run --locked --registry crates-io` in a temporary
extraction outside the checkout. When publication is disabled in a development
manifest, only that extraction's setting is temporarily enabled in both Cargo
manifests; both are unconditionally restored. With the crates.io-only release
manifest, no setting change is needed.
A separate Cargo target directory isolates the publication artifacts.
Cargo verifies/compiles
the repackaged crate and must confirm that no upload occurred. Registry metadata
access is allowed only for this phase, with no inherited registry credentials.
The source checkout is not modified and the checker always uses `--dry-run`;
no visibility or version change is made. See [distribution](docs/distribution.md) for hashes, command boundaries
and failure controls.

Dry-run success establishes Cargo packaging/publish-command viability, not
registry ownership, a name reservation, completeness of semantic proofs or
completion of the remaining release gates.

## Authorized publication

Only after the exact enablement commit passes independent review and protected
CI, use a clean checkout of that accepted merged source:

```sh
cargo +1.88.0 publish --dry-run --locked --registry crates-io
cargo +1.88.0 publish --locked --registry crates-io
```

The second command is the actual authorized upload, not a CI test. Cargo should
read the configured local credential provider directly. Never print the API
key, put it in command arguments, commit it, or copy it into chat or reports.
The ordinary distribution checker removes credential configuration and never
uploads, even when the source manifest permits crates.io.

After publication, verify the registry version and downloaded archive checksum,
run a fresh MSRV consumer using the registry dependency, and bind the release
tag/notes to the exact uploaded source. An existing registry version must be
checked before retrying an ambiguous upload; it cannot be overwritten.

For the scoped requirement-coverage decision, use the actual verifier report
and Cargo test discovery:

```sh
python3 -B tools/check_traceability.py \
  --verus-json target/verification/verus-production.json --discover-tests \
  --require-v01-scope --output target/release/v01-scope.json
```

This gate accepts only the dated owner decision for version `0.1.0`, current
owner-review evidence and checked proof/test identities. It does not declare
the other release checks complete or enable upload. The stronger
`--require-reviewed-decomposition --require-complete` gate remains unchanged
for future exhaustive verification and is expected to remain nonzero for 0.1.

The production-inventory proof audit must still use **both** completeness and exactness:

```sh
python3 -B tools/audit_production_coverage.py \
  --verus-json target/verification/verus-production.json \
  --output target/verification/production-proof-coverage-ci.json \
  --require-complete --require-exact
```

Completeness means accounted-for inventory, not universally verified bodies.
Exactness rejects runtime/proof substitutions and incomplete functions. The
observed-capacity integration removes the former MMR omission using the
separately reviewed unconstrained standard getter; this is not a proof of
allocator behavior or removal of the reviewed standard/callback limitations.
The exact standard-interface review boundary and protected encoded-family
fixture holds remain in force; no assumptions, strengthened std/equality
specifications, termination waivers, pin changes or lower gates may bypass them.

Existing deterministic work/allocation controls and coverage floors remain
mandatory. Hosted latency/RSS observations remain informational, not a 0.1
performance SLA or formal total-memory bound. No private-only official vector
workflow may be dispatched on the public repository; approved local execution
and its exact source/input identities must remain explicit.

See [publication readiness](docs/publication-readiness.md) for the qualified
snapshot, [P01–P12](docs/open-source-readiness-plan.md) for complete acceptance
criteria, and [limitations](docs/verifier-limitations.md) for trust boundaries.
Later cleanup, CI integration and release rehearsal are not automatically
complete. Publication is crates.io-only under the recorded owner authorization;
other registries and protected-asset rights are not implied.

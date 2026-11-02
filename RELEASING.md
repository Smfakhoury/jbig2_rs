# Releasing jbig2-decoder

The package publishes only to crates.io. The 0.1 series has partial Verus
coverage: existing proofs and quality checks are mandatory, while exhaustive
requirement enumeration and complete functional proofs remain
[future work](https://github.com/microsoft/jbig2_rs/issues/135).

## Validate the source

1. Select the version and supported claims. Update package metadata, local
   consumer lockfiles and the versioned release scope together. Keep unsupported
   features, resource limits and verification limitations explicit.
2. Run formatting, strict Clippy, native tests, doctests and rustdoc on the
   supported toolchains/platforms.
3. Run whole Verus verification and compilation, exact production accounting,
   semantic contrasts and native mutation controls.
4. Run provenance, executable-catalog and workflow audits; collect coverage,
   bounded fuzz and performance/resource results.
5. Record original conformance outcomes and expected rejections separately.
   Missing external assets are unavailable, not passed.

The complete source is in `src/`; separate proof/model and development assets
must remain outside the Cargo archive. Every non-direct proof status needs an
explicit source-bound disposition. Runtime/proof substitutions and incomplete
production accounting fail release checks.

## Proof and scope checks

```sh
python3 -B tools/audit_production_coverage.py \
  --verus-json target/verification/verus-production.json \
  --output target/verification/production-proof-coverage-ci.json \
  --require-complete --require-exact
python3 -B tools/check_traceability.py \
  --verus-json target/verification/verus-production.json --discover-tests \
  --require-v01-scope --output target/release/v01-scope.json
```

Inventory completeness is not complete functional proof. The 0.1 scope check
requires matching versioned scope, proof identities and test discovery; it does
not make the stronger `--require-reviewed-decomposition --require-complete`
traceability gate pass.

Keep the standard-library/callback boundaries in
[verification limitations](docs/verifier-limitations.md). Do not add trusted
algorithm bodies, assumptions or termination waivers to make verification pass.

## Test the actual archive

```sh
python3 -B tools/check_distribution.py --toolchain 1.88.0 \
  --publish-dry-run --output target/distribution/msrv.json
python3 -B tools/check_distribution.py --toolchain stable \
  --publish-dry-run --output target/distribution/stable.json
```

Distribution CI runs natively on Linux, Windows and macOS. It audits the real
archive, tests the extracted crate and a fresh exact-output consumer, and checks
internal-hook isolation and the unsafe-code prohibition.

The checker uses a fresh credential-free Cargo home. Its publication command
always includes `--dry-run`, verifies the repackaged source and manifest, and
requires an explicit aborted-upload result. It never uploads a crate.
See [distribution](docs/distribution.md) for details.

## Publish an immutable candidate

Use a clean checkout of the exact source whose checks passed. Verify source/tree
identity after integration and rebuild the final archive; its VCS metadata can
change the checksum relative to a pull-request archive.

```sh
cargo +1.88.0 package --locked
cargo +1.88.0 publish --dry-run --locked --registry crates-io
cargo +1.88.0 publish --locked --registry crates-io
```

The last command uploads the release. Use Cargo's configured credential
provider; keep credentials out of source control, command arguments and logs.
Before retrying an ambiguous upload, check whether the version already exists
and compare its checksum. Registry versions cannot be overwritten.

## Verify the published result

- Check the registry version and archive checksum.
- Download and audit the registry archive; compare runtime, documentation,
  license and manifest contents with the released source.
- Check `.cargo_vcs_info.json` against the exact source revision.
- Use a fresh Cargo home to fetch an exact registry dependency, with no local
  path override. Check the lockfile checksum and run the MSRV exact-output
  consumer offline.
- Create the version tag and release notes for that same source.
- Publish a checksum and source/tool/test-bound evidence index, using the
  [release-attestation schema](validation/schemas/release-attestation.schema.json).

Release notes must disclose partial formal coverage and known limitations.
Do not claim full JBIG2/Annex F conformance, universal panic freedom, a formal
total-memory bound or a latency SLA from the existing evidence.
External standards, vectors and comparison tools have separate terms and must
not enter the crate or public failure artifacts without suitable rights.

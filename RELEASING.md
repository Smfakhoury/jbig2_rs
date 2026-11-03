# Releasing jbig2-decoder

`main` is the unpublished `0.2.0-dev` development line with Cargo publication
disabled. The 0.1.x maintenance line lives on `release/0.1`; its publication
policy and version metadata are independent. Main CI checks actual proof/test
evidence under `--require-development-scope` without declaring the deferred
functional-proof target complete. The historical 0.1 scope is retained as
`maintenance_release_scope`, not applied as a 0.2 release authorization.

Assess runtime/API correctness fixes for a minimal maintenance backport with
its own branch-specific tests and source-bound evidence. Proof-only, tooling
and development-version changes are not automatic 0.1 backports.

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
  --require-development-scope --output target/release/development-scope.json
```

On `release/0.1`, use `--require-v01-scope` instead of the development flag.
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

## Publish from GitHub

The [publication workflow](.github/workflows/release.yml) runs when a GitHub
Release is published. Manual runs default to **dry run** and do not request OIDC
credentials, require publishing-environment approval or upload a crate.

### Branch policy

`release/0.1` maintains the 0.1.x line; `main` carries ongoing development.
The maintenance branch starts at the reviewed publication-policy baseline.
Both branches require the same 30 checks, strict up-to-date status checks,
and no bypass, force pushes or deletion. CI also runs on both branch heads.
Publishing requires a tag reachable from `release/0.1` and the exact integration
commit of a merged PR targeting that branch, with a matching reviewed tree.
A new 0.1.x version merged only to `main` is not a release candidate.

Make release fixes and version bumps in PRs against `release/0.1`. Backport only
selected changes; do not routinely merge future development from `main` into
the maintenance line. Port applicable maintenance fixes to `main` separately.
Keep package metadata, all local decoder lockfile entries, versioned release
scope/schema and their source-bound provenance consistent on each branch.
The next version on `main` is a separate development decision, not a maintenance
release prerequisite. Publication of other version lines requires an explicit
workflow/policy update.

The immutable, already-published `v0.1.0` and `v0.1.1` retain their original
`main` PR provenance. Rehearsals accept those two tags only at their original
commits with matching, unyanked registry checksums; they are never retagged or
republished. New versions have no `main` exception.

### One-time setup

1. Create the GitHub environment `crates-io` in the repository settings.
   Require a release maintainer's approval, disable administrator bypass, and
   allow deployments only from branch `release/0.1` and tags matching `v0.1.*`.
   Allow self-review if the same maintainer creates and approves releases.
   Merely naming an environment in a workflow does not configure these rules.
2. On crates.io, open **jbig2-decoder → Settings → Trusted Publishing → Add**,
   select **GitHub**, and save these exact fields:

   | Field | Value |
   |---|---|
   | Repository owner | `microsoft` |
   | Repository name | `jbig2_rs` |
   | Workflow filename | `release.yml` |
   | Environment | `crates-io` |

   This registration requires crate-management access on crates.io. See the
   [official Trusted Publishing guide](https://crates.io/docs/trusted-publishing).
   No crates.io API key or GitHub Actions secret is needed.

### Each release

1. Prepare and merge the final versioned release PR into `release/0.1`. Its integrated tree must
   equal the PR head tree; update and retest the PR if integration changes it.
2. Create `vVERSION` at that exact integration commit on `release/0.1`. Wait for all
   required checks on the PR and integrated/tagged source to pass.
3. In **Actions → Publish crate → Run workflow**, select branch `release/0.1`, enter
   the tag and leave **dry_run** enabled for a credential-free rehearsal.
4. Publish a GitHub Release using that existing tag, then approve the `crates-io`
   environment deployment. Alternatively, run the workflow manually from `release/0.1`
   with **dry_run** disabled and approve the same environment.

The read-only validation job checks tag/version/registry policy, merged PR and
tree identity, all 30 PR checks and 29 final-commit native checks. It verifies
check issuers, source workflow paths, run revisions and successful completion.
If those workflows are still queued/running, source validation waits up to
105 minutes, sharing one deadline across both revisions. This includes semantic
shards and collector checks that appear only after earlier proof jobs complete.
It polls once per minute without requesting OIDC credentials. Failed, cancelled,
unattributed or genuinely missing checks fail immediately; reaching the deadline
also fails. The read-only job has a 120-minute ceiling, including packaging;
other job budgets are unchanged. Re-run a timed-out dry run after CI completes.
It then exercises the extracted archive and real Cargo dry run. Publication
rebuilds the same checksum and rechecks the tag and registry before exchanging
OIDC identity for a short-lived token. Only the publishing job has `id-token:
write`; only `cargo publish` receives the temporary registry token. The official
authentication action revokes it at job completion.

An existing version is skipped only when its unyanked registry checksum matches
the checked archive. Different bytes or a yanked version fail; nothing is
overwritten or unyanked. Concurrent publication jobs are serialized. After upload,
the workflow verifies registry metadata and the downloaded checksum and runs a
fresh, exact-version, registry-backed MSRV consumer offline, including the hidden
internal-hook rejection. Metadata-only reports are retained as Actions artifacts.
An upload or verification failure remains a failed workflow; inspect the registry
before retrying. Restricted conformance assets are not acquired by this workflow.

## Verify the published result

- Check the registry version and archive checksum.
- Download and audit the registry archive; compare runtime, documentation,
  license and manifest contents with the released source.
- Check `.cargo_vcs_info.json` against the exact source revision.
- Use a fresh Cargo home to fetch an exact registry dependency, with no local
  path override. Check the lockfile checksum and run the MSRV exact-output
  consumer offline.
- Confirm the version tag and release notes identify that same source.
  For direct Cargo publication, create them after registry verification.
- Publish a checksum and source/tool/test-bound evidence index, using the
  [release-attestation schema](validation/schemas/release-attestation.schema.json).

Release notes must disclose partial formal coverage and known limitations.
Do not claim full JBIG2/Annex F conformance, universal panic freedom, a formal
total-memory bound or a latency SLA from the existing evidence.
External standards, vectors and comparison tools have separate terms and must
not enter the crate or public failure artifacts without suitable rights.

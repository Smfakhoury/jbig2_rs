# Ordinary Rust distribution without Verus

The crate has no runtime dependencies, uses the MIT license, and declares Rust
1.89.0 as its minimum supported Rust version (MSRV). Crates.io publication is
still disabled while the readiness plan and proof/conformance gaps are open.

## What is packaged

Cargo's explicit include list contains the runtime Rust sources, README,
security policy and license. Cargo adds its normalized manifest, original
manifest, lockfile and VCS metadata. The current archive has 24 files.

The separate `proofs/`, `specs/`, `validation/`, `fuzz/`, `tools/` and benchmark
directories are not included. Neither Verus nor `vstd` is a Cargo dependency.

Runtime sources still contain `cfg(verus_keep_ghost)` attributes, inline ghost
blocks and inactive references to the repository's proof modules. Ordinary
Cargo builds do not activate or compile those items. They do not need the
referenced proof files. This is proof erasure by conditional compilation, not
a second generated/hand-maintained decoder or a textually proof-stripped source
distribution.

The custom `verus_keep_ghost`, `fuzzing` and `jbig2_validation` cfgs are internal
repository configurations, not supported downstream features. Do not enable
them when building a published archive. Use the repository and pinned tools
for proof or validation work. Normal package metadata exposes no such feature,
and the validation-access module is not available to a normal consumer.

## Reproduce the distribution gate

With Python 3.11+ and the selected Rust toolchain already installed:

```text
python -B tools/check_distribution.py --toolchain 1.89.0 \
  --output target/distribution/msrv.json
python -B tools/check_distribution.py --toolchain stable \
  --output target/distribution/stable.json
```

This script uses only Python's standard library. The broader proof-audit test
suite separately requires `tools/requirements.txt`.

The check:

1. Creates a temporary directory outside the checkout and a fresh Cargo home.
2. Removes inherited proof/compiler flags and wrappers and forces Cargo offline.
3. Generates the actual `.crate` with the selected Cargo version, then validates
   its file allowlist, required license, root and regular-file entries.
4. Extracts only validated files and compares runtime/document/license bytes to
   the checkout. The normalized Cargo manifest is checked through Cargo metadata.
5. Confirms the extracted package has no dependencies or proof/validation
   features, the expected license/MSRV, and publication remains disabled.
6. Executes the extracted archive's unit tests and doc tests.
7. Creates an independent downstream project depending only on the extracted
   crate. It checks exact dimensions and pixels for a generated MMR stream via
   slice, reader, sink, reusable-context and embedded/global APIs. It also
   checks recovery after an invalid input.
8. Verifies that importing `runtime_access` from that normal consumer fails
   with the expected unresolved-import diagnostic.

All package/consumer Cargo operations are offline and use locked dependencies.
The consumer lockfile is first generated offline for its single local path
dependency. Temporary builds and extracted assets are removed on completion.
Failures exit nonzero and, when `--output` is supplied, produce a failure report
rather than a success-shaped fallback.

CI runs this gate natively on Linux, Windows and macOS, using both MSRV and
current stable. Each report records the actual compiler version and archive
hash. Different Cargo versions can normalize manifests differently, so the
archive hashes are not expected to match across toolchains.

## What this does not prove

Passing distribution checks establishes that the ordinary crate is independent
of repository proof files and tools. It does not establish full JBIG2
conformance or close the 10 runtime/proof divergences tracked in
[verifier-limitations.md](verifier-limitations.md). Those remain separate,
release-blocking P05 work.

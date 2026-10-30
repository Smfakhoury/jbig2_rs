# Ordinary Rust distribution without Verus

The crate has no runtime dependencies, uses the MIT license, and declares Rust
1.88.0 as its minimum supported Rust version (MSRV). Crates.io publication is
still disabled while the [release gates](../RELEASING.md) and
proof/conformance gaps are open. The accepted revision-qualified native reports
are identified in [publication readiness](publication-readiness.md).

## What is packaged

Cargo's explicit include list contains the runtime Rust sources, README,
security policy and license. Cargo adds its normalized manifest, original
manifest, lockfile and VCS metadata. The archive inventory is checked against
the real package, not inferred from a successful build.

The source allowlist is recursive: focused component submodules and their
separate unit-test files under `src/` remain packaged. Reorganization changes
the actual file count, not the requirement to compare every included source
byte and execute the extracted package's tests. Separate proof/model assets
remain excluded.

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
python -B tools/check_distribution.py --toolchain 1.88.0 \
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
7. Appends an isolated unsafe probe to the extracted crate and requires the
   compiler's `unsafe_code` rejection. The same probe must compile when only
   the crate-level prohibition is weakened. Restores the original source even
   when either control fails; the checkout and `.crate` archive are untouched.
8. Creates an independent downstream project depending only on the extracted
   crate. It checks exact dimensions and pixels for a generated MMR stream via
   slice, reader, sink, reusable-context and embedded/global APIs. It also
   checks recovery after an invalid input.
9. Verifies that importing `runtime_access` from that normal consumer fails
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
The safety controls use these existing jobs; they do not add another CI policy.

The development compiler in
[`config/toolchain.json`](../config/toolchain.json) remains 1.89.0, independently
of the runtime minimum. Verus, coverage and fuzz use their separate pins.

## What this does not prove

Passing distribution checks establishes that the ordinary crate is independent
of repository proof files and tools. It does not establish full JBIG2
conformance or remove the reviewed standard-library/callback limitations tracked
in [verifier-limitations.md](verifier-limitations.md). Exact native-body
accounting does not remove that separate, release-blocking P05 scope.

The unsafe-code control checks crate-authored source. Compiler-generated
derives can contain internal unsafe implementations/intrinsics, and the
standard library uses unsafe internally. Those are compiler/library trust
boundaries, not evidence that the decoder contains handwritten unsafe or that
the control establishes a binary-wide absence of unsafe operations.

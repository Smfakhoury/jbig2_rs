# jbig2_rs

`jbig2_rs` is a strict, dependency-free Rust JBIG2 decoder under active
development. **It is not yet suitable for untrusted production documents.**
The repository remains private; the crate is unpublished, version `0.0.1`,
with `publish = false`.

The approved first milestone is a broad **bi-level decoder**, not an encoder.
Colour is unsupported and encoder work is deferred. Implemented features are
not a claim of full JBIG2 or Annex F profile 1 conformance. The authoritative
feature and resource policy is [the support matrix](config/support-matrix.json).

## Where to start

- **Understand or audit the specification:** the [model reading guide](specs/README.md)
  explains the components, links to free official ITU downloads and walks through one small audit.
- **Build or test the decoder:** see [build and test](#build-and-test) and the [public API](src/lib.rs).
- **Run or interpret proofs:** see [Verus through Cargo](#run-verus-through-cargo) and [what the result means](#interpret-the-result).

## Decoder scope

- Standalone sequential and random-access files; embedded/PDF page streams
  with reusable validated globals.
- Known/unknown-height, multi-page and striped pages.
- Arithmetic decisions, integers and IAID; standard/custom Huffman; MMR.
- Generic/refinement regions, symbol dictionaries, text, patterns and halftones.
- Retention, clipping, combination operators, and owned/reader/page-sink APIs.

See [`src/lib.rs`](src/lib.rs) for public API documentation and
[the threat model](docs/threat-model.md) for limits and hostile-input policy.
Unsupported syntax, malformed input, resource limits, allocation failure and
I/O failure have distinct typed errors.

## Build and test

The runtime MSRV is **Rust 1.88.0**. Development uses a separate compiler;
all tool pins are in [`config/toolchain.json`](config/toolchain.json).

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo doc --no-deps
cargo package --locked
```

For explicit MSRV checks:

```sh
cargo +1.88.0 test --locked --all-targets
cargo +1.88.0 test --locked --doc
cargo +1.88.0 check --locked --release --all-targets
```

Distribution CI executes natively on Linux, Windows and macOS with MSRV and
current stable. The extracted archive is tested offline with a fresh Cargo
home and an independent exact-output consumer. It exposes no validation-access
module and needs neither Verus nor repository proof files. See
[distribution](docs/distribution.md) for reproduction and compiler safety controls.
That separate maintainer check uses Python 3.11+'s standard library:

```sh
python3 -B tools/check_distribution.py --toolchain 1.88.0
```

It is not a prerequisite for ordinary Cargo builds or Cargo-Verus.

## Implementation and proof boundary

For a short human-audit route, start with the [model reading guide](specs/README.md).
The [independent review procedure](docs/specification-review.md) separates
standard-to-model fidelity from implementation proofs.

[`src/`](src) is the sole executable implementation and Cargo source of truth.
[`specs/`](specs) defines project models; [`proofs/`](proofs) supplies invariants,
model-support lemmas and proof bodies attached to that implementation.
Separate proof/model directories are excluded from the package. Proof
annotations and inline ghost blocks remain as **cfg-inactive source** in an
ordinary archive; they are not textually stripped.

The runtime crate forbids handwritten unsafe Rust. Compiler-generated derives,
standard-library internals, the compiler and pinned Verus/vstd contracts remain
trust boundaries. This is **not** a binary-wide no-unsafe or no-panic proof.
Some verified contracts establish bounds or state preservation rather than
decoded-output semantics, and a disclosed runtime/proof divergence still
blocks a complete exact-source claim.

### Run Verus through Cargo

With the pinned Verus release and bundled `cargo-verus` on `PATH`, and the
proof Rust toolchain installed, run from the repository root:

```sh
cargo +1.98.1 verus build --manifest-path verification/Cargo.toml --locked -- \
  --num-threads 1 --rlimit 30 --triggers-mode silent
```

This verifies and builds the production library. **Python is not required for
this command.** The isolated manifest targets the same `src/lib.rs` as ordinary
Cargo; its pinned `vstd` dependency is separate from the dependency-free runtime.
Bare `cargo verus verify` against the runtime manifest is not an acceptance gate.

### Interpret the result

- A successful production-crate result with zero verification errors means
  its submitted contracts were proved under their preconditions and trusted
  interfaces. Cargo can also process dependencies; do not mistake a dependency's
  successful result for production verification.
- The number of verified obligations is **not** a count or percentage of T.88
  statements, nor proof that every contract specifies complete decoded output.
- A focused/module/function run is partial debugging evidence, even with zero
  errors. It does not replace the unfiltered proof and compiled library.
- Complete production *accounting* includes reviewed limitations and forwarders;
  it does not mean every body is directly proved. The remaining runtime/proof
  divergence still blocks the exact-source release gate.
- Functional project contracts, independent standard-to-model review, and
  official conformance results are separate evidence.

The [proof-gap and trust guide](docs/verifier-limitations.md) explains these
categories, the remaining work and focused debugging commands.
[Statement traceability](docs/spec-traceability.md) records semantic gaps;
[the auditor index](docs/verus-auditor-index.md) identifies their model/proof
entry points.

### Additional maintainer and CI checks

The Python tools are a **separate development/CI assurance layer**, not a
prerequisite for executing Cargo-Verus or using the crate. They audit
source/function accounting, statement links, compiling strong/weaker contract
tests, packaging, coverage, fuzzing and performance evidence. Their tests
check that those audits fail closed. The current CI/release contract requires
these checks; running the Cargo command alone does not claim that they ran.
See [validation](validation/README.md) for the maintained entry points.

For bounded remote proof work, see [proof delegation](docs/github-proof-delegation.md);
delegation does not replace the required verification gates.

## Evidence and publication status

[Publication readiness](docs/publication-readiness.md) is the single
revision-qualified numerical snapshot, including actual proof, coverage and
native distribution runs. Machine records remain authoritative for their
bound source revisions; a green workflow does not approve a new source,
normative interpretation or release.

- [Traceability](docs/spec-traceability.md): selected T.88/T.6 requirements,
  evidence strength and incomplete normative decomposition.
- [Runtime coverage](docs/runtime-coverage.md): ordinary-code measurements,
  enforced non-regression floors and remaining test gaps.
- [Validation](validation/README.md): official cases, differential evidence,
  exact test catalogs, mutation controls, fuzzing and informational performance.
- [Readiness gates P01–P12](docs/open-source-readiness-plan.md): complete
  acceptance criteria and dependencies, not a session log.
- [Repository layout](docs/repository-layout.md): retained policy, baselines,
  evidence and generated-artifact roles.

The owner-reported CELA project publication approval and MIT license do **not**
authorize crate publication or redistribution of standards, official vectors
or native oracle assets. Official assets remain external under their recorded
permissions. [Release requirements](RELEASING.md) and
[security reporting](SECURITY.md) remain separate gates.

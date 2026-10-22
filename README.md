# jbig2_rs

`jbig2_rs` is a production-oriented, memory-safe Rust implementation of a
strict JBIG2 decoder. It is under active development and is not yet suitable
for decoding untrusted production documents.

The approved first milestone is decoder-only. Its exact feature and resource
contract is recorded in `support-matrix.json`. Publication is disabled, and
the project does not currently claim full JBIG2 or Annex F profile
conformance.

## Architecture

- `src/` is the sole executable implementation and Cargo source of truth.
- `specs/` contains functional models used by Verus.
- `proofs/shared/` contains abstract views and representation invariants.
- `proofs/spec_properties/` contains implementation-independent theorems.
- `proofs/production_impl/` contains proof bodies and proof macros attached to
  the exact implementation under `src/`.
- `validation/` contains conformance, differential, negative, metamorphic, and
  performance validation.
- `fuzz/` contains arbitrary-input and structured-input fuzz targets.

Ordinary Cargo builds do not compile proof-only items. Packages exclude the
separate proof/model and validation assets, while inactive proof annotations
and inline ghost blocks remain in the Rust source. No Verus or `vstd` dependency
is needed by an ordinary consumer. The Verus workflow uses the same
`src/lib.rs` crate root, with the runtime/proof differences disclosed below.
See [proof-free distribution](docs/distribution.md) for the isolated archive
checks and the distinction between cfg erasure and textual source stripping.

## Current status

The decoder implements:

- standalone sequential and random-access organizations;
- contiguous embedded/PDF page streams and reusable validated globals;
- known-height, unknown-height, multi-page, and striped pages;
- arithmetic coding, arithmetic integers, IAID, standard/custom Huffman, and
  MMR;
- generic and generic-refinement regions;
- symbol dictionaries and text regions, including Huffman/arithmetic variants
  and refinement aggregation;
- pattern dictionaries, Gray reconstruction, and halftone regions;
- immediate/intermediate region buffers, retention, clipping, and all
  combination operators;
- owned, bounded-reader, and page-sink APIs.

The owner has reported CELA approval for open-source project publication, and
the repository includes the MIT license. Crates.io publication remains disabled.
Official ITU assets stay external under their separately recorded permissions.
Native Linux, Windows and macOS CI, Verus and fuzz smoke checks are operational.

The readiness audit found that earlier proof coverage overstated exact-source
verification: some same-named functions were conflated and several cfg-selected
runtime paths differed from proof paths. These are now explicit, hash-bound
limitations. The current inventory has 608 directly verified functions, 24
reviewed boundaries, 23 audited forwarders and 1 runtime/proof divergence.
The latter blocks a full exact-source proof claim; passing Verus does not alone
establish normative conformance. See [verifier limitations](docs/verifier-limitations.md).

Approved local validation has executed official ITU vectors and independent
black-box oracles: two exact positives, seven unresolved official positives and
one unsupported case, with no reviewed rejection credit. These differences
remain under `validation/` and require normative re-evaluation before stronger claims.
The [readiness plan](docs/open-source-readiness-plan.md) tracks the remaining work.

## Normative traceability

The [provisional T.88/T.6 ledger](docs/spec-traceability.md) links 249 selected
records (240 normative-rule records and nine project-policy records) to exact
contracts, model/proof definitions and test identities. It distinguishes
functional postconditions from state/bounds-only
evidence and runtime/proof substitutions. All 30 feature rows have a
decomposition worklist, but requirement decomposition and owner review remain
incomplete; passing the reference checker is not a conformance certificate.

## Local checks

```text
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo doc --no-deps
cargo package
python -B tools/check_distribution.py --toolchain 1.89.0
```

The MSRV is Rust 1.89.0; native distribution CI also tests current stable on
Linux, Windows and macOS. Distribution checks use Python 3.11+ and the standard
library, without the additional proof-audit dependencies.

Audit tooling additionally requires the pinned development-only dependencies:

```text
python -m pip install -r tools/requirements.txt
python -B -m unittest discover -s tools/tests -v
```

### Cargo-Verus verification

With the pinned Verus release and its bundled `cargo-verus` on `PATH`, run from
the repository root:

```text
cargo +1.98.1 verus verify --manifest-path verification/Cargo.toml --locked -- --num-threads 1 --rlimit 30
```

The isolated verification manifest targets the exact same `src/lib.rs`, with
the audited crate identity `jbig2_rs_production`. Its locked `vstd` dependency
does not enter the ordinary runtime manifest or the 24-file Cargo archive.
The explicit manifest path is intentional: bare `cargo verus verify` against
the runtime manifest does not verify this project and is not an acceptance
gate. For local module debugging, use `--verus-args-to roots` before `--` and
`--verify-module` after it, so selectors do not affect upstream dependencies.
Such partial results are never whole-crate evidence. CI uses `cargo verus build` with the same manifest to
also check compilation and proof erasure, then requires the exact complete
production inventory and every semantic contrast group.

The runtime audit rejects project-local trust-introducing specifications and
termination waivers. Exec-loop decreases checks are lightweight and conditional
on callees terminating; they are not an end-to-end CPU bound or normative
correctness proof. See the [auditor index](docs/verus-auditor-index.md) for
standard-to-model notation bridges and the separate implementation-only
`proofs/model_support/` lemmas.

### CI execution profile

Every pull request runs the complete validation profile once. Push validation
runs on `main` and all tags; feature-branch pushes do not duplicate the same
PR's work. Each workflow also supports manual dispatch for branch preflight.
The daily extended fuzz campaign and trusted-main corpus producer remain
unchanged.

The fifteen semantic proof groups and all 66 compiling strong/weaker contrasts
are preserved, but execute in four deterministic, sequential shards rather
than fifteen separately initialized jobs. Each shard has one worker; at most
four run concurrently, under the unchanged 20-minute job cap. The stable
`verify` collector requires the whole proof and every group exactly once,
with matching source/tool/run/attempt identities and individual report/log
hashes. No cached proof result substitutes for execution.

This profile uses 19 workflow jobs per PR, plus the repository's external CLA
check: about 20 check results rather than the former 61 push-plus-PR results.
The three native platforms and six isolated distribution combinations remain.
Workflow policy validates the actual Cargo build invocation and rejects
check-disabling flags; a `--compile` substring in another argument is not
compilation evidence.

Do not use standards documents, sample software, or conformance assets unless
their entry in `source-provenance.json` records the required legal approval.

The [generated fuzz corpus lifecycle](docs/fuzz-corpus-lifecycle.md) documents
bounded validation, owned-copy minimization, exact replay and trusted-main-only
cache promotion for the existing nine targets.

## Runtime coverage

The pinned Linux coverage workflow measures ordinary runtime code, excluding
inline tests/oracles and ghost items. Its ratcheted baseline is **81.80% lines,
76.90% regions and 72.33% branch outcomes**. CI checks aggregate/per-file
non-regression; the proposed 90% line / 85% branch release goals remain unmet.
See [runtime coverage](docs/runtime-coverage.md) for reproduction, scope,
uncovered-code artifacts and the remaining P06 test-improvement work. Public
reader/sink/error regressions and eight isolated mutation checks guard critical
failure contracts without changing decoder behavior.

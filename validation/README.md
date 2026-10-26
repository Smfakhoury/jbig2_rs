# Validation guide

Validation assets stay outside the runtime package. Runtime tests, formal
contracts, source audits, official vectors, differential results, fuzzing and
performance are distinct evidence; passing one does not certify the others.
See [publication readiness](../docs/publication-readiness.md) for the single
revision-qualified numerical snapshot and [repository layout](../docs/repository-layout.md)
for retained records and generated artifacts.

## Asset and evidence policy

| Area | Role |
| --- | --- |
| [Source provenance](source-provenance.json) | Sources, revisions/hashes, licenses, redistribution status and approved use |
| `conformance/` | Official manifest, expected differences, unresolved diagnoses and revision-qualified results/attestation; actual official assets remain external |
| `differential/` | Pinned black-box oracle identities, disagreements and source-bound observations |
| `negative/`, `metamorphic/` | Exact executable Cargo test catalogs and expectations |
| `traceability/` | Provisional requirement-level models/contracts/proofs/tests and review gaps |
| `coverage/` | Reviewed runtime ratchet and source-qualified observations |
| `performance/`, `comparison/` | Live scenarios and the optional competitive-harness guide; generated measurements belong under `target/` |
| `schemas/` | Evidence structure, including the future release attestation schema |

Before adding an external asset, record provenance, approved use and permission
for the specific location. Project publication approval is not approval to
redistribute standards, official vectors or oracle binaries. Do not put private
corpora in public failure artifacts. Schemas/hash checks validate records, not
legal permission, semantic truth or a new execution.

Routine outputs belong under ignored `target/` or CI artifacts, never new
root reports or chronological tracking documents. Superseded coverage,
implementation-tracking and single-run performance/comparison snapshots are
recoverable in Git history and retained CI artifacts, not copied into a source
archive. The coverage baseline, performance scenarios and conformance/
differential observation/expectation/provenance records remain. Source-qualified
observations speak only for their recorded revision; do not refresh them to
appear to attest a new candidate.

## Local correctness and audit checks

```sh
cargo test --all-features
python3 -m pip install -r tools/requirements.txt
python3 -B -m unittest discover -s tools/tests -v
cargo package --locked --list | python3 -B tools/audit_package.py
python3 -B tools/check_distribution.py --toolchain 1.88.0 \
  --output target/distribution/msrv.json
python3 -B tools/check_test_catalogs.py --discover-tests \
  --output target/traceability/test-catalogs.json
python3 -B tools/check_workflows.py \
  --output target/traceability/workflow-policy.json
```

The package audit needs the complete nonempty real archive inventory; empty
stdin is an error. The standard-library-only distribution checker exercises
extracted tests, exact downstream pixels, hidden validation access and
compiler-enforced unsafe prohibition with a weaker guard-only control.
Source restoration is unconditional; see [distribution](../docs/distribution.md).
Broader audit tooling uses development dependencies, never runtime dependencies.

Catalog references are exact `{target, name}` entries, not file-only claims.
Missing/ignored/duplicate/wrong-target tests and empty catalogs fail.
Discovery compiles/lists tests but does not execute them; native CI runs them
separately. Failed checking removes stale success output.

## Official conformance: unresolved is not passed

The [manifest](conformance/itu-t88-2018-manifest.json) pins archive/per-file
identities and exact case membership.
[Expected differences](conformance/expected-differences.json),
[seven-case review](conformance/seven-rejection-review.json) and
[attestation](conformance/attestation.json) retain the source-qualified state.
The reference snapshot has two exact positives, seven unresolved official
positives and one unsupported colour case, with **zero reviewed rejection
credit**. Those outcomes block stronger supported-conformance claims.

After building the repository's `decode_vector` validation adapter, an
authorized local copy can be checked without copying it into the repository:

```sh
python3 -B validation/conformance/run_itu.py \
  --vectors /path/to/extracted-package \
  --decoder target/release/examples/decode_vector \
  --expected-differences validation/conformance/expected-differences.json \
  --output target/conformance/itu-t88-2018-results.json
```

`--vectors` accepts either the extraction parent or
`JBIG2_ConformanceData-A20180829` itself. `--archive /path/to/conformance.zip`
also verifies the original archive hash. Per-decoder timeout is 60 seconds,
configurable with `--timeout-seconds`.

Zero cases, changed/missing files or images, duplicate/unknown classifications,
page/bitmap mismatches, timeouts and unresolved errors return nonzero.
`pending_normative_review` entries pin complete typed errors, stream hashes
and relevant runtime hashes; matching those observations **still fails**.
Changed errors, unexpected success/crash, stale source identity, unknown fields
and invalid JSON also fail closed. Do not relax runtime acceptance/marker/
retention/recovery policy to manufacture a pass.

`tools/verify_provenance.py` can validate truthful blocked records without
executing the corpus. Its success is not a conformance pass.
`--require-exact-conformance` separately rejects recorded nonpassing cases;
official execution remains nonzero for unresolved positives. LF checkout rules
preserve source-byte identities across native hosts. Integration refreshes
diagnosis bindings only with reviewed evidence, never skips the gate.

### Diagnosis and normalization limits

The prior TT1 MMR-truncation explanation was disproved: initial dictionaries
finish before a text Huffman read exhausts the entropy body at file byte 179.
That observation is not a normative cause. The T.88 7.2.4 field-definition versus
7.2.8 worked-example retention conflict still needs owner adjudication.
See the source-bound review for all cases, not a copied progress log.

Independent first-party BMP fixtures cover both palette orders, top-down/
bottom-up orientation, packed-bit and four-byte row padding, plus numeric
pairing for multi-page output. No normalization bug was found in those covered
layouts; that is not a general oracle-equivalence theorem.

Differential oracles remain black-box validation tools with pinned identities
in [`differential/oracles.json`](differential/oracles.json) and separately reviewed
[`expected-differences.json`](differential/expected-differences.json).
An oracle error/warning does not prove an official positive malformed.
The locked Hayro wrapper's single-image API and FNV64 summary cannot establish
full-byte multi-page equality or corroborate an entropy offset it does not
expose. See [comparison](comparison/README.md).

## Formal and native sensitivity

Whole-proof local reports belong in `target/verification/`; canonical CI artifact
filenames remain unchanged. With a fresh pinned report on the same source:

```sh
python3 -B tools/audit_production_coverage.py \
  --verus-json target/verification/verus-production.json \
  --output target/verification/production-proof-coverage-ci.json \
  --require-complete --require-exact
python3 -B tools/check_traceability.py --discover-tests \
  --verus-json target/verification/verus-production.json \
  --output target/traceability/combined-report.json
python3 -B tools/check_contract_mutations.py --toolchain 1.89.0 \
  --output target/coverage/mutations.json
```

The exact-source release gate currently fails on the disclosed MMR capacity
divergence; complete inventory alone does not claim every body is proved.
`update_proof_dispositions.py --output NEW_FILE` creates unreviewed proposals
only; use an ignored `target/` output and do not overwrite reviewed decisions.

Proof audits follow syntax/module/cfg ownership and source hashes, including
executing functions after test items. Semantic controls require a real ordinary
compilation, an accepted original baseline, strong proof rejection and success
with only the selected guarantee removed. Shared positive baselines still cover
each exact target; every strong/weaker query remains separate. Whole proof,
compilation/lifetime/erasure and complete same-run receipts remain mandatory.
See [limitations](../docs/verifier-limitations.md) and
[the auditor index](../docs/verus-auditor-index.md).

Native mutation controls likewise require an exact passing original test,
compiling mutant and failure of that selected test. Zero-test success,
compilation failure, ambiguous sites and survivors are not detection.
Isolated copies, not repository runtime files, are altered.

Approved raw numeric/state oracles, exact typed-error adapters, public metadata/
envelope/resource tests, packed-byte/pixel/scalar comparisons and approved
fixture families provide bounded independent checks. Direct error construction
does not prove a variant is reachable; opaque dictionary/region payloads may
test inspection metadata without being valid entropy streams.
Fresh/reused agreement alone is not a pixel or diagnostic oracle.

Complete standard/model/caller review and seven official positive diagnoses
remain open. **Generated encoded refinement, aggregation and custom-Huffman
fixture expansion remains held.** Do not retry/reconstruct flagged work.
Existing approved raw numeric/metadata tests and fixtures are permitted within
their scope, not implicit consent to new encoders, grammars or family expansion.
Do not add local std/equality specifications, assumptions or waivers.

## Coverage

[Runtime coverage](../docs/runtime-coverage.md) documents pinned reproduction,
the ordinary-source denominator, exact reviewed ratchet, changed-line policy
and remaining gaps. Line/region/branch coverage is not proof coverage.
Meeting numerical goals does not establish all-limit adequacy, close allocator
or capacity defects, or complete P06.

## Bounded fuzzing and replay

The existing campaign has nine targets, including context sequences and two
approved collective-bitmap codec families. Use fresh output directories:

```sh
python3 -B tools/run_fuzz.py run --profile smoke \
  --corpus-dir target/fuzz-corpus --output-dir target/fuzz-run
python3 -B tools/run_fuzz.py replay --targets document_arbitrary \
  --artifact /path/to/reproducer --output-dir target/fuzz-replay
```

The pinned nightly/cargo-fuzz are in
[`config/toolchain.json`](../config/toolchain.json). Smoke uses 60 seconds per
target; extended uses 300, with five-second per-input timeout, 1,024 MiB RSS,
16,384-byte inputs and an additional runner deadline. Only the runner's own
process group is terminated on deadline. Parser errors are expected inputs;
reproducers, target failures, build errors and timeouts fail.

Each successful target must emit one unambiguous positive final execution
counter; successful single-file replay requires exactly one execution.
Missing/zero/malformed/duplicate counters fail even on exit zero. Failed
targets retain original errors, logs and reproducers without fake counters.
Execution counts are not unique cases or coverage.

Seeds originate in first-party fixtures/constants, never official/private
assets. Changed names/bytes, symlinks, oversized/excessive corpus entries fail.
Reports bind source/tool/fixture identities, commands, raw logs and reproducers.
PR/push smoke is isolated; daily extended and explicit main dispatch may use
trusted-main corpus retention only under the workflow predicate. Untrusted PR
code receives neither trusted cache writes nor privileged persistent execution.
Failure artifacts are retained without adding secrets/write permissions.

`tools/check_fuzz_controls.py` injects a benign generated assertion/input in an
isolated copy: baseline passes, failure artifact is captured, exact replay
fails and restoring source makes it pass again. It is a control, not a
production finding. Owned-copy minimization, exact pre/post replay, promotion
rollback, bounded inventory and serialized trusted cache producers are detailed
in [corpus lifecycle](../docs/fuzz-corpus-lifecycle.md).

### Existing structured target boundaries

- **Context:** two four-byte shape controls followed by at most eight opcode/
  u16 triples; existing white/transition-heavy MMR fixture, up to 128×16 pixels
  and three pages. Operations cover standalone/embedded inspection/decode,
  empty globals, reader/sink, malformed input, typed width rejection and scratch
  release, each followed by a valid exact-pixel recovery decode. Up to 256
  bytes supply malformed input. Nonempty symbol globals/other codecs are not
  covered by this target.
- **Codec families:** sixteen shape bytes and at most four opcode/argument
  pairs, with bytes after offset 23 ignored. Existing MMR pattern dictionaries/
  halftone Gray planes and Huffman symbol/text collective bitmaps use a separate
  scalar pixel oracle, not official bytes or third-party codec code.
  Dimensions are bounded to page 1..33×1..9, pattern/symbol 1..9×1..3,
  dictionary counts 1..4 and grid 1..9×1..5. Coverage includes signed fractional/
  rotated/clipped grids, operators/default pixels, text corners/transposition/
  strips, nonempty globals and public recovery/error/limit paths.

Every decoder limit is explicitly set in the fixture. Principal maxima are
width/height/pixels 36/9/324, memory 1 MiB, stream 4 KiB, pages/segments/symbols
1/5/64, per-segment/total references 1/1, reference/nesting depth 2/2, work
262,144, cumulative bitmap 4 KiB and retained storage 64 KiB.
The nesting limit is configured, not an exercised nested-codec rejection.
Ordinary tests replay programs/seeds, prefixes, opcodes, limits and complete
packed pixels including padding; harness/fixture hashes participate in evidence
and cache identity. These are bounded approved families, not arbitrary custom
tables, arithmetic/skip halftone or new refinement/aggregation consent.

## Base/head performance

```sh
python3 -B tools/run_performance.py --base-ref origin/main \
  --output-dir target/performance-comparison
python3 -B tools/check_performance_controls.py \
  --output-dir target/performance-controls
```

The same head driver/generated fixtures link independently built base/head
release libraries with the pinned compiler. At least three paired runs
alternate execution order on the same CPU. Validate every bitmap byte,
dimensions/page numbers and exact work-limit/truncation errors before timing.
Validation, output hashing and owned-result destruction are outside the decode
measurement; raw samples reproduce upper median and nearest-rank p95.

The public corpus is bounded white/transition-heavy MMR, multiple pages, a work
guard and late malformed input, not normative vectors or representative PDFs.
**Hosted timing is informational.** Instability is inconclusive, never a pass.
The existing delegated 20% median policy is reported; p95 is informational.
Local `--enforce-timing` rejects stable regressions and inconclusive results;
hosted workflows deliberately do not enable it. Proposed 5% median/10%
p95/peak-memory release gates still need ratification and a dedicated runner.
Historical 15% release and 3% stability/optimization policies are not silently
superseded.

Isolated controls add real delay and shortened packed output; timing enforcement
must not accept the slowdown, and correctness must reject corrupted output.
No repository production source is changed. Missing samples, wrong outputs and
build failures always fail and retain diagnostics.

Allocator counts/peak bytes, per-case RSS, broader arithmetic/text/refinement/
halftone/PDF corpus and trustworthy dedicated-host evidence remain P09 gaps.
No current aggregate speed, conformance or release-performance claim follows
from this lane or historical measurements. See the
[optional Hayro comparison guide](comparison/README.md) for harness reproduction
and interpretation limits.

## Workflow contract

The policy audit parses YAML with pinned development-only PyYAML and rejects
duplicate/merge keys, custom tags, unpinned remote actions, missing budgets,
unsafe permissions and failure masking. Rust selection is explicit; version-
named wrappers must not silently override requested inputs. Unsupported
local/container actions or reusable jobs need policy review, not a silent skip.
Action pins are immutable identities, not independent approval of upstream code.

Runtime MSRV, development, proof and nightly pins remain distinct.
Native distribution executes rather than merely cross-compiles.
PR validation is not duplicated by feature-push workflows; main/tag pushes and
manual preflight retain their gates. Proof shards retain complete logical
membership, sequential one-worker execution, unchanged concurrency/timeouts and
same-source/run/attempt collection.

Dependency updates require review/rerun of workflow-policy tests. Source-bound
reports and failed-run diagnostics are retained on all native hosts.
Operational workflows do not by themselves complete conformance availability,
performance ratification, branch-protection review or
[release rehearsal](../RELEASING.md).

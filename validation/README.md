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

The production Huffman cache-map catalog uses raw metadata and independent wide
index/distance arithmetic, including compact-index boundaries and sparse slots.
It distinguishes cache storage/retention from source-table or full-stream
fidelity. The factory contracts prove the production minimum-length policy;
forced short-prefix test caches and held encoded fixture families remain
outside that new result.

## Official conformance: unresolved is not passed

The [manifest](conformance/itu-t88-2018-manifest.json) pins archive/per-file
identities and exact case membership.
[Expected differences](conformance/expected-differences.json),
[seven-case review](conformance/seven-rejection-review.json) and
[attestation](conformance/attestation.json) retain the source-qualified state.
The reference snapshot has two exact positives, seven unresolved official
positives and one unsupported colour case, with **zero reviewed rejection
credit**. Those outcomes block stronger supported-conformance claims.

The owner authorized local black-box build/run of the pinned ITU sample program
on 2026-11-01. It reproduces all seven cases and all nine expected pages, but is
not a general correctness oracle: it crashes on an unchanged first-party
Huffman case that Rust and jbig2dec decode with exact literal pixels, and its
TT3 output ignores changes to the declared export count. A TT1-specific
interpretation candidate also breaks that valid case and is rejected, not
shipped. The [seven-case review](conformance/seven-rejection-review.json)
records those input/output controls and their limits. The seven cases remain
unresolved; these observations grant no normative rejection credit.
The owner's separate request on 2026-11-01 at 23:41 UTC authorized local
implementation inspection. Code copying into the project, derivation,
restricted-asset redistribution and protected standards processing remain
outside those consents.

Source inspection and GDB traces now explain the candidate's regression.
The reference builds symbol IDs using the run-code prefix as part of the ID,
rather than treating a decoded literal as its length. The TT1 input therefore
produces reference lengths `[2,2,1]` instead of production `[1,1,0]`; the
unchanged first-party control produces reference `[3,3,3,3]` instead of
production `[2,2,2,2]`. A plus-one change reproduces this difference for those
inputs, but does not preserve the literal contract.

The reference **does consume the final Huffman OOB**. An offset-only diagnostic
matches all three TT1 pages without changing production termination, and still
fails the unchanged first-party control. The earlier proposal's count-based
stopping was unnecessary; it must not be inferred as a required fix.

The other observations also have source-level explanations: the reference uses
bitmap size as a switch and decodes individual MMR symbols without that byte
bound; its encoder writes two zero export integers, which its decoder reads
but ignores; text decoding uses accumulated symbols without selecting the
referred dictionaries' exports. The debug build reproduces all nine original
pages, so these are observations of the successful reference paths, not just
untested source hypotheses. They do not independently adjudicate the vectors'
normative status or justify weakening production acceptance.

Two local diagnostic controls strengthen that explanation without changing
the production decoder or claiming passes for the original streams:

- TT1: changing just two run-code-table metadata bytes to declare literal
  lengths `[2,2,1]` makes both unchanged Rust and jbig2dec reproduce all three
  unchanged expected pages, with no stderr.
- TT3: re-encoding with the reference and correcting the same explicit
  reference, retention and page-override headers in both variants leaves the
  normal encoder's `[0,0]` exports rejected. A GDB override of only the second
  export argument to `3` instead yields the exact unchanged expected page in
  Rust. This is a re-encoded diagnostic, not the archived official stream.

The committed first-party regressions separately exercise literal lengths
`0..31` at five prefix widths, zero-length exclusion, and nonblank pixels for
all four two-bit IDs in both file organizations. All three tests detect the
literal-plus-one mutant and pass after source restoration. No reference source,
encoded diagnostic copies or restricted bitmap assets are committed.

### What the investigation review needs to resolve

This is an evidence review, not a decoder-fix proposal or a request for the
repository owner to certify conformance. The following questions remain open:

| Cases | Observed boundary | Required technical resolution |
| --- | --- | --- |
| `codeStreamTest1_TT1` | Reference symbol IDs incorporate the run-code prefix. Offset-only changes match TT1 but break unchanged literal-code inputs; final OOB is present. | Resolve the official input's normative interpretation without changing the existing literal contract or strip termination. |
| `codeStreamTest1_TT2` | Reference size 1 permits eight bytes for the first MMR symbol, then separate six- and eight-byte symbols in the next class. | Resolve the official input's bitmap-size and collective-bitmap interpretation; extending the declared size is not a validated fix. |
| `codeStreamTest1_TT3`, `TT4`, `TT5`, `TT7`, `codeStreamTest2_TT6` | Reference encoding writes two zero export integers, decoding ignores their meaning, and text uses accumulated symbols despite zero dictionary referrals. | Resolve the official inputs' export/reference interpretation without skipping runs or inventing references. |

Review the observations and the rejection of the proposed shortcut separately
from the still-missing wire-format resolution. The follow-up pins the production
revision, reference executable, compiler and local evidence-summary hash.
Restricted inputs, executable and raw bitmap outputs are not included in the
repository. Their absence means the committed summary is not a substitute for
an independent authorized reproduction.

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

Raw dictionary-dispatch regressions exercise the existing scripted callback
without constructing an encoded refinement/aggregation stream. A required
count of one reaches the refinement callback; direct-coded paths reject an
unexpected count, and refinement rejects missing/zero counts. These tests check
routing, typed failures, resource effects and raw parameter consumption, not
encoded bitmap semantics or resolution of the official disagreements.

Text strips must consume their final out-of-band S terminator even after the
declared instance count is reached; that count is not an entropy-stream
delimiter. Raw scripted regressions preserve the next enclosing dictionary
field, exact pixels and resource effects. A non-terminating S value after the
final instance rejects before further T/ID/refinement fields. The existing
standard-Huffman text fixture includes its terminator and separately rejects
its former unterminated zero-padding form. No encoded refinement/aggregation
fixture or official conformance exception is introduced.

Successful export-run decoding now formally guarantees strictly increasing,
duplicate-free candidate indices in addition to the existing count and bounds.
The raw scripted oracle checks all 2,047 selections over zero through ten
candidates, including leading zero-run pairs, exact work and an unread sentinel.
A compiling duplicate-index mutant is rejected by the order guarantee but
accepted when only the new order postcondition/invariants are removed; the
original bounds, count and resource frame remain. This is not a proof of encoded
run membership or normative fidelity. The separate assembly helper continues to
preserve explicit caller selections, including repeated IDs.

Huffman literal fields carry the existing bit-value/error model through the raw
reader, concrete fixed-width integer and untabled symbol-ID adapters, and the
two enum-dispatch callers. Successful reads have exact values and consumed
extents; invalid widths and EOF preserve the cursor and typed diagnostics.
Independent raw-byte oracles cover 128,000 input cases across all u8 widths and
valid cursors, concrete/enum parity, codec frames and the next field. Six real
compiling wrong-value/error/width/dispatch controls distinguish these contracts
from their original validity-only boundaries. No new encoded family is built;
table-backed Huffman decoding, prepared symbol-ID tables and independent
normative field interpretation remain separate.

Selected Huffman lines now have total project outcomes, not only implications
for successful values. OOB returns without a suffix read; width/EOF errors retain
the exact diagnostic and cursor; successful suffix reads produce the checked
signed result or overflow while retaining the consumed extent. An independent
i128 oracle covers all 1,664,000 raw metadata cases across widths, cursors,
line kinds and signed/output boundaries, with six nonempty outcome classes.
Four compiling faults expose false integer/OOB rejection, wrong read-error
classification and overflow-cursor rollback. The weaker controls retain every
original success-value, buffer, validity and weak error-extent guarantee.
Prefix-to-line selection and full table/stream/normative semantics remain open;
no encoded table or new fixture family is constructed by these tests.

The prefix lookahead helper now proves complete availability and the exact
aligned/unaligned eight-bit window of the original bytes, rather than only a
successful result's extent. An independent scalar bit oracle checks all
3,407,872 byte-window/cursor cases, including exact eight-bit availability,
repeated non-consuming peeks and the following reader operation. Three real
compiling faults discard aligned/next-byte values or reject a complete final
prefix; weaker controls retain the original reader precondition and extent.
This window result does not prove canonical prefix-to-line lookup selection or
full table/stream fidelity.

For a populated cache hit, the table decoder now composes the input-indexed
cache entry with the stored line and its exact value/OOB/error/cursor result.
Malformed cached indices or excessive prefix lengths reject without consuming
input; read/overflow errors preserve the defined prefix/suffix extent. The
closed project model keeps private table fields opaque, with its definition
proved in the owning module. Four compiling wrong-slot/line/length/rejection
controls retain the original reader-validity boundary in their weaker forms.
The existing raw fixed-width table construction is checked in 524,288 scalar
cases; 368,640 raw metadata cases cover cached line/result/error frames and
cache-miss behavior. No encoded custom-table fixture is constructed.
Canonicality of generated cache entries, fallback prefix selection and full
table/stream normative fidelity remain separate, explicitly unproved claims.

Fallback prefix decoding now has complete input/table result and cursor state:
the actual bits advance the checked wide prefix, select the first matching
length bucket and ordered line, and compose the exact suffix/error outcome.
The public table decoder selects the cached or fallback state without narrowing
its original reader precondition. Closed model definitions are proved in their
owning module; no alternative executable decoder or trust assumption is added.
Independent u128 assignment and 256-Boolean prefix/borrow oracles check 138,240
standard-table input states through both dispatch modes and direct fallback,
plus exact-count long-prefix/raw metadata cases through 255 bits. Six compiling
faults target ignored bits, wrong order/bucket, EOF classification, lost wide
limbs and false fallback rejection. Earlier controls remove equivalent new
result clauses as well, retaining their original reader/success/frame bounds.
These are operational prefix/table results; cache construction canonicality,
independent table/parameter fidelity and full JBIG2 stream semantics remain
separate. Tests use existing tables and raw numeric metadata, not new encoded
custom-table or held codec-family fixtures.

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
  --output target/traceability/mutations.json
```

The former MMR capacity omission is closed by the exact-body proof and separately
reviewed unconstrained getter. Exactness still rejects runtime/proof substitutions
or incomplete functions; complete inventory does not claim every body is proved
or remove the reviewed standard-library/callback boundaries.
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

Large checkers can expose explicit, disjoint case partitions without adding
another script or CI job. The document checker keeps its complete case set for
a standalone invocation; the runner passes a named `--case-group` and the
collector requires that exact group marker, ordered case IDs, target functions
and baseline coverage. Its metadata/quota and byte-layout partitions run on
different existing shards. Every original strong/weaker query is retained;
logical group count is not CI check count, and a partial/canceled producer
still cannot support complete acceptance.

Native mutation workspaces enable isolated incremental compilation even when
the host CI disables it. Every modified source still goes through Cargo and
the compiler-selected test executable; this does not reuse a mutant result or
change mutation counts, failure requirements, or CI time limits.

The complete 49-group profile at head `2991b46` (run 41334856554, attempt 1)
was rejected because shard 9's actual combined cost was 1,066.333 seconds,
above the unchanged 1,050-second guard. All sixteen complete producer receipts
are retained; passing individual jobs is not profile acceptance. Reader-error
and halftone-header groups move to existing shards 0 and 5, leaving the growing
Huffman group alone on shard 9. No group, contrast, producer or guard is removed.
Replaying those recorded costs projects a 725.727-second maximum; that
projection is not a new run. Fresh complete current-head acceptance is required.

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
manual preflight retain their gates. The existing whole-proof job executes its
two assigned contrast groups serially after unfiltered verification; remaining
groups use six sequential one-worker shards and the spare serial capacity of
the existing collector job. Its three complete groups run before aggregation
and produce a separately required, same-run receipt. The collector requires
every configured group exactly once across these roles, with unchanged
job/concurrency/time limits and same-source/run/attempt identities.

Native mutation sensitivity runs in the existing Linux native CI job and is
retained in its traceability artifact, not the LLVM coverage artifact. Both
lanes remain required; no mutation or coverage threshold is removed.

Dependency updates require review/rerun of workflow-policy tests. Source-bound
reports and failed-run diagnostics are retained on all native hosts.
Operational workflows do not by themselves complete conformance availability,
performance ratification, branch-protection review or
[release rehearsal](../RELEASING.md).

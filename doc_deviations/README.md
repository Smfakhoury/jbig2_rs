# Standards-tested deviations

| ID | Class | Standard | Required behavior | Test input / assertion | Observed deviation (why) | Production code | Test source |
|---|---|---|---|---|---|---|---|
| [DEV-952C7B7561](#reproduce-dev-952c7b7561) | Code deviation | t88-201808.txt § 6.5.8.2.3 | Huffman symbol-dictionary refinement/aggregation decoding is intended to preserve the dictionary-specific fixed-width symbol-ID interpretation supplied by T.88 6.5.8.2.3, rather than substituting the transmitted symbol-ID table header used by an ordinary text-region segment. | All imported Bitmap values are constructed by production bitmap_from_packed_bytes as 8x1 images with stride=1 and length=1, asserted before each call. | Huffman aggregate dictionaries read an untransmitted symbol-ID table | [`symbol_text::dictionary::decode_symbol_dictionary`](../src/symbol_text/dictionary.rs#L43) | [Cited check in `dictionary_dispatch.rs`](support/tests/DEV-952C7B7561/files/src/symbol_text/tests/dictionary_dispatch.rs#L231) |

## Run coverage

All **660 extracted production-function inference items** completed, retaining
1,348 source specifications. Of these, 1,341 had valid source grounding across
650 subjects and were assigned to 25 completed SOMA mapping sessions.
The mappings contain 223 implemented associations, 7 intentional deviations,
17 uncertain associations and the single reproduced deviation above.

This is not an exhaustive conformance result: 216 inference items retain
review qualifications, 7 ungrounded specs were excluded, and only 323 source
clauses have published standard associations. Native validation covers four
finite baseline clauses plus the reproduced candidate, not every mapped clause.

For this finding, the exact failure site is
[`huffman.rs:178-184`](../src/symbol_text/dictionary/huffman.rs#L178-L184);
the actual failing assertion is
[`dictionary_dispatch.rs:287-291`](support/tests/DEV-952C7B7561/files/src/symbol_text/tests/dictionary_dispatch.rs#L287-L291).
The 119 failing encoded fixtures represent **one underlying issue**, not 119
bugs. Production sources and the original pilot database were left unchanged.

[Detailed coverage and qualifications](support/workflow-status.json) |
[All mapping evidence](support/mappings.json)

## Reproduce a finding

Run commands from the repository root with Python 3.10+ and the repository's native build dependencies. The included runner uses only the Python standard library; no DeepSpec installation is needed.

The exported folder is self-contained: replay does not need SOMA, the Core specification database or a `.deepstar` directory.

It targets JBIG2 revision
`8626788174944e2fa9be0dbf1c3d877c36218fc1`.

The recorded replay also collects LLVM branch coverage. Install its native tools
if they are not already available:

```sh
rustup toolchain install nightly-2026-10-12 --component llvm-tools-preview
cargo install cargo-llvm-cov --version 0.9.1 --locked
```

Each command selects one recipe and builds in a temporary copy; it does not modify your checkout. **Reproduced means the test failed in the recorded way**, not that the underlying test passed. The runner reports `reproduced` and exits 0. Output directories must be new; choose another `--output` path when repeating a run.

<a id="reproduce-dev-952c7b7561"></a>
<details>
<summary>DEV-952C7B7561 — Huffman aggregate dictionaries read an untransmitted symbol-ID table</summary>

**Run this finding:**

```sh
python doc_deviations/support/run_tests.py --repo . --finding DEV-952C7B7561 --output rfc-results/DEV-952C7B7561
```

For a plain Cargo reproduction without Python or coverage tooling, use a
**disposable checkout** at the revision above. Copy this `doc_deviations/`
folder into its root, then run:

```sh
cp doc_deviations/support/tests/DEV-952C7B7561/files/src/symbol_text/tests/dictionary_dispatch.rs src/symbol_text/tests/dictionary_dispatch.rs
cargo test --lib symbol_text::tests::dictionary_dispatch::huffman_dictionary_aggregation_preserves_standard_fixed_symbol_ids -- --exact --nocapture
```

The retained file preserves the original tests and adds the regression. This
native test is expected to **fail** with the recorded `UnexpectedEof`; the copy
replaces the test file, so do not use a checkout containing uncommitted edits.


**Standard:** t88-201808.txt § 6.5.8.2.3

**Required behavior:** Huffman symbol-dictionary refinement/aggregation decoding is intended to preserve the dictionary-specific fixed-width symbol-ID interpretation supplied by T.88 6.5.8.2.3, rather than substituting the transmitted symbol-ID table header used by an ordinary text-region segment.

**Observed deviation:** Huffman aggregate dictionaries read an untransmitted symbol-ID table

All imported Bitmap values are constructed by production bitmap_from_packed_bytes as 8x1 images with stride=1 and length=1, asserted before each call. Counts range from 1 through 32 with valid in-range IDs, one new/exported 16x1 symbol, two unrefined instances, default resource limits and no context reuse/custom tables. The symbol-dictionary payload is an API-level input; no file/transport wrapper is required by this entry. No private storage mutation, out-of-contract helper invocation or production source edit is used.

**Recorded failure** (matched by the replay runner):

```text
count=32 ids=[31, 12]: expected 16x1 [de, 19], got Decode(Malformed(UnexpectedEof { offset: 69, needed: 4, remaining: 3 }))
```

### Cited test code

These are the fixture and checks cited for this finding, not other tests to run.

[`src/symbol_text/tests/dictionary_dispatch.rs:L252-L252`](support/tests/DEV-952C7B7561/files/src/symbol_text/tests/dictionary_dispatch.rs#L252)

```text
let result = decode_symbol_dictionary(&SymbolDictionaryInput {
```

[`src/symbol_text/tests/dictionary_dispatch.rs:L231-L231`](support/tests/DEV-952C7B7561/files/src/symbol_text/tests/dictionary_dispatch.rs#L231)

```text
assert_eq!(bitmap.bytes().len(), 1);
```

[`src/symbol_text/tests/dictionary_dispatch.rs:L286-L286`](support/tests/DEV-952C7B7561/files/src/symbol_text/tests/dictionary_dispatch.rs#L286)

```text
assert_eq!(checked, 119);
```

<details>
<summary>Build steps and packaged files</summary>

The runner applies these files **before** the build steps. Helper scripts and patches are setup dependencies, not separate reproductions.

- [`src/symbol_text/tests/dictionary_dispatch.rs`](support/tests/DEV-952C7B7561/files/src/symbol_text/tests/dictionary_dispatch.rs)
- [`tools/deepspec_fixed_dictionary_ids.py`](support/tests/DEV-952C7B7561/files/tools/deepspec_fixed_dictionary_ids.py)

**Build commands** (in order):

```sh
python3 tools/deepspec_fixed_dictionary_ids.py build
```

**Exact test invocation:**

```sh
python3 tools/deepspec_fixed_dictionary_ids.py run
```

[Machine-readable recipe](support/tests/DEV-952C7B7561/recipe.json)

</details>

<details>
<summary>Why this is a deviation and supporting evidence</summary>

T.88 (08/2018) 6.5.8.2 sends multi-instance dictionary aggregates to the text decoder with Table 17 parameters. Section 6.5.8.2.3 supplies implicit equal-width index codes, not a 7.4.3.1.7 transmitted table header. Independent inspection of Tables B.1/B.2/B.4/B.6/B.8/B.11 confirms the fixture's DH=1, DW=16, aggregation count=2, initial and strip DT=1, FS=0, DS=1, OOB markers and export runs. The declared imported+new symbol total determines the fixed ID width; both instance IDs address already available imported bitmaps. RI=0 means no refinement arithmetic payload is needed. The expected output is the concatenation of the two original eight-pixel rows, derived from pre-state, not production output. Host replay reaches instances>1 and prepare_symbol_ids! on all 119 inputs (the retained LLVM branch receipt counts 119 true, 0 false at line 178). It rejects every complete encoded challenge as UnexpectedEof while trying to consume the extra table. The import-only control passes. This is a finite reproduced decoder deviation, not an oracle derived from agreement between implementation and its Verus model. The original per-case route marker was truncated from the host's output tail; the retained failure lines, exact native case and branch-coverage receipt still attribute the executed failure. Successful downstream aggregation is not observed, and removing this read alone is not claimed to repair all table bindings.

[Standards quote, production lines, and provenance](support/evidence.md#dev-952c7b7561)

</details>

</details>

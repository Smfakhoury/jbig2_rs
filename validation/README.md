# Validation

Validation assets are separated from the published crate.

- `conformance/` is reserved for legally approved normative vectors and their
  expected bitmap hashes.
- `differential/` records pinned independent oracles and every disagreement.
- `negative/` covers truncation, malformed lengths and references, extreme
  values, graph failures, and corrupted coding payloads.
- `metamorphic/` checks equivalent organizations, operator identities, and
  deterministic decoding.
- `performance/` measures scaling, malformed-input worst cases, allocations,
  and peak memory against owner-approved thresholds.

No external vector may be added until `source-provenance.json` contains its
source, revision, hash, license, redistribution status, and approved use.

The official ITU assets remain outside this repository. For an approved local
copy, build `decode_vector` and run:

```text
python3 validation/conformance/run_itu.py \
  --vectors /path/to/extracted-package \
  --decoder target/release/examples/decode_vector \
  --expected-differences validation/conformance/expected-differences.json \
  --output validation/conformance/itu-t88-2018-results.json
```

`--vectors` accepts either the parent extraction directory or the
`JBIG2_ConformanceData-A20180829` directory itself. The runner checks the exact
file set, sizes, and hashes against the committed manifest before decoding.
Use `--archive /path/to/conformance.zip` to check the original archive hash too.
Each decoder invocation has a 60-second timeout, configurable with
`--timeout-seconds`. Zero cases, missing assets, changed expectations and
unresolved failures exit nonzero. Reviewed rejections are reported separately
from exact-output passes and do not establish full conformance.

The gate regression tests use generated fixtures, not ITU assets:

```text
python -B -m unittest discover -s tools/tests -v
cargo package --locked --list | python tools/audit_package.py
```

The package audit requires a complete nonempty runtime inventory and rejects
unapproved entries; invoking it without a package list is an error.

`validation/differential/run_oracles.py` runs pinned external executables as
black-box processes. It records only versions, binary and normalized
diagnostic hashes, output hashes, and classifications; it never copies oracle
source or vector content into the repository. Pass `--poppler-revision` when
the executable was built from a pinned source revision. The committed
`results.json` records the versioned system comparison, while
`pinned-poppler-results.json` records the exact approved source revision.

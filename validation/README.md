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

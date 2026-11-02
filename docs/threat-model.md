# JBIG2 decoder threat model

## Scope and assets

The decoder processes attacker-controlled standalone JBIG2 files and embedded
PDF page/global segment streams. Assets to protect are:

- process memory integrity and availability;
- caller-provided page sinks and surrounding document state;
- CPU time and service availability;
- isolation between reusable global segments and page-local segments;
- deterministic error classification and audit evidence.

The decoder does not treat document authenticity, confidentiality, or visual
equivalence to an original source image as security properties.

## Trust boundaries

All input bytes, dimensions, segment numbers, lengths, references, coding
tables, arithmetic contexts, symbols, and page coordinates are untrusted.
`Limits` is trusted caller policy. Rust's standard library and the pinned
toolchains are inside the implementation trust boundary. Differential native
oracles are validation-only and are never linked into the crate.

Proof-only code, standards documents, test vectors, fuzz corpora, native
oracles, and adapters are outside the published package. Reusable PDF global
state is immutable after validation.

The controls below describe required behavior, not a claim of complete
normative, no-panic or resource-accounting proof. Compiler-generated derives
and standard-library internals remain trusted; the crate's unsafe prohibition
does not prove binary-wide unsafe absence. Post-reservation capacity checks do
not establish allocator usable-size or total process-memory bounds; complete
functional coverage is still partial. See
[verification limitations](verifier-limitations.md) and
[release validation](publication-readiness.md).

## Threats and controls

### Decompression and dimension bombs

Inputs can encode enormous pages or many intermediate bitmaps in a small
stream. Width, height, pixels, cumulative bitmap bytes, retained state, and
decoder-managed memory are checked before allocation. Multiplication and
stride calculations use checked arithmetic. Allocations use fallible reserve
operations and report `DecodeError::Allocation`.

### Segment and symbol amplification

Segment, page, symbol, reference, and per-segment reference counts are bounded.
Custom Huffman table lines and IAID context counts share those resource
budgets. Collection growth is fallible.

### Reference graph attacks

Every reference number must be lower than the referring segment number.
Standalone streams reject duplicates, missing references, non-increasing
segment order, page-crossing references, invalid type relationships, and graph
depth beyond policy. Embedded page streams resolve external references only
through explicit immutable `GlobalSegments`; local/global number collisions
are rejected.

Retention flags are validated so state cannot be discarded before its final
local reference. Traversals are iterative and depth-bounded.

### Arithmetic stream attacks

Arithmetic state indexes are limited to the normative 47-entry Qe table.
BYTEIN lookahead is bounded by the segment payload, marker bytes remain
unconsumed, and physical truncation is not treated as marker fill. Integer and
IAID loops have normative fixed bounds and checked context allocation.

### Huffman attacks

Canonical assignment rejects empty, oversubscribed, misordered, duplicate, or
non-partitioning tables. Prefix and suffix reads are bounded. LOW, HIGH, and
OOB lines are explicit, and custom-table padding must be zero.

### MMR attacks

Mode and run prefixes must be table-defined. Extension codes are rejected.
Every pass, horizontal, and vertical operation is bounds-checked and must make
semantic progress. Per-row work is bounded by dimensions and work policy.
Required EOFB markers are exact and are never found by scanning arbitrary
trailing data.

### Bitmap corruption

Bitmap indexing derives from checked width, height, stride, and coordinates.
Region composition clips only where the standard requires clipping, applies a
closed set of combination operators, and writes only inside the destination
bitmap. Padding bits are deterministic.

### CPU denial of service

Input bytes, work units, graph depth, pixels, symbols, and segments are
independently bounded. Parsing and decoding loops either consume input, advance
an output coordinate, reduce a finite counter, or return an error.

### PDF global/page confusion

Global streams must contain only globally associated segments. Page streams
cannot resolve missing references implicitly. Callers must pass the validated
global state explicitly, and page-associated segments cannot be referenced
across page numbers.

### Truncation, trailing bytes, and unsupported syntax

Failed bounded reads preserve cursor state. Fixed-size payloads require exact
lengths. Random-access files require a final end-of-file header; bytes after a
terminal end-of-file segment are rejected. Necessary unknown extensions and
colour syntax return typed unsupported errors rather than being ignored.

## Validation evidence

Validation combines:

- Cargo, Clippy, test, documentation, and package audits on Linux, Windows, and
  macOS;
- deterministic production proof coverage with zero incomplete functions and
  no unreviewed assumptions;
- external conformance vectors with pinned identities, bitmap hashes and
  separately recorded strict rejections/unsupported outcomes;
- two pinned independent differential oracles and reviewed disagreements;
- truncation, malformed graph, coding-table, extreme-value, and corruption
  tests;
- bounded arbitrary and structured fuzzing with recorded toolchains and
  duration;
- performance and peak-memory results at policy limits;
- an audited proof-free package and immutable release attestation.

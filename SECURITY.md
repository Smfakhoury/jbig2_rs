# Security policy

## Supported versions

No released version is currently supported. The crate is unpublished and
under active development.

## Reporting a vulnerability

Report suspected vulnerabilities privately through the repository owner's
security reporting channel. Do not attach proprietary documents, purchased
standards, or non-redistributable test assets.

## Security posture

JBIG2 input is always treated as hostile. Production code forbids unsafe Rust
and must not panic on malformed input. All dimensions, offsets, lengths,
reference counts, allocations, retained state, and work are subject to checked
arithmetic and explicit limits.

The threat model includes:

- decompression bombs and excessive page dimensions;
- integer overflow in strides, offsets, and segment lengths;
- cyclic, missing, duplicate, and pathological reference graphs;
- malformed arithmetic, Huffman, and MMR streams;
- symbol-dictionary and refinement amplification;
- CPU amplification and non-progressing loops;
- confusion between PDF global and page segment streams;
- out-of-bounds bitmap reads and writes.

Unsupported syntax must produce a typed error. Malformed input, unsupported
features, resource exhaustion, allocation failure, and I/O failure are
distinct failure classes.


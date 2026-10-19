<!-- BEGIN MICROSOFT SECURITY.MD V1.0.0 BLOCK -->

## Security

Microsoft takes the security of our software products and services seriously, which
includes all source code repositories in our GitHub organizations.

**Please do not report security vulnerabilities through public GitHub issues.**

For security reporting information, locations, contact information, and policies,
please review the latest guidance for Microsoft repositories at
[https://aka.ms/SECURITY.md](https://aka.ms/SECURITY.md).

<!-- END MICROSOFT SECURITY.MD BLOCK -->

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

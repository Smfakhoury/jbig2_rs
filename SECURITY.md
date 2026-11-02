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

The 0.1 series is an early release under active development. Use the latest
patch release and review the limitations below.

## Reporting a vulnerability

Use the Microsoft reporting guidance above. Do not attach proprietary documents, purchased
standards, or non-redistributable test assets.

## Security posture

JBIG2 input is always treated as hostile. Crate-authored production code forbids
unsafe Rust. The requirement is to return typed failures rather than panic on
malformed input, with checked arithmetic and explicit limits for dimensions,
offsets, lengths, references, allocations, retained state and work.

These are implementation requirements, not a binary-wide no-unsafe/no-panic or
complete resource-accounting proof. Compiler-generated derives, standard-library
internals and Rust/Verus/vstd toolchains remain trust boundaries. Formal coverage remains partial; see
[verification limitations](docs/verifier-limitations.md) and
[publication readiness](docs/publication-readiness.md). The decoder is not yet
recommended for untrusted production documents.

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

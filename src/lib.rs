#![forbid(unsafe_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]
//! Memory-safe JBIG2 decoding primitives.
//!
//! The crate is incomplete and unpublished. Its supported milestone is defined
//! by the repository's machine-readable support matrix.

#[cfg(verus_keep_ghost)]
extern crate vstd;

#[cfg(verus_keep_ghost)]
#[rustfmt::skip]
#[path = "../specs/mod.rs"]
mod spec;

#[cfg(verus_keep_ghost)]
#[macro_use]
#[rustfmt::skip]
#[path = "../proofs/production_impl/readers.rs"]
mod reader_proofs;

#[cfg(verus_keep_ghost)]
#[macro_use]
#[rustfmt::skip]
#[path = "../proofs/production_impl/file_header.rs"]
mod file_header_proofs;

#[cfg(verus_keep_ghost)]
#[macro_use]
#[rustfmt::skip]
#[path = "../proofs/production_impl/bitmap.rs"]
mod bitmap_proofs;

#[cfg(verus_keep_ghost)]
#[macro_use]
#[rustfmt::skip]
#[path = "../proofs/production_impl/document.rs"]
mod document_proofs;

#[cfg(verus_keep_ghost)]
#[macro_use]
#[rustfmt::skip]
#[path = "../proofs/production_impl/decode.rs"]
mod decode_proofs;

#[allow(dead_code)]
mod arithmetic;
mod bitmap;
mod decode;
mod document;
mod error;
mod file_header;
#[allow(dead_code)]
mod generic_region;
#[allow(dead_code)]
mod halftone;
#[allow(dead_code)]
mod huffman;
mod limits;
#[allow(dead_code)]
mod mmr;
mod reader;
#[allow(dead_code)]
mod segment_data;
#[allow(dead_code)]
mod segment_header;
#[allow(dead_code)]
mod symbol_text;

pub use bitmap::Bitmap;
pub use decode::{DecodedDocument, Page, PageSink};
pub use document::{Decoder, DecoderContext, DocumentInfo, GlobalSegments, PageInfo};
pub use error::{
    DecodeError, GenericRegionErrorKind, HalftoneErrorKind, HuffmanErrorKind, IoErrorKind,
    LimitExceeded, LimitKind, MalformedError, MmrErrorKind, UnsupportedFeature,
};
pub use file_header::{inspect_file_header, FileHeader, Organization};
pub use limits::Limits;

#[cfg(any(fuzzing, jbig2_validation))]
#[path = "../validation/runtime_access.rs"]
pub mod runtime_access;

#[cfg(verus_keep_ghost)]
#[rustfmt::skip]
#[path = "../proofs/shared/views.rs"]
mod views;

#[cfg(verus_keep_ghost)]
#[rustfmt::skip]
#[path = "../proofs/shared/invariants.rs"]
mod invariants;

#[cfg(verus_keep_ghost)]
#[rustfmt::skip]
#[path = "../proofs/spec_properties/readers.rs"]
mod reader_properties;

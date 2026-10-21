use std::io::Read;

use crate::bitmap::CombinationOperator;
use crate::document::{
    parse_embedded_document, parse_standalone_document, read_bounded_input, Decoder,
    DecoderContext, DocumentInfo, GlobalSegments, PageInfo, ParsedData, ParsedSegment,
};
use crate::generic_region::{
    decode_arithmetic_refinement, decode_generic_region_segment_into,
    decode_generic_region_segment_reused, parse_generic_region_header,
    parse_refinement_region_header, GenericRegionError, GenericTemplate, RefinementParameters,
};
use crate::halftone::{
    decode_halftone_region, decode_pattern_dictionary, halftone_error_into_decode,
    new_gray_mmr_state, new_halftone_arithmetic_state, new_pattern_arithmetic_state,
    new_pattern_mmr_state, parse_halftone_header, parse_pattern_dictionary_header, WorkBudget,
};
use crate::huffman::{HuffmanError, HuffmanTable};
use crate::mmr::MmrScratch;
use crate::reader::ByteReader;
use crate::segment_data::parse_region_info;
use crate::segment_header::SegmentType;
use crate::symbol_text::{
    decode_symbol_dictionary, decode_text_region, DictionaryContexts, SymbolCodebook,
    SymbolDictionaryInput, SymbolTextError, TextRegionInput,
};
use crate::{
    Bitmap, DecodeError, GenericRegionErrorKind, Limits, MalformedError, UnsupportedFeature,
};
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg(verus_keep_ghost)]
#[rustfmt::skip]
#[path = "../specs/decode.rs"]
mod decode_spec;

#[cfg(verus_keep_ghost)]
#[rustfmt::skip]
#[path = "../proofs/spec_properties/decode.rs"]
mod decode_properties;

#[cfg(verus_keep_ghost)]
#[rustfmt::skip]
#[path = "../proofs/shared/decode.rs"]
mod decode_shared;

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn generic_error_into_decode(error: GenericRegionError) -> DecodeError {
    let kind = match error {
        GenericRegionError::Decode(error) => return error,
        GenericRegionError::ReservedFlags(flags) => GenericRegionErrorKind::ReservedFlags(flags),
        GenericRegionError::InvalidFlags(flags) => GenericRegionErrorKind::InvalidFlags(flags),
        GenericRegionError::InvalidAdaptivePixel { number, x, y } => {
            GenericRegionErrorKind::InvalidAdaptivePixel { number, x, y }
        }
        GenericRegionError::InvalidSkipDimensions => GenericRegionErrorKind::InvalidSkipDimensions,
        GenericRegionError::InvalidArithmeticTerminator => {
            GenericRegionErrorKind::InvalidArithmeticTerminator
        }
        GenericRegionError::UnexpectedArithmeticMarker { offset, code } => {
            GenericRegionErrorKind::UnexpectedArithmeticMarker { offset, code }
        }
        GenericRegionError::RowCountMismatch { expected, actual } => {
            GenericRegionErrorKind::RowCountMismatch { expected, actual }
        }
        GenericRegionError::ContextIndex { index, count } => {
            GenericRegionErrorKind::ContextIndex { index, count }
        }
    };
    DecodeError::Malformed(MalformedError::InvalidGenericRegion(kind))
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn symbol_text_error_into_decode(error: SymbolTextError) -> DecodeError {
    match error {
        SymbolTextError::Decode(error) => error,
        SymbolTextError::InvalidFlags(reason)
        | SymbolTextError::InvalidTableAssociation(reason)
        | SymbolTextError::InvalidCount(reason)
        | SymbolTextError::InvalidEntropyValue(reason)
        | SymbolTextError::UnexpectedOutOfBand(reason)
        | SymbolTextError::InvalidState(reason) => {
            DecodeError::Malformed(MalformedError::InvalidSymbolText(reason))
        }

        SymbolTextError::InvalidDimensions => {
            DecodeError::Malformed(MalformedError::InvalidSymbolDimensions)
        }
        SymbolTextError::SymbolIdOutOfRange { id, available } => {
            DecodeError::Malformed(MalformedError::SymbolIdOutOfRange { id, available })
        }
        SymbolTextError::InvalidSymbolLengthRun => DecodeError::Malformed(
            MalformedError::InvalidSymbolText("invalid symbol-code length run"),
        ),
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn huffman_error_into_decode(error: HuffmanError) -> DecodeError {
    match error {
        HuffmanError::Decode(error) => error,
        HuffmanError::InvalidTable(kind) => {
            DecodeError::Malformed(MalformedError::InvalidHuffman(kind))
        }
    }
}

/// One fully decoded page.
#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub struct Page {
    pub(crate) number: u32,
    pub(crate) bitmap: Bitmap,
}

impl Page {
    /// One-based page number.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == crate::views::decoded_page_number(self),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub const fn number(&self) -> u32 {
        #[cfg(verus_keep_ghost)]
        production_decoded_page_fields!(self);
        self.number
    }

    /// Decoded page bitmap.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == decode_spec::page_bitmap(self),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub const fn bitmap(&self) -> &Bitmap {
        #[cfg(verus_keep_ghost)]
        production_decode_page_fields!(self);
        &self.bitmap
    }

    /// Consumes the page and returns its bitmap.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures
            crate::views::bitmap_width(&out)
                == crate::views::bitmap_width(decode_spec::page_bitmap(&self)),
            crate::views::bitmap_height(&out)
                == crate::views::bitmap_height(decode_spec::page_bitmap(&self)),
            crate::views::bitmap_stride(&out)
                == crate::views::bitmap_stride(decode_spec::page_bitmap(&self)),
            crate::views::bitmap_data(&out)
                == crate::views::bitmap_data(decode_spec::page_bitmap(&self)),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub fn into_bitmap(self) -> Bitmap {
        #[cfg(verus_keep_ghost)]
        {
            production_decode_page_fields!(&self);
            proof! {
                crate::views::bitmap_fields(&self.bitmap);
            }
        }
        self.bitmap
    }
}

/// Fully collected decoded document.
#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub struct DecodedDocument {
    info: DocumentInfo,
    pages: Vec<Page>,
}

impl DecodedDocument {
    /// Validated document metadata.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == decode_spec::document_info(self),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub const fn info(&self) -> &DocumentInfo {
        #[cfg(verus_keep_ghost)]
        production_decode_document_fields!(self);
        &self.info
    }

    /// Decoded pages in stream order.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out@ == decode_spec::document_pages(self),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub fn pages(&self) -> &[Page] {
        #[cfg(verus_keep_ghost)]
        production_decode_document_fields!(self);
        &self.pages
    }
}

/// Streaming destination for completed pages.
pub trait PageSink {
    /// Receives ownership of one completed page.
    ///
    /// # Errors
    ///
    /// Returns a typed error if the sink cannot accept the page.
    fn write_page(&mut self, page: Page) -> Result<(), DecodeError>;
}

impl<F> PageSink for F
where
    F: FnMut(Page) -> Result<(), DecodeError>,
{
    fn write_page(&mut self, page: Page) -> Result<(), DecodeError> {
        self(page)
    }
}

impl Decoder {
    /// Decodes a standalone document into owned pages.
    ///
    /// # Errors
    ///
    /// Returns a typed malformed, unsupported, resource, allocation, I/O, or
    /// sink error.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub fn decode(&self, input: &[u8]) -> Result<DecodedDocument, DecodeError> {
        self.context().decode(input)
    }

    /// Reads a bounded standalone document and decodes it into owned pages.
    ///
    /// # Errors
    ///
    /// Returns a typed malformed, unsupported, resource, allocation, or I/O
    /// error.
    pub fn decode_reader<R: Read>(&self, reader: &mut R) -> Result<DecodedDocument, DecodeError> {
        let bytes = read_bounded_input(reader, self.limits())?;
        self.context().decode(&bytes)
    }

    /// Decodes a contiguous embedded page stream without external globals.
    ///
    /// # Errors
    ///
    /// Returns a typed error for malformed, unresolved, unsupported, or
    /// over-limit input.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub fn decode_embedded(&self, input: &[u8]) -> Result<DecodedDocument, DecodeError> {
        self.context().decode_embedded(input)
    }

    /// Decodes a contiguous embedded page stream with validated globals.
    ///
    /// # Errors
    ///
    /// Returns a typed error for malformed, unresolved, unsupported, or
    /// over-limit input.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub fn decode_embedded_with_globals(
        &self,
        input: &[u8],
        globals: &GlobalSegments,
    ) -> Result<DecodedDocument, DecodeError> {
        self.context().decode_embedded_with_globals(input, globals)
    }

    /// Decodes a standalone document and emits each completed page.
    ///
    /// # Errors
    ///
    /// Returns a typed malformed, unsupported, resource, allocation, I/O, or
    /// sink error.
    pub fn decode_with_sink<S: PageSink>(
        &self,
        input: &[u8],
        sink: &mut S,
    ) -> Result<DocumentInfo, DecodeError> {
        let (info, segments) = parse_standalone_document(input, self.limits())?;
        let initial_work = structural_work(input.len(), &segments)?;
        let mut scratch = MmrScratch::default();
        decode_segments(
            &segments,
            &info,
            initial_work,
            0,
            self.limits(),
            &mut scratch,
            None,
            sink,
        )?;
        Ok(info)
    }

    /// Reads a bounded standalone document and emits each completed page.
    ///
    /// # Errors
    ///
    /// Returns a typed malformed, unsupported, resource, allocation, I/O, or
    /// sink error.
    pub fn decode_reader_with_sink<R: Read, S: PageSink>(
        &self,
        reader: &mut R,
        sink: &mut S,
    ) -> Result<DocumentInfo, DecodeError> {
        let bytes = read_bounded_input(reader, self.limits())?;
        self.decode_with_sink(&bytes, sink)
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn decode_embedded_internal(
        &self,
        input: &[u8],
        globals: Option<&GlobalSegments>,
        mmr_scratch: &mut MmrScratch,
    ) -> Result<DecodedDocument, DecodeError> {
        let (info, segments) = parse_embedded_document(input, self.limits(), globals)?;
        let mut collector = PageCollector::new(self.limits());
        let global_bytes = match globals {
            Some(state) => {
                u64::try_from(state.bytes().len()).map_err(|_| MalformedError::IntegerOverflow)?
            }
            None => 0,
        };
        let input_work = structural_work(input.len(), &segments)?
            .checked_add(global_bytes)
            .ok_or(MalformedError::IntegerOverflow)?;
        let global_resources = match globals {
            Some(state) => Some(decode_global_resources(state.bytes(), self.limits())?),
            None => None,
        };
        decode_segments_collected(
            &segments,
            &info,
            input_work,
            global_bytes,
            self.limits(),
            mmr_scratch,
            global_resources,
            &mut collector,
        )?;
        Ok(DecodedDocument {
            info,
            pages: collector.pages,
        })
    }
}

impl DecoderContext<'_> {
    /// Decodes one standalone document using retained scratch capacity.
    ///
    /// # Errors
    ///
    /// Returns the same typed errors as [`Decoder::decode`].
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub fn decode(&mut self, input: &[u8]) -> Result<DecodedDocument, DecodeError> {
        let result = self.decode_internal(input);
        self.finish_call();
        result
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn decode_internal(&mut self, input: &[u8]) -> Result<DecodedDocument, DecodeError> {
        let (info, segments) = parse_standalone_document(input, self.decoder.limits())?;
        let mut collector = PageCollector::new(self.decoder.limits());
        let initial_work = structural_work(input.len(), &segments)?;
        decode_segments_collected(
            &segments,
            &info,
            initial_work,
            0,
            self.decoder.limits(),
            &mut self.mmr,
            None,
            &mut collector,
        )?;
        Ok(DecodedDocument {
            info,
            pages: collector.pages,
        })
    }

    /// Decodes one embedded page stream using retained scratch capacity.
    ///
    /// # Errors
    ///
    /// Returns the same typed errors as [`Decoder::decode_embedded`].
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub fn decode_embedded(&mut self, input: &[u8]) -> Result<DecodedDocument, DecodeError> {
        let result = self
            .decoder
            .decode_embedded_internal(input, None, &mut self.mmr);
        self.finish_call();
        result
    }

    /// Decodes one embedded page stream with validated globals.
    ///
    /// # Errors
    ///
    /// Returns the same typed errors as [`Decoder::decode_embedded_with_globals`].
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub fn decode_embedded_with_globals(
        &mut self,
        input: &[u8],
        globals: &GlobalSegments,
    ) -> Result<DecodedDocument, DecodeError> {
        let result = self
            .decoder
            .decode_embedded_internal(input, Some(globals), &mut self.mmr);
        self.finish_call();
        result
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
struct PageCollector {
    pages: Vec<Page>,
    bitmap_bytes: u64,
    memory_limit: u64,
    decoded_limit: u64,
}

impl PageCollector {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures
            decode_spec::collector_pages(&out).len() == 0,
            decode_spec::collector_bitmap_bytes(&out) == 0,
            decode_spec::collector_memory_limit(&out)
                == limits.max_decoder_managed_memory_bytes,
            decode_spec::collector_decoded_limit(&out)
                == limits.max_cumulative_decoded_bitmap_bytes,
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    const fn new(limits: &Limits) -> Self {
        let out = Self {
            pages: Vec::new(),
            bitmap_bytes: 0,
            memory_limit: limits.max_decoder_managed_memory_bytes,
            decoded_limit: limits.max_cumulative_decoded_bitmap_bytes,
        };
        #[cfg(verus_keep_ghost)]
        production_collector_fields!(&out);
        out
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures match out {
            Ok(()) =>
                decode_spec::collector_bitmap_bytes(final(self)) as nat
                    == decode_spec::collector_bitmap_bytes(old(self)) as nat
                        + decode_spec::bitmap_byte_count(
                            decode_spec::page_bitmap(&page),
                        )
                && decode_spec::collector_bitmap_bytes(final(self))
                    <= decode_spec::collector_decoded_limit(final(self))
                && decode_spec::collector_bitmap_bytes(final(self))
                    <= decode_spec::collector_memory_limit(final(self))
                && decode_spec::collector_pages(final(self))
                    == decode_spec::collector_pages(old(self)).push(page),
            Err(_) =>
                decode_spec::collector_bitmap_bytes(final(self))
                    == decode_spec::collector_bitmap_bytes(old(self))
                && decode_spec::collector_pages(final(self))
                    == decode_spec::collector_pages(old(self)),
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    fn collect_page(&mut self, page: Page) -> Result<(), DecodeError> {
        #[cfg(verus_keep_ghost)]
        {
            production_collector_fields!(self);
            production_decode_page_fields!(&page);
        }
        let page_bytes = bitmap_byte_len(&page.bitmap)?;
        let next_bytes = self
            .bitmap_bytes
            .checked_add(page_bytes)
            .ok_or(MalformedError::IntegerOverflow)?;
        if next_bytes > self.decoded_limit {
            return Err(crate::LimitExceeded {
                kind: crate::LimitKind::DecodedBitmapBytes,
                limit: self.decoded_limit,
                actual: next_bytes,
            }
            .into());
        }
        if next_bytes > self.memory_limit {
            return Err(crate::LimitExceeded {
                kind: crate::LimitKind::Memory,
                limit: self.memory_limit,
                actual: next_bytes,
            }
            .into());
        }
        let new_length = self
            .pages
            .len()
            .checked_add(1)
            .ok_or(MalformedError::IntegerOverflow)?;
        let requested_bytes = allocation_size::<Page>(new_length)?;
        self.pages
            .try_reserve(1)
            .map_err(|_error| DecodeError::Allocation { requested_bytes })?;
        self.bitmap_bytes = next_bytes;
        self.pages.push(page);
        #[cfg(verus_keep_ghost)]
        production_collector_fields!(self);
        Ok(())
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn push_page(&mut self, page: Page) -> Result<(), DecodeError> {
        self.collect_page(page)
    }
}

impl PageSink for PageCollector {
    fn write_page(&mut self, page: Page) -> Result<(), DecodeError> {
        self.push_page(page)
    }
}

fn write_page_to_sink<S: PageSink>(sink: &mut S, page: Page) -> Result<(), DecodeError> {
    sink.write_page(page)
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[allow(clippy::struct_excessive_bools)]
struct RuntimePage {
    default_pixel: bool,
    pristine: bool,
    default_operator: crate::bitmap::CombinationOperator,
    combination_override: bool,
    refinements_allowed: bool,
    auxiliary_allowed: bool,
    last_stripe_end: Option<u32>,
    open_stripe_max_row: Option<u32>,
    bitmap: Bitmap,
}

type RuntimePages = Vec<(u32, RuntimePage)>;

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Some(index) =>
            index < pages@.len()
            && pages@[index as int].0 == number,
        None =>
            forall|index: int| 0 <= index < pages@.len() ==>
                pages@[index].0 != number,
    },
))]
fn runtime_page_index(pages: &[(u32, RuntimePage)], number: u32) -> Option<usize> {
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        invariant
            index <= pages@.len(),
            forall|completed: int| 0 <= completed < index ==>
                pages@[completed].0 != number,
        decreases pages@.len() - index,
    ))]
    while index < pages.len() {
        if pages[index].0 == number {
            return Some(index);
        }
        index += 1;
    }
    None
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires decode_spec::runtime_pages_valid(pages),
    ensures match out {
        Some(page) => crate::spec::bitmap::storage_valid(&page.bitmap),
        None => true,
    },
))]
fn get_runtime_page(pages: &RuntimePages, number: u32) -> Option<&RuntimePage> {
    let index = runtime_page_index(pages, number)?;
    Some(&pages[index].1)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        decode_spec::runtime_pages_valid(old(pages)),
        crate::spec::bitmap::storage_valid(&page.bitmap),
    ensures decode_spec::runtime_pages_valid(final(pages)),
))]
fn insert_runtime_page(pages: &mut RuntimePages, number: u32, page: RuntimePage) {
    if let Some(index) = runtime_page_index(pages, number) {
        pages[index].1 = page;
    } else {
        pages.push((number, page));
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
struct RegionBuffer {
    region: crate::segment_data::RegionInfo,
    bitmap: Bitmap,
}

type RegionBuffers = Vec<(u32, RegionBuffer)>;

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Some(index) =>
            index < buffers@.len()
            && buffers@[index as int].0 == number,
        None =>
            forall|index: int| 0 <= index < buffers@.len() ==>
                buffers@[index].0 != number,
    },
))]
fn region_buffer_index(buffers: &[(u32, RegionBuffer)], number: u32) -> Option<usize> {
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        invariant
            index <= buffers@.len(),
            forall|completed: int| 0 <= completed < index ==>
                buffers@[completed].0 != number,
        decreases buffers@.len() - index,
    ))]
    while index < buffers.len() {
        if buffers[index].0 == number {
            return Some(index);
        }
        index += 1;
    }
    None
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires decode_spec::region_buffers_valid(buffers),
    ensures match out {
        Some(buffer) => crate::spec::bitmap::storage_valid(&buffer.bitmap),
        None => true,
    },
))]
fn get_region_buffer(buffers: &RegionBuffers, number: u32) -> Option<&RegionBuffer> {
    let index = region_buffer_index(buffers, number)?;
    Some(&buffers[index].1)
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
struct SymbolResource {
    exported: Vec<Bitmap>,
    contexts: Option<DictionaryContexts>,
}

type SymbolStore = Vec<(u32, SymbolResource)>;

#[cfg_attr(verus_keep_ghost, verus_verify)]
enum CollectedSymbols<'a> {
    Contiguous(&'a [Bitmap]),
    Borrowed(Vec<&'a Bitmap>),
}

impl<'a> CollectedSymbols<'a> {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires decode_spec::collected_symbols_valid(self),
        ensures crate::symbol_text::symbol_text_spec::codebook_valid(out),
    ))]
    fn codebook(&'a self) -> SymbolCodebook<'a> {
        match self {
            Self::Contiguous(symbols) => SymbolCodebook::Contiguous(symbols),
            Self::Borrowed(symbols) => SymbolCodebook::Borrowed(symbols),
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Some(index) =>
            index < symbols@.len()
            && symbols@[index as int].0 == number,
        None =>
            forall|index: int| 0 <= index < symbols@.len() ==>
                symbols@[index].0 != number,
    },
))]
fn symbol_index(symbols: &[(u32, SymbolResource)], number: u32) -> Option<usize> {
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index <= symbols@.len(),
            forall|completed: int| 0 <= completed < index ==>
                symbols@[completed].0 != number,
        decreases symbols@.len() - index,
    ))]
    while index < symbols.len() {
        if symbols[index].0 == number {
            return Some(index);
        }
        index += 1;
    }
    None
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires decode_spec::symbol_store_valid(symbols),
    ensures match out {
        Some(resource) =>
            crate::halftone::halftone_spec::bitmaps_valid(resource.exported@),
        None => true,
    },
))]
fn get_symbol_resource(symbols: &SymbolStore, number: u32) -> Option<&SymbolResource> {
    let index = symbol_index(symbols, number)?;
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(crate::halftone::halftone_spec::bitmaps_valid(
            symbols@[index as int].1.exported@,
        ));
    }
    Some(&symbols[index].1)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        decode_spec::symbol_store_valid(old(symbols)),
        crate::halftone::halftone_spec::bitmaps_valid(resource.exported@),
    ensures decode_spec::symbol_store_valid(final(symbols)),
))]
fn insert_symbol_resource(
    symbols: &mut SymbolStore,
    number: u32,
    resource: SymbolResource,
) -> bool {
    if symbol_index(symbols, number).is_some() {
        return false;
    }
    symbols.push((number, resource));
    #[cfg(verus_keep_ghost)]
    proof! {
        assert forall|index: int| 0 <= index < symbols@.len() implies
            crate::halftone::halftone_spec::bitmaps_valid(
                #[trigger] symbols@[index].1.exported@,
            ) by {
            if index < old(symbols)@.len() {
                assert(symbols@[index] == old(symbols)@[index]);
                assert(crate::halftone::halftone_spec::bitmaps_valid(
                    old(symbols)@[index].1.exported@,
                ));
            } else {
                assert(symbols@[index].1 == resource);
                assert(crate::halftone::halftone_spec::bitmaps_valid(resource.exported@));
            }
        }
    }
    true
}

type HuffmanTables = Vec<(u32, HuffmanTable)>;

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Some(index) =>
            index < tables@.len()
            && tables@[index as int].0 == number,
        None =>
            forall|index: int| 0 <= index < tables@.len() ==>
                tables@[index].0 != number,
    },
))]
fn huffman_table_index(tables: &[(u32, HuffmanTable)], number: u32) -> Option<usize> {
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index <= tables@.len(),
            forall|completed: int| 0 <= completed < index ==>
                tables@[completed].0 != number,
        decreases tables@.len() - index,
    ))]
    while index < tables.len() {
        if tables[index].0 == number {
            return Some(index);
        }
        index += 1;
    }
    None
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn get_huffman_table(tables: &HuffmanTables, number: u32) -> Option<&HuffmanTable> {
    let index = huffman_table_index(tables, number)?;
    Some(&tables[index].1)
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn insert_huffman_table(tables: &mut HuffmanTables, number: u32, table: HuffmanTable) -> bool {
    if huffman_table_index(tables, number).is_some() {
        return false;
    }
    tables.push((number, table));
    true
}

type PatternStore = Vec<(u32, Vec<Bitmap>)>;

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Some(index) =>
            index < patterns@.len()
            && patterns@[index as int].0 == number,
        None =>
            forall|index: int| 0 <= index < patterns@.len() ==>
                patterns@[index].0 != number,
    },
))]
fn pattern_index(patterns: &[(u32, Vec<Bitmap>)], number: u32) -> Option<usize> {
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index <= patterns@.len(),
            forall|completed: int| 0 <= completed < index ==>
                patterns@[completed].0 != number,
        decreases patterns@.len() - index,
    ))]
    while index < patterns.len() {
        if patterns[index].0 == number {
            return Some(index);
        }
        index += 1;
    }
    None
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires decode_spec::pattern_store_valid(patterns),
    ensures match out {
        Some(bitmaps) =>
            crate::halftone::halftone_spec::bitmaps_valid(bitmaps@),
        None => true,
    },
))]
fn get_patterns(patterns: &PatternStore, number: u32) -> Option<&[Bitmap]> {
    let index = pattern_index(patterns, number)?;
    Some(&patterns[index].1)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        decode_spec::pattern_store_valid(old(patterns)),
        crate::halftone::halftone_spec::bitmaps_valid(bitmaps@),
    ensures decode_spec::pattern_store_valid(final(patterns)),
))]
fn insert_patterns(patterns: &mut PatternStore, number: u32, bitmaps: Vec<Bitmap>) -> bool {
    if pattern_index(patterns, number).is_some() {
        return false;
    }
    patterns.push((number, bitmaps));
    true
}

#[derive(Default)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
struct InitialResources {
    patterns: PatternStore,
    symbols: SymbolStore,
    tables: HuffmanTables,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
struct RetainedBudget {
    bytes: u64,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
struct DecodeWork {
    used: u64,
}

impl DecodeWork {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures match out {
            Ok(work) =>
                decode_spec::work_used(&work) == initial
                    && decode_spec::work_used(&work) <= limits.max_work_units,
            Err(_) => true,
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    fn new(initial: u64, limits: &Limits) -> Result<Self, DecodeError> {
        let mut work = Self { used: 0 };
        #[cfg(verus_keep_ghost)]
        production_work_fields!(&work);
        work.charge(initial, limits)?;
        Ok(work)
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures match out {
            Ok(()) =>
                decode_spec::work_used(final(self))
                    == decode_spec::work_used(old(self)) + units
                && decode_spec::work_used(final(self)) <= limits.max_work_units,
            Err(_) =>
                decode_spec::work_used(final(self))
                    == decode_spec::work_used(old(self)),
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    fn charge(&mut self, units: u64, limits: &Limits) -> Result<(), DecodeError> {
        #[cfg(verus_keep_ghost)]
        production_work_fields!(self);
        let next = self
            .used
            .checked_add(units)
            .ok_or(MalformedError::IntegerOverflow)?;
        if next > limits.max_work_units {
            return Err(crate::LimitExceeded {
                kind: crate::LimitKind::Work,
                limit: limits.max_work_units,
                actual: next,
            }
            .into());
        }
        self.used = next;
        #[cfg(verus_keep_ghost)]
        production_work_fields!(self);
        Ok(())
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn can_charge_pair(&self, first: u64, second: u64, limits: &Limits) -> bool {
        match self
            .used
            .checked_add(first)
            .and_then(|used| used.checked_add(second))
        {
            Some(used) => used <= limits.max_work_units,
            None => false,
        }
    }
}

impl RetainedBudget {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures decode_spec::retained_bytes(&out) == 0,
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    const fn new() -> Self {
        let out = Self { bytes: 0 };
        #[cfg(verus_keep_ghost)]
        production_retained_fields!(&out);
        out
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures match out {
            Ok(()) =>
                decode_spec::retained_bytes(final(self))
                    == decode_spec::retained_bytes(old(self)) + bytes
                && decode_spec::retained_bytes(final(self))
                    <= limits.max_retained_state_bytes,
            Err(_) =>
                decode_spec::retained_bytes(final(self))
                    == decode_spec::retained_bytes(old(self)),
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    fn retain(&mut self, bytes: u64, limits: &Limits) -> Result<(), DecodeError> {
        #[cfg(verus_keep_ghost)]
        production_retained_fields!(self);
        let next = self
            .bytes
            .checked_add(bytes)
            .ok_or(MalformedError::IntegerOverflow)?;
        if next > limits.max_retained_state_bytes {
            return Err(crate::LimitExceeded {
                kind: crate::LimitKind::RetainedState,
                limit: limits.max_retained_state_bytes,
                actual: next,
            }
            .into());
        }
        self.bytes = next;
        #[cfg(verus_keep_ghost)]
        production_retained_fields!(self);
        Ok(())
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures match out {
            Ok(()) =>
                decode_spec::retained_bytes(final(self)) + bytes
                    == decode_spec::retained_bytes(old(self)),
            Err(_) =>
                decode_spec::retained_bytes(final(self))
                    == decode_spec::retained_bytes(old(self)),
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    fn release(&mut self, bytes: u64) -> Result<(), DecodeError> {
        #[cfg(verus_keep_ghost)]
        production_retained_fields!(self);
        self.bytes = self
            .bytes
            .checked_sub(bytes)
            .ok_or(MalformedError::IntegerOverflow)?;
        #[cfg(verus_keep_ghost)]
        production_retained_fields!(self);
        Ok(())
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(resources) =>
            decode_spec::pattern_store_valid(&resources.patterns)
                && decode_spec::symbol_store_valid(&resources.symbols),
        Err(_) => true,
    },
))]
fn decode_global_resources(input: &[u8], limits: &Limits) -> Result<InitialResources, DecodeError> {
    let (_info, segments) = parse_embedded_document(input, limits, None)?;
    let mut resources = InitialResources {
        patterns: Vec::new(),
        symbols: Vec::new(),
        tables: Vec::new(),
    };
    let requested_bytes = allocation_size::<(u32, Vec<Bitmap>)>(segments.len())?;
    resources
        .patterns
        .try_reserve(segments.len())
        .map_err(|_| DecodeError::Allocation { requested_bytes })?;
    let symbol_bytes = allocation_size::<(u32, SymbolResource)>(segments.len())?;
    resources
        .symbols
        .try_reserve(segments.len())
        .map_err(|_| DecodeError::Allocation {
            requested_bytes: symbol_bytes,
        })?;
    let table_bytes = allocation_size::<(u32, HuffmanTable)>(segments.len())?;
    resources
        .tables
        .try_reserve(segments.len())
        .map_err(|_| DecodeError::Allocation {
            requested_bytes: table_bytes,
        })?;
    let mut retained = RetainedBudget::new();
    let mut work = DecodeWork::new(structural_work(input.len(), &segments)?, limits)?;
    let mut segment_index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            segment_index <= segments@.len(),
            decode_spec::pattern_store_valid(&resources.patterns),
            decode_spec::symbol_store_valid(&resources.symbols),
        decreases segments@.len() - segment_index,
    ))]
    while segment_index < segments.len() {
        let segment = &segments[segment_index];
        match segment.header.segment_type {
            SegmentType::Tables => {
                let table = HuffmanTable::from_encoded(segment.payload, limits)
                    .map_err(huffman_error_into_decode)?;
                let _ = insert_huffman_table(&mut resources.tables, segment.header.number, table);
            }
            SegmentType::PatternDictionary => {
                decode_pattern_segment(
                    segment,
                    &mut resources.patterns,
                    &mut retained,
                    &mut work,
                    limits,
                )?;
            }
            SegmentType::SymbolDictionary => {
                decode_symbol_dictionary_segment(
                    segment,
                    &mut resources.symbols,
                    &resources.tables,
                    &mut retained,
                    &mut work,
                    limits,
                )?;
            }
            _ => {}
        }
        release_referred_patterns(segment, &mut resources.patterns, &mut retained)?;
        release_referred_symbols(segment, &mut resources.symbols, &mut retained)?;
        release_referred_tables(segment, &mut resources.tables);
        segment_index += 1;
    }
    Ok(resources)
}

macro_rules! decode_segments_body {
    (
        $segments:expr,
        $info:expr,
        $initial_work:expr,
        $initial_retained_bytes:expr,
        $limits:expr,
        $mmr_scratch:expr,
        $initial_resources:expr,
        $sink:expr,
        $write_page:path
    ) => {{
        let segments = $segments;
        let info = $info;
        let initial_work = $initial_work;
        let initial_retained_bytes = $initial_retained_bytes;
        let limits = $limits;
        let mmr_scratch = $mmr_scratch;
        let initial_resources = $initial_resources;
        let sink = $sink;
        let mut pages = Vec::new();
        let page_allocation = allocation_size::<(u32, RuntimePage)>(info.pages().len())?;
        pages
            .try_reserve(info.pages().len())
            .map_err(|_| DecodeError::Allocation {
                requested_bytes: page_allocation,
            })?;
        let mut buffers = Vec::new();
        let buffer_allocation = allocation_size::<(u32, RegionBuffer)>(segments.len())?;
        buffers
            .try_reserve(segments.len())
            .map_err(|_| DecodeError::Allocation {
                requested_bytes: buffer_allocation,
            })?;
        let initial_resources = match initial_resources {
            Some(resources) => resources,
            None => InitialResources {
                patterns: Vec::new(),
                symbols: Vec::new(),
                tables: Vec::new(),
            },
        };
        let InitialResources {
            mut patterns,
            mut symbols,
            mut tables,
        } = initial_resources;
        let pattern_allocation = allocation_size::<(u32, Vec<Bitmap>)>(segments.len())?;
        patterns
            .try_reserve(segments.len())
            .map_err(|_| DecodeError::Allocation {
                requested_bytes: pattern_allocation,
            })?;
        let table_allocation = allocation_size::<(u32, HuffmanTable)>(segments.len())?;
        tables
            .try_reserve(segments.len())
            .map_err(|_| DecodeError::Allocation {
                requested_bytes: table_allocation,
            })?;
        let symbol_allocation = allocation_size::<(u32, SymbolResource)>(segments.len())?;
        symbols
            .try_reserve(segments.len())
            .map_err(|_| DecodeError::Allocation {
                requested_bytes: symbol_allocation,
            })?;
        let mut retained = RetainedBudget::new();
        retained.retain(initial_retained_bytes, limits)?;
        let mut cumulative_work = DecodeWork::new(initial_work, limits)?;
        let mut pattern_position = 0_usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
                                    invariant
                                        pattern_position <= patterns@.len(),
                                        decode_spec::pattern_store_valid(&patterns),
                                    decreases patterns@.len() - pattern_position,
                                ))]
        while pattern_position < patterns.len() {
            let bitmaps = &patterns[pattern_position].1;
            let mut bytes = 0_u64;
            let mut bitmap_position = 0_usize;
            #[cfg_attr(verus_keep_ghost, verus_spec(
                                        invariant
                                            bitmap_position <= bitmaps@.len(),
                                            crate::halftone::halftone_spec::bitmaps_valid(bitmaps@),
                                        decreases bitmaps@.len() - bitmap_position,
                                    ))]
            while bitmap_position < bitmaps.len() {
                bytes = bytes
                    .checked_add(bitmap_byte_len(&bitmaps[bitmap_position])?)
                    .ok_or(DecodeError::Malformed(MalformedError::IntegerOverflow))?;
                bitmap_position += 1;
            }
            retained.retain(bytes, limits)?;
            pattern_position += 1;
        }
        let mut symbol_position = 0_usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
                                    invariant
                                        symbol_position <= symbols@.len(),
                                        decode_spec::symbol_store_valid(&symbols),
                                    decreases symbols@.len() - symbol_position,
                                ))]
        while symbol_position < symbols.len() {
            let resource = &symbols[symbol_position].1;
            let mut bytes = 0_u64;
            let mut bitmap_position = 0_usize;
            #[cfg_attr(verus_keep_ghost, verus_spec(
                                        invariant
                                            bitmap_position <= resource.exported@.len(),
                                            crate::halftone::halftone_spec::bitmaps_valid(
                                                resource.exported@
                                            ),
                                        decreases resource.exported@.len() - bitmap_position,
                                    ))]
            while bitmap_position < resource.exported.len() {
                bytes = bytes
                    .checked_add(bitmap_byte_len(&resource.exported[bitmap_position])?)
                    .ok_or(DecodeError::Malformed(MalformedError::IntegerOverflow))?;
                bitmap_position += 1;
            }
            retained.retain(bytes, limits)?;
            symbol_position += 1;
        }

        let mut segment_position = 0_usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
                                    invariant
                                        segment_position <= segments@.len(),
                                        decode_spec::runtime_pages_valid(&pages),
                                        decode_spec::region_buffers_valid(&buffers),
                                        decode_spec::pattern_store_valid(&patterns),
                                        decode_spec::symbol_store_valid(&symbols),
                                    decreases segments@.len() - segment_position,
                                ))]
        while segment_position < segments.len() {
            let segment = &segments[segment_position];
            #[cfg(verus_keep_ghost)]
            proof! {
                decode_properties::dispatch_is_deterministic(segment.header.segment_type);
            }
            match segment.header.segment_type {
                SegmentType::PageInformation => {
                    let ParsedData::Page(page_data) = segment.data else {
                        return Err(MalformedError::InvalidPageState(
                            "page information payload was not parsed",
                        )
                        .into());
                    };
                    let public = find_page_info(info, segment.header.page_association)?;
                    let bitmap = Bitmap::filled(
                        public.width(),
                        public.height(),
                        page_data.default_pixel,
                        limits,
                    )?;
                    retained.retain(bitmap_byte_len(&bitmap)?, limits)?;
                    insert_runtime_page(
                        &mut pages,
                        public.number(),
                        RuntimePage {
                            default_pixel: page_data.default_pixel,
                            pristine: true,
                            default_operator: page_data.default_operator,
                            combination_override: page_data.combination_operator_override,
                            refinements_allowed: page_data.might_contain_refinements,
                            auxiliary_allowed: page_data.requires_auxiliary_buffers,
                            last_stripe_end: None,
                            open_stripe_max_row: None,
                            bitmap,
                        },
                    );
                }
                SegmentType::IntermediateGenericRegion
                | SegmentType::ImmediateGenericRegion
                | SegmentType::ImmediateLosslessGenericRegion => {
                    decode_generic_segment(
                        segment,
                        &mut pages,
                        &mut buffers,
                        &mut retained,
                        &mut cumulative_work,
                        limits,
                        mmr_scratch,
                    )?;
                }
                SegmentType::IntermediateGenericRefinementRegion
                | SegmentType::ImmediateGenericRefinementRegion
                | SegmentType::ImmediateLosslessGenericRefinementRegion => {
                    decode_refinement_segment(
                        segment,
                        &mut pages,
                        &mut buffers,
                        &mut retained,
                        &mut cumulative_work,
                        limits,
                    )?;
                }
                SegmentType::PatternDictionary => {
                    decode_pattern_segment(
                        segment,
                        &mut patterns,
                        &mut retained,
                        &mut cumulative_work,
                        limits,
                    )?;
                }
                SegmentType::SymbolDictionary => {
                    decode_symbol_dictionary_segment(
                        segment,
                        &mut symbols,
                        &tables,
                        &mut retained,
                        &mut cumulative_work,
                        limits,
                    )?;
                }
                SegmentType::IntermediateTextRegion
                | SegmentType::ImmediateTextRegion
                | SegmentType::ImmediateLosslessTextRegion => {
                    decode_text_segment(
                        segment,
                        &mut pages,
                        &mut buffers,
                        &symbols,
                        &tables,
                        &mut retained,
                        &mut cumulative_work,
                        limits,
                    )?;
                }
                SegmentType::IntermediateHalftoneRegion
                | SegmentType::ImmediateHalftoneRegion
                | SegmentType::ImmediateLosslessHalftoneRegion => {
                    decode_halftone_segment(
                        segment,
                        &mut pages,
                        &mut buffers,
                        &patterns,
                        &mut retained,
                        &mut cumulative_work,
                        limits,
                    )?;
                }
                SegmentType::EndOfPage => {
                    let number = segment.header.page_association;
                    let index = runtime_page_index(&pages, number).ok_or(
                        MalformedError::InvalidPageState("end-of-page without active page"),
                    )?;
                    #[cfg(verus_keep_ghost)]
                    proof_decl! {
                        let ghost original_pages = pages@;
                    }
                    let page = pages.remove(index).1;
                    #[cfg(verus_keep_ghost)]
                    proof! {
                        reveal(decode_spec::runtime_pages_valid);
                        assert forall|entry: int| 0 <= entry < pages@.len()
                            implies crate::spec::bitmap::storage_valid(
                                #[trigger] &pages@[entry].1.bitmap
                            ) by {
                            if entry < index {
                                assert(pages@[entry] == original_pages[entry]);
                            } else {
                                assert(pages@[entry] == original_pages[entry + 1]);
                            }
                        }
                    }
                    retained.release(bitmap_byte_len(&page.bitmap)?)?;
                    $write_page(
                        sink,
                        Page {
                            number,
                            bitmap: page.bitmap,
                        },
                    )?;
                }
                SegmentType::EndOfStripe => {
                    let ParsedData::EndOfStripe(row) = segment.data else {
                        return Err(MalformedError::InvalidPageState(
                            "end-of-stripe payload was not parsed",
                        )
                        .into());
                    };
                    seal_stripe(&mut pages, segment.header.page_association, row)?;
                }
                SegmentType::EndOfFile | SegmentType::Profiles | SegmentType::Extension => {}
                SegmentType::Tables => {
                    let table = HuffmanTable::from_encoded(segment.payload, limits)
                        .map_err(huffman_error_into_decode)?;
                    if !insert_huffman_table(&mut tables, segment.header.number, table) {
                        return Err(
                            MalformedError::DuplicateSegmentNumber(segment.header.number).into(),
                        );
                    }
                }
                SegmentType::ColourPalette => {
                    return Err(DecodeError::Unsupported(UnsupportedFeature::Colour));
                }
            }
            release_referred_buffers(segment, &mut buffers, &mut retained)?;
            release_referred_patterns(segment, &mut patterns, &mut retained)?;
            release_referred_symbols(segment, &mut symbols, &mut retained)?;
            release_referred_tables(segment, &mut tables);
            segment_position += 1;
        }
        if !pages.is_empty() {
            return Err(MalformedError::InvalidPageState("active page after final segment").into());
        }
        Ok(())
    }};
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires match initial_resources {
        Some(resources) => decode_spec::initial_resources_valid(&resources),
        None => true,
    },
))]
fn decode_segments_collected(
    segments: &[ParsedSegment<'_>],
    info: &DocumentInfo,
    initial_work: u64,
    initial_retained_bytes: u64,
    limits: &Limits,
    mmr_scratch: &mut MmrScratch,
    initial_resources: Option<InitialResources>,
    sink: &mut PageCollector,
) -> Result<(), DecodeError> {
    decode_segments_body!(
        segments,
        info,
        initial_work,
        initial_retained_bytes,
        limits,
        mmr_scratch,
        initial_resources,
        sink,
        PageCollector::push_page
    )
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn decode_segments<S: PageSink>(
    segments: &[ParsedSegment<'_>],
    info: &DocumentInfo,
    initial_work: u64,
    initial_retained_bytes: u64,
    limits: &Limits,
    mmr_scratch: &mut MmrScratch,
    initial_resources: Option<InitialResources>,
    sink: &mut S,
) -> Result<(), DecodeError> {
    decode_segments_body!(
        segments,
        info,
        initial_work,
        initial_retained_bytes,
        limits,
        mmr_scratch,
        initial_resources,
        sink,
        write_page_to_sink::<S>
    )
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires decode_spec::runtime_pages_valid(pages),
    ensures match out {
        Some(index) => index < pages@.len(),
        None => true,
    },
))]
fn direct_generic_page_index(
    segment: &ParsedSegment<'_>,
    pages: &RuntimePages,
    work: &DecodeWork,
    limits: &Limits,
) -> Option<usize> {
    if !matches!(
        segment.header.segment_type,
        SegmentType::ImmediateGenericRegion | SegmentType::ImmediateLosslessGenericRegion
    ) {
        return None;
    }
    let (header, _) = match parse_generic_region_header(segment.payload, limits) {
        Ok(header) => header,
        Err(_error) => return None,
    };
    if header.mmr
        || header.region.x != 0
        || header.region.y != 0
        || !matches!(
            header.region.operator,
            CombinationOperator::Or | CombinationOperator::Replace
        )
    {
        return None;
    }
    let index = runtime_page_index(pages, segment.header.page_association)?;
    let page = &pages[index].1;
    if !page.pristine
        || page.default_pixel
        || page.last_stripe_end.is_some()
        || page.open_stripe_max_row.is_some()
        || header.region.width != page.bitmap.width()
        || header.region.height != page.bitmap.height()
        || (!page.combination_override && page.default_operator != header.region.operator)
    {
        return None;
    }
    let pixels = u64::from(header.region.width).checked_mul(u64::from(header.region.height))?;
    let region_work = pixels.checked_mul(18)?;
    if !work.can_charge_pair(region_work, pixels, limits) {
        return None;
    }
    Some(index)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        crate::spec::bitmap::storage_valid(&old(page).bitmap),
    ensures out.is_ok() ==> crate::spec::bitmap::storage_valid(&final(page).bitmap),
))]
fn decode_generic_segment_direct_on_page(
    segment: &ParsedSegment<'_>,
    page: &mut RuntimePage,
    work: &mut DecodeWork,
    limits: &Limits,
) -> Result<bool, DecodeError> {
    let Some(decoded) = decode_generic_region_segment_into(
        segment.payload,
        segment.header.data_length.is_none(),
        limits,
        &mut page.bitmap,
    )
    .map_err(generic_error_into_decode)?
    else {
        return Ok(false);
    };
    if decoded.consumed_bytes != segment.payload.len() {
        let trailing = segment
            .payload
            .len()
            .checked_sub(decoded.consumed_bytes)
            .and_then(|bytes| u64::try_from(bytes).ok())
            .ok_or(MalformedError::IntegerOverflow)?;
        return Err(MalformedError::TrailingData(trailing).into());
    }
    let region_work = u64::from(decoded.header.region.width)
        .checked_mul(u64::from(decoded.decoded_rows))
        .and_then(|pixels| pixels.checked_mul(18))
        .ok_or(MalformedError::IntegerOverflow)?;
    work.charge(region_work, limits)?;
    check_combination_operator(
        page.combination_override,
        decoded.header.region.operator,
        page.default_operator,
        false,
    )?;
    note_page_write(page, &decoded.header.region)?;
    let composition_work = u64::from(decoded.header.region.width)
        .checked_mul(u64::from(decoded.header.region.height))
        .ok_or(MalformedError::IntegerOverflow)?;
    work.charge(composition_work, limits)?;
    page.pristine = false;
    Ok(true)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        decode_spec::runtime_pages_valid(old(pages)),
        decode_spec::region_buffers_valid(old(buffers)),
    ensures match out {
        Ok(()) =>
            decode_spec::runtime_pages_valid(final(pages))
                && decode_spec::region_buffers_valid(final(buffers)),
        Err(_) => true,
    },
))]
#[allow(clippy::too_many_arguments)]
fn decode_generic_segment(
    segment: &ParsedSegment<'_>,
    pages: &mut RuntimePages,
    buffers: &mut RegionBuffers,
    retained: &mut RetainedBudget,
    work: &mut DecodeWork,
    limits: &Limits,
    mmr_scratch: &mut MmrScratch,
) -> Result<(), DecodeError> {
    let arithmetic = match segment.payload.get(17) {
        Some(flags) => *flags & 0x01 == 0,
        None => false,
    };
    if arithmetic {
        if let Some(index) = direct_generic_page_index(segment, pages, work, limits) {
            if decode_generic_segment_direct_on_page(segment, &mut pages[index].1, work, limits)? {
                return Ok(());
            }
        }
    }
    decode_generic_segment_buffered(segment, pages, buffers, retained, work, limits, mmr_scratch)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        decode_spec::runtime_pages_valid(old(pages)),
        decode_spec::region_buffers_valid(old(buffers)),
    ensures match out {
        Ok(()) =>
            decode_spec::runtime_pages_valid(final(pages))
                && decode_spec::region_buffers_valid(final(buffers)),
        Err(_) => true,
    },
))]
fn decode_generic_segment_buffered(
    segment: &ParsedSegment<'_>,
    pages: &mut RuntimePages,
    buffers: &mut RegionBuffers,
    retained: &mut RetainedBudget,
    work: &mut DecodeWork,
    limits: &Limits,
    mmr_scratch: &mut MmrScratch,
) -> Result<(), DecodeError> {
    let decoded = decode_generic_region_segment_reused(
        segment.payload,
        segment.header.data_length.is_none(),
        limits,
        mmr_scratch,
    )
    .map_err(generic_error_into_decode)?;
    if decoded.consumed_bytes != segment.payload.len() {
        let trailing = segment
            .payload
            .len()
            .checked_sub(decoded.consumed_bytes)
            .and_then(|bytes| u64::try_from(bytes).ok())
            .ok_or(MalformedError::IntegerOverflow)?;
        return Err(MalformedError::TrailingData(trailing).into());
    }
    let region_work = u64::from(decoded.header.region.width)
        .checked_mul(u64::from(decoded.decoded_rows))
        .and_then(|pixels| pixels.checked_mul(18))
        .ok_or(MalformedError::IntegerOverflow)?;
    work.charge(region_work, limits)?;
    if segment.header.segment_type == SegmentType::IntermediateGenericRegion {
        require_auxiliary_page(segment, pages)?;
        insert_buffer(
            buffers,
            segment.header.number,
            decoded.header.region,
            decoded.bitmap,
            retained,
            limits,
        )?;
    } else {
        compose_immediate(
            segment,
            pages,
            &decoded.header.region,
            &decoded.bitmap,
            false,
            work,
            limits,
        )?;
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn decode_generic_segment_oracle(
    segment: &ParsedSegment<'_>,
    pages: &mut RuntimePages,
    buffers: &mut RegionBuffers,
    retained: &mut RetainedBudget,
    work: &mut DecodeWork,
    limits: &Limits,
    mmr_scratch: &mut MmrScratch,
) -> Result<(), DecodeError> {
    decode_generic_segment_buffered(segment, pages, buffers, retained, work, limits, mmr_scratch)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        decode_spec::runtime_pages_valid(old(pages)),
        decode_spec::region_buffers_valid(old(buffers)),
    ensures match out {
        Ok(()) =>
            decode_spec::runtime_pages_valid(final(pages))
                && decode_spec::region_buffers_valid(final(buffers)),
        Err(_) => true,
    },
))]
fn decode_refinement_segment(
    segment: &ParsedSegment<'_>,
    pages: &mut RuntimePages,
    buffers: &mut RegionBuffers,
    retained: &mut RetainedBudget,
    work: &mut DecodeWork,
    limits: &Limits,
) -> Result<(), DecodeError> {
    let (header, header_length) = parse_refinement_region_header(segment.payload, limits)
        .map_err(generic_error_into_decode)?;
    let coded = segment
        .payload
        .get(header_length..)
        .ok_or(MalformedError::IntegerOverflow)?;
    let owned_snapshot;
    let reference = if let Some(number) = segment.header.referred_segments.first() {
        let buffer =
            get_region_buffer(buffers, *number).ok_or(MalformedError::MissingReferredSegment {
                segment: segment.header.number,
                referred: *number,
            })?;
        require_matching_geometry(buffer.region == header.region)?;
        &buffer.bitmap
    } else {
        if header.region.operator != crate::bitmap::CombinationOperator::Replace {
            return Err(MalformedError::InvalidPageState(
                "unreferenced refinement must use REPLACE",
            )
            .into());
        }
        let page = get_runtime_page(pages, segment.header.page_association).ok_or(
            MalformedError::MissingPageInformation(segment.header.page_association),
        )?;
        owned_snapshot = snapshot_region(&page.bitmap, &header.region, limits)?;
        &owned_snapshot
    };
    let bitmap = decode_arithmetic_refinement(
        coded,
        &header,
        RefinementParameters {
            reference_dx: 0,
            reference_dy: 0,
        },
        reference,
        limits,
    )
    .map_err(generic_error_into_decode)?;
    let refinement_work = u64::from(header.region.width)
        .checked_mul(u64::from(header.region.height))
        .and_then(|pixels| pixels.checked_mul(23))
        .ok_or(MalformedError::IntegerOverflow)?;
    work.charge(refinement_work, limits)?;

    if segment.header.segment_type == SegmentType::IntermediateGenericRefinementRegion {
        require_auxiliary_page(segment, pages)?;
        insert_buffer(
            buffers,
            segment.header.number,
            header.region,
            bitmap,
            retained,
            limits,
        )?;
    } else {
        let page = get_runtime_page(pages, segment.header.page_association).ok_or(
            MalformedError::MissingPageInformation(segment.header.page_association),
        )?;
        if !page.refinements_allowed {
            return Err(MalformedError::InvalidPageState(
                "refinement on page that forbids refinements",
            )
            .into());
        }
        compose_immediate(segment, pages, &header.region, &bitmap, true, work, limits)?;
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires decode_spec::pattern_store_valid(old(patterns)),
    ensures match out {
        Ok(()) => decode_spec::pattern_store_valid(final(patterns)),
        Err(_) => true,
    },
))]
fn decode_pattern_segment(
    segment: &ParsedSegment<'_>,
    patterns: &mut PatternStore,
    retained: &mut RetainedBudget,
    cumulative_work: &mut DecodeWork,
    limits: &Limits,
) -> Result<(), DecodeError> {
    if pattern_index(patterns, segment.header.number).is_some() {
        return Err(MalformedError::DuplicateSegmentNumber(segment.header.number).into());
    }
    let mut reader = ByteReader::new(segment.payload);
    let header =
        parse_pattern_dictionary_header(&mut reader).map_err(halftone_error_into_decode)?;
    let coded = reader.remaining_bytes();
    let mut work = WorkBudget::new(limits);
    let mut decoder = if header.mmr {
        new_pattern_mmr_state(coded, limits)
    } else {
        let template = generic_template(header.template)?;
        new_pattern_arithmetic_state(coded, template, header.template, limits)?
    };
    let pattern_bitmaps = decode_pattern_dictionary(header, &mut decoder, limits, &mut work)
        .map_err(halftone_error_into_decode)?;
    decoder.finish()?;

    let requested_bytes = u64::try_from(std::mem::size_of::<(u32, Vec<Bitmap>)>())
        .map_err(|_| MalformedError::IntegerOverflow)?;
    patterns
        .try_reserve(1)
        .map_err(|_| DecodeError::Allocation { requested_bytes })?;
    let mut retained_bytes = 0_u64;
    let mut decoded_index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant decoded_index <= pattern_bitmaps@.len(),
        decreases pattern_bitmaps@.len() - decoded_index,
    ))]
    while decoded_index < pattern_bitmaps.len() {
        retained_bytes = retained_bytes
            .checked_add(bitmap_byte_len(&pattern_bitmaps[decoded_index])?)
            .ok_or(DecodeError::Malformed(MalformedError::IntegerOverflow))?;
        decoded_index += 1;
    }
    cumulative_work.charge(work.used(), limits)?;
    retained.retain(retained_bytes, limits)?;
    if !insert_patterns(patterns, segment.header.number, pattern_bitmaps) {
        return Err(MalformedError::DuplicateSegmentNumber(segment.header.number).into());
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        decode_spec::runtime_pages_valid(old(pages)),
        decode_spec::pattern_store_valid(patterns),
        decode_spec::region_buffers_valid(old(buffers)),
    ensures match out {
        Ok(()) =>
            decode_spec::runtime_pages_valid(final(pages))
                && decode_spec::region_buffers_valid(final(buffers)),
        Err(_) => true,
    },
))]
fn decode_halftone_segment(
    segment: &ParsedSegment<'_>,
    pages: &mut RuntimePages,
    buffers: &mut RegionBuffers,
    patterns: &PatternStore,
    retained: &mut RetainedBudget,
    cumulative_work: &mut DecodeWork,
    limits: &Limits,
) -> Result<(), DecodeError> {
    let pattern_number =
        first_referred_segment(&segment.header.referred_segments, segment.header.number)?;
    let dictionary =
        get_patterns(patterns, pattern_number).ok_or(MalformedError::MissingReferredSegment {
            segment: segment.header.number,
            referred: pattern_number,
        })?;
    let mut reader = ByteReader::new(segment.payload);
    let region = parse_region_info(&mut reader, limits)?;
    let header = parse_halftone_header(&mut reader).map_err(halftone_error_into_decode)?;
    let coded = reader.remaining_bytes();
    let mut work = WorkBudget::new(limits);

    let mut decoder = if header.mmr {
        new_gray_mmr_state(coded, limits)
    } else {
        let template = generic_template(header.template)?;
        new_halftone_arithmetic_state(coded, template, header.template, limits)?
    };
    let bitmap = decode_halftone_region(
        &header,
        region.width,
        region.height,
        dictionary,
        &mut decoder,
        limits,
        &mut work,
    )
    .map_err(halftone_error_into_decode)?;
    decoder.finish()?;

    cumulative_work.charge(work.used(), limits)?;
    if segment.header.segment_type == SegmentType::IntermediateHalftoneRegion {
        require_auxiliary_page(segment, pages)?;
        insert_buffer(
            buffers,
            segment.header.number,
            region,
            bitmap,
            retained,
            limits,
        )?;
    } else {
        compose_immediate(
            segment,
            pages,
            &region,
            &bitmap,
            false,
            cumulative_work,
            limits,
        )?;
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(number) =>
            referred@.len() > 0
                && number == referred@[0],
        Err(_) => referred@.len() == 0,
    },
))]
fn first_referred_segment(referred: &[u32], owner: u32) -> Result<u32, DecodeError> {
    match referred.first() {
        Some(number) => Ok(*number),
        None => Err(MalformedError::InvalidSegmentReferences(owner).into()),
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires decode_spec::symbol_store_valid(old(symbols)),
    ensures match out {
        Ok(()) => decode_spec::symbol_store_valid(final(symbols)),
        Err(_) => true,
    },
))]
fn decode_symbol_dictionary_segment(
    segment: &ParsedSegment<'_>,
    symbols: &mut SymbolStore,
    tables: &HuffmanTables,
    retained: &mut RetainedBudget,
    work: &mut DecodeWork,
    limits: &Limits,
) -> Result<(), DecodeError> {
    if symbol_index(symbols, segment.header.number).is_some() {
        return Err(MalformedError::DuplicateSegmentNumber(segment.header.number).into());
    }
    let imported = collect_referred_symbols(segment, symbols, retained.bytes, limits)?;
    let custom_tables = collect_referred_tables(segment, tables)?;
    let mut used_contexts = None;
    let mut reference_index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            reference_index <= segment.header.referred_segments@.len(),
            decode_spec::symbol_store_valid(symbols),
        decreases segment.header.referred_segments@.len() - reference_index,
    ))]
    while reference_index < segment.header.referred_segments.len() {
        let number = segment.header.referred_segments[reference_index];
        if let Some(resource) = get_symbol_resource(symbols, number) {
            if resource.contexts.is_some() {
                used_contexts = resource.contexts.as_ref();
            }
        }
        reference_index += 1;
    }
    let output = decode_symbol_dictionary(&SymbolDictionaryInput {
        payload: segment.payload,
        imported_symbols: imported.codebook(),
        custom_tables: &custom_tables,
        used_contexts,
        limits,
    })
    .map_err(symbol_text_error_into_decode)?;
    if output.consumed_bytes != segment.payload.len() {
        let trailing = segment
            .payload
            .len()
            .checked_sub(output.consumed_bytes)
            .and_then(|bytes| u64::try_from(bytes).ok())
            .ok_or(MalformedError::IntegerOverflow)?;
        return Err(MalformedError::TrailingData(trailing).into());
    }
    let mut bitmap_bytes = 0_u64;
    let mut bitmap_index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant bitmap_index <= output.exported_symbols@.len(),
        decreases output.exported_symbols@.len() - bitmap_index,
    ))]
    while bitmap_index < output.exported_symbols.len() {
        bitmap_bytes = bitmap_bytes
            .checked_add(bitmap_byte_len(&output.exported_symbols[bitmap_index])?)
            .ok_or(DecodeError::Malformed(MalformedError::IntegerOverflow))?;
        bitmap_index += 1;
    }
    retained.retain(bitmap_bytes, limits)?;
    let mut symbol_work = 0_u64;
    let mut work_index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant work_index <= output.exported_symbols@.len(),
        decreases output.exported_symbols@.len() - work_index,
    ))]
    while work_index < output.exported_symbols.len() {
        let bitmap = &output.exported_symbols[work_index];
        let pixels = u64::from(bitmap.width())
            .checked_mul(u64::from(bitmap.height()))
            .ok_or(MalformedError::IntegerOverflow)?;
        symbol_work = symbol_work
            .checked_add(pixels)
            .ok_or(MalformedError::IntegerOverflow)?;
        work_index += 1;
    }
    work.charge(symbol_work, limits)?;
    let resource = SymbolResource {
        exported: output.exported_symbols,
        contexts: output.retained_contexts,
    };
    if !insert_symbol_resource(symbols, segment.header.number, resource) {
        return Err(MalformedError::DuplicateSegmentNumber(segment.header.number).into());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        decode_spec::runtime_pages_valid(old(pages)),
        decode_spec::region_buffers_valid(old(buffers)),
        decode_spec::symbol_store_valid(symbols),
    ensures match out {
        Ok(()) =>
            decode_spec::runtime_pages_valid(final(pages))
                && decode_spec::region_buffers_valid(final(buffers)),
        Err(_) => true,
    },
))]
fn decode_text_segment(
    segment: &ParsedSegment<'_>,
    pages: &mut RuntimePages,
    buffers: &mut RegionBuffers,
    symbols: &SymbolStore,
    tables: &HuffmanTables,
    retained: &mut RetainedBudget,
    work: &mut DecodeWork,
    limits: &Limits,
) -> Result<(), DecodeError> {
    let available_symbols = collect_referred_symbols(segment, symbols, retained.bytes, limits)?;
    let custom_tables = collect_referred_tables(segment, tables)?;
    let output = decode_text_region(&TextRegionInput {
        payload: segment.payload,
        symbols: available_symbols.codebook(),
        custom_tables: &custom_tables,
        limits,
    })
    .map_err(symbol_text_error_into_decode)?;
    if output.consumed_bytes != segment.payload.len() {
        let trailing = segment
            .payload
            .len()
            .checked_sub(output.consumed_bytes)
            .and_then(|bytes| u64::try_from(bytes).ok())
            .ok_or(MalformedError::IntegerOverflow)?;
        return Err(MalformedError::TrailingData(trailing).into());
    }
    let region_work = u64::from(output.region.width)
        .checked_mul(u64::from(output.region.height))
        .ok_or(MalformedError::IntegerOverflow)?;
    work.charge(region_work, limits)?;
    if segment.header.segment_type == SegmentType::IntermediateTextRegion {
        require_auxiliary_page(segment, pages)?;
        insert_buffer(
            buffers,
            segment.header.number,
            output.region,
            output.bitmap,
            retained,
            limits,
        )?;
    } else {
        compose_immediate(
            segment,
            pages,
            &output.region,
            &output.bitmap,
            false,
            work,
            limits,
        )?;
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires decode_spec::symbol_store_valid(symbols),
    ensures match out {
        Ok(bitmaps) => decode_spec::collected_symbols_valid(&bitmaps),
        Err(_) => true,
    },
))]
fn collect_referred_symbols<'a>(
    segment: &ParsedSegment<'_>,
    symbols: &'a SymbolStore,
    active_retained_bytes: u64,
    limits: &Limits,
) -> Result<CollectedSymbols<'a>, DecodeError> {
    let mut count = 0_usize;
    let mut nonempty_groups = 0_usize;
    let mut contiguous: Option<&[Bitmap]> = None;
    let mut reference_index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            reference_index <= segment.header.referred_segments@.len(),
            decode_spec::symbol_store_valid(symbols),
            match contiguous {
                Some(bitmaps) =>
                    crate::halftone::halftone_spec::bitmaps_valid(bitmaps@),
                None => true,
            },
        decreases segment.header.referred_segments@.len() - reference_index,
    ))]
    while reference_index < segment.header.referred_segments.len() {
        let number = segment.header.referred_segments[reference_index];
        if let Some(resource) = get_symbol_resource(symbols, number) {
            if !resource.exported.is_empty() {
                nonempty_groups = nonempty_groups
                    .checked_add(1)
                    .ok_or(MalformedError::IntegerOverflow)?;
                contiguous = Some(&resource.exported);
            }
            let mut bitmap_index = 0_usize;
            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant
                    bitmap_index <= resource.exported@.len(),
                    crate::halftone::halftone_spec::bitmaps_valid(
                        resource.exported@
                    ),
                decreases resource.exported@.len() - bitmap_index,
            ))]
            while bitmap_index < resource.exported.len() {
                count = count
                    .checked_add(1)
                    .ok_or(MalformedError::IntegerOverflow)?;
                bitmap_index += 1;
            }
        }
        reference_index += 1;
    }
    if nonempty_groups <= 1 {
        let direct = match contiguous {
            Some(bitmaps) => bitmaps,
            None => &[],
        };
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal(decode_spec::collected_symbols_valid);
        }
        return Ok(CollectedSymbols::Contiguous(direct));
    }
    let (requested_bytes, peak) = borrowed_symbol_memory(count, active_retained_bytes)?;
    if peak > limits.max_decoder_managed_memory_bytes {
        return Err(crate::LimitExceeded {
            kind: crate::LimitKind::Memory,
            limit: limits.max_decoder_managed_memory_bytes,
            actual: peak,
        }
        .into());
    }
    let mut imported: Vec<&Bitmap> = Vec::new();
    if imported.try_reserve(count).is_err() {
        return Err(DecodeError::Allocation { requested_bytes });
    }
    reference_index = 0;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            reference_index <= segment.header.referred_segments@.len(),
            decode_spec::symbol_store_valid(symbols),
            decode_spec::bitmap_refs_valid(imported@),
        decreases segment.header.referred_segments@.len() - reference_index,
    ))]
    while reference_index < segment.header.referred_segments.len() {
        let number = segment.header.referred_segments[reference_index];
        if let Some(resource) = get_symbol_resource(symbols, number) {
            let mut bitmap_index = 0_usize;
            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant
                    bitmap_index <= resource.exported@.len(),
                    crate::halftone::halftone_spec::bitmaps_valid(
                        resource.exported@
                    ),
                    decode_spec::bitmap_refs_valid(imported@),
                decreases resource.exported@.len() - bitmap_index,
            ))]
            while bitmap_index < resource.exported.len() {
                imported.push(&resource.exported[bitmap_index]);
                bitmap_index += 1;
            }
        }
        reference_index += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        reveal(decode_spec::collected_symbols_valid);
    }
    Ok(CollectedSymbols::Borrowed(imported))
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn borrowed_symbol_memory(
    count: usize,
    active_retained_bytes: u64,
) -> Result<(u64, u64), DecodeError> {
    let requested_bytes = allocation_size::<&Bitmap>(count)?;
    let peak = active_retained_bytes
        .checked_add(requested_bytes)
        .ok_or(MalformedError::IntegerOverflow)?;
    Ok((requested_bytes, peak))
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn collect_referred_tables<'a>(
    segment: &ParsedSegment<'_>,
    tables: &'a HuffmanTables,
) -> Result<Vec<&'a HuffmanTable>, DecodeError> {
    let mut count = 0_usize;
    let mut reference_index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant reference_index <= segment.header.referred_segments@.len(),
        decreases segment.header.referred_segments@.len() - reference_index,
    ))]
    while reference_index < segment.header.referred_segments.len() {
        if huffman_table_index(tables, segment.header.referred_segments[reference_index]).is_some()
        {
            count = count
                .checked_add(1)
                .ok_or(MalformedError::IntegerOverflow)?;
        }
        reference_index += 1;
    }
    let requested_bytes = allocation_size::<&HuffmanTable>(count)?;
    let mut result = Vec::new();
    if result.try_reserve(count).is_err() {
        return Err(DecodeError::Allocation { requested_bytes });
    }
    reference_index = 0;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant reference_index <= segment.header.referred_segments@.len(),
        decreases segment.header.referred_segments@.len() - reference_index,
    ))]
    while reference_index < segment.header.referred_segments.len() {
        let number = segment.header.referred_segments[reference_index];
        if let Some(table) = get_huffman_table(tables, number) {
            result.push(table);
        }
        reference_index += 1;
    }
    Ok(result)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(value) => decode_spec::generic_template_code(value) == template as nat,
        Err(_) => template > 3,
    },
))]
fn generic_template(template: u8) -> Result<GenericTemplate, DecodeError> {
    let template = match template {
        0 => GenericTemplate::Template0,
        1 => GenericTemplate::Template1,
        2 => GenericTemplate::Template2,
        3 => GenericTemplate::Template3,
        _ => return Err(MalformedError::InvalidPageState("invalid generic template").into()),
    };
    Ok(template)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out == (left == right),
))]
fn same_combination_operator(
    left: crate::bitmap::CombinationOperator,
    right: crate::bitmap::CombinationOperator,
) -> bool {
    match left {
        crate::bitmap::CombinationOperator::Or => {
            matches!(right, crate::bitmap::CombinationOperator::Or)
        }
        crate::bitmap::CombinationOperator::And => {
            matches!(right, crate::bitmap::CombinationOperator::And)
        }
        crate::bitmap::CombinationOperator::Xor => {
            matches!(right, crate::bitmap::CombinationOperator::Xor)
        }
        crate::bitmap::CombinationOperator::Xnor => {
            matches!(right, crate::bitmap::CombinationOperator::Xnor)
        }
        crate::bitmap::CombinationOperator::Replace => {
            matches!(right, crate::bitmap::CombinationOperator::Replace)
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) =>
            combination_override
                || region_operator == page_operator
                || refinement,
        Err(_) =>
            !combination_override
                && region_operator != page_operator
                && !refinement,
    },
))]
fn check_combination_operator(
    combination_override: bool,
    region_operator: crate::bitmap::CombinationOperator,
    page_operator: crate::bitmap::CombinationOperator,
    refinement: bool,
) -> Result<(), DecodeError> {
    let same_operator = same_combination_operator(region_operator, page_operator);
    if !combination_override && !same_operator && !refinement {
        return Err(MalformedError::InvalidPageState(
            "region overrides the page combination operator",
        )
        .into());
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) => equal,
        Err(_) => !equal,
    },
))]
fn require_matching_geometry(equal: bool) -> Result<(), DecodeError> {
    if !equal {
        return Err(MalformedError::InvalidPageState(
            "refinement geometry differs from its intermediate reference",
        )
        .into());
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        decode_spec::runtime_pages_valid(old(pages)),
        crate::spec::bitmap::storage_valid(bitmap),
    ensures decode_spec::runtime_pages_valid(final(pages)),
))]
fn compose_immediate(
    segment: &ParsedSegment<'_>,
    pages: &mut RuntimePages,
    region: &crate::segment_data::RegionInfo,
    bitmap: &Bitmap,
    refinement: bool,
    work: &mut DecodeWork,
    limits: &Limits,
) -> Result<(), DecodeError> {
    let index = runtime_page_index(pages, segment.header.page_association).ok_or(
        MalformedError::MissingPageInformation(segment.header.page_association),
    )?;
    #[cfg(verus_keep_ghost)]
    proof_decl! {
        let ghost original_pages = pages@;
    }
    let result = compose_immediate_on_page(
        &mut pages[index].1,
        region,
        bitmap,
        refinement,
        work,
        limits,
    );
    #[cfg(verus_keep_ghost)]
    proof! {
        reveal(decode_spec::runtime_pages_valid);
        assert forall|entry: int| 0 <= entry < pages@.len()
            implies crate::spec::bitmap::storage_valid(
                #[trigger] &pages@[entry].1.bitmap
            ) by {
            if entry == index {
            } else {
                assert(pages@[entry] == original_pages[entry]);
            }
        }
    }
    result
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        crate::spec::bitmap::storage_valid(&old(page).bitmap),
        crate::spec::bitmap::storage_valid(bitmap),
    ensures crate::spec::bitmap::storage_valid(&final(page).bitmap),
))]
fn compose_immediate_on_page(
    page: &mut RuntimePage,
    region: &crate::segment_data::RegionInfo,
    bitmap: &Bitmap,
    refinement: bool,
    work: &mut DecodeWork,
    limits: &Limits,
) -> Result<(), DecodeError> {
    check_combination_operator(
        page.combination_override,
        region.operator,
        page.default_operator,
        refinement,
    )?;
    note_page_write(page, region)?;
    let composition_work = u64::from(bitmap.width())
        .checked_mul(u64::from(bitmap.height()))
        .ok_or(MalformedError::IntegerOverflow)?;
    work.charge(composition_work, limits)?;
    page.bitmap
        .compose(bitmap, region.x, region.y, region.operator)?;
    page.pristine = false;
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(next_open) =>
            if decode_spec::region_intersects_page(
                region_width,
                region_height,
                region_x,
                region_y,
                page_width,
                page_height,
            ) {
                next_open.is_some()
                    && match last_stripe_end {
                        Some(sealed) => region_y > sealed,
                        None => true,
                    }
                    && match open_stripe_max_row {
                        Some(previous) => next_open.unwrap() >= previous,
                        None => true,
                    }
            } else {
                next_open == open_stripe_max_row
            },
        Err(_) => true,
    },
))]
#[allow(clippy::too_many_arguments)]
fn checked_page_write_state(
    last_stripe_end: Option<u32>,
    open_stripe_max_row: Option<u32>,
    region_width: u32,
    region_height: u32,
    region_x: u32,
    region_y: u32,
    page_width: u32,
    page_height: u32,
) -> Result<Option<u32>, DecodeError> {
    if region_width == 0 || region_height == 0 || region_x >= page_width || region_y >= page_height
    {
        return Ok(open_stripe_max_row);
    }
    if let Some(sealed) = last_stripe_end {
        if region_y <= sealed {
            return Err(MalformedError::InvalidPageState("region modifies a sealed stripe").into());
        }
    }
    let bottom = region_y
        .checked_add(region_height)
        .ok_or(MalformedError::IntegerOverflow)?
        .min(page_height)
        .checked_sub(1)
        .ok_or(MalformedError::IntegerOverflow)?;
    let next = match open_stripe_max_row {
        Some(current) => current.max(bottom),
        None => bottom,
    };
    Ok(Some(next))
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires crate::spec::bitmap::storage_valid(&old(page).bitmap),
    ensures
        crate::spec::bitmap::storage_valid(&final(page).bitmap),
        final(page).bitmap.width == old(page).bitmap.width,
        final(page).bitmap.height == old(page).bitmap.height,
        final(page).bitmap.stride == old(page).bitmap.stride,
        final(page).bitmap.data@ == old(page).bitmap.data@,
))]
fn note_page_write(
    page: &mut RuntimePage,
    region: &crate::segment_data::RegionInfo,
) -> Result<(), DecodeError> {
    page.open_stripe_max_row = checked_page_write_state(
        page.last_stripe_end,
        page.open_stripe_max_row,
        region.width,
        region.height,
        region.x,
        region.y,
        page.bitmap.width(),
        page.bitmap.height(),
    )?;
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(next_open) =>
            next_open.is_none()
                && match open_stripe_max_row {
                    Some(maximum) => maximum <= row,
                    None => true,
                },
        Err(_) =>
            match open_stripe_max_row {
                Some(maximum) => maximum > row,
                None => false,
            },
    },
))]
fn checked_seal_stripe_state(
    open_stripe_max_row: Option<u32>,
    row: u32,
) -> Result<Option<u32>, DecodeError> {
    if let Some(maximum) = open_stripe_max_row {
        if maximum > row {
            return Err(MalformedError::InvalidPageState(
                "region preceding stripe extends below its end row",
            )
            .into());
        }
    }
    Ok(None)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires decode_spec::runtime_pages_valid(old(pages)),
    ensures decode_spec::runtime_pages_valid(final(pages)),
))]
fn seal_stripe(pages: &mut RuntimePages, page_number: u32, row: u32) -> Result<(), DecodeError> {
    let index = runtime_page_index(pages, page_number)
        .ok_or(MalformedError::MissingPageInformation(page_number))?;
    #[cfg(verus_keep_ghost)]
    proof_decl! {
        let ghost original_pages = pages@;
    }
    let result = seal_stripe_on_page(&mut pages[index].1, row);
    #[cfg(verus_keep_ghost)]
    proof! {
        reveal(decode_spec::runtime_pages_valid);
        assert forall|entry: int| 0 <= entry < pages@.len()
            implies crate::spec::bitmap::storage_valid(
                #[trigger] &pages@[entry].1.bitmap
            ) by {
            if entry == index {
            } else {
                assert(pages@[entry] == original_pages[entry]);
            }
        }
    }
    result
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires crate::spec::bitmap::storage_valid(&old(page).bitmap),
    ensures crate::spec::bitmap::storage_valid(&final(page).bitmap),
))]
fn seal_stripe_on_page(page: &mut RuntimePage, row: u32) -> Result<(), DecodeError> {
    let next_open = checked_seal_stripe_state(page.open_stripe_max_row, row)?;
    page.last_stripe_end = Some(row);
    page.open_stripe_max_row = next_open;
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires decode_spec::runtime_pages_valid(pages),
))]
fn require_auxiliary_page(
    segment: &ParsedSegment<'_>,
    pages: &RuntimePages,
) -> Result<(), DecodeError> {
    let page = get_runtime_page(pages, segment.header.page_association).ok_or(
        MalformedError::MissingPageInformation(segment.header.page_association),
    )?;
    require_auxiliary_page_state(page)
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn require_auxiliary_page_state(page: &RuntimePage) -> Result<(), DecodeError> {
    if !page.auxiliary_allowed {
        return Err(MalformedError::InvalidPageState(
            "intermediate region on page that forbids auxiliary buffers",
        )
        .into());
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        decode_spec::region_buffers_valid(old(buffers)),
        crate::spec::bitmap::storage_valid(&bitmap),
    ensures match out {
        Ok(()) => decode_spec::region_buffers_valid(final(buffers)),
        Err(_) => true,
    },
))]
fn insert_buffer(
    buffers: &mut RegionBuffers,
    number: u32,
    region: crate::segment_data::RegionInfo,
    bitmap: Bitmap,
    retained: &mut RetainedBudget,
    limits: &Limits,
) -> Result<(), DecodeError> {
    if region_buffer_index(buffers, number).is_some() {
        return Err(MalformedError::DuplicateSegmentNumber(number).into());
    }
    let requested_bytes = allocation_size::<(u32, RegionBuffer)>(1)?;
    if buffers.try_reserve(1).is_err() {
        return Err(DecodeError::Allocation { requested_bytes });
    }
    retain_bitmap(retained, &bitmap, limits)?;
    buffers.push((number, RegionBuffer { region, bitmap }));
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires decode_spec::region_buffers_valid(old(buffers)),
    ensures decode_spec::region_buffers_valid(final(buffers)),
))]
fn release_referred_buffers(
    segment: &ParsedSegment<'_>,
    buffers: &mut RegionBuffers,
    retained: &mut RetainedBudget,
) -> Result<(), DecodeError> {
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index <= segment.header.referred_segments@.len(),
            decode_spec::region_buffers_valid(buffers),
        decreases segment.header.referred_segments@.len() - index,
    ))]
    while index < segment.header.referred_segments.len() {
        let number = segment.header.referred_segments[index];
        if segment
            .header
            .retention_flags
            .retain_referred_segment(index)
            == Some(false)
        {
            if let Some(buffer_index) = region_buffer_index(buffers, number) {
                let buffer = buffers.remove(buffer_index).1;
                release_bitmap(retained, &buffer.bitmap)?;
            }
        }
        index += 1;
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) =>
            decode_spec::retained_bytes(final(retained)) as nat
                == decode_spec::retained_bytes(old(retained)) as nat
                    + decode_spec::bitmap_byte_count(bitmap)
            && decode_spec::retained_bytes(final(retained))
                <= limits.max_retained_state_bytes,
        Err(_) =>
            decode_spec::retained_bytes(final(retained))
                == decode_spec::retained_bytes(old(retained)),
    },
))]
fn retain_bitmap(
    retained: &mut RetainedBudget,
    bitmap: &Bitmap,
    limits: &Limits,
) -> Result<(), DecodeError> {
    retained.retain(bitmap_byte_len(bitmap)?, limits)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) =>
            decode_spec::retained_bytes(final(retained)) as nat
                    + decode_spec::bitmap_byte_count(bitmap)
                == decode_spec::retained_bytes(old(retained)) as nat,
        Err(_) =>
            decode_spec::retained_bytes(final(retained))
                == decode_spec::retained_bytes(old(retained)),
    },
))]
fn release_bitmap(retained: &mut RetainedBudget, bitmap: &Bitmap) -> Result<(), DecodeError> {
    retained.release(bitmap_byte_len(bitmap)?)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires decode_spec::pattern_store_valid(old(patterns)),
    ensures decode_spec::pattern_store_valid(final(patterns)),
))]
fn release_referred_patterns(
    segment: &ParsedSegment<'_>,
    patterns: &mut PatternStore,
    retained: &mut RetainedBudget,
) -> Result<(), DecodeError> {
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index <= segment.header.referred_segments@.len(),
            decode_spec::pattern_store_valid(patterns),
        decreases segment.header.referred_segments@.len() - index,
    ))]
    while index < segment.header.referred_segments.len() {
        let number = segment.header.referred_segments[index];
        if segment
            .header
            .retention_flags
            .retain_referred_segment(index)
            == Some(false)
        {
            if let Some(pattern_index) = pattern_index(patterns, number) {
                #[cfg(verus_keep_ghost)]
                proof_decl! {
                    let ghost original_patterns = patterns@;
                }
                let bitmaps = patterns.remove(pattern_index).1;
                #[cfg(verus_keep_ghost)]
                proof! {
                    reveal(decode_spec::pattern_store_valid);
                    assert forall|entry: int| 0 <= entry < patterns@.len()
                        implies crate::halftone::halftone_spec::bitmaps_valid(
                            #[trigger] patterns@[entry].1@
                        ) by {
                        if entry < pattern_index {
                            assert(patterns@[entry] == original_patterns[entry]);
                        } else {
                            assert(patterns@[entry] == original_patterns[entry + 1]);
                        }
                    }
                }
                let mut bitmap_index = 0_usize;
                let mut released = 0_u64;
                #[cfg_attr(verus_keep_ghost, verus_spec(
                    invariant
                        bitmap_index <= bitmaps@.len(),
                        decode_spec::pattern_store_valid(patterns),
                        crate::halftone::halftone_spec::bitmaps_valid(bitmaps@),
                    decreases bitmaps@.len() - bitmap_index,
                ))]
                while bitmap_index < bitmaps.len() {
                    released = released
                        .checked_add(bitmap_byte_len(&bitmaps[bitmap_index])?)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    bitmap_index += 1;
                }
                retained.release(released)?;
            }
        }
        index += 1;
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires decode_spec::symbol_store_valid(old(symbols)),
    ensures decode_spec::symbol_store_valid(final(symbols)),
))]
fn release_referred_symbols(
    segment: &ParsedSegment<'_>,
    symbols: &mut SymbolStore,
    retained: &mut RetainedBudget,
) -> Result<(), DecodeError> {
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index <= segment.header.referred_segments@.len(),
            decode_spec::symbol_store_valid(symbols),
        decreases segment.header.referred_segments@.len() - index,
    ))]
    while index < segment.header.referred_segments.len() {
        let number = segment.header.referred_segments[index];
        if segment
            .header
            .retention_flags
            .retain_referred_segment(index)
            == Some(false)
        {
            if let Some(symbol_index) = symbol_index(symbols, number) {
                #[cfg(verus_keep_ghost)]
                proof_decl! {
                    let ghost original_symbols = symbols@;
                }
                let resource = symbols.remove(symbol_index).1;
                #[cfg(verus_keep_ghost)]
                proof! {
                    reveal(decode_spec::symbol_store_valid);
                    assert forall|entry: int| 0 <= entry < symbols@.len()
                        implies crate::halftone::halftone_spec::bitmaps_valid(
                            #[trigger] symbols@[entry].1.exported@
                        ) by {
                        if entry < symbol_index {
                            assert(symbols@[entry] == original_symbols[entry]);
                            assert(crate::halftone::halftone_spec::bitmaps_valid(
                                original_symbols[entry].1.exported@,
                            ));
                        } else {
                            assert(symbols@[entry] == original_symbols[entry + 1]);
                            assert(crate::halftone::halftone_spec::bitmaps_valid(
                                original_symbols[entry + 1].1.exported@,
                            ));
                        }
                    }
                }
                let mut bitmap_index = 0_usize;
                let mut released = 0_u64;
                #[cfg_attr(verus_keep_ghost, verus_spec(
                    invariant
                        bitmap_index <= resource.exported@.len(),
                        decode_spec::symbol_store_valid(symbols),
                    decreases resource.exported@.len() - bitmap_index,
                ))]
                while bitmap_index < resource.exported.len() {
                    released = released
                        .checked_add(bitmap_byte_len(&resource.exported[bitmap_index])?)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    bitmap_index += 1;
                }
                retained.release(released)?;
            }
        }
        index += 1;
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn release_referred_tables(segment: &ParsedSegment<'_>, tables: &mut HuffmanTables) {
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= segment.header.referred_segments@.len(),
        decreases segment.header.referred_segments@.len() - index,
    ))]
    while index < segment.header.referred_segments.len() {
        let number = segment.header.referred_segments[index];
        if segment
            .header
            .retention_flags
            .retain_referred_segment(index)
            == Some(false)
        {
            if let Some(table_index) = huffman_table_index(tables, number) {
                tables.remove(table_index);
            }
        }
        index += 1;
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires crate::spec::bitmap::storage_valid(page),
    ensures match out {
        Ok(snapshot) =>
            crate::spec::bitmap::storage_valid(&snapshot)
            && crate::views::bitmap_width(&snapshot) == region.width
            && crate::views::bitmap_height(&snapshot) == region.height,
        Err(_) => true,
    },
))]
fn snapshot_region(
    page: &Bitmap,
    region: &crate::segment_data::RegionInfo,
    limits: &Limits,
) -> Result<Bitmap, DecodeError> {
    let mut snapshot = Bitmap::blank(region.width, region.height, limits)?;
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::views::bitmap_fields(&snapshot);
        assert(snapshot.width == region.width);
        assert(snapshot.height == region.height);
    }
    let width = region.width.min(page.width().saturating_sub(region.x));
    let height = region.height.min(page.height().saturating_sub(region.y));
    if width != 0 && height != 0 {
        snapshot.compose_region(
            page,
            region.x,
            region.y,
            0,
            0,
            width,
            height,
            CombinationOperator::Replace,
        )?;
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::views::bitmap_fields(&snapshot);
        reveal(crate::spec::bitmap::storage_valid);
        assert(crate::spec::bitmap::storage_valid(&snapshot));
        assert(crate::views::bitmap_width(&snapshot) == region.width);
        assert(crate::views::bitmap_height(&snapshot) == region.height);
    }
    Ok(snapshot)
}

#[cfg(test)]
fn clipped_coordinate(origin: u32, offset: u32, bound: u32) -> Option<u32> {
    let coordinate = origin.checked_add(offset)?;
    if coordinate < bound {
        Some(coordinate)
    } else {
        None
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(page) => crate::views::page_number(page) == number,
        Err(_) => true,
    },
))]
fn find_page_info(info: &DocumentInfo, number: u32) -> Result<&PageInfo, DecodeError> {
    let pages = info.pages();
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= pages@.len(),
        decreases pages@.len() - index,
    ))]
    while index < pages.len() {
        if pages[index].number() == number {
            return Ok(&pages[index]);
        }
        index += 1;
    }
    Err(MalformedError::MissingPageInformation(number).into())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(bytes) => bytes as nat == decode_spec::allocation_bytes::<T>(count),
        Err(_) => true,
    },
))]
fn allocation_size<T>(count: usize) -> Result<u64, DecodeError> {
    let Some(bytes) = count.checked_mul(std::mem::size_of::<T>()) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    let Ok(bytes) = u64::try_from(bytes) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    Ok(bytes)
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn structural_work(
    input_length: usize,
    segments: &[ParsedSegment<'_>],
) -> Result<u64, DecodeError> {
    let Ok(input) = u64::try_from(input_length) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    let Ok(segment_count) = u64::try_from(segments.len()) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    let mut references = 0_u64;
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= segments@.len(),
        decreases segments@.len() - index,
    ))]
    while index < segments.len() {
        let Ok(count) = u64::try_from(segments[index].header.referred_segments.len()) else {
            return Err(MalformedError::IntegerOverflow.into());
        };
        let Some(next) = references.checked_add(count) else {
            return Err(MalformedError::IntegerOverflow.into());
        };
        references = next;
        index += 1;
    }
    structural_work_totals(input, segment_count, references)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(total) =>
            total as nat
                == input as nat + segment_count as nat + references as nat
            && total >= input
            && total >= segment_count
            && total >= references,
        Err(_) =>
            input as nat + segment_count as nat + references as nat
                > u64::MAX as nat,
    },
))]
fn structural_work_totals(
    input: u64,
    segment_count: u64,
    references: u64,
) -> Result<u64, DecodeError> {
    checked_work_add(checked_work_add(input, segment_count)?, references)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(total) =>
            total as nat == left as nat + right as nat
                && total >= left
                && total >= right,
        Err(_) => left as nat + right as nat > u64::MAX as nat,
    },
))]
fn checked_work_add(left: u64, right: u64) -> Result<u64, DecodeError> {
    match left.checked_add(right) {
        Some(total) => Ok(total),
        None => Err(MalformedError::IntegerOverflow.into()),
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(bytes) => bytes as nat == decode_spec::bitmap_byte_count(bitmap),
        Err(_) => decode_spec::bitmap_byte_count(bitmap) > u64::MAX as nat,
    },
))]
fn bitmap_byte_len(bitmap: &Bitmap) -> Result<u64, DecodeError> {
    u64::try_from(bitmap.bytes().len()).map_err(|_error| MalformedError::IntegerOverflow.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_combination_operator_pairs_match_derived_equality() {
        let operators = [
            CombinationOperator::Or,
            CombinationOperator::And,
            CombinationOperator::Xor,
            CombinationOperator::Xnor,
            CombinationOperator::Replace,
        ];
        for left in operators {
            for right in operators {
                assert_eq!(same_combination_operator(left, right), left == right);
            }
        }
    }

    const SIGNATURE: [u8; 8] = [0x97, 0x4A, 0x42, 0x32, 0x0D, 0x0A, 0x1A, 0x0A];

    fn segment_header(number: u32, segment_type: u8, page: u8, length: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&number.to_be_bytes());
        bytes.push(segment_type);
        bytes.push(0);
        bytes.push(page);
        bytes.extend_from_slice(&length.to_be_bytes());
        bytes
    }

    fn referenced_segment_header(
        number: u32,
        segment_type: u8,
        page: u8,
        referred: u8,
        length: u32,
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&number.to_be_bytes());
        bytes.push(segment_type);
        bytes.push(0x20);
        bytes.push(referred);
        bytes.push(page);
        bytes.extend_from_slice(&length.to_be_bytes());
        bytes
    }

    fn page_data() -> [u8; 19] {
        let mut data = [0_u8; 19];
        data[0..4].copy_from_slice(&8_u32.to_be_bytes());
        data[4..8].copy_from_slice(&1_u32.to_be_bytes());
        data[16] = 0x40;
        data
    }

    #[test]
    fn borrowed_symbol_memory_counts_only_pointer_capacity() {
        let retained = 4_096_u64;
        let count = 37_usize;
        let (allocation, peak) =
            borrowed_symbol_memory(count, retained).expect("representable pointer storage");
        assert_eq!(
            allocation,
            u64::try_from(count * core::mem::size_of::<&Bitmap>()).expect("allocation")
        );
        assert_eq!(peak, retained + allocation);
    }

    fn generic_data(coded: &[u8]) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&8_u32.to_be_bytes());
        data.extend_from_slice(&1_u32.to_be_bytes());
        data.extend_from_slice(&0_u32.to_be_bytes());
        data.extend_from_slice(&0_u32.to_be_bytes());
        data.push(0);
        data.push(1);
        data.extend_from_slice(coded);
        data
    }

    fn document_with_generic(coded: &[u8]) -> Vec<u8> {
        let generic = generic_data(coded);
        let mut bytes = SIGNATURE.to_vec();
        bytes.push(0x01);
        bytes.extend_from_slice(&1_u32.to_be_bytes());
        bytes.extend_from_slice(&segment_header(0, 48, 1, 19));
        bytes.extend_from_slice(&page_data());
        bytes.extend_from_slice(&segment_header(
            1,
            38,
            1,
            u32::try_from(generic.len()).expect("test payload fits"),
        ));
        bytes.extend_from_slice(&generic);
        bytes.extend_from_slice(&segment_header(2, 49, 1, 0));
        bytes.extend_from_slice(&segment_header(3, 51, 0, 0));
        bytes
    }

    fn arithmetic_generic_document(
        width: u32,
        height: u32,
        region_flags: u8,
        coded: &[u8],
    ) -> Vec<u8> {
        let mut page = [0_u8; 19];
        page[0..4].copy_from_slice(&width.to_be_bytes());
        page[4..8].copy_from_slice(&height.to_be_bytes());
        let mut generic = Vec::new();
        generic.extend_from_slice(&width.to_be_bytes());
        generic.extend_from_slice(&height.to_be_bytes());
        generic.extend_from_slice(&0_u32.to_be_bytes());
        generic.extend_from_slice(&0_u32.to_be_bytes());
        generic.push(region_flags);
        generic.push(0);
        generic.extend_from_slice(&[3, 0xff, 0xfd, 0xff, 2, 0xfe, 0xfe, 0xfe]);
        generic.extend_from_slice(coded);
        let mut bytes = SIGNATURE.to_vec();
        bytes.push(0x01);
        bytes.extend_from_slice(&1_u32.to_be_bytes());
        bytes.extend_from_slice(&segment_header(0, 48, 1, 19));
        bytes.extend_from_slice(&page);
        bytes.extend_from_slice(&segment_header(
            1,
            38,
            1,
            u32::try_from(generic.len()).expect("payload fits"),
        ));
        bytes.extend_from_slice(&generic);
        bytes.extend_from_slice(&segment_header(2, 49, 1, 0));
        bytes.extend_from_slice(&segment_header(3, 51, 0, 0));
        bytes
    }

    fn arithmetic_test_bytes(state: &mut u64) -> Vec<u8> {
        let mut coded = Vec::with_capacity(1_026);
        for _ in 0..1_024 {
            *state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            coded.push(state.to_be_bytes()[3] & 0x7f);
        }
        coded.extend_from_slice(&[0xff, 0xac]);
        coded
    }

    fn all_white_mmr_document(width: u32, height: u32) -> Vec<u8> {
        let mut page = [0_u8; 19];
        page[0..4].copy_from_slice(&width.to_be_bytes());
        page[4..8].copy_from_slice(&height.to_be_bytes());
        page[16] = 0x40;
        let mut generic = Vec::new();
        generic.extend_from_slice(&width.to_be_bytes());
        generic.extend_from_slice(&height.to_be_bytes());
        generic.extend_from_slice(&0_u32.to_be_bytes());
        generic.extend_from_slice(&0_u32.to_be_bytes());
        generic.push(0);
        generic.push(1);
        let coded_len = usize::try_from(height.div_ceil(8)).expect("test height fits");
        generic.extend(std::iter::repeat_n(0xFF, coded_len));
        if height % 8 != 0 {
            *generic.last_mut().expect("coded byte") = 0xFF << (8 - height % 8);
        }
        let mut bytes = SIGNATURE.to_vec();
        bytes.push(0x01);
        bytes.extend_from_slice(&1_u32.to_be_bytes());
        bytes.extend_from_slice(&segment_header(0, 48, 1, 19));
        bytes.extend_from_slice(&page);
        bytes.extend_from_slice(&segment_header(
            1,
            38,
            1,
            u32::try_from(generic.len()).expect("payload fits"),
        ));
        bytes.extend_from_slice(&generic);
        bytes.extend_from_slice(&segment_header(2, 49, 1, 0));
        bytes.extend_from_slice(&segment_header(3, 51, 0, 0));
        bytes
    }

    #[test]
    fn decodes_generic_page_and_collects_document() {
        let decoded = Decoder::default()
            .decode(&document_with_generic(&[0x26, 0xA2, 0x80]))
            .expect("MMR page decodes");
        assert_eq!(decoded.pages().len(), 1);
        assert_eq!(decoded.pages()[0].number(), 1);
        assert_eq!(decoded.pages()[0].bitmap().bytes(), &[0xFF]);
    }

    fn runtime_test_page(
        width: u32,
        height: u32,
        default_pixel: bool,
        pristine: bool,
        operator: CombinationOperator,
        limits: &Limits,
    ) -> RuntimePage {
        RuntimePage {
            default_pixel,
            pristine,
            default_operator: operator,
            combination_override: false,
            refinements_allowed: false,
            auxiliary_allowed: false,
            last_stripe_end: None,
            open_stripe_max_row: None,
            bitmap: Bitmap::filled(width, height, default_pixel, limits).expect("test page bitmap"),
        }
    }

    fn assert_direct_page_matches_oracle(
        document: &[u8],
        default_pixel: bool,
        pristine: bool,
        page_operator: CombinationOperator,
        limits: &Limits,
    ) {
        let (_info, segments) =
            parse_standalone_document(document, &Limits::default()).expect("test document");
        let segment = &segments[1];
        let (header, _) =
            parse_generic_region_header(segment.payload, &Limits::default()).expect("header");
        let mut direct_pages = vec![(
            1,
            runtime_test_page(
                header.region.width,
                header.region.height,
                default_pixel,
                pristine,
                page_operator,
                &Limits::default(),
            ),
        )];
        let mut oracle_pages = vec![(
            1,
            runtime_test_page(
                header.region.width,
                header.region.height,
                default_pixel,
                pristine,
                page_operator,
                &Limits::default(),
            ),
        )];
        let mut direct_buffers = Vec::new();
        let mut oracle_buffers = Vec::new();
        let mut direct_retained = RetainedBudget::new();
        let mut oracle_retained = RetainedBudget::new();
        let mut direct_work = DecodeWork::new(0, limits).expect("direct work");
        let mut oracle_work = DecodeWork::new(0, limits).expect("oracle work");
        let mut direct_scratch = MmrScratch::default();
        let mut oracle_scratch = MmrScratch::default();
        let direct = decode_generic_segment(
            segment,
            &mut direct_pages,
            &mut direct_buffers,
            &mut direct_retained,
            &mut direct_work,
            limits,
            &mut direct_scratch,
        );
        let oracle = decode_generic_segment_oracle(
            segment,
            &mut oracle_pages,
            &mut oracle_buffers,
            &mut oracle_retained,
            &mut oracle_work,
            limits,
            &mut oracle_scratch,
        );
        assert_eq!(
            direct.as_ref().err().map(ToString::to_string),
            oracle.as_ref().err().map(ToString::to_string)
        );
        if direct.is_ok() {
            let direct_page = &direct_pages[0].1;
            let oracle_page = &oracle_pages[0].1;
            assert_eq!(direct_page.bitmap, oracle_page.bitmap);
            assert_eq!(direct_page.pristine, oracle_page.pristine);
            assert_eq!(
                direct_page.open_stripe_max_row,
                oracle_page.open_stripe_max_row
            );
            assert_eq!(direct_work.used, oracle_work.used);
            assert_eq!(direct_retained.bytes, oracle_retained.bytes);
        }
    }

    #[test]
    fn direct_page_matches_intermediate_compose_oracle() {
        let limits = Limits::default();
        let mut state = 0x5ca1_49e7_2d83_b604;
        for width in 1..=65_u32 {
            let height = 1 + width % 4;
            let coded = arithmetic_test_bytes(&mut state);
            let document = arithmetic_generic_document(width, height, 0, &coded);
            assert_direct_page_matches_oracle(
                &document,
                false,
                true,
                CombinationOperator::Or,
                &limits,
            );
        }
    }

    #[test]
    fn incompatible_and_limited_direct_pages_use_exact_oracle_fallback() {
        let mut state = 0xd630_18ab_795c_42ef;
        let coded = arithmetic_test_bytes(&mut state);
        let document = arithmetic_generic_document(97, 11, 0, &coded);
        for (default_pixel, pristine, operator) in [
            (true, true, CombinationOperator::Or),
            (false, false, CombinationOperator::Or),
            (false, true, CombinationOperator::Xor),
        ] {
            assert_direct_page_matches_oracle(
                &document,
                default_pixel,
                pristine,
                operator,
                &Limits::default(),
            );
        }
        let limits = Limits {
            max_work_units: 1,
            ..Limits::default()
        };
        assert_direct_page_matches_oracle(&document, false, true, CombinationOperator::Or, &limits);
    }

    #[test]
    fn reusable_context_matches_stateless_and_reuses_capacity() {
        let input = document_with_generic(&[0x80]);
        let decoder = Decoder::default();
        let expected = decoder.decode(&input).expect("stateless decode");
        let mut context = decoder.context();
        let first = context.decode(&input).expect("context decode");
        assert_eq!(first, expected);
        let retained = context.retained_scratch_bytes();
        let reserve_events = context.mmr.reserve_events;
        assert!(retained > 0);
        assert_eq!(reserve_events, 2);
        let second = context.decode(&input).expect("reused context decode");
        assert_eq!(second, expected);
        assert_eq!(context.retained_scratch_bytes(), retained);
        assert_eq!(context.mmr.reserve_events, reserve_events);
        context.release_scratch();
        assert_eq!(context.retained_scratch_bytes(), 0);
        assert_eq!(context.mmr.reserve_events, 0);
    }

    #[test]
    fn reusable_context_recovers_after_error_without_state_leakage() {
        let decoder = Decoder::default();
        let valid = document_with_generic(&[0x80]);
        let expected = decoder.decode(&valid).expect("stateless decode");
        let mut context = decoder.context();
        assert!(context.decode(&[0]).is_err());
        let recovered = context.decode(&valid).expect("decode after error");
        assert_eq!(recovered, expected);
        context.reset();
        assert_eq!(
            context.decode(&valid).expect("decode after reset"),
            expected
        );
    }

    #[test]
    fn reusable_context_obeys_retained_state_limit() {
        let limits = Limits {
            max_retained_state_bytes: 1,
            ..Limits::default()
        };
        let decoder = Decoder::new(limits);
        let mut context = decoder.context();
        context
            .decode(&document_with_generic(&[0x80]))
            .expect("scratch retention is optional");
        assert!(context.retained_scratch_bytes() <= 1);
    }

    #[test]
    fn reusable_context_handles_mixed_sizes_deterministically() {
        let decoder = Decoder::default();
        let small = all_white_mmr_document(64, 64);
        let large = all_white_mmr_document(512, 512);
        let mut context = decoder.context();
        let small_expected = decoder.decode(&small).expect("small stateless");
        let large_expected = decoder.decode(&large).expect("large stateless");
        assert_eq!(
            context.decode(&small).expect("small context"),
            small_expected
        );
        let small_capacity = context.retained_scratch_bytes();
        let small_events = context.mmr.reserve_events;
        assert_eq!(
            context.decode(&large).expect("large context"),
            large_expected
        );
        let large_capacity = context.retained_scratch_bytes();
        assert!(large_capacity >= small_capacity);
        assert!(context.mmr.reserve_events > small_events);
        let large_events = context.mmr.reserve_events;
        assert_eq!(
            context.decode(&small).expect("small reused"),
            small_expected
        );
        assert_eq!(context.retained_scratch_bytes(), large_capacity);
        assert_eq!(context.mmr.reserve_events, large_events);
    }

    #[test]
    #[ignore = "microbenchmark; run explicitly with --release --ignored --nocapture"]
    fn benchmark_reusable_context_against_d669954_stateless() {
        let decoder = Decoder::default();
        let same = all_white_mmr_document(64, 64);
        let mixed = [
            all_white_mmr_document(64, 64),
            same.clone(),
            all_white_mmr_document(2048, 2048),
            same.clone(),
        ];
        let mut stateless_times = Vec::new();
        let mut reused_times = Vec::new();
        for sample in 0..9 {
            if sample % 2 == 0 {
                let start = std::time::Instant::now();
                for _ in 0..50 {
                    std::hint::black_box(decoder.decode(&same).expect("stateless"));
                }
                stateless_times.push(start.elapsed());
                let mut context = decoder.context();
                let start = std::time::Instant::now();
                for _ in 0..50 {
                    std::hint::black_box(context.decode(&same).expect("reused"));
                }
                reused_times.push(start.elapsed());
            } else {
                let mut context = decoder.context();
                let start = std::time::Instant::now();
                for _ in 0..50 {
                    std::hint::black_box(context.decode(&same).expect("reused"));
                }
                reused_times.push(start.elapsed());
                let start = std::time::Instant::now();
                for _ in 0..50 {
                    std::hint::black_box(decoder.decode(&same).expect("stateless"));
                }
                stateless_times.push(start.elapsed());
            }
        }
        stateless_times.sort_unstable();
        reused_times.sort_unstable();
        let stateless = stateless_times[stateless_times.len() / 2];
        let reused = reused_times[reused_times.len() / 2];

        let mut stateless_mixed = Vec::new();
        let mut reused_mixed = Vec::new();
        for sample in 0..7 {
            if sample % 2 == 0 {
                let start = std::time::Instant::now();
                for _ in 0..20 {
                    for input in &mixed {
                        std::hint::black_box(decoder.decode(input).expect("stateless mixed"));
                    }
                }
                stateless_mixed.push(start.elapsed());
                let mut context = decoder.context();
                let start = std::time::Instant::now();
                for _ in 0..20 {
                    for input in &mixed {
                        std::hint::black_box(context.decode(input).expect("reused mixed"));
                    }
                }
                reused_mixed.push(start.elapsed());
            } else {
                let mut context = decoder.context();
                let start = std::time::Instant::now();
                for _ in 0..20 {
                    for input in &mixed {
                        std::hint::black_box(context.decode(input).expect("reused mixed"));
                    }
                }
                reused_mixed.push(start.elapsed());
                let start = std::time::Instant::now();
                for _ in 0..20 {
                    for input in &mixed {
                        std::hint::black_box(decoder.decode(input).expect("stateless mixed"));
                    }
                }
                stateless_mixed.push(start.elapsed());
            }
        }
        stateless_mixed.sort_unstable();
        reused_mixed.sort_unstable();
        let stateless_mixed = stateless_mixed[stateless_mixed.len() / 2];
        let reused_mixed = reused_mixed[reused_mixed.len() / 2];
        println!(
            "context same_stateless_ns={} same_reused_ns={} same_speedup={:.3} \
             mixed_stateless_ns={} mixed_reused_ns={} mixed_speedup={:.3}",
            stateless.as_nanos(),
            reused.as_nanos(),
            stateless.as_secs_f64() / reused.as_secs_f64(),
            stateless_mixed.as_nanos(),
            reused_mixed.as_nanos(),
            stateless_mixed.as_secs_f64() / reused_mixed.as_secs_f64(),
        );
    }

    #[test]
    fn page_sink_receives_completed_page() {
        let mut pages = 0;
        let mut sink = |page: Page| {
            pages += 1;
            assert_eq!(page.bitmap().width(), 8);
            Ok(())
        };
        Decoder::default()
            .decode_with_sink(&document_with_generic(&[0x80]), &mut sink)
            .expect("page streams");
        assert_eq!(pages, 1);
    }

    #[test]
    fn decodes_random_access_generic_page() {
        let generic = generic_data(&[0x80]);
        let mut bytes = SIGNATURE.to_vec();
        bytes.push(0x00);
        bytes.extend_from_slice(&1_u32.to_be_bytes());
        bytes.extend_from_slice(&segment_header(0, 48, 1, 19));
        bytes.extend_from_slice(&segment_header(
            1,
            38,
            1,
            u32::try_from(generic.len()).expect("payload fits"),
        ));
        bytes.extend_from_slice(&segment_header(2, 49, 1, 0));
        bytes.extend_from_slice(&segment_header(3, 51, 0, 0));
        bytes.extend_from_slice(&page_data());
        bytes.extend_from_slice(&generic);

        let decoded = Decoder::default()
            .decode(&bytes)
            .expect("random access decodes");
        assert_eq!(
            decoded.info().organization(),
            crate::Organization::RandomAccess
        );
        assert_eq!(decoded.pages()[0].bitmap().bytes(), &[0]);
    }

    #[test]
    fn sequential_and_random_access_outputs_match() {
        let sequential = Decoder::default()
            .decode(&document_with_generic(&[0x80]))
            .expect("sequential decodes");

        let generic = generic_data(&[0x80]);
        let mut random = SIGNATURE.to_vec();
        random.push(0x00);
        random.extend_from_slice(&1_u32.to_be_bytes());
        random.extend_from_slice(&segment_header(0, 48, 1, 19));
        random.extend_from_slice(&segment_header(
            1,
            38,
            1,
            u32::try_from(generic.len()).expect("payload fits"),
        ));
        random.extend_from_slice(&segment_header(2, 49, 1, 0));
        random.extend_from_slice(&segment_header(3, 51, 0, 0));
        random.extend_from_slice(&page_data());
        random.extend_from_slice(&generic);
        let random = Decoder::default().decode(&random).expect("random decodes");

        assert_eq!(sequential.pages()[0].bitmap(), random.pages()[0].bitmap());
    }

    #[test]
    fn decodes_unknown_length_mmr_region_and_row_count() {
        let mut page = page_data();
        page[4..8].copy_from_slice(&2_u32.to_be_bytes());
        let mut generic = Vec::new();
        generic.extend_from_slice(&8_u32.to_be_bytes());
        generic.extend_from_slice(&2_u32.to_be_bytes());
        generic.extend_from_slice(&0_u32.to_be_bytes());
        generic.extend_from_slice(&0_u32.to_be_bytes());
        generic.push(0);
        generic.push(1);
        generic.extend_from_slice(&[0x80, 0x08, 0x00, 0x80]);
        generic.extend_from_slice(&[0x00, 0x00]);
        generic.extend_from_slice(&1_u32.to_be_bytes());

        let mut bytes = SIGNATURE.to_vec();
        bytes.push(0x01);
        bytes.extend_from_slice(&1_u32.to_be_bytes());
        bytes.extend_from_slice(&segment_header(0, 48, 1, 19));
        bytes.extend_from_slice(&page);
        bytes.extend_from_slice(&segment_header(1, 38, 1, u32::MAX));
        bytes.extend_from_slice(&generic);
        bytes.extend_from_slice(&segment_header(2, 49, 1, 0));

        let decoded = Decoder::default()
            .decode(&bytes)
            .expect("unknown-length region decodes");
        assert_eq!(decoded.pages()[0].bitmap().height(), 2);
        assert_eq!(decoded.pages()[0].bitmap().bytes(), &[0, 0]);
    }

    #[test]
    fn decodes_unknown_height_striped_page() {
        let mut page = page_data();
        page[4..8].copy_from_slice(&u32::MAX.to_be_bytes());
        page[17..19].copy_from_slice(&0x8001_u16.to_be_bytes());
        let generic = generic_data(&[0x80]);

        let mut bytes = SIGNATURE.to_vec();
        bytes.push(0x01);
        bytes.extend_from_slice(&1_u32.to_be_bytes());
        bytes.extend_from_slice(&segment_header(0, 48, 1, 19));
        bytes.extend_from_slice(&page);
        bytes.extend_from_slice(&segment_header(
            1,
            38,
            1,
            u32::try_from(generic.len()).expect("payload fits"),
        ));
        bytes.extend_from_slice(&generic);
        bytes.extend_from_slice(&segment_header(2, 50, 1, 4));
        bytes.extend_from_slice(&0_u32.to_be_bytes());
        bytes.extend_from_slice(&segment_header(3, 49, 1, 0));

        let decoded = Decoder::default()
            .decode(&bytes)
            .expect("striped page decodes");
        assert_eq!(decoded.pages()[0].bitmap().height(), 1);
    }

    #[test]
    fn bounded_reader_decode_matches_slice_decode() {
        let bytes = document_with_generic(&[0x80]);
        let expected = Decoder::default().decode(&bytes).expect("slice decodes");
        let mut reader = std::io::Cursor::new(bytes);
        let actual = Decoder::default()
            .decode_reader(&mut reader)
            .expect("reader decodes");
        assert_eq!(actual, expected);
    }

    #[test]
    fn embedded_decode_matches_standalone_page() {
        let bytes = document_with_generic(&[0x80]);
        let standalone = Decoder::default()
            .decode(&bytes)
            .expect("standalone decodes");
        let embedded = Decoder::default()
            .decode_embedded(&bytes[13..])
            .expect("embedded stream decodes");
        assert_eq!(embedded.pages(), standalone.pages());
    }

    #[test]
    fn decodes_mmr_pattern_dictionary_and_halftone_page() {
        let mut pattern = vec![0x01, 0x01, 0x01, 0, 0, 0, 0];
        pattern.extend_from_slice(&[0x26, 0xA8]);

        let mut halftone = Vec::new();
        halftone.extend_from_slice(&1_u32.to_be_bytes());
        halftone.extend_from_slice(&1_u32.to_be_bytes());
        halftone.extend_from_slice(&0_u32.to_be_bytes());
        halftone.extend_from_slice(&0_u32.to_be_bytes());
        halftone.push(0);
        halftone.push(0x01);
        halftone.extend_from_slice(&1_u32.to_be_bytes());
        halftone.extend_from_slice(&1_u32.to_be_bytes());
        halftone.extend_from_slice(&0_i32.to_be_bytes());
        halftone.extend_from_slice(&0_i32.to_be_bytes());
        halftone.extend_from_slice(&256_u16.to_be_bytes());
        halftone.extend_from_slice(&0_u16.to_be_bytes());

        let mut page = page_data();
        page[0..4].copy_from_slice(&1_u32.to_be_bytes());

        let mut bytes = SIGNATURE.to_vec();
        bytes.push(0x01);
        bytes.extend_from_slice(&1_u32.to_be_bytes());
        let mut pattern_header = segment_header(
            0,
            16,
            0,
            u32::try_from(pattern.len()).expect("pattern payload fits"),
        );
        pattern_header[5] = 1;
        bytes.extend_from_slice(&pattern_header);
        bytes.extend_from_slice(&pattern);
        bytes.extend_from_slice(&segment_header(1, 48, 1, 19));
        bytes.extend_from_slice(&page);
        bytes.extend_from_slice(&referenced_segment_header(
            2,
            22,
            1,
            0,
            u32::try_from(halftone.len()).expect("halftone payload fits"),
        ));
        bytes.extend_from_slice(&halftone);
        bytes.extend_from_slice(&segment_header(3, 49, 1, 0));
        bytes.extend_from_slice(&segment_header(4, 51, 0, 0));

        let document = Decoder::default().decode(&bytes).expect("halftone decodes");
        assert_eq!(document.pages()[0].bitmap().bytes(), &[0x80]);
    }

    #[test]
    fn embedded_halftone_uses_reusable_global_pattern_dictionary() {
        let mut pattern = vec![0x01, 0x01, 0x01, 0, 0, 0, 0];
        pattern.extend_from_slice(&[0x26, 0xA8]);
        let mut global_bytes = segment_header(
            0,
            16,
            0,
            u32::try_from(pattern.len()).expect("pattern payload fits"),
        );
        global_bytes.extend_from_slice(&pattern);
        let globals = Decoder::default()
            .parse_globals(&global_bytes)
            .expect("globals validate");

        let mut halftone = Vec::new();
        halftone.extend_from_slice(&1_u32.to_be_bytes());
        halftone.extend_from_slice(&1_u32.to_be_bytes());
        halftone.extend_from_slice(&0_u32.to_be_bytes());
        halftone.extend_from_slice(&0_u32.to_be_bytes());
        halftone.push(0);
        halftone.push(0x01);
        halftone.extend_from_slice(&1_u32.to_be_bytes());
        halftone.extend_from_slice(&1_u32.to_be_bytes());
        halftone.extend_from_slice(&0_i32.to_be_bytes());
        halftone.extend_from_slice(&0_i32.to_be_bytes());
        halftone.extend_from_slice(&256_u16.to_be_bytes());
        halftone.extend_from_slice(&0_u16.to_be_bytes());

        let mut page = page_data();
        page[0..4].copy_from_slice(&1_u32.to_be_bytes());
        let mut embedded = segment_header(1, 48, 1, 19);
        embedded.extend_from_slice(&page);
        embedded.extend_from_slice(&referenced_segment_header(
            2,
            22,
            1,
            0,
            u32::try_from(halftone.len()).expect("halftone payload fits"),
        ));
        embedded.extend_from_slice(&halftone);
        embedded.extend_from_slice(&segment_header(3, 49, 1, 0));

        let document = Decoder::default()
            .decode_embedded_with_globals(&embedded, &globals)
            .expect("global pattern resolves");
        assert_eq!(document.pages()[0].bitmap().bytes(), &[0x80]);
    }

    #[test]
    fn decodes_symbol_dictionary_and_arithmetic_text_region() {
        const ANNEX_H_ENCODED: [u8; 30] = [
            0x84, 0xc7, 0x3b, 0xfc, 0xe1, 0xa1, 0x43, 0x04, 0x02, 0x20, 0x00, 0x00, 0x41, 0x0d,
            0xbb, 0x86, 0xf4, 0x31, 0x7f, 0xff, 0x88, 0xff, 0x37, 0x47, 0x1a, 0xdb, 0x6a, 0xdf,
            0xff, 0xac,
        ];
        let mut dictionary = vec![0x00, 0x00];
        dictionary.extend_from_slice(&[3, 0xff, 0xfd, 0xff, 2, 0xfe, 0xfe, 0xfe]);
        dictionary.extend_from_slice(&0_u32.to_be_bytes());
        dictionary.extend_from_slice(&0_u32.to_be_bytes());
        dictionary.extend_from_slice(&[0xff, 0xac]);

        let mut text = vec![0_u8; 17];
        text[3] = 1;
        text[7] = 1;
        text.extend_from_slice(&0_u16.to_be_bytes());
        text.extend_from_slice(&0_u32.to_be_bytes());
        text.extend_from_slice(&ANNEX_H_ENCODED);

        let mut page = page_data();
        page[0..4].copy_from_slice(&1_u32.to_be_bytes());
        let mut bytes = SIGNATURE.to_vec();
        bytes.push(0x01);
        bytes.extend_from_slice(&1_u32.to_be_bytes());
        let mut dictionary_header = segment_header(
            0,
            0,
            0,
            u32::try_from(dictionary.len()).expect("dictionary payload fits"),
        );
        dictionary_header[5] = 1;
        bytes.extend_from_slice(&dictionary_header);
        bytes.extend_from_slice(&dictionary);
        bytes.extend_from_slice(&segment_header(1, 48, 1, 19));
        bytes.extend_from_slice(&page);
        bytes.extend_from_slice(&referenced_segment_header(
            2,
            6,
            1,
            0,
            u32::try_from(text.len()).expect("text payload fits"),
        ));
        bytes.extend_from_slice(&text);
        bytes.extend_from_slice(&segment_header(3, 49, 1, 0));
        bytes.extend_from_slice(&segment_header(4, 51, 0, 0));

        let document = Decoder::default()
            .decode(&bytes)
            .expect("text document decodes");
        assert_eq!(document.pages()[0].bitmap().bytes(), &[0]);
    }

    #[test]
    fn embedded_text_uses_reusable_global_symbol_dictionary() {
        const ANNEX_H_ENCODED: [u8; 30] = [
            0x84, 0xc7, 0x3b, 0xfc, 0xe1, 0xa1, 0x43, 0x04, 0x02, 0x20, 0x00, 0x00, 0x41, 0x0d,
            0xbb, 0x86, 0xf4, 0x31, 0x7f, 0xff, 0x88, 0xff, 0x37, 0x47, 0x1a, 0xdb, 0x6a, 0xdf,
            0xff, 0xac,
        ];
        let mut dictionary = vec![0x00, 0x00];
        dictionary.extend_from_slice(&[3, 0xff, 0xfd, 0xff, 2, 0xfe, 0xfe, 0xfe]);
        dictionary.extend_from_slice(&0_u32.to_be_bytes());
        dictionary.extend_from_slice(&0_u32.to_be_bytes());
        dictionary.extend_from_slice(&[0xff, 0xac]);
        let mut globals_bytes = segment_header(
            0,
            0,
            0,
            u32::try_from(dictionary.len()).expect("dictionary payload fits"),
        );
        globals_bytes.extend_from_slice(&dictionary);
        let globals = Decoder::default()
            .parse_globals(&globals_bytes)
            .expect("global dictionary validates");

        let mut text = vec![0_u8; 17];
        text[3] = 1;
        text[7] = 1;
        text.extend_from_slice(&0_u16.to_be_bytes());
        text.extend_from_slice(&0_u32.to_be_bytes());
        text.extend_from_slice(&ANNEX_H_ENCODED);
        let mut page = page_data();
        page[0..4].copy_from_slice(&1_u32.to_be_bytes());
        let mut embedded = segment_header(1, 48, 1, 19);
        embedded.extend_from_slice(&page);
        embedded.extend_from_slice(&referenced_segment_header(
            2,
            6,
            1,
            0,
            u32::try_from(text.len()).expect("text payload fits"),
        ));
        embedded.extend_from_slice(&text);
        embedded.extend_from_slice(&segment_header(3, 49, 1, 0));

        let document = Decoder::default()
            .decode_embedded_with_globals(&embedded, &globals)
            .expect("global symbols resolve");
        assert_eq!(document.pages()[0].bitmap().bytes(), &[0]);
    }

    #[test]
    fn decodes_huffman_symbol_dictionary_and_text_instance() {
        let mut dictionary = vec![0x00, 0x01];
        dictionary.extend_from_slice(&1_u32.to_be_bytes());
        dictionary.extend_from_slice(&1_u32.to_be_bytes());
        dictionary.extend_from_slice(&[0x5f, 0x80, 0x80, 0x00, 0x40]);

        let mut text = vec![0_u8; 17];
        text[3] = 1;
        text[7] = 1;
        text.extend_from_slice(&1_u16.to_be_bytes());
        text.extend_from_slice(&0_u16.to_be_bytes());
        text.extend_from_slice(&1_u32.to_be_bytes());
        text.push(0x01);
        text.extend_from_slice(&[0_u8; 19]);

        let mut page = page_data();
        page[0..4].copy_from_slice(&1_u32.to_be_bytes());
        let mut bytes = SIGNATURE.to_vec();
        bytes.push(0x01);
        bytes.extend_from_slice(&1_u32.to_be_bytes());
        let mut dictionary_header = segment_header(
            0,
            0,
            0,
            u32::try_from(dictionary.len()).expect("dictionary payload fits"),
        );
        dictionary_header[5] = 1;
        bytes.extend_from_slice(&dictionary_header);
        bytes.extend_from_slice(&dictionary);
        bytes.extend_from_slice(&segment_header(1, 48, 1, 19));
        bytes.extend_from_slice(&page);
        bytes.extend_from_slice(&referenced_segment_header(
            2,
            6,
            1,
            0,
            u32::try_from(text.len()).expect("text payload fits"),
        ));
        bytes.extend_from_slice(&text);
        bytes.extend_from_slice(&segment_header(3, 49, 1, 0));

        let document = Decoder::default()
            .decode(&bytes)
            .expect("Huffman text document decodes");
        assert_eq!(document.pages()[0].bitmap().bytes(), &[0x80]);
    }

    #[test]
    fn rejects_region_that_modifies_a_sealed_stripe() {
        let mut page = page_data();
        page[4..8].copy_from_slice(&2_u32.to_be_bytes());
        page[17..19].copy_from_slice(&0x8002_u16.to_be_bytes());
        let first = generic_data(&[0x80]);
        let second = generic_data(&[0x80]);

        let mut bytes = SIGNATURE.to_vec();
        bytes.push(0x01);
        bytes.extend_from_slice(&1_u32.to_be_bytes());
        bytes.extend_from_slice(&segment_header(0, 48, 1, 19));
        bytes.extend_from_slice(&page);
        bytes.extend_from_slice(&segment_header(
            1,
            38,
            1,
            u32::try_from(first.len()).expect("payload fits"),
        ));
        bytes.extend_from_slice(&first);
        bytes.extend_from_slice(&segment_header(2, 50, 1, 4));
        bytes.extend_from_slice(&0_u32.to_be_bytes());
        bytes.extend_from_slice(&segment_header(
            3,
            38,
            1,
            u32::try_from(second.len()).expect("payload fits"),
        ));
        bytes.extend_from_slice(&second);
        bytes.extend_from_slice(&segment_header(4, 49, 1, 0));
        bytes.extend_from_slice(&segment_header(5, 51, 0, 0));

        assert!(matches!(
            Decoder::default().decode(&bytes),
            Err(DecodeError::Malformed(MalformedError::InvalidPageState(
                "region modifies a sealed stripe"
            )))
        ));
    }

    #[test]
    fn decoder_enforces_cumulative_retained_state_limit() {
        let limits = Limits {
            max_retained_state_bytes: 0,
            ..Limits::default()
        };
        assert!(matches!(
            Decoder::new(limits).decode(&document_with_generic(&[0x80])),
            Err(DecodeError::LimitExceeded(crate::LimitExceeded {
                kind: crate::LimitKind::RetainedState,
                limit: 0,
                actual: 1
            }))
        ));
    }

    #[test]
    fn owned_document_enforces_cumulative_page_memory() {
        let mut bytes = SIGNATURE.to_vec();
        bytes.push(0x01);
        bytes.extend_from_slice(&2_u32.to_be_bytes());
        bytes.extend_from_slice(&segment_header(0, 48, 1, 19));
        bytes.extend_from_slice(&page_data());
        bytes.extend_from_slice(&segment_header(1, 49, 1, 0));
        bytes.extend_from_slice(&segment_header(2, 48, 2, 19));
        bytes.extend_from_slice(&page_data());
        bytes.extend_from_slice(&segment_header(3, 49, 2, 0));

        let limits = Limits {
            max_decoder_managed_memory_bytes: 1,
            ..Limits::default()
        };
        let mut emitted = 0;
        Decoder::new(limits)
            .decode_with_sink(&bytes, &mut |_page: Page| {
                emitted += 1;
                Ok(())
            })
            .expect("streaming sink releases each page");
        assert_eq!(emitted, 2);
        assert!(matches!(
            Decoder::new(limits).decode(&bytes),
            Err(DecodeError::LimitExceeded(crate::LimitExceeded {
                kind: crate::LimitKind::Memory,
                limit: 1,
                actual: 2
            }))
        ));
    }

    #[test]
    fn verified_decode_helpers_enforce_boundaries() {
        assert_eq!(
            checked_page_write_state(None, None, 1, 1, 0, 0, 8, 8)
                .expect("first write is accepted"),
            Some(0)
        );
        assert!(checked_page_write_state(Some(3), None, 1, 1, 0, 3, 8, 8).is_err());
        assert_eq!(
            checked_seal_stripe_state(Some(3), 3).expect("stripe closes at its maximum"),
            None
        );
        assert!(checked_seal_stripe_state(Some(4), 3).is_err());
        assert_eq!(clipped_coordinate(u32::MAX, 1, u32::MAX), None);
        assert_eq!(clipped_coordinate(3, 4, 8), Some(7));
        assert_eq!(structural_work_totals(5, 2, 3).expect("work sum fits"), 10);
        assert!(structural_work_totals(u64::MAX, 1, 0).is_err());
    }

    #[test]
    fn duplicate_buffer_does_not_change_retained_accounting() {
        let limits = Limits::default();
        let region = crate::segment_data::RegionInfo {
            width: 8,
            height: 1,
            x: 0,
            y: 0,
            operator: crate::bitmap::CombinationOperator::Replace,
        };
        let mut buffers = Vec::new();
        let mut retained = RetainedBudget::new();
        insert_buffer(
            &mut buffers,
            7,
            region,
            Bitmap::blank(8, 1, &limits).expect("bitmap allocates"),
            &mut retained,
            &limits,
        )
        .expect("first buffer inserts");
        assert_eq!(retained.bytes, 1);

        assert!(matches!(
            insert_buffer(
                &mut buffers,
                7,
                region,
                Bitmap::blank(8, 1, &limits).expect("bitmap allocates"),
                &mut retained,
                &limits,
            ),
            Err(DecodeError::Malformed(
                MalformedError::DuplicateSegmentNumber(7)
            ))
        ));
        assert_eq!(retained.bytes, 1);
        assert_eq!(buffers.len(), 1);
    }
}

use std::collections::HashMap;
use std::io::Read;

use crate::document::{
    parse_embedded_document, parse_standalone_document, read_bounded_input, Decoder, DocumentInfo,
    GlobalSegments, PageInfo, ParsedData, ParsedSegment,
};
use crate::generic_region::{
    decode_arithmetic_refinement, decode_generic_region_segment, parse_refinement_region_header,
    GenericArithmeticCall, GenericArithmeticSession, GenericTemplate, RefinementParameters,
    TemplateOffset,
};
use crate::halftone::{
    decode_halftone_region, decode_pattern_dictionary, parse_halftone_header,
    parse_pattern_dictionary_header, GenericRegionCoding, GenericRegionRequest, WorkBudget,
};
use crate::huffman::HuffmanTable;
use crate::mmr::{decode_mmr, decode_mmr_prefix, EofbPolicy};
use crate::reader::ByteReader;
use crate::segment_data::parse_region_info;
use crate::segment_header::SegmentType;
use crate::symbol_text::{
    decode_symbol_dictionary, decode_text_region, DictionaryContexts, SymbolDictionaryInput,
    TextRegionInput,
};
use crate::{Bitmap, DecodeError, Limits, MalformedError, UnsupportedFeature};
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
    pub fn decode(&self, input: &[u8]) -> Result<DecodedDocument, DecodeError> {
        let mut collector = PageCollector::new(self.limits());
        let info = self.decode_with_sink(input, &mut collector)?;
        Ok(DecodedDocument {
            info,
            pages: collector.pages,
        })
    }

    /// Reads a bounded standalone document and decodes it into owned pages.
    ///
    /// # Errors
    ///
    /// Returns a typed malformed, unsupported, resource, allocation, or I/O
    /// error.
    pub fn decode_reader<R: Read>(&self, reader: &mut R) -> Result<DecodedDocument, DecodeError> {
        let bytes = read_bounded_input(reader, self.limits())?;
        self.decode(&bytes)
    }

    /// Decodes a contiguous embedded page stream without external globals.
    ///
    /// # Errors
    ///
    /// Returns a typed error for malformed, unresolved, unsupported, or
    /// over-limit input.
    pub fn decode_embedded(&self, input: &[u8]) -> Result<DecodedDocument, DecodeError> {
        self.decode_embedded_internal(input, None)
    }

    /// Decodes a contiguous embedded page stream with validated globals.
    ///
    /// # Errors
    ///
    /// Returns a typed error for malformed, unresolved, unsupported, or
    /// over-limit input.
    pub fn decode_embedded_with_globals(
        &self,
        input: &[u8],
        globals: &GlobalSegments,
    ) -> Result<DecodedDocument, DecodeError> {
        self.decode_embedded_internal(input, Some(globals))
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
        decode_segments(&segments, &info, initial_work, 0, self.limits(), None, sink)?;
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

    fn decode_embedded_internal(
        &self,
        input: &[u8],
        globals: Option<&GlobalSegments>,
    ) -> Result<DecodedDocument, DecodeError> {
        let (info, segments) = parse_embedded_document(input, self.limits(), globals)?;
        let mut collector = PageCollector::new(self.limits());
        let input_work = structural_work(input.len(), &segments)?
            .checked_add(
                u64::try_from(globals.map_or(0, |state| state.bytes().len()))
                    .map_err(|_| MalformedError::IntegerOverflow)?,
            )
            .ok_or(MalformedError::IntegerOverflow)?;
        let global_resources = match globals {
            Some(state) => Some(decode_global_resources(state.bytes(), self.limits())?),
            None => None,
        };
        decode_segments(
            &segments,
            &info,
            input_work,
            u64::try_from(globals.map_or(0, |state| state.bytes().len()))
                .map_err(|_| MalformedError::IntegerOverflow)?,
            self.limits(),
            global_resources,
            &mut collector,
        )?;
        Ok(DecodedDocument {
            info,
            pages: collector.pages,
        })
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
}

impl PageSink for PageCollector {
    fn write_page(&mut self, page: Page) -> Result<(), DecodeError> {
        self.collect_page(page)
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
struct RuntimePage {
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
    #[cfg_attr(verus_keep_ghost, verus_spec(
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

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn get_runtime_page(pages: &RuntimePages, number: u32) -> Option<&RuntimePage> {
    let index = runtime_page_index(pages, number)?;
    Some(&pages[index].1)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires decode_spec::runtime_pages_valid(old(pages)),
    ensures match out {
        Some(page) => crate::spec::bitmap::storage_valid(&page.bitmap),
        None => true,
    },
))]
fn get_runtime_page_mut(pages: &mut RuntimePages, number: u32) -> Option<&mut RuntimePage> {
    let index = runtime_page_index(pages, number)?;
    #[cfg(verus_keep_ghost)]
    proof! {
        reveal(decode_spec::runtime_pages_valid);
    }
    Some(&mut pages[index].1)
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
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
    #[cfg_attr(verus_keep_ghost, verus_spec(
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

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn get_region_buffer(buffers: &RegionBuffers, number: u32) -> Option<&RegionBuffer> {
    let index = region_buffer_index(buffers, number)?;
    Some(&buffers[index].1)
}

struct SymbolResource {
    exported: Vec<Bitmap>,
    contexts: Option<DictionaryContexts>,
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

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn get_patterns(patterns: &PatternStore, number: u32) -> Option<&[Bitmap]> {
    let index = pattern_index(patterns, number)?;
    Some(&patterns[index].1)
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn insert_patterns(patterns: &mut PatternStore, number: u32, bitmaps: Vec<Bitmap>) -> bool {
    if pattern_index(patterns, number).is_some() {
        return false;
    }
    patterns.push((number, bitmaps));
    true
}

#[derive(Default)]
struct InitialResources {
    patterns: PatternStore,
    symbols: HashMap<u32, SymbolResource>,
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

fn decode_global_resources(input: &[u8], limits: &Limits) -> Result<InitialResources, DecodeError> {
    let (_info, segments) = parse_embedded_document(input, limits, None)?;
    let mut resources = InitialResources::default();
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
    for segment in &segments {
        match segment.header.segment_type {
            SegmentType::Tables => {
                let table = HuffmanTable::from_encoded(segment.payload, limits)
                    .map_err(DecodeError::from)?;
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
    }
    Ok(resources)
}

#[allow(clippy::too_many_lines)]
fn decode_segments<S: PageSink>(
    segments: &[ParsedSegment<'_>],
    info: &DocumentInfo,
    initial_work: u64,
    initial_retained_bytes: u64,
    limits: &Limits,
    initial_resources: Option<InitialResources>,
    sink: &mut S,
) -> Result<(), DecodeError> {
    let mut pages = Vec::new();
    let page_allocation = allocation_size::<(u32, RuntimePage)>(info.pages().len())?;
    pages
        .try_reserve_exact(info.pages().len())
        .map_err(|_| DecodeError::Allocation {
            requested_bytes: page_allocation,
        })?;
    let mut buffers = Vec::new();
    let buffer_allocation = allocation_size::<(u32, RegionBuffer)>(segments.len())?;
    buffers
        .try_reserve_exact(segments.len())
        .map_err(|_| DecodeError::Allocation {
            requested_bytes: buffer_allocation,
        })?;
    let InitialResources {
        mut patterns,
        mut symbols,
        mut tables,
    } = initial_resources.unwrap_or_default();
    let pattern_allocation = allocation_size::<(u32, Vec<Bitmap>)>(segments.len())?;
    patterns
        .try_reserve_exact(segments.len())
        .map_err(|_| DecodeError::Allocation {
            requested_bytes: pattern_allocation,
        })?;
    let table_allocation = allocation_size::<(u32, HuffmanTable)>(segments.len())?;
    tables
        .try_reserve_exact(segments.len())
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
    for (_, bitmaps) in &patterns {
        let bytes = bitmaps.iter().try_fold(0_u64, |total, bitmap| {
            total
                .checked_add(bitmap_byte_len(bitmap)?)
                .ok_or(DecodeError::Malformed(MalformedError::IntegerOverflow))
        })?;
        retained.retain(bytes, limits)?;
    }
    for resource in symbols.values() {
        let bytes = resource.exported.iter().try_fold(0_u64, |total, bitmap| {
            total
                .checked_add(bitmap_byte_len(bitmap)?)
                .ok_or(DecodeError::Malformed(MalformedError::IntegerOverflow))
        })?;
        retained.retain(bytes, limits)?;
    }

    for segment in segments {
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
                let page = pages.remove(index).1;
                retained.release(bitmap_byte_len(&page.bitmap)?)?;
                sink.write_page(Page {
                    number,
                    bitmap: page.bitmap,
                })?;
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
                    .map_err(DecodeError::from)?;
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
    }
    if !pages.is_empty() {
        return Err(MalformedError::InvalidPageState("active page after final segment").into());
    }
    Ok(())
}

fn decode_generic_segment(
    segment: &ParsedSegment<'_>,
    pages: &mut RuntimePages,
    buffers: &mut RegionBuffers,
    retained: &mut RetainedBudget,
    work: &mut DecodeWork,
    limits: &Limits,
) -> Result<(), DecodeError> {
    let decoded = decode_generic_region_segment(
        segment.payload,
        segment.header.data_length.is_none(),
        limits,
    )
    .map_err(DecodeError::from)?;
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

fn decode_refinement_segment(
    segment: &ParsedSegment<'_>,
    pages: &mut RuntimePages,
    buffers: &mut RegionBuffers,
    retained: &mut RetainedBudget,
    work: &mut DecodeWork,
    limits: &Limits,
) -> Result<(), DecodeError> {
    let (header, header_length) =
        parse_refinement_region_header(segment.payload, limits).map_err(DecodeError::from)?;
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
    .map_err(DecodeError::from)?;
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
    let header = parse_pattern_dictionary_header(&mut reader).map_err(DecodeError::from)?;
    let coded = reader.remaining_bytes();
    let mut work = WorkBudget::new(limits);
    let decoded = if header.mmr {
        let mut used = false;
        let mut decoder = |request: GenericRegionRequest<'_>| {
            if used {
                return Err(MalformedError::InvalidPageState(
                    "pattern dictionary requested multiple MMR bitmaps",
                )
                .into());
            }
            if request.coding
                != (GenericRegionCoding::Mmr {
                    require_eofb: false,
                })
            {
                return Err(MalformedError::InvalidPageState(
                    "pattern dictionary coding mode changed",
                )
                .into());
            }
            used = true;
            decode_mmr(
                coded,
                request.width,
                request.height,
                limits,
                EofbPolicy::Optional,
            )
            .map_err(DecodeError::from)
        };
        decode_pattern_dictionary(header, &mut decoder, limits, &mut work)
            .map_err(DecodeError::from)?
    } else {
        let template = generic_template(header.template)?;
        let mut session = GenericArithmeticSession::new(coded, template, false, limits)
            .map_err(DecodeError::from)?;
        let pattern_bitmaps = {
            let mut decoder = |request: GenericRegionRequest<'_>| {
                let GenericRegionCoding::Arithmetic {
                    template: requested_template,
                    adaptive_pixels,
                } = request.coding
                else {
                    return Err(MalformedError::InvalidPageState(
                        "pattern dictionary coding mode changed",
                    )
                    .into());
                };
                if requested_template != header.template {
                    return Err(MalformedError::InvalidPageState(
                        "pattern dictionary template changed",
                    )
                    .into());
                }
                session
                    .decode_bitmap(GenericArithmeticCall {
                        width: request.width,
                        height: request.height,
                        adaptive: generic_adaptive(adaptive_pixels),
                        typical_prediction: request.typical_prediction,
                        skip: request.skip,
                    })
                    .map_err(DecodeError::from)
            };
            decode_pattern_dictionary(header, &mut decoder, limits, &mut work)
                .map_err(DecodeError::from)?
        };
        session.validate_final_marker().map_err(DecodeError::from)?;
        pattern_bitmaps
    };

    let requested_bytes = u64::try_from(std::mem::size_of::<(u32, Vec<Bitmap>)>())
        .map_err(|_| MalformedError::IntegerOverflow)?;
    patterns
        .try_reserve(1)
        .map_err(|_| DecodeError::Allocation { requested_bytes })?;
    let retained_bytes = decoded.iter().try_fold(0_u64, |total, bitmap| {
        total
            .checked_add(bitmap_byte_len(bitmap)?)
            .ok_or(DecodeError::Malformed(MalformedError::IntegerOverflow))
    })?;
    cumulative_work.charge(work.used(), limits)?;
    retained.retain(retained_bytes, limits)?;
    if !insert_patterns(patterns, segment.header.number, decoded) {
        return Err(MalformedError::DuplicateSegmentNumber(segment.header.number).into());
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
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
    let header = parse_halftone_header(&mut reader).map_err(DecodeError::from)?;
    let coded = reader.remaining_bytes();
    let mut work = WorkBudget::new(limits);

    let bitmap = if header.mmr {
        let mut offset = 0_usize;
        let bitmap = {
            let mut decoder = |request: GenericRegionRequest<'_>| {
                if request.coding != (GenericRegionCoding::Mmr { require_eofb: true }) {
                    return Err(
                        MalformedError::InvalidPageState("halftone coding mode changed").into(),
                    );
                }
                let remaining = coded.get(offset..).ok_or(MalformedError::IntegerOverflow)?;
                let (bitmap, consumed) =
                    decode_mmr_prefix(remaining, request.width, request.height, limits)
                        .map_err(DecodeError::from)?;
                offset = offset
                    .checked_add(consumed)
                    .ok_or(MalformedError::IntegerOverflow)?;
                Ok(bitmap)
            };
            decode_halftone_region(
                &header,
                region.width,
                region.height,
                dictionary,
                &mut decoder,
                limits,
                &mut work,
            )
            .map_err(DecodeError::from)?
        };
        if offset != coded.len() {
            let trailing = coded
                .len()
                .checked_sub(offset)
                .and_then(|bytes| u64::try_from(bytes).ok())
                .ok_or(MalformedError::IntegerOverflow)?;
            return Err(MalformedError::TrailingData(trailing).into());
        }
        bitmap
    } else {
        let template = generic_template(header.template)?;
        let mut session = GenericArithmeticSession::new(coded, template, false, limits)
            .map_err(DecodeError::from)?;
        let bitmap = {
            let mut decoder = |request: GenericRegionRequest<'_>| {
                let GenericRegionCoding::Arithmetic {
                    template: requested_template,
                    adaptive_pixels,
                } = request.coding
                else {
                    return Err(
                        MalformedError::InvalidPageState("halftone coding mode changed").into(),
                    );
                };
                if requested_template != header.template {
                    return Err(
                        MalformedError::InvalidPageState("halftone template changed").into(),
                    );
                }
                session
                    .decode_bitmap(GenericArithmeticCall {
                        width: request.width,
                        height: request.height,
                        adaptive: generic_adaptive(adaptive_pixels),
                        typical_prediction: request.typical_prediction,
                        skip: request.skip,
                    })
                    .map_err(DecodeError::from)
            };
            decode_halftone_region(
                &header,
                region.width,
                region.height,
                dictionary,
                &mut decoder,
                limits,
                &mut work,
            )
            .map_err(DecodeError::from)?
        };
        session.validate_final_marker().map_err(DecodeError::from)?;
        bitmap
    };

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

fn decode_symbol_dictionary_segment(
    segment: &ParsedSegment<'_>,
    symbols: &mut HashMap<u32, SymbolResource>,
    tables: &HuffmanTables,
    retained: &mut RetainedBudget,
    work: &mut DecodeWork,
    limits: &Limits,
) -> Result<(), DecodeError> {
    if symbols.contains_key(&segment.header.number) {
        return Err(MalformedError::DuplicateSegmentNumber(segment.header.number).into());
    }
    let imported = collect_referred_symbols(segment, symbols, retained.bytes, limits)?;
    let custom_tables = collect_referred_tables(segment, tables)?;
    let used_contexts = segment
        .header
        .referred_segments
        .iter()
        .filter_map(|number| symbols.get(number))
        .next_back()
        .and_then(|resource| resource.contexts.as_ref());
    let output = decode_symbol_dictionary(&SymbolDictionaryInput {
        payload: segment.payload,
        imported_symbols: &imported,
        custom_tables: &custom_tables,
        used_contexts,
        limits,
    })
    .map_err(DecodeError::from)?;
    if output.consumed_bytes != segment.payload.len() {
        let trailing = segment
            .payload
            .len()
            .checked_sub(output.consumed_bytes)
            .and_then(|bytes| u64::try_from(bytes).ok())
            .ok_or(MalformedError::IntegerOverflow)?;
        return Err(MalformedError::TrailingData(trailing).into());
    }
    let bitmap_bytes = output
        .exported_symbols
        .iter()
        .try_fold(0_u64, |total, bitmap| {
            total
                .checked_add(bitmap_byte_len(bitmap)?)
                .ok_or(DecodeError::Malformed(MalformedError::IntegerOverflow))
        })?;
    retained.retain(bitmap_bytes, limits)?;
    let symbol_work = output
        .exported_symbols
        .iter()
        .try_fold(0_u64, |total, bitmap| {
            total
                .checked_add(
                    u64::from(bitmap.width())
                        .checked_mul(u64::from(bitmap.height()))
                        .ok_or(MalformedError::IntegerOverflow)?,
                )
                .ok_or(MalformedError::IntegerOverflow)
        })?;
    work.charge(symbol_work, limits)?;
    let resource = SymbolResource {
        exported: output.exported_symbols,
        contexts: output.retained_contexts,
    };
    symbols.insert(segment.header.number, resource);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn decode_text_segment(
    segment: &ParsedSegment<'_>,
    pages: &mut RuntimePages,
    buffers: &mut RegionBuffers,
    symbols: &HashMap<u32, SymbolResource>,
    tables: &HuffmanTables,
    retained: &mut RetainedBudget,
    work: &mut DecodeWork,
    limits: &Limits,
) -> Result<(), DecodeError> {
    let available_symbols = collect_referred_symbols(segment, symbols, retained.bytes, limits)?;
    let custom_tables = collect_referred_tables(segment, tables)?;
    let output = decode_text_region(&TextRegionInput {
        payload: segment.payload,
        symbols: &available_symbols,
        custom_tables: &custom_tables,
        limits,
    })
    .map_err(DecodeError::from)?;
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

fn collect_referred_symbols(
    segment: &ParsedSegment<'_>,
    symbols: &HashMap<u32, SymbolResource>,
    active_retained_bytes: u64,
    limits: &Limits,
) -> Result<Vec<Bitmap>, DecodeError> {
    let count = segment
        .header
        .referred_segments
        .iter()
        .filter_map(|number| symbols.get(number))
        .try_fold(0_usize, |total, resource| {
            total
                .checked_add(resource.exported.len())
                .ok_or(MalformedError::IntegerOverflow)
        })?;
    let requested_bytes = allocation_size::<Bitmap>(count)?;
    let bitmap_bytes = segment
        .header
        .referred_segments
        .iter()
        .filter_map(|number| symbols.get(number))
        .try_fold(0_u64, |total, resource| {
            resource.exported.iter().try_fold(total, |sum, bitmap| {
                sum.checked_add(bitmap_byte_len(bitmap)?)
                    .ok_or(DecodeError::Malformed(MalformedError::IntegerOverflow))
            })
        })?;
    let peak = active_retained_bytes
        .checked_add(bitmap_bytes)
        .ok_or(MalformedError::IntegerOverflow)?;
    if peak > limits.max_decoder_managed_memory_bytes {
        return Err(crate::LimitExceeded {
            kind: crate::LimitKind::Memory,
            limit: limits.max_decoder_managed_memory_bytes,
            actual: peak,
        }
        .into());
    }
    let mut imported = Vec::new();
    imported
        .try_reserve_exact(count)
        .map_err(|_| DecodeError::Allocation { requested_bytes })?;
    for number in &segment.header.referred_segments {
        if let Some(resource) = symbols.get(number) {
            for bitmap in &resource.exported {
                imported.push(copy_bitmap(bitmap, limits)?);
            }
        }
    }
    Ok(imported)
}

fn collect_referred_tables<'a>(
    segment: &ParsedSegment<'_>,
    tables: &'a HuffmanTables,
) -> Result<Vec<&'a HuffmanTable>, DecodeError> {
    let count = segment
        .header
        .referred_segments
        .iter()
        .filter(|number| huffman_table_index(tables, **number).is_some())
        .count();
    let requested_bytes = allocation_size::<&HuffmanTable>(count)?;
    let mut result = Vec::new();
    result
        .try_reserve_exact(count)
        .map_err(|_| DecodeError::Allocation { requested_bytes })?;
    for number in &segment.header.referred_segments {
        if let Some(table) = get_huffman_table(tables, *number) {
            result.push(table);
        }
    }
    Ok(result)
}

fn copy_bitmap(source: &Bitmap, limits: &Limits) -> Result<Bitmap, DecodeError> {
    let requested_bytes = bitmap_byte_len(source)?;
    if requested_bytes > limits.max_decoder_managed_memory_bytes {
        return Err(crate::LimitExceeded {
            kind: crate::LimitKind::Memory,
            limit: limits.max_decoder_managed_memory_bytes,
            actual: requested_bytes,
        }
        .into());
    }
    let mut data = Vec::new();
    data.try_reserve_exact(source.bytes().len())
        .map_err(|_error| DecodeError::Allocation { requested_bytes })?;
    data.extend_from_slice(source.bytes());
    Ok(Bitmap {
        width: source.width,
        height: source.height,
        stride: source.stride,
        data,
    })
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
    ensures
        out@.len() == 12,
        forall|i: int| 0 <= i < 4 ==>
            out[i].x == values[i].0 && out[i].y == values[i].1,
        forall|i: int| 4 <= i < 12 ==>
            out[i].x == 0 && out[i].y == 0,
))]
fn generic_adaptive(values: [(i32, i32); 4]) -> [TemplateOffset; 12] {
    let mut adaptive = [TemplateOffset::new(0, 0); 12];
    adaptive[0] = TemplateOffset::new(values[0].0, values[0].1);
    adaptive[1] = TemplateOffset::new(values[1].0, values[1].1);
    adaptive[2] = TemplateOffset::new(values[2].0, values[2].1);
    adaptive[3] = TemplateOffset::new(values[3].0, values[3].1);
    adaptive
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out == (left == right),
))]
fn same_combination_operator(
    left: crate::bitmap::CombinationOperator,
    right: crate::bitmap::CombinationOperator,
) -> bool {
    #[cfg(verus_keep_ghost)]
    {
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
    #[cfg(not(verus_keep_ghost))]
    {
        left == right
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

#[cfg_attr(verus_keep_ghost, verus_spec(
    requires
        decode_spec::runtime_pages_valid(old(pages)),
        crate::spec::bitmap::storage_valid(bitmap),
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
    let page = get_runtime_page_mut(pages, segment.header.page_association).ok_or(
        MalformedError::MissingPageInformation(segment.header.page_association),
    )?;
    compose_immediate_on_page(page, region, bitmap, refinement, work, limits)
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

#[cfg_attr(verus_keep_ghost, verus_spec(
    requires decode_spec::runtime_pages_valid(old(pages)),
))]
fn seal_stripe(pages: &mut RuntimePages, page_number: u32, row: u32) -> Result<(), DecodeError> {
    let page = get_runtime_page_mut(pages, page_number)
        .ok_or(MalformedError::MissingPageInformation(page_number))?;
    seal_stripe_on_page(page, row)
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn seal_stripe_on_page(page: &mut RuntimePage, row: u32) -> Result<(), DecodeError> {
    let next_open = checked_seal_stripe_state(page.open_stripe_max_row, row)?;
    page.last_stripe_end = Some(row);
    page.open_stripe_max_row = next_open;
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
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
    let requested_bytes = u64::try_from(std::mem::size_of::<(u32, RegionBuffer)>())
        .map_err(|_| MalformedError::IntegerOverflow)?;
    buffers
        .try_reserve(1)
        .map_err(|_| DecodeError::Allocation { requested_bytes })?;
    retain_bitmap(retained, &bitmap, limits)?;
    buffers.push((number, RegionBuffer { region, bitmap }));
    Ok(())
}

fn release_referred_buffers(
    segment: &ParsedSegment<'_>,
    buffers: &mut RegionBuffers,
    retained: &mut RetainedBudget,
) -> Result<(), DecodeError> {
    for (index, number) in segment.header.referred_segments.iter().enumerate() {
        if segment
            .header
            .retention_flags
            .retain_referred_segment(index)
            == Some(false)
        {
            if let Some(index) = region_buffer_index(buffers, *number) {
                let buffer = buffers.remove(index).1;
                release_bitmap(retained, &buffer.bitmap)?;
            }
        }
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

fn release_referred_patterns(
    segment: &ParsedSegment<'_>,
    patterns: &mut PatternStore,
    retained: &mut RetainedBudget,
) -> Result<(), DecodeError> {
    for (index, number) in segment.header.referred_segments.iter().enumerate() {
        if segment
            .header
            .retention_flags
            .retain_referred_segment(index)
            == Some(false)
        {
            if let Some(pattern_index) = pattern_index(patterns, *number) {
                let bitmaps = patterns.remove(pattern_index).1;
                let released = bitmaps.iter().try_fold(0_u64, |total, bitmap| {
                    total
                        .checked_add(bitmap_byte_len(bitmap)?)
                        .ok_or(DecodeError::Malformed(MalformedError::IntegerOverflow))
                })?;
                retained.release(released)?;
            }
        }
    }
    Ok(())
}

fn release_referred_symbols(
    segment: &ParsedSegment<'_>,
    symbols: &mut HashMap<u32, SymbolResource>,
    retained: &mut RetainedBudget,
) -> Result<(), DecodeError> {
    for (index, number) in segment.header.referred_segments.iter().enumerate() {
        if segment
            .header
            .retention_flags
            .retain_referred_segment(index)
            == Some(false)
        {
            if let Some(resource) = symbols.remove(number) {
                let released = resource.exported.iter().try_fold(0_u64, |total, bitmap| {
                    total
                        .checked_add(bitmap_byte_len(bitmap)?)
                        .ok_or(DecodeError::Malformed(MalformedError::IntegerOverflow))
                })?;
                retained.release(released)?;
            }
        }
    }
    Ok(())
}

fn release_referred_tables(segment: &ParsedSegment<'_>, tables: &mut HuffmanTables) {
    for (index, number) in segment.header.referred_segments.iter().enumerate() {
        if segment
            .header
            .retention_flags
            .retain_referred_segment(index)
            == Some(false)
        {
            if let Some(table_index) = huffman_table_index(tables, *number) {
                tables.remove(table_index);
            }
        }
    }
}

fn snapshot_region(
    page: &Bitmap,
    region: &crate::segment_data::RegionInfo,
    limits: &Limits,
) -> Result<Bitmap, DecodeError> {
    let mut snapshot = Bitmap::blank(region.width, region.height, limits)?;
    for y in 0..region.height {
        for x in 0..region.width {
            let page_x = clipped_coordinate(region.x, x, page.width());
            let page_y = clipped_coordinate(region.y, y, page.height());
            let value = match (page_x, page_y) {
                (Some(page_x), Some(page_y)) => page.pixel(page_x, page_y).unwrap_or(false),
                _ => false,
            };
            snapshot.set_pixel(x, y, value)?;
        }
    }
    Ok(snapshot)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Some(coordinate) =>
            coordinate as nat == origin as nat + offset as nat
                && coordinate < bound,
        None =>
            origin as nat + offset as nat > u32::MAX as nat
                || origin as nat + offset as nat >= bound as nat,
    },
))]
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

    #[test]
    fn decodes_generic_page_and_collects_document() {
        let decoded = Decoder::default()
            .decode(&document_with_generic(&[0x26, 0xA2, 0x80]))
            .expect("MMR page decodes");
        assert_eq!(decoded.pages().len(), 1);
        assert_eq!(decoded.pages()[0].number(), 1);
        assert_eq!(decoded.pages()[0].bitmap().bytes(), &[0xFF]);
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

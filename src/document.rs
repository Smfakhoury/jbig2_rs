use std::io::Read;

use crate::file_header::{inspect_file_header, Organization};
use crate::huffman::HuffmanTable;
use crate::reader::ByteReader;
use crate::segment_data::{
    parse_end_of_stripe, parse_extension, parse_page_information, parse_profiles, require_empty,
    PageInformation,
};
use crate::segment_header::{parse_segment_header, SegmentHeader, SegmentType};
use crate::{DecodeError, LimitExceeded, LimitKind, Limits, MalformedError, UnsupportedFeature};
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg(verus_keep_ghost)]
#[rustfmt::skip]
#[path = "../specs/document.rs"]
mod document_spec;

#[cfg(verus_keep_ghost)]
#[rustfmt::skip]
#[path = "../proofs/spec_properties/document.rs"]
mod document_properties;

#[cfg(verus_keep_ghost)]
#[rustfmt::skip]
#[path = "../proofs/shared/document.rs"]
mod document_shared;

type SegmentTypes = Vec<(u32, SegmentType)>;
type SegmentDepths = Vec<(u32, u32)>;
type SegmentIndexes = Vec<(u32, usize)>;
type PageStates = Vec<(u32, PageState)>;

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Some(index) =>
            index < entries@.len()
            && entries@[index as int].0 == number,
        None =>
            forall|index: int| 0 <= index < entries@.len() ==>
                entries@[index].0 != number,
    },
))]
fn keyed_index<T>(entries: &[(u32, T)], number: u32) -> Option<usize> {
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index <= entries@.len(),
            forall|completed: int| 0 <= completed < index ==>
                entries@[completed].0 != number,
        decreases entries@.len() - index,
    ))]
    while index < entries.len() {
        if entries[index].0 == number {
            return Some(index);
        }
        index += 1;
    }
    None
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn keyed_get<T>(entries: &[(u32, T)], number: u32) -> Option<&T> {
    let index = keyed_index(entries, number)?;
    Some(&entries[index].1)
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn keyed_get_mut<T>(entries: &mut [(u32, T)], number: u32) -> Option<&mut T> {
    let index = keyed_index(entries, number)?;
    Some(&mut entries[index].1)
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn keyed_insert<T>(entries: &mut Vec<(u32, T)>, number: u32, value: T) -> bool {
    if keyed_index(entries, number).is_some() {
        return false;
    }
    entries.push((number, value));
    true
}

/// High-level decoder configuration.
#[derive(Debug, Clone)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub struct Decoder {
    limits: Limits,
}

impl Decoder {
    /// Creates a decoder using the supplied resource limits.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures document_spec::decoder_limits(&out) == limits,
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub const fn new(limits: Limits) -> Self {
        #[cfg(verus_keep_ghost)]
        production_document_policy_bundle!();
        let out = Self { limits };
        #[cfg(verus_keep_ghost)]
        production_decoder_fields!(&out);
        out
    }

    /// Returns the active resource limits.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures *out == document_spec::decoder_limits(self),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub const fn limits(&self) -> &Limits {
        #[cfg(verus_keep_ghost)]
        production_decoder_fields!(self);
        &self.limits
    }

    /// Parses and validates a standalone sequential or random-access stream.
    ///
    /// This method validates the document envelope and page lifecycle without
    /// decoding compressed region payloads.
    ///
    /// # Errors
    ///
    /// Returns a typed error for malformed, unsupported, over-limit, or
    /// allocation-failing input.
    pub fn inspect(&self, input: &[u8]) -> Result<DocumentInfo, DecodeError> {
        parse_standalone_document(input, &self.limits).map(|(info, _segments)| info)
    }

    /// Reads a bounded standalone stream and validates its document envelope.
    ///
    /// # Errors
    ///
    /// Returns a typed I/O, malformed, unsupported, resource-limit, or
    /// allocation error.
    pub fn inspect_reader<R: Read>(&self, reader: &mut R) -> Result<DocumentInfo, DecodeError> {
        let bytes = read_bounded_input(reader, &self.limits)?;
        self.inspect(&bytes)
    }

    /// Parses a contiguous embedded segment stream such as PDF page data.
    ///
    /// # Errors
    ///
    /// Returns a typed error for malformed, unsupported, over-limit, or
    /// allocation-failing input.
    pub fn inspect_embedded(&self, input: &[u8]) -> Result<DocumentInfo, DecodeError> {
        self.inspect_embedded_internal(input, None)
    }

    /// Parses an embedded page stream with reusable validated global segments.
    ///
    /// # Errors
    ///
    /// Returns a typed error for malformed, unsupported, over-limit,
    /// allocation-failing, or unresolved input.
    pub fn inspect_embedded_with_globals(
        &self,
        input: &[u8],
        globals: &GlobalSegments,
    ) -> Result<DocumentInfo, DecodeError> {
        self.inspect_embedded_internal(input, Some(globals))
    }

    /// Parses and validates an immutable embedded global-segment stream.
    ///
    /// # Errors
    ///
    /// Returns a typed error if a global segment is malformed, associated with
    /// a page, unresolved, unsupported, over-limit, or cannot be allocated.
    pub fn parse_globals(&self, input: &[u8]) -> Result<GlobalSegments, DecodeError> {
        enforce_stream_limit(input.len(), &self.limits)?;
        enforce_input_work(input.len(), &self.limits)?;
        let segments = parse_sequential(input, &self.limits, false)?;
        for segment in &segments {
            if segment.header.page_association != 0 {
                return Err(MalformedError::InvalidPageAssociation {
                    segment: segment.header.number,
                    page: segment.header.page_association,
                }
                .into());
            }
        }
        validate_profiles(&segments)?;
        enforce_work_limit(input.len(), &segments, &self.limits)?;
        let (types, depths) = validate_references(&segments, None, &self.limits)?;
        let input_bytes =
            u64::try_from(input.len()).map_err(|_| MalformedError::IntegerOverflow)?;
        if input_bytes > self.limits.max_retained_state_bytes {
            return Err(LimitExceeded {
                kind: LimitKind::RetainedState,
                limit: self.limits.max_retained_state_bytes,
                actual: input_bytes,
            }
            .into());
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(input.len())
            .map_err(|_| DecodeError::Allocation {
                requested_bytes: input_bytes,
            })?;
        bytes.extend_from_slice(input);
        Ok(GlobalSegments {
            types,
            depths,
            bytes,
        })
    }

    fn inspect_embedded_internal(
        &self,
        input: &[u8],
        globals: Option<&GlobalSegments>,
    ) -> Result<DocumentInfo, DecodeError> {
        parse_embedded_document(input, &self.limits, globals).map(|(info, _segments)| info)
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl Default for Decoder {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures document_spec::default_limits(
            document_spec::decoder_limits(&out),
        ),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    fn default() -> Self {
        Self::new(Limits::default())
    }
}

/// Immutable validated metadata for reusable embedded global segments.
#[derive(Debug)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub struct GlobalSegments {
    types: SegmentTypes,
    depths: SegmentDepths,
    bytes: Vec<u8>,
}

impl GlobalSegments {
    /// Number of global segments.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out as nat == document_spec::global_segment_count(self),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub fn segment_count(&self) -> usize {
        #[cfg(verus_keep_ghost)]
        production_global_fields!(self);
        self.types.len()
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out@ == document_spec::global_bytes(self),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn bytes(&self) -> &[u8] {
        #[cfg(verus_keep_ghost)]
        production_global_fields!(self);
        &self.bytes
    }
}

/// Validated document-level metadata.
#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub struct DocumentInfo {
    organization: Organization,
    declared_page_count: Option<u32>,
    segment_count: u32,
    pages: Vec<PageInfo>,
}

impl DocumentInfo {
    /// Standalone organization, or sequential envelope semantics for embedded
    /// streams.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == document_spec::document_organization(self),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub const fn organization(&self) -> Organization {
        #[cfg(verus_keep_ghost)]
        production_document_fields!(self);
        self.organization
    }

    /// Page count declared by a standalone header.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == document_spec::document_declared_page_count(self),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub const fn declared_page_count(&self) -> Option<u32> {
        #[cfg(verus_keep_ghost)]
        production_document_fields!(self);
        self.declared_page_count
    }

    /// Number of parsed segments.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == document_spec::document_segment_count(self),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub const fn segment_count(&self) -> u32 {
        #[cfg(verus_keep_ghost)]
        production_document_fields!(self);
        self.segment_count
    }

    /// Validated pages in ascending page-number order.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out@ == document_spec::document_pages(self),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub fn pages(&self) -> &[PageInfo] {
        #[cfg(verus_keep_ghost)]
        production_document_fields!(self);
        &self.pages
    }
}

/// Public page metadata established by page and stripe boundary segments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub struct PageInfo {
    pub(crate) number: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) x_resolution: u32,
    pub(crate) y_resolution: u32,
    pub(crate) striped: bool,
}

impl PageInfo {
    /// One-based page number.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == crate::views::page_number(self),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub const fn number(&self) -> u32 {
        #[cfg(verus_keep_ghost)]
        production_page_info_fields!(self);
        self.number
    }

    /// Page width in pixels.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == crate::views::page_width(self),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub const fn width(&self) -> u32 {
        #[cfg(verus_keep_ghost)]
        production_page_info_fields!(self);
        self.width
    }

    /// Final page height in pixels.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == crate::views::page_height(self),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub const fn height(&self) -> u32 {
        #[cfg(verus_keep_ghost)]
        production_page_info_fields!(self);
        self.height
    }

    /// Horizontal resolution in pixels per metre, or zero when unknown.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == crate::views::page_x_resolution(self),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub const fn x_resolution(&self) -> u32 {
        #[cfg(verus_keep_ghost)]
        production_page_info_fields!(self);
        self.x_resolution
    }

    /// Vertical resolution in pixels per metre, or zero when unknown.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == crate::views::page_y_resolution(self),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub const fn y_resolution(&self) -> u32 {
        #[cfg(verus_keep_ghost)]
        production_page_info_fields!(self);
        self.y_resolution
    }

    /// Whether the page uses stripe boundaries.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == crate::views::page_is_striped(self),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub const fn is_striped(&self) -> bool {
        #[cfg(verus_keep_ghost)]
        production_page_info_fields!(self);
        self.striped
    }
}

#[derive(Debug)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct ParsedSegment<'a> {
    pub(crate) header: SegmentHeader,
    #[allow(dead_code)]
    pub(crate) payload: &'a [u8],
    pub(crate) data: ParsedData,
}

#[derive(Debug)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) enum ParsedData {
    Page(PageInformation),
    EndOfStripe(u32),
    Profiles(Vec<u32>),
    Other,
}

pub(crate) fn parse_standalone_document<'a>(
    input: &'a [u8],
    limits: &Limits,
) -> Result<(DocumentInfo, Vec<ParsedSegment<'a>>), DecodeError> {
    enforce_stream_limit(input.len(), limits)?;
    enforce_input_work(input.len(), limits)?;
    let file_header = inspect_file_header(input)?;
    if file_header.uses_colour_regions() {
        return Err(DecodeError::Unsupported(UnsupportedFeature::Colour));
    }
    let body = input
        .get(file_header.byte_length()..)
        .ok_or(MalformedError::IntegerOverflow)?;
    let segments = match file_header.organization() {
        Organization::Sequential => parse_sequential(body, limits, true)?,
        Organization::RandomAccess => parse_random_access(body, limits)?,
    };
    enforce_work_limit(input.len(), &segments, limits)?;
    let info = build_document_info(
        file_header.organization(),
        file_header.page_count(),
        &segments,
        limits,
        None,
    )?;
    Ok((info, segments))
}

pub(crate) fn parse_embedded_document<'a>(
    input: &'a [u8],
    limits: &Limits,
    globals: Option<&GlobalSegments>,
) -> Result<(DocumentInfo, Vec<ParsedSegment<'a>>), DecodeError> {
    enforce_stream_limit(input.len(), limits)?;
    enforce_input_work(input.len(), limits)?;
    let segments = parse_sequential(input, limits, false)?;
    enforce_work_limit(input.len(), &segments, limits)?;
    let info = build_document_info(Organization::Sequential, None, &segments, limits, globals)?;
    Ok((info, segments))
}

fn parse_sequential<'a>(
    input: &'a [u8],
    limits: &Limits,
    require_local_references: bool,
) -> Result<Vec<ParsedSegment<'a>>, DecodeError> {
    let mut reader = ByteReader::new(input);
    let mut segments = Vec::new();
    let mut previous_number = None;
    while reader.remaining() != 0 {
        enforce_segment_limit(segments.len(), limits)?;
        let header = parse_segment_header(&mut reader, limits)?;
        if let Some(previous) = previous_number {
            if require_local_references && header.number <= previous {
                return Err(MalformedError::NonIncreasingSegmentNumber {
                    previous,
                    current: header.number,
                }
                .into());
            }
        }
        previous_number = Some(header.number);
        let payload = read_payload(&mut reader, &header)?;
        let is_end = header.segment_type == SegmentType::EndOfFile;
        let data = parse_known_segment_data(&header, payload, limits)?;
        let new_len = segments
            .len()
            .checked_add(1)
            .ok_or(MalformedError::IntegerOverflow)?;
        let segment_allocation = allocation_size::<ParsedSegment<'_>>(new_len)?;
        segments
            .try_reserve(1)
            .map_err(|_| DecodeError::Allocation {
                requested_bytes: segment_allocation,
            })?;
        segments.push(ParsedSegment {
            header,
            payload,
            data,
        });
        if is_end {
            if reader.remaining() != 0 {
                return Err(MalformedError::TrailingData(reader.remaining() as u64).into());
            }
            break;
        }
    }
    Ok(segments)
}

fn parse_random_access<'a>(
    input: &'a [u8],
    limits: &Limits,
) -> Result<Vec<ParsedSegment<'a>>, DecodeError> {
    let mut reader = ByteReader::new(input);
    let mut headers = Vec::new();
    let mut previous_number = None;
    loop {
        if reader.remaining() == 0 {
            return Err(MalformedError::MissingEndOfFile.into());
        }
        enforce_segment_limit(headers.len(), limits)?;
        let header = parse_segment_header(&mut reader, limits)?;
        if let Some(previous) = previous_number {
            if header.number <= previous {
                return Err(MalformedError::NonIncreasingSegmentNumber {
                    previous,
                    current: header.number,
                }
                .into());
            }
        }
        previous_number = Some(header.number);
        let is_end = header.segment_type == SegmentType::EndOfFile;
        headers
            .try_reserve(1)
            .map_err(|_| DecodeError::Allocation {
                requested_bytes: u64::try_from(std::mem::size_of::<SegmentHeader>())
                    .unwrap_or(u64::MAX),
            })?;
        headers.push(header);
        if is_end {
            break;
        }
    }

    let mut segments = Vec::new();
    let segment_allocation = allocation_size::<ParsedSegment<'_>>(headers.len())?;
    segments
        .try_reserve_exact(headers.len())
        .map_err(|_| DecodeError::Allocation {
            requested_bytes: segment_allocation,
        })?;
    for header in headers {
        let payload = read_payload(&mut reader, &header)?;
        let data = parse_known_segment_data(&header, payload, limits)?;
        segments.push(ParsedSegment {
            header,
            payload,
            data,
        });
    }
    if reader.remaining() != 0 {
        return Err(MalformedError::TrailingData(reader.remaining() as u64).into());
    }
    Ok(segments)
}

#[cfg_attr(verus_keep_ghost, verus_spec(
    requires old(reader).position <= old(reader).bytes@.len(),
))]
fn read_payload<'a>(
    reader: &mut ByteReader<'a>,
    header: &SegmentHeader,
) -> Result<&'a [u8], DecodeError> {
    if let Some(length) = header.data_length {
        let Ok(length) = usize::try_from(length) else {
            return Err(MalformedError::IntegerOverflow.into());
        };
        return Ok(reader.read_bytes(length)?);
    }
    let length = find_unknown_generic_length(reader.remaining_bytes())?;
    Ok(reader.read_bytes(length)?)
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn find_unknown_generic_length(bytes: &[u8]) -> Result<usize, DecodeError> {
    const GENERIC_FLAGS_INDEX: usize = 17;
    const SEARCH_START: usize = 18;
    const TERMINAL_BYTES: usize = 6;
    if bytes.len() < SEARCH_START + TERMINAL_BYTES {
        return Err(MalformedError::MissingGenericRegionTerminator.into());
    }
    let declared_height = read_be_u32_at(bytes, 4);
    let marker = if bytes[GENERIC_FLAGS_INDEX] & 0x01 != 0 {
        [0x00, 0x00]
    } else {
        [0xFF, 0xAC]
    };
    let final_start = bytes
        .len()
        .checked_sub(TERMINAL_BYTES)
        .ok_or(MalformedError::IntegerOverflow)?;
    let mut offset = SEARCH_START;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            bytes@.len() >= SEARCH_START + TERMINAL_BYTES,
            final_start == bytes@.len() - TERMINAL_BYTES,
            SEARCH_START <= offset <= final_start + 1,
        decreases final_start + 1 - offset,
    ))]
    while offset <= final_start {
        if bytes[offset] != marker[0] || bytes[offset + 1] != marker[1] {
            offset += 1;
            continue;
        }
        let row_count = read_be_u32_at(bytes, offset + 2);
        if row_count > declared_height {
            return Err(MalformedError::GenericRegionRowCount {
                declared: declared_height,
                actual: row_count,
            }
            .into());
        }
        return offset
            .checked_add(TERMINAL_BYTES)
            .ok_or(MalformedError::IntegerOverflow.into());
    }
    Err(MalformedError::MissingGenericRegionTerminator.into())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires offset + 4 <= bytes@.len(),
    ensures out
        == ((bytes@[offset as int] as u32) << 24)
            | ((bytes@[(offset + 1) as int] as u32) << 16)
            | ((bytes@[(offset + 2) as int] as u32) << 8)
            | bytes@[(offset + 3) as int] as u32,
))]
fn read_be_u32_at(bytes: &[u8], offset: usize) -> u32 {
    (u32::from(bytes[offset]) << 24)
        | (u32::from(bytes[offset + 1]) << 16)
        | (u32::from(bytes[offset + 2]) << 8)
        | u32::from(bytes[offset + 3])
}

fn parse_known_segment_data(
    header: &SegmentHeader,
    payload: &[u8],
    limits: &Limits,
) -> Result<ParsedData, DecodeError> {
    let data = match header.segment_type {
        SegmentType::PageInformation => ParsedData::Page(parse_page_information(payload, limits)?),
        SegmentType::EndOfStripe => ParsedData::EndOfStripe(parse_end_of_stripe(payload)?),
        SegmentType::EndOfPage => {
            require_empty(49, payload)?;
            ParsedData::Other
        }
        SegmentType::EndOfFile => {
            require_empty(51, payload)?;
            ParsedData::Other
        }
        SegmentType::Profiles => ParsedData::Profiles(parse_profiles(payload)?),
        SegmentType::Tables => {
            HuffmanTable::from_encoded(payload, limits).map_err(DecodeError::from)?;
            ParsedData::Other
        }
        SegmentType::Extension => {
            parse_extension(payload)?;
            ParsedData::Other
        }
        SegmentType::ColourPalette => {
            return Err(DecodeError::Unsupported(UnsupportedFeature::Colour));
        }
        _ => ParsedData::Other,
    };
    Ok(data)
}

fn build_document_info(
    organization: Organization,
    declared_page_count: Option<u32>,
    segments: &[ParsedSegment<'_>],
    limits: &Limits,
    globals: Option<&GlobalSegments>,
) -> Result<DocumentInfo, DecodeError> {
    validate_references(segments, globals, limits)?;
    validate_profiles(segments)?;
    let (page_states, page_order) = collect_page_states(segments, limits)?;
    let actual_page_count =
        u32::try_from(page_states.len()).map_err(|_| MalformedError::IntegerOverflow)?;
    if let Some(declared) = declared_page_count {
        if declared != actual_page_count {
            return Err(MalformedError::PageCountMismatch {
                declared,
                actual: actual_page_count,
            }
            .into());
        }
    }
    let pages = finalize_pages(page_states, page_order)?;

    Ok(DocumentInfo {
        organization,
        declared_page_count,
        segment_count: u32::try_from(segments.len())
            .map_err(|_| MalformedError::IntegerOverflow)?,
        pages,
    })
}

fn collect_page_states(
    segments: &[ParsedSegment<'_>],
    limits: &Limits,
) -> Result<(PageStates, Vec<u32>), DecodeError> {
    let mut page_states = Vec::new();
    let page_state_allocation =
        allocation_size::<PageState>(segments.len().min(limits.max_pages as usize))?;
    page_states
        .try_reserve_exact(segments.len().min(limits.max_pages as usize))
        .map_err(|_| DecodeError::Allocation {
            requested_bytes: page_state_allocation,
        })?;
    let mut page_order = Vec::new();
    let page_order_capacity = segments.len().min(limits.max_pages as usize);
    let page_order_allocation = allocation_size::<u32>(page_order_capacity)?;
    page_order
        .try_reserve_exact(page_order_capacity)
        .map_err(|_| DecodeError::Allocation {
            requested_bytes: page_order_allocation,
        })?;

    for segment in segments {
        validate_page_association(&segment.header)?;
        let page = segment.header.page_association;
        if page == 0 {
            continue;
        }

        if keyed_index(&page_states, page).is_none() {
            if segment.header.segment_type != SegmentType::PageInformation {
                return Err(MalformedError::MissingPageInformation(page).into());
            }
            enforce_page_limit(page_states.len(), limits)?;
            let ParsedData::Page(info) = segment.data else {
                return Err(MalformedError::MissingPageInformation(page).into());
            };
            let inserted = keyed_insert(&mut page_states, page, PageState::new(info));
            debug_assert!(inserted);
            page_order.push(page);
            continue;
        }

        let state = keyed_get_mut(&mut page_states, page)
            .ok_or(MalformedError::MissingPageInformation(page))?;
        if state.ended {
            return Err(MalformedError::SegmentAfterEndOfPage {
                segment: segment.header.number,
                page,
            }
            .into());
        }
        match (&segment.header.segment_type, &segment.data) {
            (SegmentType::PageInformation, _) => {
                return Err(MalformedError::DuplicatePageBoundary {
                    page,
                    segment_type: 48,
                }
                .into());
            }
            (SegmentType::EndOfStripe, ParsedData::EndOfStripe(row)) => {
                state.add_stripe(*row)?;
            }
            (SegmentType::EndOfPage, _) => state.finish()?,
            _ => {}
        }
    }
    Ok((page_states, page_order))
}

fn finalize_pages(
    mut page_states: PageStates,
    page_order: Vec<u32>,
) -> Result<Vec<PageInfo>, DecodeError> {
    let mut pages = Vec::new();
    let page_allocation = allocation_size::<PageInfo>(page_states.len())?;
    pages
        .try_reserve_exact(page_states.len())
        .map_err(|_| DecodeError::Allocation {
            requested_bytes: page_allocation,
        })?;
    for number in page_order {
        let state_index = keyed_index(&page_states, number)
            .ok_or(MalformedError::MissingPageInformation(number))?;
        let state = page_states.remove(state_index).1;
        if !state.ended {
            return Err(MalformedError::MissingEndOfPage(number).into());
        }
        pages.push(state.into_public(number)?);
    }
    pages.sort_unstable_by_key(PageInfo::number);
    for (index, page) in pages.iter().enumerate() {
        let expected = u32::try_from(index)
            .map_err(|_| MalformedError::IntegerOverflow)?
            .checked_add(1)
            .ok_or(MalformedError::IntegerOverflow)?;
        if page.number != expected {
            return Err(MalformedError::MissingPageInformation(expected).into());
        }
    }
    Ok(pages)
}

fn validate_profiles(segments: &[ParsedSegment<'_>]) -> Result<(), DecodeError> {
    let Some((first_index, first_segment)) = segments
        .iter()
        .enumerate()
        .find(|(_, segment)| segment.header.segment_type == SegmentType::Profiles)
    else {
        return Ok(());
    };
    if first_index != 0 || first_segment.header.page_association != 0 {
        return Err(MalformedError::InvalidProfilesSegment(first_segment.header.number).into());
    }
    let ParsedData::Profiles(global_profiles) = &first_segment.data else {
        return Err(MalformedError::InvalidProfilesSegment(first_segment.header.number).into());
    };
    let mut page_profiles = Vec::new();
    let profile_allocation = allocation_size::<(u32, u32)>(segments.len())?;
    page_profiles
        .try_reserve_exact(segments.len())
        .map_err(|_| DecodeError::Allocation {
            requested_bytes: profile_allocation,
        })?;
    for segment in &segments[first_index + 1..] {
        if segment.header.segment_type != SegmentType::Profiles {
            continue;
        }
        let page = segment.header.page_association;
        if page == 0 || !keyed_insert(&mut page_profiles, page, segment.header.number) {
            return Err(MalformedError::InvalidProfilesSegment(segment.header.number).into());
        }
        let ParsedData::Profiles(profiles) = &segment.data else {
            return Err(MalformedError::InvalidProfilesSegment(segment.header.number).into());
        };
        if !global_profiles
            .iter()
            .all(|profile| profiles.contains(profile))
        {
            return Err(MalformedError::InvalidProfilesSegment(segment.header.number).into());
        }
    }
    Ok(())
}

#[derive(Debug)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
struct PageState {
    info: PageInformation,
    last_stripe: Option<u32>,
    ended: bool,
}

impl PageState {
    const fn new(info: PageInformation) -> Self {
        Self {
            info,
            last_stripe: None,
            ended: false,
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn add_stripe(&mut self, row: u32) -> Result<(), DecodeError> {
        if !self.info.striped {
            return Err(MalformedError::InvalidStripeEndRow(row).into());
        }
        if let Some(previous) = self.last_stripe {
            if row <= previous || row - previous > u32::from(self.info.max_stripe_size) {
                return Err(MalformedError::InvalidStripeEndRow(row).into());
            }
        } else if row > u32::from(self.info.max_stripe_size) {
            return Err(MalformedError::InvalidStripeEndRow(row).into());
        }
        if let Some(height) = self.info.height {
            if row >= height {
                return Err(MalformedError::InvalidStripeEndRow(row).into());
            }
        }
        self.last_stripe = Some(row);
        Ok(())
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn finish(&mut self) -> Result<(), DecodeError> {
        if self.ended {
            return Err(MalformedError::DuplicatePageBoundary {
                page: 0,
                segment_type: 49,
            }
            .into());
        }
        if self.info.height.is_none() && self.last_stripe.is_none() {
            return Err(MalformedError::InvalidStripeEndRow(0).into());
        }
        self.ended = true;
        Ok(())
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn into_public(self, number: u32) -> Result<PageInfo, DecodeError> {
        let height = match (self.info.height, self.last_stripe) {
            (Some(height), _) => height,
            (None, Some(last)) => last.checked_add(1).ok_or(MalformedError::IntegerOverflow)?,
            (None, None) => return Err(MalformedError::InvalidStripeEndRow(0).into()),
        };
        Ok(PageInfo {
            number,
            width: self.info.width,
            height,
            x_resolution: self.info.x_resolution,
            y_resolution: self.info.y_resolution,
            striped: self.info.striped,
        })
    }
}

fn validate_references(
    segments: &[ParsedSegment<'_>],
    globals: Option<&GlobalSegments>,
    limits: &Limits,
) -> Result<(SegmentTypes, SegmentDepths), DecodeError> {
    let total_references = segments.iter().try_fold(0_u64, |total, segment| {
        total
            .checked_add(
                u64::try_from(segment.header.referred_segments.len())
                    .map_err(|_| MalformedError::IntegerOverflow)?,
            )
            .ok_or(MalformedError::IntegerOverflow)
    })?;
    if total_references > limits.max_total_references {
        return Err(LimitExceeded {
            kind: LimitKind::TotalReferences,
            limit: limits.max_total_references,
            actual: total_references,
        }
        .into());
    }

    let mut indexes = Vec::new();
    let index_allocation = allocation_size::<(u32, usize)>(segments.len())?;
    indexes
        .try_reserve_exact(segments.len())
        .map_err(|_| DecodeError::Allocation {
            requested_bytes: index_allocation,
        })?;
    for (index, segment) in segments.iter().enumerate() {
        if globals.is_some_and(|state| keyed_index(&state.types, segment.header.number).is_some()) {
            return Err(MalformedError::DuplicateSegmentNumber(segment.header.number).into());
        }
        if !keyed_insert(&mut indexes, segment.header.number, index) {
            return Err(MalformedError::DuplicateSegmentNumber(segment.header.number).into());
        }
    }

    for segment in segments {
        for referred in &segment.header.referred_segments {
            let Some(index) = keyed_get(&indexes, *referred) else {
                if globals.is_none_or(|state| keyed_index(&state.types, *referred).is_none()) {
                    return Err(MalformedError::MissingReferredSegment {
                        segment: segment.header.number,
                        referred: *referred,
                    }
                    .into());
                }
                continue;
            };
            let target = &segments[*index].header;
            if target.page_association != 0
                && target.page_association != segment.header.page_association
            {
                return Err(MalformedError::InvalidPageAssociation {
                    segment: segment.header.number,
                    page: segment.header.page_association,
                }
                .into());
            }
        }
        validate_reference_types(&segment.header, segments, &indexes, globals)?;
    }
    validate_retention(segments, &indexes)?;
    let depths = validate_reference_depth(segments, &indexes, globals, limits)?;
    let mut types = Vec::new();
    let type_allocation = allocation_size::<(u32, SegmentType)>(segments.len())?;
    types
        .try_reserve_exact(segments.len())
        .map_err(|_| DecodeError::Allocation {
            requested_bytes: type_allocation,
        })?;
    for segment in segments {
        let inserted = keyed_insert(
            &mut types,
            segment.header.number,
            segment.header.segment_type,
        );
        debug_assert!(inserted);
    }
    Ok((types, depths))
}

fn validate_retention(
    segments: &[ParsedSegment<'_>],
    indexes: &SegmentIndexes,
) -> Result<(), DecodeError> {
    let mut last_references: Vec<(u32, u32)> = Vec::new();
    let retention_allocation = allocation_size::<(u32, u32)>(segments.len())?;
    last_references
        .try_reserve_exact(segments.len())
        .map_err(|_| DecodeError::Allocation {
            requested_bytes: retention_allocation,
        })?;
    for segment in segments {
        for referred in &segment.header.referred_segments {
            if keyed_index(indexes, *referred).is_none() {
                continue;
            }
            if let Some(last) = keyed_get_mut(&mut last_references, *referred) {
                *last = (*last).max(segment.header.number);
            } else {
                let inserted = keyed_insert(&mut last_references, *referred, segment.header.number);
                debug_assert!(inserted);
            }
        }
    }

    for segment in segments {
        if let Some(last) = keyed_get(&last_references, segment.header.number) {
            if !segment.header.retention_flags.retain_this_segment() {
                return Err(MalformedError::InvalidRetentionFlags {
                    segment: segment.header.number,
                    referred: segment.header.number,
                }
                .into());
            }
            if *last <= segment.header.number {
                return Err(MalformedError::InvalidRetentionFlags {
                    segment: segment.header.number,
                    referred: segment.header.number,
                }
                .into());
            }
        }
        for (index, referred) in segment.header.referred_segments.iter().enumerate() {
            let Some(last) = keyed_get(&last_references, *referred) else {
                continue;
            };
            if segment
                .header
                .retention_flags
                .retain_referred_segment(index)
                == Some(false)
                && *last != segment.header.number
            {
                return Err(MalformedError::InvalidRetentionFlags {
                    segment: segment.header.number,
                    referred: *referred,
                }
                .into());
            }
        }
    }
    Ok(())
}

fn validate_reference_depth(
    segments: &[ParsedSegment<'_>],
    indexes: &SegmentIndexes,
    globals: Option<&GlobalSegments>,
    limits: &Limits,
) -> Result<SegmentDepths, DecodeError> {
    let ordered_allocation = allocation_size::<(u32, usize)>(segments.len())?;
    let mut ordered = Vec::new();
    ordered
        .try_reserve_exact(segments.len())
        .map_err(|_| DecodeError::Allocation {
            requested_bytes: ordered_allocation,
        })?;
    ordered.extend(
        segments
            .iter()
            .enumerate()
            .map(|(index, segment)| (segment.header.number, index)),
    );
    ordered.sort_unstable_by_key(|entry| entry.0);

    let mut depths = Vec::new();
    let depth_allocation = allocation_size::<(u32, u32)>(segments.len())?;
    depths
        .try_reserve_exact(segments.len())
        .map_err(|_| DecodeError::Allocation {
            requested_bytes: depth_allocation,
        })?;
    for (_, index) in ordered {
        let segment = &segments[index].header;
        let mut depth = 1_u32;
        for referred in &segment.referred_segments {
            let referred_depth = if keyed_index(indexes, *referred).is_some() {
                keyed_get(&depths, *referred).copied()
            } else {
                globals.and_then(|state| keyed_get(&state.depths, *referred).copied())
            }
            .ok_or(MalformedError::MissingReferredSegment {
                segment: segment.number,
                referred: *referred,
            })?;
            depth = depth.max(
                referred_depth
                    .checked_add(1)
                    .ok_or(MalformedError::IntegerOverflow)?,
            );
        }
        if depth > limits.max_reference_graph_depth {
            return Err(LimitExceeded {
                kind: LimitKind::ReferenceDepth,
                limit: u64::from(limits.max_reference_graph_depth),
                actual: u64::from(depth),
            }
            .into());
        }
        if !keyed_insert(&mut depths, segment.number, depth) {
            return Err(MalformedError::DuplicateSegmentNumber(segment.number).into());
        }
    }
    Ok(depths)
}

fn validate_reference_types(
    header: &SegmentHeader,
    segments: &[ParsedSegment<'_>],
    indexes: &SegmentIndexes,
    globals: Option<&GlobalSegments>,
) -> Result<(), DecodeError> {
    let reference_count = header.referred_segments.len();
    let known_kind = |reference_index: usize| {
        header
            .referred_segments
            .get(reference_index)
            .and_then(|number| {
                keyed_get(indexes, *number)
                    .map(|index| segments[*index].header.segment_type)
                    .or_else(|| globals.and_then(|state| keyed_get(&state.types, *number).copied()))
            })
    };
    let every_known_is = |predicate: fn(SegmentType) -> bool| {
        (0..reference_count).all(|index| known_kind(index).is_none_or(predicate))
    };
    let table_count = (0..reference_count)
        .filter(|index| known_kind(*index) == Some(SegmentType::Tables))
        .count();
    let valid = match header.segment_type {
        SegmentType::SymbolDictionary => {
            every_known_is(|kind| {
                matches!(kind, SegmentType::SymbolDictionary | SegmentType::Tables)
            }) && table_count <= 4
        }
        SegmentType::IntermediateTextRegion
        | SegmentType::ImmediateTextRegion
        | SegmentType::ImmediateLosslessTextRegion => {
            every_known_is(|kind| {
                matches!(kind, SegmentType::SymbolDictionary | SegmentType::Tables)
            }) && table_count <= 8
        }
        SegmentType::PatternDictionary
        | SegmentType::IntermediateGenericRegion
        | SegmentType::ImmediateGenericRegion
        | SegmentType::ImmediateLosslessGenericRegion
        | SegmentType::PageInformation
        | SegmentType::EndOfPage
        | SegmentType::EndOfStripe
        | SegmentType::EndOfFile
        | SegmentType::Profiles
        | SegmentType::Tables
        | SegmentType::ColourPalette => reference_count == 0,
        SegmentType::IntermediateHalftoneRegion
        | SegmentType::ImmediateHalftoneRegion
        | SegmentType::ImmediateLosslessHalftoneRegion => {
            reference_count == 1 && known_kind(0) == Some(SegmentType::PatternDictionary)
        }
        SegmentType::IntermediateGenericRefinementRegion => {
            reference_count == 1 && known_kind(0).is_some_and(is_intermediate_region)
        }
        SegmentType::ImmediateGenericRefinementRegion
        | SegmentType::ImmediateLosslessGenericRefinementRegion => {
            reference_count == 0
                || (reference_count == 1 && known_kind(0).is_some_and(is_intermediate_region))
        }
        SegmentType::Extension => true,
    };
    if !valid {
        return Err(MalformedError::InvalidSegmentReferences(header.number).into());
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out == document_spec::production_is_intermediate_region(segment_type),
))]
const fn is_intermediate_region(segment_type: SegmentType) -> bool {
    #[cfg(verus_keep_ghost)]
    proof! {
        document_shared::intermediate_region_runtime_fact(segment_type);
    }
    matches!(
        segment_type,
        SegmentType::IntermediateTextRegion
            | SegmentType::IntermediateHalftoneRegion
            | SegmentType::IntermediateGenericRegion
            | SegmentType::IntermediateGenericRefinementRegion
    )
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn validate_page_association(header: &SegmentHeader) -> Result<(), DecodeError> {
    let must_have_page = matches!(
        header.segment_type,
        SegmentType::IntermediateTextRegion
            | SegmentType::ImmediateTextRegion
            | SegmentType::ImmediateLosslessTextRegion
            | SegmentType::IntermediateHalftoneRegion
            | SegmentType::ImmediateHalftoneRegion
            | SegmentType::ImmediateLosslessHalftoneRegion
            | SegmentType::IntermediateGenericRegion
            | SegmentType::ImmediateGenericRegion
            | SegmentType::ImmediateLosslessGenericRegion
            | SegmentType::IntermediateGenericRefinementRegion
            | SegmentType::ImmediateGenericRefinementRegion
            | SegmentType::ImmediateLosslessGenericRefinementRegion
            | SegmentType::PageInformation
            | SegmentType::EndOfPage
            | SegmentType::EndOfStripe
    );
    if (must_have_page && header.page_association == 0)
        || (matches!(header.segment_type, SegmentType::EndOfFile) && header.page_association != 0)
    {
        return Err(MalformedError::InvalidPageAssociation {
            segment: header.number,
            page: header.page_association,
        }
        .into());
    }
    Ok(())
}

pub(crate) fn read_bounded_input<R: Read>(
    reader: &mut R,
    limits: &Limits,
) -> Result<Vec<u8>, DecodeError> {
    let mut output = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let count = reader.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        let new_length = output
            .len()
            .checked_add(count)
            .ok_or(MalformedError::IntegerOverflow)?;
        let new_length_u64 =
            u64::try_from(new_length).map_err(|_| MalformedError::IntegerOverflow)?;
        if new_length_u64 > limits.max_stream_bytes {
            return Err(LimitExceeded {
                kind: LimitKind::StreamBytes,
                limit: limits.max_stream_bytes,
                actual: new_length_u64,
            }
            .into());
        }
        output
            .try_reserve_exact(count)
            .map_err(|_| DecodeError::Allocation {
                requested_bytes: new_length_u64,
            })?;
        output.extend_from_slice(&chunk[..count]);
    }
    Ok(output)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) => length as nat <= limits.max_stream_bytes,
        Err(_) => true,
    },
))]
fn enforce_stream_limit(length: usize, limits: &Limits) -> Result<(), DecodeError> {
    let Ok(actual) = u64::try_from(length) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    if actual > limits.max_stream_bytes {
        return Err(LimitExceeded {
            kind: LimitKind::StreamBytes,
            limit: limits.max_stream_bytes,
            actual,
        }
        .into());
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) => count as nat + 1 <= limits.max_segments,
        Err(_) => true,
    },
))]
fn enforce_segment_limit(count: usize, limits: &Limits) -> Result<(), DecodeError> {
    let Some(next_count) = count.checked_add(1) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    let Ok(actual) = u64::try_from(next_count) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    if actual > u64::from(limits.max_segments) {
        return Err(LimitExceeded {
            kind: LimitKind::Segments,
            limit: u64::from(limits.max_segments),
            actual,
        }
        .into());
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) => count as nat + 1 <= limits.max_pages,
        Err(_) => true,
    },
))]
fn enforce_page_limit(count: usize, limits: &Limits) -> Result<(), DecodeError> {
    let Some(next_count) = count.checked_add(1) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    let Ok(actual) = u64::try_from(next_count) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    if actual > u64::from(limits.max_pages) {
        return Err(LimitExceeded {
            kind: LimitKind::Pages,
            limit: u64::from(limits.max_pages),
            actual,
        }
        .into());
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn enforce_work_limit(
    input_length: usize,
    segments: &[ParsedSegment<'_>],
    limits: &Limits,
) -> Result<(), DecodeError> {
    let Ok(input_work) = u64::try_from(input_length) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    let Ok(segment_work) = u64::try_from(segments.len()) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    let mut reference_work = 0_u64;
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= segments@.len(),
        decreases segments@.len() - index,
    ))]
    while index < segments.len() {
        let Ok(count) = u64::try_from(segments[index].header.referred_segments.len()) else {
            return Err(MalformedError::IntegerOverflow.into());
        };
        let Some(next) = reference_work.checked_add(count) else {
            return Err(MalformedError::IntegerOverflow.into());
        };
        reference_work = next;
        index += 1;
    }
    let Some(partial) = input_work.checked_add(segment_work) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    let Some(actual) = partial.checked_add(reference_work) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    if actual > limits.max_work_units {
        return Err(LimitExceeded {
            kind: LimitKind::Work,
            limit: limits.max_work_units,
            actual,
        }
        .into());
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) => input_length as nat <= limits.max_work_units,
        Err(_) => true,
    },
))]
fn enforce_input_work(input_length: usize, limits: &Limits) -> Result<(), DecodeError> {
    let Ok(actual) = u64::try_from(input_length) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    if actual > limits.max_work_units {
        return Err(LimitExceeded {
            kind: LimitKind::Work,
            limit: limits.max_work_units,
            actual,
        }
        .into());
    }
    Ok(())
}

fn allocation_size<T>(count: usize) -> Result<u64, MalformedError> {
    count
        .checked_mul(std::mem::size_of::<T>())
        .and_then(|value| u64::try_from(value).ok())
        .ok_or(MalformedError::IntegerOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIGNATURE: [u8; 8] = [0x97, 0x4A, 0x42, 0x32, 0x0D, 0x0A, 0x1A, 0x0A];

    fn header(number: u32, segment_type: u8, page: u8, data_length: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&number.to_be_bytes());
        bytes.push(segment_type);
        bytes.push(0);
        bytes.push(page);
        bytes.extend_from_slice(&data_length.to_be_bytes());
        bytes
    }

    fn header_with_reference(number: u32, segment_type: u8, referred: u8) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&number.to_be_bytes());
        bytes.push(segment_type);
        bytes.push(0x21);
        bytes.push(referred);
        bytes.push(0);
        bytes.extend_from_slice(&0_u32.to_be_bytes());
        bytes
    }

    fn page_data(height: u32) -> [u8; 19] {
        let mut data = [0_u8; 19];
        data[0..4].copy_from_slice(&8_u32.to_be_bytes());
        data[4..8].copy_from_slice(&height.to_be_bytes());
        data
    }

    #[test]
    fn inspects_sequential_document() {
        let bytes = valid_sequential_document();

        let info = Decoder::default()
            .inspect(&bytes)
            .expect("document is valid");
        assert_eq!(info.segment_count(), 3);
        assert_eq!(info.pages().len(), 1);
        assert_eq!(info.pages()[0].width(), 8);
        assert_eq!(info.pages()[0].height(), 8);
    }

    #[test]
    fn inspects_random_access_document() {
        let mut bytes = SIGNATURE.to_vec();
        bytes.push(0x00);
        bytes.extend_from_slice(&1_u32.to_be_bytes());
        bytes.extend_from_slice(&header(0, 48, 1, 19));
        bytes.extend_from_slice(&header(1, 49, 1, 0));
        bytes.extend_from_slice(&header(2, 51, 0, 0));
        bytes.extend_from_slice(&page_data(8));

        let info = Decoder::default()
            .inspect(&bytes)
            .expect("document is valid");
        assert_eq!(info.organization(), Organization::RandomAccess);
        assert_eq!(info.segment_count(), 3);
    }

    #[test]
    fn finalizes_unknown_height_from_stripe() {
        let mut bytes = SIGNATURE.to_vec();
        bytes.push(0x03);
        bytes.extend_from_slice(&header(0, 48, 1, 19));
        let mut page = page_data(u32::MAX);
        page[17..19].copy_from_slice(&0x800A_u16.to_be_bytes());
        bytes.extend_from_slice(&page);
        bytes.extend_from_slice(&header(1, 50, 1, 4));
        bytes.extend_from_slice(&7_u32.to_be_bytes());
        bytes.extend_from_slice(&header(2, 49, 1, 0));

        let info = Decoder::default()
            .inspect(&bytes)
            .expect("document is valid");
        assert_eq!(info.pages()[0].height(), 8);
        assert!(info.pages()[0].is_striped());
    }

    #[test]
    fn rejects_page_count_mismatch() {
        let mut bytes = SIGNATURE.to_vec();
        bytes.push(0x01);
        bytes.extend_from_slice(&2_u32.to_be_bytes());
        bytes.extend_from_slice(&header(0, 48, 1, 19));
        bytes.extend_from_slice(&page_data(8));
        bytes.extend_from_slice(&header(1, 49, 1, 0));
        assert!(matches!(
            Decoder::default().inspect(&bytes),
            Err(DecodeError::Malformed(MalformedError::PageCountMismatch {
                declared: 2,
                actual: 1
            }))
        ));
    }

    #[test]
    fn bounded_reader_enforces_stream_limit() {
        let limits = Limits {
            max_stream_bytes: 4,
            ..Limits::default()
        };
        let mut input = std::io::Cursor::new([0_u8; 5]);
        assert!(matches!(
            Decoder::new(limits).inspect_reader(&mut input),
            Err(DecodeError::LimitExceeded(LimitExceeded {
                kind: LimitKind::StreamBytes,
                limit: 4,
                actual: 5
            }))
        ));
    }

    #[test]
    fn inspection_enforces_work_limit() {
        let bytes = valid_sequential_document();
        let limits = Limits {
            max_work_units: 10,
            ..Limits::default()
        };
        assert!(matches!(
            Decoder::new(limits).inspect(&bytes),
            Err(DecodeError::LimitExceeded(LimitExceeded {
                kind: LimitKind::Work,
                limit: 10,
                ..
            }))
        ));
    }

    #[test]
    fn locates_unknown_length_generic_terminator() {
        let mut data = vec![0_u8; 18];
        data[4..8].copy_from_slice(&3_u32.to_be_bytes());
        data[17] = 0;
        data.extend_from_slice(&[0x11, 0x22, 0xFF, 0xAC]);
        data.extend_from_slice(&3_u32.to_be_bytes());
        data.extend_from_slice(&[0xAA, 0xBB]);
        assert_eq!(
            find_unknown_generic_length(&data).expect("terminator is present"),
            26
        );
    }

    #[test]
    fn rejects_every_byte_truncation_of_minimal_document() {
        let bytes = valid_sequential_document();
        let valid_without_optional_eof = bytes.len() - header(2, 51, 0, 0).len();
        for length in 0..bytes.len() {
            if Decoder::default().inspect(&bytes[..length]).is_ok() {
                assert_eq!(
                    length, valid_without_optional_eof,
                    "truncation at byte {length} unexpectedly succeeded"
                );
            }
        }
    }

    #[test]
    fn document_inspection_is_deterministic() {
        let bytes = valid_sequential_document();
        let first = Decoder::default()
            .inspect(&bytes)
            .expect("document is valid");
        let second = Decoder::default()
            .inspect(&bytes)
            .expect("document is valid");
        assert_eq!(first, second);
    }

    #[test]
    fn enforces_total_reference_and_depth_limits() {
        let mut bytes = SIGNATURE.to_vec();
        bytes.push(0x01);
        bytes.extend_from_slice(&0_u32.to_be_bytes());
        bytes.extend_from_slice(&header(0, 0, 0, 0));
        let retention_index = SIGNATURE.len() + 1 + 4 + 5;
        bytes[retention_index] = 1;
        bytes.extend_from_slice(&header_with_reference(1, 0, 0));
        bytes.extend_from_slice(&header_with_reference(2, 0, 1));
        bytes.extend_from_slice(&header(3, 51, 0, 0));

        let depth_limits = Limits {
            max_reference_graph_depth: 2,
            ..Limits::default()
        };
        assert!(matches!(
            Decoder::new(depth_limits).inspect(&bytes),
            Err(DecodeError::LimitExceeded(LimitExceeded {
                kind: LimitKind::ReferenceDepth,
                limit: 2,
                actual: 3
            }))
        ));

        let reference_limits = Limits {
            max_total_references: 1,
            ..Limits::default()
        };
        assert!(matches!(
            Decoder::new(reference_limits).inspect(&bytes),
            Err(DecodeError::LimitExceeded(LimitExceeded {
                kind: LimitKind::TotalReferences,
                limit: 1,
                actual: 2
            }))
        ));
    }

    #[test]
    fn rejects_reference_to_nonretained_segment() {
        let mut bytes = SIGNATURE.to_vec();
        bytes.push(0x01);
        bytes.extend_from_slice(&0_u32.to_be_bytes());
        bytes.extend_from_slice(&header(0, 0, 0, 0));
        bytes.extend_from_slice(&header_with_reference(1, 0, 0));
        assert!(matches!(
            Decoder::default().inspect(&bytes),
            Err(DecodeError::Malformed(
                MalformedError::InvalidRetentionFlags {
                    segment: 0,
                    referred: 0
                }
            ))
        ));
    }

    #[test]
    fn embedded_page_resolves_reusable_globals_explicitly() {
        let global_bytes = header(0, 0, 0, 0);
        let globals = Decoder::default()
            .parse_globals(&global_bytes)
            .expect("global dictionary is valid");
        assert_eq!(globals.segment_count(), 1);

        let mut page_bytes = header(1, 48, 1, 19);
        page_bytes.extend_from_slice(&page_data(8));
        let mut text_header = header_with_reference(2, 6, 0);
        text_header[7] = 1;
        page_bytes.extend_from_slice(&text_header);
        page_bytes.extend_from_slice(&header(3, 49, 1, 0));

        assert!(matches!(
            Decoder::default().inspect_embedded(&page_bytes),
            Err(DecodeError::Malformed(
                MalformedError::MissingReferredSegment {
                    segment: 2,
                    referred: 0
                }
            ))
        ));
        let info = Decoder::default()
            .inspect_embedded_with_globals(&page_bytes, &globals)
            .expect("page resolves its global dictionary");
        assert_eq!(info.pages().len(), 1);
    }

    fn valid_sequential_document() -> Vec<u8> {
        let mut bytes = SIGNATURE.to_vec();
        bytes.push(0x01);
        bytes.extend_from_slice(&1_u32.to_be_bytes());
        bytes.extend_from_slice(&header(0, 48, 1, 19));
        bytes.extend_from_slice(&page_data(8));
        bytes.extend_from_slice(&header(1, 49, 1, 0));
        bytes.extend_from_slice(&header(2, 51, 0, 0));
        bytes
    }
}

use std::io::Read;

use crate::file_header::{inspect_file_header, Organization};
use crate::huffman::{HuffmanError, HuffmanTable};
use crate::mmr::MmrScratch;
use crate::reader::ByteReader;
use crate::segment_data::{
    parse_end_of_stripe, parse_extension, parse_page_information, parse_profiles, require_empty,
    PageInformation,
};
use crate::segment_header::{parse_segment_header, SegmentHeader, SegmentType};
use crate::{DecodeError, LimitExceeded, LimitKind, Limits, MalformedError, UnsupportedFeature};
#[cfg(verus_keep_ghost)]
use vstd::layout;
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

#[cfg_attr(verus_keep_ghost, verus_spec(
    ensures final(entries)@.len() == old(entries)@.len(),
))]
fn keyed_get_mut<T>(entries: &mut [(u32, T)], number: u32) -> Option<&mut T> {
    let index = keyed_index(entries, number)?;
    Some(&mut entries[index].1)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures if out {
        final(entries)@ == old(entries)@.push((number, value))
    } else {
        final(entries)@ == old(entries)@
    },
))]
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

/// Reusable decoding state for repeated documents with one [`Decoder`].
///
/// The context retains only allocation capacity, never document semantics.
/// Every operation clears scratch lengths after success or error. Call
/// [`Self::reset`] to clear scratch while retaining capacity, or
/// [`Self::release_scratch`] to return all retained allocations.
#[derive(Debug)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub struct DecoderContext<'a> {
    pub(crate) decoder: &'a Decoder,
    pub(crate) mmr: MmrScratch,
}

impl Decoder {
    /// Creates reusable scratch state tied to this decoder's limits.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub fn context(&self) -> DecoderContext<'_> {
        DecoderContext {
            decoder: self,
            mmr: MmrScratch::default(),
        }
    }

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
}

impl DecoderContext<'_> {
    /// Returns the limits governing this context.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub const fn limits(&self) -> &Limits {
        self.decoder.limits()
    }

    /// Exact bytes currently retained by reusable allocation capacities.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub fn retained_scratch_bytes(&self) -> u64 {
        self.mmr.retained_bytes()
    }

    /// Clears all per-document scratch contents while retaining capacity.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub fn reset(&mut self) {
        self.mmr.clear();
    }

    /// Clears per-document state and releases every retained allocation.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub fn release_scratch(&mut self) {
        self.mmr.release();
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub(crate) fn finish_call(&mut self) {
        self.reset();
        if self.retained_scratch_bytes() > self.limits().max_retained_state_bytes {
            self.release_scratch();
        }
    }

    /// Parses and validates one standalone document.
    ///
    /// # Errors
    ///
    /// Returns the same typed errors as [`Decoder::inspect`].
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures match out {
            Ok(info) => document_spec::standalone_metadata(input@, &info),
            Err(_) => true,
        },
    ))]
    pub fn inspect(&mut self, input: &[u8]) -> Result<DocumentInfo, DecodeError> {
        let result = self.decoder.inspect(input);
        self.finish_call();
        result
    }

    /// Parses one embedded page stream.
    ///
    /// # Errors
    ///
    /// Returns the same typed errors as [`Decoder::inspect_embedded`].
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures match out {
            Ok(info) => document_spec::embedded_metadata(&info),
            Err(_) => true,
        },
    ))]
    pub fn inspect_embedded(&mut self, input: &[u8]) -> Result<DocumentInfo, DecodeError> {
        let result = self.decoder.inspect_embedded(input);
        self.finish_call();
        result
    }

    /// Parses one embedded page stream with validated globals.
    ///
    /// # Errors
    ///
    /// Returns the same typed errors as [`Decoder::inspect_embedded_with_globals`].
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures match out {
            Ok(info) => document_spec::embedded_metadata(&info),
            Err(_) => true,
        },
    ))]
    pub fn inspect_embedded_with_globals(
        &mut self,
        input: &[u8],
        globals: &GlobalSegments,
    ) -> Result<DocumentInfo, DecodeError> {
        let result = self.decoder.inspect_embedded_with_globals(input, globals);
        self.finish_call();
        result
    }

    /// Parses and validates immutable embedded global segments.
    ///
    /// # Errors
    ///
    /// Returns the same typed errors as [`Decoder::parse_globals`].
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub fn parse_globals(&mut self, input: &[u8]) -> Result<GlobalSegments, DecodeError> {
        let result = self.decoder.parse_globals(input);
        self.finish_call();
        result
    }
}

impl Decoder {
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
    /// On success, organization and the optional declared count match the
    /// standalone header; a known count equals the returned page count.
    ///
    /// # Errors
    ///
    /// Returns a typed error for malformed, unsupported, over-limit, or
    /// allocation-failing input.
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures match out {
            Ok(info) => document_spec::standalone_metadata(input@, &info),
            Err(_) => true,
        },
    ))]
    pub fn inspect(&self, input: &[u8]) -> Result<DocumentInfo, DecodeError> {
        let (info, _segments) = parse_standalone_document(input, &self.limits)?;
        Ok(info)
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
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures match out {
            Ok(info) => document_spec::embedded_metadata(&info),
            Err(_) => true,
        },
    ))]
    pub fn inspect_embedded(&self, input: &[u8]) -> Result<DocumentInfo, DecodeError> {
        self.inspect_embedded_internal(input, None)
    }

    /// Parses an embedded page stream with reusable validated global segments.
    ///
    /// # Errors
    ///
    /// Returns a typed error for malformed, unsupported, over-limit,
    /// allocation-failing, or unresolved input.
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures match out {
            Ok(info) => document_spec::embedded_metadata(&info),
            Err(_) => true,
        },
    ))]
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
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub fn parse_globals(&self, input: &[u8]) -> Result<GlobalSegments, DecodeError> {
        enforce_stream_limit(input.len(), &self.limits)?;
        enforce_input_work(input.len(), &self.limits)?;
        let segments = parse_sequential(input, &self.limits, false)?;
        let mut segment_index = 0_usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant segment_index <= segments@.len(),
            decreases segments@.len() - segment_index,
        ))]
        while segment_index < segments.len() {
            let segment = &segments[segment_index];
            if segment.header.page_association != 0 {
                return Err(MalformedError::InvalidPageAssociation {
                    segment: segment.header.number,
                    page: segment.header.page_association,
                }
                .into());
            }
            segment_index += 1;
        }
        validate_profiles(&segments)?;
        enforce_work_limit(input.len(), &segments, &self.limits)?;
        let (types, depths) = validate_references(&segments, None, &self.limits)?;
        let Ok(input_bytes) = u64::try_from(input.len()) else {
            return Err(MalformedError::IntegerOverflow.into());
        };
        if input_bytes > self.limits.max_retained_state_bytes {
            return Err(LimitExceeded {
                kind: LimitKind::RetainedState,
                limit: self.limits.max_retained_state_bytes,
                actual: input_bytes,
            }
            .into());
        }
        let mut bytes = Vec::new();
        if bytes.try_reserve(input.len()).is_err() {
            return Err(DecodeError::Allocation {
                requested_bytes: input_bytes,
            });
        }
        let mut byte_index = 0_usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                byte_index <= input@.len(),
                bytes@ == input@.subrange(0, byte_index as int),
            decreases input@.len() - byte_index,
        ))]
        while byte_index < input.len() {
            bytes.push(input[byte_index]);
            byte_index += 1;
        }
        Ok(GlobalSegments {
            types,
            depths,
            bytes,
        })
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures match out {
            Ok(info) => document_spec::embedded_metadata(&info),
            Err(_) => true,
        },
    ))]
    fn inspect_embedded_internal(
        &self,
        input: &[u8],
        globals: Option<&GlobalSegments>,
    ) -> Result<DocumentInfo, DecodeError> {
        let (info, _segments) = parse_embedded_document(input, &self.limits, globals)?;
        Ok(info)
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures document_spec::default_limits(
            document_spec::decoder_limits(&out),
        ),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    fn project_default() -> Self {
        Self::new(Limits::project_default())
    }
}

impl Default for Decoder {
    fn default() -> Self {
        Self::project_default()
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

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(result) =>
            document_spec::standalone_metadata(input@, &result.0)
            && document_spec::document_segment_count(&result.0) as nat == result.1@.len()
            && document_spec::document_pages(&result.0).len()
                == document_spec::page_information_count(result.1@, result.1@.len()),
        Err(_) => true,
    },
))]
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
    let header_length = file_header.byte_length();
    if header_length > input.len() {
        return Err(MalformedError::IntegerOverflow.into());
    }
    let body = &input[header_length..];
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

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(result) =>
            document_spec::embedded_metadata(&result.0)
            && document_spec::document_segment_count(&result.0) as nat == result.1@.len()
            && document_spec::document_pages(&result.0).len()
                == document_spec::page_information_count(result.1@, result.1@.len()),
        Err(_) => true,
    },
))]
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

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn parse_sequential<'a>(
    input: &'a [u8],
    limits: &Limits,
    require_local_references: bool,
) -> Result<Vec<ParsedSegment<'a>>, DecodeError> {
    let mut reader = ByteReader::new(input);
    let mut segments = Vec::new();
    let mut previous_number = None;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            reader.position <= reader.bytes@.len(),
            reader.bytes@ == input@,
        decreases reader.bytes@.len() - reader.position,
    ))]
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
        if segments.try_reserve(1).is_err() {
            return Err(DecodeError::Allocation {
                requested_bytes: segment_allocation,
            });
        }
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

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn parse_random_access<'a>(
    input: &'a [u8],
    limits: &Limits,
) -> Result<Vec<ParsedSegment<'a>>, DecodeError> {
    let mut reader = ByteReader::new(input);
    let mut headers = Vec::new();
    let mut previous_number = None;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            reader.position <= reader.bytes@.len(),
            reader.bytes@ == input@,
        decreases reader.bytes@.len() - reader.position,
    ))]
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
        let requested_bytes = allocation_size::<SegmentHeader>(1)?;
        if headers.try_reserve(1).is_err() {
            return Err(DecodeError::Allocation { requested_bytes });
        }
        headers.push(header);
        if is_end {
            break;
        }
    }

    let mut segments = Vec::new();
    let segment_allocation = allocation_size::<ParsedSegment<'_>>(headers.len())?;
    if segments.try_reserve(headers.len()).is_err() {
        return Err(DecodeError::Allocation {
            requested_bytes: segment_allocation,
        });
    }
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            reader.position <= reader.bytes@.len(),
            reader.bytes@ == input@,
    ))]
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
    ensures
        final(reader).bytes@ == old(reader).bytes@,
        final(reader).position <= final(reader).bytes@.len(),
        final(reader).position >= old(reader).position,
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

#[cfg_attr(verus_keep_ghost, verus_verify)]
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
            if let Err(error) = HuffmanTable::from_encoded(payload, limits) {
                let error = match error {
                    HuffmanError::Decode(error) => error,
                    HuffmanError::InvalidTable(kind) => {
                        DecodeError::Malformed(MalformedError::InvalidHuffman(kind))
                    }
                };
                return Err(error);
            }
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

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(info) =>
            document_spec::document_organization(&info) == organization
            && document_spec::document_declared_page_count(&info) == declared_page_count
            && document_spec::document_segment_count(&info) as nat == segments@.len()
            && document_spec::document_pages(&info).len()
                == document_spec::page_information_count(segments@, segments@.len())
            && match declared_page_count {
                Some(count) => document_spec::document_pages(&info).len() == count as nat,
                None => true,
            },
        Err(_) => true,
    },
))]
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
    let Ok(actual_page_count) = u32::try_from(page_states.len()) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    if let Some(declared) = declared_page_count {
        if declared != actual_page_count {
            return Err(MalformedError::PageCountMismatch {
                declared,
                actual: actual_page_count,
            }
            .into());
        }
    }
    let pages = finalize_pages(page_states, &page_order)?;

    let Ok(segment_count) = u32::try_from(segments.len()) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    let info = DocumentInfo {
        organization,
        declared_page_count,
        segment_count,
        pages,
    };
    #[cfg(verus_keep_ghost)]
    production_document_fields!(&info);
    Ok(info)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(result) =>
            result.0@.len() == result.1@.len()
            && result.0@.len()
                == document_spec::page_information_count(segments@, segments@.len()),
        Err(_) => true,
    },
))]
fn collect_page_states(
    segments: &[ParsedSegment<'_>],
    limits: &Limits,
) -> Result<(PageStates, Vec<u32>), DecodeError> {
    let mut page_states = Vec::new();
    let page_state_allocation =
        allocation_size::<PageState>(segments.len().min(limits.max_pages as usize))?;
    if page_states
        .try_reserve(segments.len().min(limits.max_pages as usize))
        .is_err()
    {
        return Err(DecodeError::Allocation {
            requested_bytes: page_state_allocation,
        });
    }
    let mut page_order = Vec::new();
    let page_order_capacity = segments.len().min(limits.max_pages as usize);
    let page_order_allocation = allocation_size::<u32>(page_order_capacity)?;
    if page_order.try_reserve(page_order_capacity).is_err() {
        return Err(DecodeError::Allocation {
            requested_bytes: page_order_allocation,
        });
    }

    collect_page_states_into(segments, limits, &mut page_states, &mut page_order)?;
    Ok((page_states, page_order))
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        old(page_states)@.len() == 0,
        old(page_order)@.len() == 0,
    ensures out.is_ok() ==>
        final(page_states)@.len() == final(page_order)@.len()
        && final(page_states)@.len()
            == document_spec::page_information_count(segments@, segments@.len()),
))]
fn collect_page_states_into(
    segments: &[ParsedSegment<'_>],
    limits: &Limits,
    page_states: &mut PageStates,
    page_order: &mut Vec<u32>,
) -> Result<(), DecodeError> {
    let mut segment_index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            segment_index <= segments@.len(),
            page_states@.len() == page_order@.len(),
            page_states@.len()
                == document_spec::page_information_count(segments@, segment_index as nat),
        decreases segments@.len() - segment_index,
    ))]
    while segment_index < segments.len() {
        #[cfg(verus_keep_ghost)]
        production_page_information_count_step!(segments, segment_index);
        let segment = &segments[segment_index];
        validate_page_association(&segment.header)?;
        let page = segment.header.page_association;
        if page == 0 {
            segment_index += 1;
            continue;
        }

        if keyed_index(page_states, page).is_none() {
            if !matches!(segment.header.segment_type, SegmentType::PageInformation) {
                return Err(MalformedError::MissingPageInformation(page).into());
            }
            enforce_page_limit(page_states.len(), limits)?;
            let ParsedData::Page(info) = segment.data else {
                return Err(MalformedError::MissingPageInformation(page).into());
            };
            if !keyed_insert(page_states, page, PageState::new(info)) {
                return Err(MalformedError::DuplicatePageBoundary {
                    page,
                    segment_type: 48,
                }
                .into());
            }
            page_order.push(page);
            segment_index += 1;
            continue;
        }

        let state =
            keyed_get_mut(page_states, page).ok_or(MalformedError::MissingPageInformation(page))?;
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
        segment_index += 1;
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(pages) => pages@.len() == page_order@.len(),
        Err(_) => true,
    },
))]
fn finalize_pages(
    mut page_states: PageStates,
    page_order: &[u32],
) -> Result<Vec<PageInfo>, DecodeError> {
    let mut pages: Vec<PageInfo> = Vec::new();
    let page_allocation = allocation_size::<PageInfo>(page_states.len())?;
    if pages.try_reserve(page_states.len()).is_err() {
        return Err(DecodeError::Allocation {
            requested_bytes: page_allocation,
        });
    }
    let mut order_index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            order_index <= page_order@.len(),
            pages@.len() == order_index,
        decreases page_order@.len() - order_index,
    ))]
    while order_index < page_order.len() {
        let number = page_order[order_index];
        let state_index = keyed_index(&page_states, number)
            .ok_or(MalformedError::MissingPageInformation(number))?;
        let state = page_states.remove(state_index).1;
        if !state.ended {
            return Err(MalformedError::MissingEndOfPage(number).into());
        }
        let page = state.into_public(number)?;
        let mut position = 0_usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant position <= pages@.len(),
            decreases pages@.len() - position,
        ))]
        while position < pages.len() && pages[position].number < number {
            position += 1;
        }
        pages.insert(position, page);
        order_index += 1;
    }
    validate_public_page_numbers(&pages)?;
    Ok(pages)
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn validate_public_page_numbers(pages: &[PageInfo]) -> Result<(), DecodeError> {
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= pages@.len(),
        decreases pages@.len() - index,
    ))]
    while index < pages.len() {
        let Ok(index_u32) = u32::try_from(index) else {
            return Err(MalformedError::IntegerOverflow.into());
        };
        let Some(expected) = index_u32.checked_add(1) else {
            return Err(MalformedError::IntegerOverflow.into());
        };
        if pages[index].number != expected {
            return Err(MalformedError::MissingPageInformation(expected).into());
        }
        index += 1;
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn validate_profiles(segments: &[ParsedSegment<'_>]) -> Result<(), DecodeError> {
    let mut first_index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant first_index <= segments@.len(),
        decreases segments@.len() - first_index,
    ))]
    while first_index < segments.len()
        && !matches!(
            segments[first_index].header.segment_type,
            SegmentType::Profiles
        )
    {
        first_index += 1;
    }
    if first_index == segments.len() {
        return Ok(());
    }
    let first_segment = &segments[first_index];
    if first_index != 0 || first_segment.header.page_association != 0 {
        return Err(MalformedError::InvalidProfilesSegment(first_segment.header.number).into());
    }
    let ParsedData::Profiles(global_profiles) = &first_segment.data else {
        return Err(MalformedError::InvalidProfilesSegment(first_segment.header.number).into());
    };
    let mut page_profiles = Vec::new();
    let profile_allocation = allocation_size::<(u32, u32)>(segments.len())?;
    if page_profiles.try_reserve(segments.len()).is_err() {
        return Err(DecodeError::Allocation {
            requested_bytes: profile_allocation,
        });
    }
    validate_page_profiles(
        segments,
        first_index + 1,
        global_profiles,
        &mut page_profiles,
    )
}

#[cfg_attr(verus_keep_ghost, verus_spec(
    requires start <= segments@.len(),
))]
fn validate_page_profiles(
    segments: &[ParsedSegment<'_>],
    start: usize,
    global_profiles: &[u32],
    page_profiles: &mut Vec<(u32, u32)>,
) -> Result<(), DecodeError> {
    let mut segment_index = start;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant start <= segment_index <= segments@.len(),
        decreases segments@.len() - segment_index,
    ))]
    while segment_index < segments.len() {
        let segment = &segments[segment_index];
        if !matches!(segment.header.segment_type, SegmentType::Profiles) {
            segment_index += 1;
            continue;
        }
        let page = segment.header.page_association;
        if page == 0 || !keyed_insert(page_profiles, page, segment.header.number) {
            return Err(MalformedError::InvalidProfilesSegment(segment.header.number).into());
        }
        let ParsedData::Profiles(profiles) = &segment.data else {
            return Err(MalformedError::InvalidProfilesSegment(segment.header.number).into());
        };
        let mut profile_index = 0_usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant profile_index <= global_profiles@.len(),
            decreases global_profiles@.len() - profile_index,
        ))]
        while profile_index < global_profiles.len() {
            if !contains_profile(profiles, global_profiles[profile_index]) {
                return Err(MalformedError::InvalidProfilesSegment(segment.header.number).into());
            }
            profile_index += 1;
        }
        segment_index += 1;
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn contains_profile(profiles: &[u32], wanted: u32) -> bool {
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= profiles@.len(),
        decreases profiles@.len() - index,
    ))]
    while index < profiles.len() {
        if profiles[index] == wanted {
            return true;
        }
        index += 1;
    }
    false
}

#[derive(Debug)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
struct PageState {
    info: PageInformation,
    last_stripe: Option<u32>,
    ended: bool,
}

impl PageState {
    #[cfg_attr(verus_keep_ghost, verus_verify)]
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

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn validate_references(
    segments: &[ParsedSegment<'_>],
    globals: Option<&GlobalSegments>,
    limits: &Limits,
) -> Result<(SegmentTypes, SegmentDepths), DecodeError> {
    let total_references = count_total_references(segments)?;
    if total_references > limits.max_total_references {
        return Err(LimitExceeded {
            kind: LimitKind::TotalReferences,
            limit: limits.max_total_references,
            actual: total_references,
        }
        .into());
    }

    let mut indexes: SegmentIndexes = Vec::new();
    let index_allocation = allocation_size::<(u32, usize)>(segments.len())?;
    if indexes.try_reserve(segments.len()).is_err() {
        return Err(DecodeError::Allocation {
            requested_bytes: index_allocation,
        });
    }
    let mut segment_index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            segment_index <= segments@.len(),
            forall|entry: int| 0 <= entry < indexes@.len() ==>
                indexes@[entry].1 < segments@.len(),
        decreases segments@.len() - segment_index,
    ))]
    while segment_index < segments.len() {
        let segment = &segments[segment_index];
        let global_duplicate = match globals {
            Some(state) => keyed_index(&state.types, segment.header.number).is_some(),
            None => false,
        };
        if global_duplicate {
            return Err(MalformedError::DuplicateSegmentNumber(segment.header.number).into());
        }
        if !keyed_insert(&mut indexes, segment.header.number, segment_index) {
            return Err(MalformedError::DuplicateSegmentNumber(segment.header.number).into());
        }
        segment_index += 1;
    }

    validate_reference_links(segments, &indexes, globals)?;
    validate_retention(segments, &indexes)?;
    let depths = validate_reference_depth(segments, &indexes, globals, limits)?;
    let mut types: SegmentTypes = Vec::new();
    let type_allocation = allocation_size::<(u32, SegmentType)>(segments.len())?;
    if types.try_reserve(segments.len()).is_err() {
        return Err(DecodeError::Allocation {
            requested_bytes: type_allocation,
        });
    }
    let mut type_index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant type_index <= segments@.len(),
        decreases segments@.len() - type_index,
    ))]
    while type_index < segments.len() {
        let segment = &segments[type_index];
        if !keyed_insert(
            &mut types,
            segment.header.number,
            segment.header.segment_type,
        ) {
            return Err(MalformedError::DuplicateSegmentNumber(segment.header.number).into());
        }
        type_index += 1;
    }
    Ok((types, depths))
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn count_total_references(segments: &[ParsedSegment<'_>]) -> Result<u64, DecodeError> {
    let mut total = 0_u64;
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= segments@.len(),
        decreases segments@.len() - index,
    ))]
    while index < segments.len() {
        let Ok(count) = u64::try_from(segments[index].header.referred_segments.len()) else {
            return Err(MalformedError::IntegerOverflow.into());
        };
        let Some(next) = total.checked_add(count) else {
            return Err(MalformedError::IntegerOverflow.into());
        };
        total = next;
        index += 1;
    }
    Ok(total)
}

#[cfg_attr(verus_keep_ghost, verus_spec(
    requires forall|entry: int| 0 <= entry < indexes@.len() ==>
        indexes@[entry].1 < segments@.len(),
))]
fn validate_reference_links(
    segments: &[ParsedSegment<'_>],
    indexes: &SegmentIndexes,
    globals: Option<&GlobalSegments>,
) -> Result<(), DecodeError> {
    let mut segment_index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            segment_index <= segments@.len(),
            forall|entry: int| 0 <= entry < indexes@.len() ==>
                indexes@[entry].1 < segments@.len(),
        decreases segments@.len() - segment_index,
    ))]
    while segment_index < segments.len() {
        let segment = &segments[segment_index];
        let mut reference_index = 0_usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                reference_index <= segment.header.referred_segments@.len(),
                forall|entry: int| 0 <= entry < indexes@.len() ==>
                    indexes@[entry].1 < segments@.len(),
            decreases segment.header.referred_segments@.len() - reference_index,
        ))]
        while reference_index < segment.header.referred_segments.len() {
            let referred = segment.header.referred_segments[reference_index];
            let Some(entry) = keyed_index(indexes, referred) else {
                let global_exists = match globals {
                    Some(state) => keyed_index(&state.types, referred).is_some(),
                    None => false,
                };
                if !global_exists {
                    return Err(MalformedError::MissingReferredSegment {
                        segment: segment.header.number,
                        referred,
                    }
                    .into());
                }
                reference_index += 1;
                continue;
            };
            let target_index = indexes[entry].1;
            let target = &segments[target_index].header;
            if target.page_association != 0
                && target.page_association != segment.header.page_association
            {
                return Err(MalformedError::InvalidPageAssociation {
                    segment: segment.header.number,
                    page: segment.header.page_association,
                }
                .into());
            }
            reference_index += 1;
        }
        validate_reference_types(&segment.header, segments, indexes, globals)?;
        segment_index += 1;
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn validate_retention(
    segments: &[ParsedSegment<'_>],
    indexes: &SegmentIndexes,
) -> Result<(), DecodeError> {
    let mut last_references: Vec<(u32, u32)> = Vec::new();
    let retention_allocation = allocation_size::<(u32, u32)>(segments.len())?;
    if last_references.try_reserve(segments.len()).is_err() {
        return Err(DecodeError::Allocation {
            requested_bytes: retention_allocation,
        });
    }
    let mut segment_index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant segment_index <= segments@.len(),
        decreases segments@.len() - segment_index,
    ))]
    while segment_index < segments.len() {
        let segment = &segments[segment_index];
        let mut reference_index = 0_usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant reference_index <= segment.header.referred_segments@.len(),
            decreases segment.header.referred_segments@.len() - reference_index,
        ))]
        while reference_index < segment.header.referred_segments.len() {
            let referred = segment.header.referred_segments[reference_index];
            if keyed_index(indexes, referred).is_none() {
                reference_index += 1;
                continue;
            }
            if let Some(last) = keyed_get_mut(&mut last_references, referred) {
                *last = (*last).max(segment.header.number);
            } else {
                let inserted = keyed_insert(&mut last_references, referred, segment.header.number);
                if !inserted {
                    return Err(MalformedError::DuplicateSegmentNumber(referred).into());
                }
            }
            reference_index += 1;
        }
        segment_index += 1;
    }

    validate_retention_flags(segments, &last_references)
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn validate_retention_flags(
    segments: &[ParsedSegment<'_>],
    last_references: &[(u32, u32)],
) -> Result<(), DecodeError> {
    let mut segment_index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant segment_index <= segments@.len(),
        decreases segments@.len() - segment_index,
    ))]
    while segment_index < segments.len() {
        let segment = &segments[segment_index];
        if segment.header.retention_flags.values.is_empty() {
            return Err(MalformedError::InvalidRetentionFlags {
                segment: segment.header.number,
                referred: segment.header.number,
            }
            .into());
        }
        if let Some(last) = keyed_get(last_references, segment.header.number) {
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
        let mut reference_index = 0_usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant reference_index <= segment.header.referred_segments@.len(),
            decreases segment.header.referred_segments@.len() - reference_index,
        ))]
        while reference_index < segment.header.referred_segments.len() {
            let referred = segment.header.referred_segments[reference_index];
            let Some(last) = keyed_get(last_references, referred) else {
                reference_index += 1;
                continue;
            };
            if segment
                .header
                .retention_flags
                .retain_referred_segment(reference_index)
                == Some(false)
                && *last != segment.header.number
            {
                return Err(MalformedError::InvalidRetentionFlags {
                    segment: segment.header.number,
                    referred,
                }
                .into());
            }
            reference_index += 1;
        }
        segment_index += 1;
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(
    requires forall|entry: int| 0 <= entry < indexes@.len() ==>
        indexes@[entry].1 < segments@.len(),
))]
fn validate_reference_depth(
    segments: &[ParsedSegment<'_>],
    indexes: &SegmentIndexes,
    globals: Option<&GlobalSegments>,
    limits: &Limits,
) -> Result<SegmentDepths, DecodeError> {
    let ordered_allocation = allocation_size::<(u32, usize)>(segments.len())?;
    let mut ordered: Vec<(u32, usize)> = Vec::new();
    if ordered.try_reserve(segments.len()).is_err() {
        return Err(DecodeError::Allocation {
            requested_bytes: ordered_allocation,
        });
    }
    let mut segment_index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            segment_index <= segments@.len(),
            forall|entry: int| 0 <= entry < ordered@.len() ==>
                ordered@[entry].1 < segments@.len(),
        decreases segments@.len() - segment_index,
    ))]
    while segment_index < segments.len() {
        let number = segments[segment_index].header.number;
        let mut position = 0_usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                position <= ordered@.len(),
                forall|entry: int| 0 <= entry < ordered@.len() ==>
                    ordered@[entry].1 < segments@.len(),
            decreases ordered@.len() - position,
        ))]
        while position < ordered.len() && ordered[position].0 < number {
            position += 1;
        }
        ordered.insert(position, (number, segment_index));
        segment_index += 1;
    }

    let mut depths = Vec::new();
    let depth_allocation = allocation_size::<(u32, u32)>(segments.len())?;
    if depths.try_reserve(segments.len()).is_err() {
        return Err(DecodeError::Allocation {
            requested_bytes: depth_allocation,
        });
    }
    validate_reference_depth_ordered(&ordered, segments, indexes, globals, limits, &mut depths)?;
    Ok(depths)
}

#[cfg_attr(verus_keep_ghost, verus_spec(
    requires
        forall|entry: int| 0 <= entry < ordered@.len() ==>
            ordered@[entry].1 < segments@.len(),
        forall|entry: int| 0 <= entry < indexes@.len() ==>
            indexes@[entry].1 < segments@.len(),
))]
#[allow(clippy::manual_map)]
fn validate_reference_depth_ordered(
    ordered: &[(u32, usize)],
    segments: &[ParsedSegment<'_>],
    indexes: &SegmentIndexes,
    globals: Option<&GlobalSegments>,
    limits: &Limits,
    depths: &mut SegmentDepths,
) -> Result<(), DecodeError> {
    let mut ordered_index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            ordered_index <= ordered@.len(),
            forall|entry: int| 0 <= entry < ordered@.len() ==>
                ordered@[entry].1 < segments@.len(),
            forall|entry: int| 0 <= entry < indexes@.len() ==>
                indexes@[entry].1 < segments@.len(),
        decreases ordered@.len() - ordered_index,
    ))]
    while ordered_index < ordered.len() {
        let index = ordered[ordered_index].1;
        let segment = &segments[index].header;
        let mut depth = 1_u32;
        let mut reference_index = 0_usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                reference_index <= segment.referred_segments@.len(),
                forall|entry: int| 0 <= entry < indexes@.len() ==>
                    indexes@[entry].1 < segments@.len(),
            decreases segment.referred_segments@.len() - reference_index,
        ))]
        while reference_index < segment.referred_segments.len() {
            let referred = segment.referred_segments[reference_index];
            let referred_depth = if keyed_index(indexes, referred).is_some() {
                match keyed_get(depths, referred) {
                    Some(value) => Some(*value),
                    None => None,
                }
            } else {
                match globals {
                    Some(state) => match keyed_get(&state.depths, referred) {
                        Some(value) => Some(*value),
                        None => None,
                    },
                    None => None,
                }
            };
            let Some(referred_depth) = referred_depth else {
                return Err(MalformedError::MissingReferredSegment {
                    segment: segment.number,
                    referred,
                }
                .into());
            };
            let Some(candidate) = referred_depth.checked_add(1) else {
                return Err(MalformedError::IntegerOverflow.into());
            };
            if candidate > depth {
                depth = candidate;
            }
            reference_index += 1;
        }
        if depth > limits.max_reference_graph_depth {
            return Err(LimitExceeded {
                kind: LimitKind::ReferenceDepth,
                limit: u64::from(limits.max_reference_graph_depth),
                actual: u64::from(depth),
            }
            .into());
        }
        if !keyed_insert(depths, segment.number, depth) {
            return Err(MalformedError::DuplicateSegmentNumber(segment.number).into());
        }
        ordered_index += 1;
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(
    requires forall|index: int| 0 <= index < indexes@.len() ==>
        indexes@[index].1 < segments@.len(),
))]
fn validate_reference_types(
    header: &SegmentHeader,
    segments: &[ParsedSegment<'_>],
    indexes: &SegmentIndexes,
    globals: Option<&GlobalSegments>,
) -> Result<(), DecodeError> {
    let reference_count = header.referred_segments.len();
    let valid = match header.segment_type {
        SegmentType::SymbolDictionary => {
            references_are_symbol_resources(header, segments, indexes, globals, 4)
        }
        SegmentType::IntermediateTextRegion
        | SegmentType::ImmediateTextRegion
        | SegmentType::ImmediateLosslessTextRegion => {
            references_are_symbol_resources(header, segments, indexes, globals, 8)
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
            reference_count == 1
                && matches!(
                    known_reference_type(header, 0, segments, indexes, globals),
                    Some(SegmentType::PatternDictionary)
                )
        }
        SegmentType::IntermediateGenericRefinementRegion => {
            reference_count == 1
                && known_reference_is_intermediate(header, segments, indexes, globals)
        }
        SegmentType::ImmediateGenericRefinementRegion
        | SegmentType::ImmediateLosslessGenericRefinementRegion => {
            reference_count == 0
                || (reference_count == 1
                    && known_reference_is_intermediate(header, segments, indexes, globals))
        }
        SegmentType::Extension => true,
    };
    if !valid {
        return Err(MalformedError::InvalidSegmentReferences(header.number).into());
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(
    requires forall|index: int| 0 <= index < indexes@.len() ==>
        indexes@[index].1 < segments@.len(),
))]
fn known_reference_type(
    header: &SegmentHeader,
    reference_index: usize,
    segments: &[ParsedSegment<'_>],
    indexes: &SegmentIndexes,
    globals: Option<&GlobalSegments>,
) -> Option<SegmentType> {
    if reference_index >= header.referred_segments.len() {
        return None;
    }
    let number = header.referred_segments[reference_index];
    if let Some(entry) = keyed_index(indexes, number) {
        let segment_index = indexes[entry].1;
        return Some(segments[segment_index].header.segment_type);
    }
    let state = globals?;
    let kind = keyed_get(&state.types, number)?;
    Some(*kind)
}

#[cfg_attr(verus_keep_ghost, verus_spec(
    requires forall|index: int| 0 <= index < indexes@.len() ==>
        indexes@[index].1 < segments@.len(),
))]
fn known_reference_is_intermediate(
    header: &SegmentHeader,
    segments: &[ParsedSegment<'_>],
    indexes: &SegmentIndexes,
    globals: Option<&GlobalSegments>,
) -> bool {
    match known_reference_type(header, 0, segments, indexes, globals) {
        Some(kind) => is_intermediate_region(kind),
        None => false,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(
    requires forall|entry: int| 0 <= entry < indexes@.len() ==>
        indexes@[entry].1 < segments@.len(),
))]
fn references_are_symbol_resources(
    header: &SegmentHeader,
    segments: &[ParsedSegment<'_>],
    indexes: &SegmentIndexes,
    globals: Option<&GlobalSegments>,
    maximum_tables: usize,
) -> bool {
    let mut table_count = 0_usize;
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index <= header.referred_segments@.len(),
            table_count <= index,
            forall|entry: int| 0 <= entry < indexes@.len() ==>
                indexes@[entry].1 < segments@.len(),
        decreases header.referred_segments@.len() - index,
    ))]
    while index < header.referred_segments.len() {
        if let Some(kind) = known_reference_type(header, index, segments, indexes, globals) {
            match kind {
                SegmentType::SymbolDictionary => {}
                SegmentType::Tables => {
                    table_count += 1;
                    if table_count > maximum_tables {
                        return false;
                    }
                }
                _ => return false,
            }
        }
        index += 1;
    }
    true
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
            .try_reserve(count)
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
    out == document_spec::quota_result(
        length as nat, limits.max_stream_bytes, LimitKind::StreamBytes,
    ),
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
    out == document_spec::next_count_quota_result(
        count, limits.max_segments, LimitKind::Segments,
    ),
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
    out == document_spec::next_count_quota_result(
        count, limits.max_pages, LimitKind::Pages,
    ),
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
    out == document_spec::quota_result(
        input_length as nat, limits.max_work_units, LimitKind::Work,
    ),
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

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(bytes) =>
            bytes as nat == count as nat * layout::size_of::<T>(),
        Err(_) =>
            count as nat * layout::size_of::<T>() > usize::MAX
            || count as nat * layout::size_of::<T>() > u64::MAX,
    },
))]
fn allocation_size<T>(count: usize) -> Result<u64, MalformedError> {
    let Some(bytes) = count.checked_mul(std::mem::size_of::<T>()) else {
        return Err(MalformedError::IntegerOverflow);
    };
    let Ok(bytes) = u64::try_from(bytes) else {
        return Err(MalformedError::IntegerOverflow);
    };
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quota_view(
        result: Result<(), DecodeError>,
    ) -> Result<Option<LimitExceeded>, MalformedError> {
        match result {
            Ok(()) => Ok(None),
            Err(DecodeError::LimitExceeded(limit)) => Ok(Some(limit)),
            Err(DecodeError::Malformed(error)) => Err(error),
            Err(error) => panic!("unexpected preflight error: {error:?}"),
        }
    }

    fn wide_quota(
        actual: u128,
        maximum: u128,
        limit: u64,
        kind: LimitKind,
    ) -> Result<Option<LimitExceeded>, MalformedError> {
        if actual > maximum {
            Err(MalformedError::IntegerOverflow)
        } else if actual > u128::from(limit) {
            Ok(Some(LimitExceeded {
                kind,
                limit,
                actual: u64::try_from(actual).unwrap(),
            }))
        } else {
            Ok(None)
        }
    }

    #[test]
    fn numeric_preflight_quotas_match_wide_results_and_priority() {
        let counts = [
            0_usize,
            1,
            2,
            7,
            8,
            9,
            15,
            16,
            17,
            31,
            32,
            33,
            usize::MAX / 2,
            usize::MAX - 1,
            usize::MAX,
        ];
        let scalar_limits = [0_u64, 1, 7, 8, u64::MAX / 2, u64::MAX - 1, u64::MAX];
        let count_limits = [0_u32, 1, 7, 8, u32::MAX];
        let mut cases = 0;
        for count in counts {
            for limit in scalar_limits {
                let limits = Limits {
                    max_stream_bytes: limit,
                    max_work_units: limit,
                    ..Limits::default()
                };
                assert_eq!(
                    quota_view(enforce_stream_limit(count, &limits)),
                    wide_quota(
                        count as u128,
                        u128::from(u64::MAX),
                        limit,
                        LimitKind::StreamBytes
                    ),
                    "stream {count}/{limit}",
                );
                assert_eq!(
                    quota_view(enforce_input_work(count, &limits)),
                    wide_quota(count as u128, u128::from(u64::MAX), limit, LimitKind::Work),
                    "work {count}/{limit}",
                );
                cases += 2;
            }
            for limit in count_limits {
                let limits = Limits {
                    max_pages: limit,
                    max_segments: limit,
                    ..Limits::default()
                };
                let actual = count as u128 + 1;
                let maximum = (usize::MAX as u128).min(u128::from(u64::MAX));
                assert_eq!(
                    quota_view(enforce_segment_limit(count, &limits)),
                    wide_quota(actual, maximum, u64::from(limit), LimitKind::Segments),
                    "segments {count}/{limit}",
                );
                assert_eq!(
                    quota_view(enforce_page_limit(count, &limits)),
                    wide_quota(actual, maximum, u64::from(limit), LimitKind::Pages),
                    "pages {count}/{limit}",
                );
                cases += 2;
            }
        }
        assert_eq!(cases, 360);
    }

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

    fn reference_validation_segment(
        number: u32,
        references: &[u32],
        retention: u8,
    ) -> ParsedSegment<'static> {
        assert!(references.len() <= 2);
        let mut bytes = header(number, 0, 0, 0);
        bytes[5] = (u8::try_from(references.len()).unwrap() << 5) | retention;
        for (index, referred) in references.iter().enumerate() {
            bytes.insert(6 + index, u8::try_from(*referred).unwrap());
        }
        let mut reader = crate::reader::ByteReader::new(&bytes);
        let header =
            crate::segment_header::parse_segment_header(&mut reader, &Limits::default()).unwrap();
        assert_eq!(reader.position(), bytes.len());
        ParsedSegment {
            header,
            payload: &[],
            data: ParsedData::Other,
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn reference_graph_retention_and_budget_precedence_match_scalar_policy() {
        let mut graphs = 0;
        let mut cases = 0;
        for first in [&[][..], &[0][..]] {
            for second in [&[][..], &[0][..], &[1][..], &[0, 1][..]] {
                let references: [&[u32]; 3] = [&[], first, second];
                let mut last = [None; 3];
                let mut depths = [1_u32; 3];
                for (owner, referred) in references.iter().enumerate() {
                    for number in *referred {
                        let index = usize::try_from(*number).unwrap();
                        last[index] = Some(owner);
                        depths[owner] = depths[owner].max(depths[index] + 1);
                    }
                }
                let total =
                    u64::try_from(references.iter().map(|refs| refs.len()).sum::<usize>()).unwrap();
                let maximum_depth = *depths.iter().max().unwrap();
                for flags0 in 0..2_u8 {
                    for flags1 in 0..(1_u8 << (first.len() + 1)) {
                        for flags2 in 0..(1_u8 << (second.len() + 1)) {
                            let flags = [flags0, flags1, flags2];
                            let segments = [
                                reference_validation_segment(0, &[], flags0),
                                reference_validation_segment(1, first, flags1),
                                reference_validation_segment(2, second, flags2),
                            ];
                            let headers: Vec<_> =
                                segments.iter().map(|s| s.header.clone()).collect();
                            let mut retention_error = None;
                            for owner in 0..3 {
                                if last[owner].is_some() && flags[owner] & 1 == 0 {
                                    let number = u64::try_from(owner).unwrap();
                                    retention_error = Some(("retention", number, number));
                                    break;
                                }
                                for (index, number) in references[owner].iter().enumerate() {
                                    if last[usize::try_from(*number).unwrap()] != Some(owner)
                                        && flags[owner] & (1 << (index + 1)) == 0
                                    {
                                        retention_error = Some((
                                            "retention",
                                            u64::try_from(owner).unwrap(),
                                            u64::from(*number),
                                        ));
                                        break;
                                    }
                                }
                                if retention_error.is_some() {
                                    break;
                                }
                            }
                            for reference_limit in [total.saturating_sub(1), total, total + 1] {
                                for depth_limit in 0..=maximum_depth + 1 {
                                    let expected = if reference_limit < total {
                                        Some(("references", reference_limit, total))
                                    } else if retention_error.is_some() {
                                        retention_error
                                    } else {
                                        depths.iter().find(|depth| **depth > depth_limit).map(
                                            |depth| {
                                                ("depth", u64::from(depth_limit), u64::from(*depth))
                                            },
                                        )
                                    };
                                    let limits = Limits {
                                        max_total_references: reference_limit,
                                        max_reference_graph_depth: depth_limit,
                                        ..Limits::default()
                                    };
                                    match validate_references(&segments, None, &limits) {
                                        Ok((types, actual_depths)) => {
                                            assert_eq!(expected, None);
                                            assert_eq!(
                                                types,
                                                vec![
                                                    (0, SegmentType::SymbolDictionary),
                                                    (1, SegmentType::SymbolDictionary),
                                                    (2, SegmentType::SymbolDictionary),
                                                ],
                                            );
                                            assert_eq!(
                                                actual_depths,
                                                vec![
                                                    (0, depths[0]),
                                                    (1, depths[1]),
                                                    (2, depths[2])
                                                ],
                                            );
                                        }
                                        Err(DecodeError::LimitExceeded(error)) => {
                                            let kind = match error.kind {
                                                LimitKind::TotalReferences => "references",
                                                LimitKind::ReferenceDepth => "depth",
                                                other => {
                                                    panic!("unexpected graph limit: {other:?}")
                                                }
                                            };
                                            assert_eq!(
                                                Some((kind, error.limit, error.actual)),
                                                expected,
                                            );
                                        }
                                        Err(DecodeError::Malformed(
                                            MalformedError::InvalidRetentionFlags {
                                                segment,
                                                referred,
                                            },
                                        )) => {
                                            assert_eq!(
                                                Some((
                                                    "retention",
                                                    u64::from(segment),
                                                    u64::from(referred)
                                                )),
                                                expected,
                                            );
                                        }
                                        other => panic!("unexpected graph result: {other:?}"),
                                    }
                                    assert_eq!(
                                        segments
                                            .iter()
                                            .map(|s| s.header.clone())
                                            .collect::<Vec<_>>(),
                                        headers,
                                    );
                                    cases += 1;
                                }
                            }
                            graphs += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(graphs, 216);
        assert_eq!(cases, 2856);
        let mut malformed = reference_validation_segment(0, &[], 1);
        malformed.header.retention_flags.values.clear();
        assert!(matches!(
            validate_retention_flags(&[malformed], &[]),
            Err(DecodeError::Malformed(
                MalformedError::InvalidRetentionFlags {
                    segment: 0,
                    referred: 0,
                }
            ))
        ));
    }

    fn page_data(height: u32) -> [u8; 19] {
        let mut data = [0_u8; 19];
        data[0..4].copy_from_slice(&8_u32.to_be_bytes());
        data[4..8].copy_from_slice(&height.to_be_bytes());
        data
    }

    fn direct_page_info(height: Option<u32>, striped: bool, maximum: u16) -> PageInformation {
        let mut info = parse_page_information(&page_data(4), &Limits::default()).unwrap();
        info.width = 13;
        info.height = height;
        info.x_resolution = 123;
        info.y_resolution = 456;
        info.striped = striped;
        info.max_stripe_size = maximum;
        info
    }

    #[test]
    fn page_stripe_boundaries_match_wide_policy_and_preserve_rejected_state() {
        for height in [None, Some(1), Some(4)] {
            for striped in [false, true] {
                for maximum in [0, 1, 3, u16::MAX] {
                    let info = direct_page_info(height, striped, maximum);
                    for previous in [None, Some(0), Some(2), Some(u32::MAX - 1), Some(u32::MAX)] {
                        for row in [0, 1, 2, 3, u32::from(u16::MAX), u32::MAX] {
                            let mut state = PageState::new(info);
                            state.last_stripe = previous;
                            let progress = match previous {
                                Some(before) => {
                                    row > before
                                        && u64::from(row) - u64::from(before) <= u64::from(maximum)
                                }
                                None => row <= u32::from(maximum),
                            };
                            let valid =
                                striped && progress && height.is_none_or(|height| row < height);
                            match state.add_stripe(row) {
                                Ok(()) => {
                                    assert!(valid);
                                    assert_eq!(state.last_stripe, Some(row));
                                }
                                Err(DecodeError::Malformed(
                                    MalformedError::InvalidStripeEndRow(actual),
                                )) => {
                                    assert!(!valid);
                                    assert_eq!(actual, row);
                                    assert_eq!(state.last_stripe, previous);
                                }
                                other => panic!("unexpected stripe result: {other:?}"),
                            }
                            assert_eq!(state.info, info);
                            assert!(!state.ended);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn page_finish_and_public_metadata_keep_exact_error_precedence_and_fields() {
        for height in [None, Some(1), Some(4)] {
            let info = direct_page_info(height, true, 3);
            for last in [None, Some(0), Some(3), Some(u32::MAX)] {
                for ended in [false, true] {
                    let mut state = PageState::new(info);
                    state.last_stripe = last;
                    state.ended = ended;
                    match state.finish() {
                        Err(DecodeError::Malformed(MalformedError::DuplicatePageBoundary {
                            page,
                            segment_type,
                        })) => {
                            assert!(ended);
                            assert_eq!((page, segment_type), (0, 49));
                        }
                        Err(DecodeError::Malformed(MalformedError::InvalidStripeEndRow(row))) => {
                            assert!(!ended && height.is_none() && last.is_none());
                            assert_eq!(row, 0);
                        }
                        Ok(()) => {
                            assert!(!ended && (height.is_some() || last.is_some()));
                            assert!(state.ended);
                        }
                        other => panic!("unexpected page finish result: {other:?}"),
                    }
                    assert_eq!(state.info, info);
                    assert_eq!(state.last_stripe, last);
                    if ended || (height.is_none() && last.is_none()) {
                        assert_eq!(state.ended, ended);
                    }
                }
                let mut state = PageState::new(info);
                state.last_stripe = last;
                match state.into_public(u32::MAX) {
                    Ok(page) => {
                        let expected = height
                            .map(u64::from)
                            .or_else(|| last.map(|row| u64::from(row) + 1))
                            .unwrap();
                        assert!(u32::try_from(expected).is_ok());
                        assert_eq!(
                            (page.number, page.width, u64::from(page.height)),
                            (u32::MAX, 13, expected)
                        );
                        assert_eq!(
                            (page.x_resolution, page.y_resolution, page.striped),
                            (123, 456, true)
                        );
                    }
                    Err(DecodeError::Malformed(MalformedError::IntegerOverflow)) => {
                        assert!(height.is_none() && last == Some(u32::MAX));
                    }
                    Err(DecodeError::Malformed(MalformedError::InvalidStripeEndRow(row))) => {
                        assert!(height.is_none() && last.is_none());
                        assert_eq!(row, 0);
                    }
                    other => panic!("unexpected page metadata result: {other:?}"),
                }
            }
        }
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

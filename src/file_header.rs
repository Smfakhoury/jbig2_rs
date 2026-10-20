use crate::reader::ByteReader;
use crate::{DecodeError, MalformedError};
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg(verus_keep_ghost)]
#[rustfmt::skip]
#[path = "../specs/file_header.rs"]
pub(crate) mod file_header_spec;

/// Segment and payload organization for a standalone JBIG2 file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub enum Organization {
    /// All segment headers precede all segment payloads.
    RandomAccess,
    /// Every segment header is immediately followed by its payload.
    Sequential,
}

/// Parsed standalone JBIG2 file-header metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub struct FileHeader {
    pub(crate) organization: Organization,
    pub(crate) page_count: Option<u32>,
    pub(crate) extended_templates: bool,
    pub(crate) colour_regions: bool,
    pub(crate) byte_length: usize,
}

impl FileHeader {
    /// Standalone segment organization.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == crate::views::file_header_organization(self),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub const fn organization(self) -> Organization {
        #[cfg(verus_keep_ghost)]
        production_file_header_organization!(self);
        self.organization
    }

    /// Declared page count, or `None` if the header marks it unknown.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == crate::views::file_header_page_count(self),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub const fn page_count(self) -> Option<u32> {
        #[cfg(verus_keep_ghost)]
        production_file_header_page_count!(self);
        self.page_count
    }

    /// Whether the stream may use generic templates with 12 adaptive pixels.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == crate::views::file_header_extended_templates(self),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub const fn uses_extended_templates(self) -> bool {
        #[cfg(verus_keep_ghost)]
        production_file_header_extended_templates!(self);
        self.extended_templates
    }

    /// Whether the stream declares coloured regions.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == crate::views::file_header_colour_regions(self),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub const fn uses_colour_regions(self) -> bool {
        #[cfg(verus_keep_ghost)]
        production_file_header_colour_regions!(self);
        self.colour_regions
    }

    /// Number of bytes occupied by the file header.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == crate::views::file_header_byte_length(self),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub const fn byte_length(self) -> usize {
        #[cfg(verus_keep_ghost)]
        production_file_header_byte_length!(self);
        self.byte_length
    }
}

/// Parses a standalone JBIG2 file header without decoding its segments.
///
/// On failure, no successful metadata is fabricated. Embedded streams do not
/// contain this header and must not be passed to this function.
///
/// # Errors
///
/// Returns [`DecodeError::Malformed`] if the signature, reserved flags, or
/// required page-count field is invalid or truncated.
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(header) =>
            file_header_spec::valid(input@)
            && crate::views::file_header_organization(header)
                == file_header_spec::organization(input@)
            && crate::views::file_header_page_count(header)
                == file_header_spec::page_count(input@)
            && crate::views::file_header_extended_templates(header)
                == file_header_spec::extended_templates(input@)
            && crate::views::file_header_colour_regions(header)
                == file_header_spec::colour_regions(input@)
            && crate::views::file_header_byte_length(header)
                == file_header_spec::byte_length(input@),
        Err(_) => !file_header_spec::valid(input@),
    },
))]
pub fn inspect_file_header(input: &[u8]) -> Result<FileHeader, DecodeError> {
    let mut reader = ByteReader::new(input);
    let signature = match reader.read_bytes(8) {
        Ok(value) => value,
        Err(error) => return Err(DecodeError::Malformed(error)),
    };
    if signature[0] != 0x97
        || signature[1] != 0x4A
        || signature[2] != 0x42
        || signature[3] != 0x32
        || signature[4] != 0x0D
        || signature[5] != 0x0A
        || signature[6] != 0x1A
        || signature[7] != 0x0A
    {
        return Err(DecodeError::Malformed(MalformedError::InvalidFileSignature));
    }

    let flags = match reader.read_u8() {
        Ok(value) => value,
        Err(error) => return Err(DecodeError::Malformed(error)),
    };
    let reserved = flags & 0xF0;
    if reserved != 0 {
        return Err(DecodeError::Malformed(
            MalformedError::ReservedFileHeaderFlags(reserved),
        ));
    }

    let colour_regions = flags & 0x08 != 0;
    let extended_templates = flags & 0x04 != 0;
    let unknown_page_count = flags & 0x02 != 0;
    let organization = if flags & 0x01 == 0 {
        Organization::RandomAccess
    } else {
        Organization::Sequential
    };
    let page_count = if unknown_page_count {
        None
    } else {
        let value = match reader.read_u32() {
            Ok(value) => value,
            Err(error) => return Err(DecodeError::Malformed(error)),
        };
        Some(value)
    };

    let header = FileHeader {
        organization,
        page_count,
        extended_templates,
        colour_regions,
        byte_length: reader.position(),
    };
    #[cfg(verus_keep_ghost)]
    production_file_header_result!(header);
    Ok(header)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_page_sequential_header() {
        let input = [
            0x97, 0x4A, 0x42, 0x32, 0x0D, 0x0A, 0x1A, 0x0A, 0x01, 0x00, 0x00, 0x00, 0x03,
        ];
        let header = inspect_file_header(&input).expect("header is valid");
        assert_eq!(header.organization(), Organization::Sequential);
        assert_eq!(header.page_count(), Some(3));
        assert_eq!(header.byte_length(), 13);
        assert!(!header.uses_extended_templates());
        assert!(!header.uses_colour_regions());
    }

    #[test]
    fn parses_unknown_page_random_access_header() {
        let input = [0x97, 0x4A, 0x42, 0x32, 0x0D, 0x0A, 0x1A, 0x0A, 0x0E];
        let header = inspect_file_header(&input).expect("header is valid");
        assert_eq!(header.organization(), Organization::RandomAccess);
        assert_eq!(header.page_count(), None);
        assert_eq!(header.byte_length(), 9);
        assert!(header.uses_extended_templates());
        assert!(header.uses_colour_regions());
    }

    #[test]
    fn rejects_reserved_flags() {
        let input = [0x97, 0x4A, 0x42, 0x32, 0x0D, 0x0A, 0x1A, 0x0A, 0x10];
        assert!(matches!(
            inspect_file_header(&input),
            Err(DecodeError::Malformed(
                MalformedError::ReservedFileHeaderFlags(0x10)
            ))
        ));
    }

    #[test]
    fn truncation_does_not_become_valid_metadata() {
        let input = [0x97, 0x4A, 0x42, 0x32, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00];
        assert!(matches!(
            inspect_file_header(&input),
            Err(DecodeError::Malformed(MalformedError::UnexpectedEof { .. }))
        ));
    }
}

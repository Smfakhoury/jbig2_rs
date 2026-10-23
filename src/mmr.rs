//! Modified Modified READ (MMR) bitmap decoding.
//!
//! This implements the T.6 two-dimensional modes and run tables with the
//! restrictions imposed by T.88 6.2.6: black is one, white is zero, extension
//! modes are forbidden, and an EOFB may be omitted only when the caller has a
//! bounded MMR data block.

// Keep normative prefix bitstrings contiguous so their encoded lengths can be
// audited directly against T.6 tables.
#![allow(clippy::unreadable_literal)]

use crate::bitmap::Bitmap;
use crate::reader::{bit_reader, BitReader};
use crate::{DecodeError, LimitExceeded, LimitKind, Limits, MalformedError, MmrErrorKind};
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg(verus_keep_ghost)]
#[rustfmt::skip]
#[path = "../specs/mmr.rs"]
mod mmr_spec;

#[cfg(verus_keep_ghost)]
#[rustfmt::skip]
#[path = "../proofs/shared/mmr.rs"]
mod mmr_shared;

#[cfg(verus_keep_ghost)]
#[rustfmt::skip]
#[path = "../proofs/spec_properties/mmr.rs"]
mod mmr_properties;

#[cfg(verus_keep_ghost)]
#[macro_use]
#[rustfmt::skip]
#[path = "../proofs/production_impl/mmr.rs"]
mod mmr_proofs;

#[cfg_attr(verus_keep_ghost, verus_verify)]
const EOFB: u32 = 0x00_1001;
#[cfg_attr(verus_keep_ghost, verus_verify)]
const MAX_MODE_CODE_BITS: u8 = 7;
#[cfg_attr(verus_keep_ghost, verus_verify)]
const MAX_RUN_CODE_BITS: u8 = 13;

/// Whether the enclosing JBIG2 syntax requires an end-of-facsimile-block code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) enum EofbPolicy {
    /// T.88 requires EOFB because the encoded byte count is not otherwise known.
    Required,
    /// The input slice is the complete bounded MMR block, so EOFB is optional.
    Optional,
}

#[derive(Debug, Default)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct MmrScratch {
    pub(crate) transitions: Vec<u32>,
    pub(crate) row_transitions: Vec<u32>,
    pub(crate) retained_bytes: u64,
    #[cfg(test)]
    pub(crate) reserve_events: u64,
}

impl MmrScratch {
    #[cfg_attr(verus_keep_ghost, verus_spec(
        ensures
            final(self).transitions@.len() == 0,
            final(self).row_transitions@.len() == 0,
    ))]
    pub(crate) fn clear(&mut self) {
        self.transitions.clear();
        self.row_transitions.clear();
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(
        ensures
            final(self).transitions@.len() == 0,
            final(self).row_transitions@.len() == 0,
            final(self).retained_bytes == 0,
    ))]
    pub(crate) fn release(&mut self) {
        self.transitions = Vec::new();
        self.row_transitions = Vec::new();
        self.retained_bytes = 0;
        #[cfg(test)]
        {
            self.reserve_events = 0;
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == self.retained_bytes,
    ))]
    pub(crate) fn retained_bytes(&self) -> u64 {
        self.retained_bytes
    }
}

/// A fail-closed MMR decoding failure.
///
/// Wiring this module into the public decoder should add equivalent cases to
/// `MalformedError` rather than collapsing these distinctions.
#[derive(Debug)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) enum MmrError {
    /// The shared bit reader rejected or exhausted the input.
    Reader(MalformedError),
    /// Bitmap allocation or a configured resource limit failed.
    Bitmap(DecodeError),
    /// Temporary transition storage could not be allocated.
    Allocation { requested_bytes: u64 },
    /// No T.6 two-dimensional mode has the encoded prefix.
    InvalidMode { bit_offset: usize },
    /// T.88 forbids all T.6 extension modes, including uncompressed mode.
    ExtensionMode { bit_offset: usize },
    /// No run code for the selected colour has the encoded prefix.
    InvalidRunCode { bit_offset: usize, black: bool },
    /// A decoded run exceeds the pixels remaining in the row.
    RunExceedsRow,
    /// A pass or vertical transition is outside the current row or moves back.
    InvalidTransition,
    /// A mode made no semantic progress and could otherwise loop forever.
    NonProgress,
    /// The number of modes exceeded a dimension-derived valid-row bound.
    TooManyModes,
    /// Required EOFB was absent or truncated.
    MissingEofb,
    /// The EOFB value was not the T.6 24-bit marker.
    InvalidEofb,
    /// More than byte-alignment padding remained after the bitmap or EOFB.
    TrailingData,
    /// T.6 requires zero-valued padding after a present EOFB.
    NonZeroEofbPadding,
    /// Input-derived arithmetic could not be represented.
    IntegerOverflow,
}

impl From<MalformedError> for MmrError {
    fn from(error: MalformedError) -> Self {
        Self::Reader(error)
    }
}

impl From<MmrError> for DecodeError {
    fn from(error: MmrError) -> Self {
        let kind = match error {
            MmrError::Reader(error) => return Self::Malformed(error),
            MmrError::Bitmap(error) => return error,
            MmrError::Allocation { requested_bytes } => {
                return Self::Allocation { requested_bytes };
            }
            MmrError::IntegerOverflow => return Self::Malformed(MalformedError::IntegerOverflow),
            MmrError::InvalidMode { bit_offset } => MmrErrorKind::InvalidMode { bit_offset },
            MmrError::ExtensionMode { bit_offset } => MmrErrorKind::ExtensionMode { bit_offset },
            MmrError::InvalidRunCode { bit_offset, black } => {
                MmrErrorKind::InvalidRunCode { bit_offset, black }
            }
            MmrError::RunExceedsRow => MmrErrorKind::RunExceedsRow,
            MmrError::InvalidTransition => MmrErrorKind::InvalidTransition,
            MmrError::NonProgress => MmrErrorKind::NonProgress,
            MmrError::TooManyModes => MmrErrorKind::TooManyModes,
            MmrError::MissingEofb => MmrErrorKind::MissingEofb,
            MmrError::InvalidEofb => MmrErrorKind::InvalidEofb,
            MmrError::TrailingData => MmrErrorKind::TrailingData,
            MmrError::NonZeroEofbPadding => MmrErrorKind::NonZeroPadding,
        };
        Self::Malformed(MalformedError::InvalidMmr(kind))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
enum Mode {
    Pass,
    Horizontal,
    Vertical(i8),
    Extension,
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
struct ModeCode {
    bits: u16,
    length: u8,
    mode: Mode,
}

impl ModeCode {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out.bits == bits, out.length == length, out.mode == mode,
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    const fn new(bits: u16, length: u8, mode: Mode) -> Self {
        Self { bits, length, mode }
    }
}

macro_rules! mode_code {
    ($bits:expr, $length:expr, $mode:expr) => {
        ModeCode {
            bits: $bits,
            length: $length,
            mode: $mode,
        }
    };
}

// ITU-T T.6, Table 1. Extension represents the fixed 0000001 prefix;
// the table's following xxx bits do not alter T.88's requirement to reject it.
#[cfg_attr(verus_keep_ghost, verus_verify)]
const MODE_CODES: [ModeCode; 10] = [
    mode_code!(0b1, 1, Mode::Vertical(0)),
    mode_code!(0b011, 3, Mode::Vertical(1)),
    mode_code!(0b010, 3, Mode::Vertical(-1)),
    mode_code!(0b001, 3, Mode::Horizontal),
    mode_code!(0b0001, 4, Mode::Pass),
    mode_code!(0b000011, 6, Mode::Vertical(2)),
    mode_code!(0b000010, 6, Mode::Vertical(-2)),
    mode_code!(0b0000011, 7, Mode::Vertical(3)),
    mode_code!(0b0000010, 7, Mode::Vertical(-3)),
    mode_code!(0b0000001, 7, Mode::Extension),
];

#[derive(Debug, Clone, Copy)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
struct RunCode {
    bits: u16,
    length: u8,
    value: u16,
    terminal: bool,
}

impl RunCode {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures
            out.bits == bits,
            out.length == length,
            out.value == value,
            out.terminal,
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    const fn terminal(value: u16, bits: u16, length: u8) -> Self {
        Self {
            bits,
            length,
            value,
            terminal: true,
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures
            out.bits == bits,
            out.length == length,
            out.value == value,
            !out.terminal,
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    const fn makeup(value: u16, bits: u16, length: u8) -> Self {
        Self {
            bits,
            length,
            value,
            terminal: false,
        }
    }
}

macro_rules! terminal_code {
    ($value:expr, $bits:expr, $length:expr) => {
        RunCode {
            bits: $bits,
            length: $length,
            value: $value,
            terminal: true,
        }
    };
}

macro_rules! makeup_code {
    ($value:expr, $bits:expr, $length:expr) => {
        RunCode {
            bits: $bits,
            length: $length,
            value: $value,
            terminal: false,
        }
    };
}

// ITU-T T.6, Table 2: white terminating codes for run lengths 0 through 63.
#[cfg_attr(verus_keep_ghost, verus_verify)]
const WHITE_TERMINATING: [RunCode; 64] = [
    terminal_code!(0, 0b00110101, 8),
    terminal_code!(1, 0b000111, 6),
    terminal_code!(2, 0b0111, 4),
    terminal_code!(3, 0b1000, 4),
    terminal_code!(4, 0b1011, 4),
    terminal_code!(5, 0b1100, 4),
    terminal_code!(6, 0b1110, 4),
    terminal_code!(7, 0b1111, 4),
    terminal_code!(8, 0b10011, 5),
    terminal_code!(9, 0b10100, 5),
    terminal_code!(10, 0b00111, 5),
    terminal_code!(11, 0b01000, 5),
    terminal_code!(12, 0b001000, 6),
    terminal_code!(13, 0b000011, 6),
    terminal_code!(14, 0b110100, 6),
    terminal_code!(15, 0b110101, 6),
    terminal_code!(16, 0b101010, 6),
    terminal_code!(17, 0b101011, 6),
    terminal_code!(18, 0b0100111, 7),
    terminal_code!(19, 0b0001100, 7),
    terminal_code!(20, 0b0001000, 7),
    terminal_code!(21, 0b0010111, 7),
    terminal_code!(22, 0b0000011, 7),
    terminal_code!(23, 0b0000100, 7),
    terminal_code!(24, 0b0101000, 7),
    terminal_code!(25, 0b0101011, 7),
    terminal_code!(26, 0b0010011, 7),
    terminal_code!(27, 0b0100100, 7),
    terminal_code!(28, 0b0011000, 7),
    terminal_code!(29, 0b00000010, 8),
    terminal_code!(30, 0b00000011, 8),
    terminal_code!(31, 0b00011010, 8),
    terminal_code!(32, 0b00011011, 8),
    terminal_code!(33, 0b00010010, 8),
    terminal_code!(34, 0b00010011, 8),
    terminal_code!(35, 0b00010100, 8),
    terminal_code!(36, 0b00010101, 8),
    terminal_code!(37, 0b00010110, 8),
    terminal_code!(38, 0b00010111, 8),
    terminal_code!(39, 0b00101000, 8),
    terminal_code!(40, 0b00101001, 8),
    terminal_code!(41, 0b00101010, 8),
    terminal_code!(42, 0b00101011, 8),
    terminal_code!(43, 0b00101100, 8),
    terminal_code!(44, 0b00101101, 8),
    terminal_code!(45, 0b00000100, 8),
    terminal_code!(46, 0b00000101, 8),
    terminal_code!(47, 0b00001010, 8),
    terminal_code!(48, 0b00001011, 8),
    terminal_code!(49, 0b01010010, 8),
    terminal_code!(50, 0b01010011, 8),
    terminal_code!(51, 0b01010100, 8),
    terminal_code!(52, 0b01010101, 8),
    terminal_code!(53, 0b00100100, 8),
    terminal_code!(54, 0b00100101, 8),
    terminal_code!(55, 0b01011000, 8),
    terminal_code!(56, 0b01011001, 8),
    terminal_code!(57, 0b01011010, 8),
    terminal_code!(58, 0b01011011, 8),
    terminal_code!(59, 0b01001010, 8),
    terminal_code!(60, 0b01001011, 8),
    terminal_code!(61, 0b00110010, 8),
    terminal_code!(62, 0b00110011, 8),
    terminal_code!(63, 0b00110100, 8),
];

// ITU-T T.6, Table 2: black terminating codes for run lengths 0 through 63.
#[cfg_attr(verus_keep_ghost, verus_verify)]
const BLACK_TERMINATING: [RunCode; 64] = [
    terminal_code!(0, 0b0000110111, 10),
    terminal_code!(1, 0b010, 3),
    terminal_code!(2, 0b11, 2),
    terminal_code!(3, 0b10, 2),
    terminal_code!(4, 0b011, 3),
    terminal_code!(5, 0b0011, 4),
    terminal_code!(6, 0b0010, 4),
    terminal_code!(7, 0b00011, 5),
    terminal_code!(8, 0b000101, 6),
    terminal_code!(9, 0b000100, 6),
    terminal_code!(10, 0b0000100, 7),
    terminal_code!(11, 0b0000101, 7),
    terminal_code!(12, 0b0000111, 7),
    terminal_code!(13, 0b00000100, 8),
    terminal_code!(14, 0b00000111, 8),
    terminal_code!(15, 0b000011000, 9),
    terminal_code!(16, 0b0000010111, 10),
    terminal_code!(17, 0b0000011000, 10),
    terminal_code!(18, 0b0000001000, 10),
    terminal_code!(19, 0b00001100111, 11),
    terminal_code!(20, 0b00001101000, 11),
    terminal_code!(21, 0b00001101100, 11),
    terminal_code!(22, 0b00000110111, 11),
    terminal_code!(23, 0b00000101000, 11),
    terminal_code!(24, 0b00000010111, 11),
    terminal_code!(25, 0b00000011000, 11),
    terminal_code!(26, 0b000011001010, 12),
    terminal_code!(27, 0b000011001011, 12),
    terminal_code!(28, 0b000011001100, 12),
    terminal_code!(29, 0b000011001101, 12),
    terminal_code!(30, 0b000001101000, 12),
    terminal_code!(31, 0b000001101001, 12),
    terminal_code!(32, 0b000001101010, 12),
    terminal_code!(33, 0b000001101011, 12),
    terminal_code!(34, 0b000011010010, 12),
    terminal_code!(35, 0b000011010011, 12),
    terminal_code!(36, 0b000011010100, 12),
    terminal_code!(37, 0b000011010101, 12),
    terminal_code!(38, 0b000011010110, 12),
    terminal_code!(39, 0b000011010111, 12),
    terminal_code!(40, 0b000001101100, 12),
    terminal_code!(41, 0b000001101101, 12),
    terminal_code!(42, 0b000011011010, 12),
    terminal_code!(43, 0b000011011011, 12),
    terminal_code!(44, 0b000001010100, 12),
    terminal_code!(45, 0b000001010101, 12),
    terminal_code!(46, 0b000001010110, 12),
    terminal_code!(47, 0b000001010111, 12),
    terminal_code!(48, 0b000001100100, 12),
    terminal_code!(49, 0b000001100101, 12),
    terminal_code!(50, 0b000001010010, 12),
    terminal_code!(51, 0b000001010011, 12),
    terminal_code!(52, 0b000000100100, 12),
    terminal_code!(53, 0b000000110111, 12),
    terminal_code!(54, 0b000000111000, 12),
    terminal_code!(55, 0b000000100111, 12),
    terminal_code!(56, 0b000000101000, 12),
    terminal_code!(57, 0b000001011000, 12),
    terminal_code!(58, 0b000001011001, 12),
    terminal_code!(59, 0b000000101011, 12),
    terminal_code!(60, 0b000000101100, 12),
    terminal_code!(61, 0b000001011010, 12),
    terminal_code!(62, 0b000001100110, 12),
    terminal_code!(63, 0b000001100111, 12),
];

// ITU-T T.6, Table 3: colour-specific make-up codes, 64 through 1728.
#[cfg_attr(verus_keep_ghost, verus_verify)]
const WHITE_MAKEUP: [RunCode; 27] = [
    makeup_code!(64, 0b11011, 5),
    makeup_code!(128, 0b10010, 5),
    makeup_code!(192, 0b010111, 6),
    makeup_code!(256, 0b0110111, 7),
    makeup_code!(320, 0b00110110, 8),
    makeup_code!(384, 0b00110111, 8),
    makeup_code!(448, 0b01100100, 8),
    makeup_code!(512, 0b01100101, 8),
    makeup_code!(576, 0b01101000, 8),
    makeup_code!(640, 0b01100111, 8),
    makeup_code!(704, 0b011001100, 9),
    makeup_code!(768, 0b011001101, 9),
    makeup_code!(832, 0b011010010, 9),
    makeup_code!(896, 0b011010011, 9),
    makeup_code!(960, 0b011010100, 9),
    makeup_code!(1024, 0b011010101, 9),
    makeup_code!(1088, 0b011010110, 9),
    makeup_code!(1152, 0b011010111, 9),
    makeup_code!(1216, 0b011011000, 9),
    makeup_code!(1280, 0b011011001, 9),
    makeup_code!(1344, 0b011011010, 9),
    makeup_code!(1408, 0b011011011, 9),
    makeup_code!(1472, 0b010011000, 9),
    makeup_code!(1536, 0b010011001, 9),
    makeup_code!(1600, 0b010011010, 9),
    makeup_code!(1664, 0b011000, 6),
    makeup_code!(1728, 0b010011011, 9),
];

#[cfg_attr(verus_keep_ghost, verus_verify)]
const BLACK_MAKEUP: [RunCode; 27] = [
    makeup_code!(64, 0b0000001111, 10),
    makeup_code!(128, 0b000011001000, 12),
    makeup_code!(192, 0b000011001001, 12),
    makeup_code!(256, 0b000001011011, 12),
    makeup_code!(320, 0b000000110011, 12),
    makeup_code!(384, 0b000000110100, 12),
    makeup_code!(448, 0b000000110101, 12),
    makeup_code!(512, 0b0000001101100, 13),
    makeup_code!(576, 0b0000001101101, 13),
    makeup_code!(640, 0b0000001001010, 13),
    makeup_code!(704, 0b0000001001011, 13),
    makeup_code!(768, 0b0000001001100, 13),
    makeup_code!(832, 0b0000001001101, 13),
    makeup_code!(896, 0b0000001110010, 13),
    makeup_code!(960, 0b0000001110011, 13),
    makeup_code!(1024, 0b0000001110100, 13),
    makeup_code!(1088, 0b0000001110101, 13),
    makeup_code!(1152, 0b0000001110110, 13),
    makeup_code!(1216, 0b0000001110111, 13),
    makeup_code!(1280, 0b0000001010010, 13),
    makeup_code!(1344, 0b0000001010011, 13),
    makeup_code!(1408, 0b0000001010100, 13),
    makeup_code!(1472, 0b0000001010101, 13),
    makeup_code!(1536, 0b0000001011010, 13),
    makeup_code!(1600, 0b0000001011011, 13),
    makeup_code!(1664, 0b0000001100100, 13),
    makeup_code!(1728, 0b0000001100101, 13),
];

// ITU-T T.6, Table 3: make-up codes shared by white and black.
#[cfg_attr(verus_keep_ghost, verus_verify)]
const COMMON_MAKEUP: [RunCode; 13] = [
    makeup_code!(1792, 0b00000001000, 11),
    makeup_code!(1856, 0b00000001100, 11),
    makeup_code!(1920, 0b00000001101, 11),
    makeup_code!(1984, 0b000000010010, 12),
    makeup_code!(2048, 0b000000010011, 12),
    makeup_code!(2112, 0b000000010100, 12),
    makeup_code!(2176, 0b000000010101, 12),
    makeup_code!(2240, 0b000000010110, 12),
    makeup_code!(2304, 0b000000010111, 12),
    makeup_code!(2368, 0b000000011100, 12),
    makeup_code!(2432, 0b000000011101, 12),
    makeup_code!(2496, 0b000000011110, 12),
    makeup_code!(2560, 0b000000011111, 12),
];

/// Decode one complete, byte-bounded JBIG2 MMR bitmap.
///
/// `data` must contain exactly the MMR data block, including any optional or
/// required EOFB and final byte-alignment bits.
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(bitmap) => crate::spec::bitmap::storage_valid(&bitmap),
        Err(_) => true,
    },
))]
pub(crate) fn decode_mmr(
    data: &[u8],
    width: u32,
    height: u32,
    limits: &Limits,
    eofb_policy: EofbPolicy,
) -> Result<Bitmap, MmrError> {
    let mut scratch = MmrScratch::default();
    decode_mmr_reused(data, width, height, limits, eofb_policy, &mut scratch)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(bitmap) => crate::spec::bitmap::storage_valid(&bitmap),
        Err(_) => true,
    },
))]
pub(crate) fn decode_mmr_reused(
    data: &[u8],
    width: u32,
    height: u32,
    limits: &Limits,
    eofb_policy: EofbPolicy,
    scratch: &mut MmrScratch,
) -> Result<Bitmap, MmrError> {
    let result = decode_bitmap(data, width, height, limits, scratch);
    let (bitmap, mut reader) = match result {
        Ok(value) => value,
        Err(error) => {
            scratch.clear();
            return Err(error);
        }
    };
    let finish = finish_block(&mut reader, eofb_policy);
    scratch.clear();
    finish?;
    Ok(bitmap)
}

/// Decode one EOFB-terminated MMR bitmap at the start of `data`.
///
/// Unlike [`decode_mmr`], this accepts bytes after the decoded bitmap. It
/// requires an exact EOFB immediately after the declared rows, requires every
/// bit between EOFB and the next byte boundary to be zero, and returns that
/// byte boundary as `consumed_bytes`. A caller can decode a following bitplane
/// from `&data[consumed_bytes..]`.
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok((bitmap, consumed_bytes)) =>
            crate::spec::bitmap::storage_valid(&bitmap)
            && consumed_bytes <= data@.len(),
        Err(_) => true,
    },
))]
pub(crate) fn decode_mmr_prefix(
    data: &[u8],
    width: u32,
    height: u32,
    limits: &Limits,
) -> Result<(Bitmap, usize), MmrError> {
    let mut scratch = MmrScratch::default();
    let (bitmap, mut reader) = decode_bitmap(data, width, height, limits, &mut scratch)?;
    let finish = finish_required_prefix(&mut reader);
    scratch.clear();
    finish?;
    let consumed_bytes = reader
        .bit_position
        .checked_div(8)
        .ok_or(MmrError::IntegerOverflow)?;
    Ok((bitmap, consumed_bytes))
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok((bitmap, reader)) =>
            crate::spec::bitmap::storage_valid(&bitmap)
            && mmr_spec::reader_valid(&reader)
            && reader.bytes@ == data@,
        Err(_) => true,
    },
))]
#[allow(clippy::too_many_lines)]
fn decode_bitmap<'a>(
    data: &'a [u8],
    width: u32,
    height: u32,
    limits: &Limits,
    scratch: &mut MmrScratch,
) -> Result<(Bitmap, BitReader<'a>), MmrError> {
    enforce_input_limit(data, limits)?;
    if data.len() > usize::MAX / 8 {
        return Err(MmrError::IntegerOverflow);
    }
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or(MmrError::IntegerOverflow)?;
    let mut work = pixels.checked_mul(2).ok_or(MmrError::IntegerOverflow)?;
    enforce_work(work, limits)?;
    let mut bitmap = match Bitmap::blank(width, height, limits) {
        Ok(bitmap) => bitmap,
        Err(error) => return Err(MmrError::Bitmap(error)),
    };
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::views::bitmap_fields(&bitmap);
        reveal(crate::spec::bitmap::storage_valid);
        reveal(mmr_spec::bitmap_storage_valid);
        crate::bitmap::bitmap_shared::stride_covers_width(width);
    }
    prepare_transition_storage(width, &bitmap, limits, scratch)?;
    let mut transitions = &mut scratch.transitions;
    let mut row_transitions = &mut scratch.row_transitions;
    transitions.push(width);
    transitions.push(width);
    #[cfg(verus_keep_ghost)]
    proof! {
        reveal(mmr_spec::reference_line_valid);
        reveal(mmr_spec::transitions_ordered);
    }

    let mut reader = bit_reader(data);
    #[cfg(verus_keep_ghost)]
    proof! {
        reveal(mmr_spec::reader_valid);
    }
    let max_modes = u64::from(width)
        .checked_mul(2)
        .and_then(|value| value.checked_add(2))
        .ok_or(MmrError::IntegerOverflow)?;

    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            row <= height,
            bitmap.width == width,
            bitmap.height == height,
            crate::spec::bitmap::storage_valid(&bitmap),
            mmr_spec::bitmap_storage_valid(&bitmap),
            mmr_spec::reference_line_valid(transitions@, width),
            transitions@.len() <= width as nat + 2,
            mmr_spec::reader_valid(&reader),
            reader.bytes@ == data@,
        decreases height - row,
    ))]
    for row in 0..height {
        let mut x = 0_u32;
        let mut black = false;
        let mut emitted_black = false;
        let mut initial = true;
        let mut mode_count = 0_u64;
        let mut reference_index = 0_usize;
        row_transitions.clear();
        #[cfg(verus_keep_ghost)]
        production_mmr_transition_empty!(width);
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::views::bitmap_fields(&bitmap);
            reveal(crate::spec::bitmap::storage_valid);
        }

        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                row < height,
                x <= width,
                mode_count <= max_modes,
                bitmap.width == width,
                bitmap.height == height,
                bitmap.stride as nat
                    == crate::spec::bitmap::stride_for_width(bitmap.width as nat),
                mmr_spec::bitmap_storage_valid(&bitmap),
                mmr_spec::reference_line_valid(transitions@, width),
                transitions@.len() <= width as nat + 2,
                reference_index < transitions@.len(),
                mmr_spec::transition_prefix_valid(row_transitions@, x, width),
                row_transitions@.len() <= x as nat,
                mmr_spec::reader_valid(&reader),
                reader.bytes@ == data@,
            decreases max_modes + 1 - mode_count,
        ))]
        while x < width {
            mode_count = mode_count.checked_add(1).ok_or(MmrError::IntegerOverflow)?;
            work = work.checked_add(1).ok_or(MmrError::IntegerOverflow)?;
            enforce_work(work, limits)?;
            if mode_count > max_modes {
                return Err(MmrError::TooManyModes);
            }
            #[cfg(verus_keep_ghost)]
            production_mmr_mode_budget!(mode_count, max_modes);

            let (b1, b2) = reference_elements_from(
                transitions,
                x,
                black,
                width,
                initial,
                &mut reference_index,
            )?;
            let mode_offset = reader.bit_position;
            match decode_mode(&mut reader)? {
                Mode::Extension => {
                    return Err(MmrError::ExtensionMode {
                        bit_offset: mode_offset,
                    });
                }
                Mode::Pass => {
                    if b2 <= x || b2 > width {
                        return Err(MmrError::NonProgress);
                    }
                    #[cfg(verus_keep_ghost)]
                    production_mmr_pass_progress!(x, b2, width);
                    fill_recorded_span(
                        &mut bitmap,
                        row,
                        x,
                        b2,
                        black,
                        width,
                        row_transitions,
                        &mut emitted_black,
                    )?;
                    x = b2;
                }
                Mode::Horizontal => {
                    let (after_first, end) = decode_horizontal_runs(&mut reader, black, x, width)?;
                    fill_recorded_span(
                        &mut bitmap,
                        row,
                        x,
                        after_first,
                        black,
                        width,
                        row_transitions,
                        &mut emitted_black,
                    )?;
                    fill_recorded_span(
                        &mut bitmap,
                        row,
                        after_first,
                        end,
                        !black,
                        width,
                        row_transitions,
                        &mut emitted_black,
                    )?;
                    x = end;
                }
                Mode::Vertical(displacement) => {
                    let target = match displacement {
                        -3 => b1.checked_sub(3),
                        -2 => b1.checked_sub(2),
                        -1 => b1.checked_sub(1),
                        0 => Some(b1),
                        1 => b1.checked_add(1),
                        2 => b1.checked_add(2),
                        3 => b1.checked_add(3),
                        _ => None,
                    }
                    .ok_or(MmrError::InvalidTransition)?;
                    if target < x || target > width {
                        return Err(MmrError::InvalidTransition);
                    }
                    #[cfg(verus_keep_ghost)]
                    production_mmr_vertical_progress!(x, target, width, black);
                    fill_recorded_span(
                        &mut bitmap,
                        row,
                        x,
                        target,
                        black,
                        width,
                        row_transitions,
                        &mut emitted_black,
                    )?;
                    x = target;
                    black = !black;
                }
            }
            #[cfg(verus_keep_ghost)]
            proof! {
                crate::views::bitmap_fields(&bitmap);
                reveal(crate::spec::bitmap::storage_valid);
            }
            initial = false;
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::views::bitmap_fields(&bitmap);
            reveal(crate::spec::bitmap::storage_valid);
            reveal(mmr_spec::bitmap_storage_valid);
        }

        finish_transition_row(row_transitions, width);
        core::mem::swap(&mut transitions, &mut row_transitions);
        #[cfg(verus_keep_ghost)]
        production_mmr_row_termination!(row, height);
    }

    Ok((bitmap, reader))
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) => data@.len() <= limits.max_stream_bytes,
        Err(_) => true,
    },
))]
fn enforce_input_limit(data: &[u8], limits: &Limits) -> Result<(), MmrError> {
    let actual = u64::try_from(data.len()).map_err(|_error| MmrError::IntegerOverflow)?;
    if actual > limits.max_stream_bytes {
        return Err(MmrError::Bitmap(DecodeError::LimitExceeded(
            LimitExceeded {
                kind: LimitKind::StreamBytes,
                limit: limits.max_stream_bytes,
                actual,
            },
        )));
    }

    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) => actual <= limits.max_work_units,
        Err(_) => true,
    },
))]
fn enforce_work(actual: u64, limits: &Limits) -> Result<(), MmrError> {
    if actual > limits.max_work_units {
        return Err(MmrError::Bitmap(DecodeError::LimitExceeded(
            LimitExceeded {
                kind: LimitKind::Work,
                limit: limits.max_work_units,
                actual,
            },
        )));
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) =>
            final(scratch).transitions@.len() == 0
            && final(scratch).row_transitions@.len() == 0,
        Err(_) => true,
    },
))]
fn prepare_transition_storage(
    width: u32,
    bitmap: &Bitmap,
    limits: &Limits,
    scratch: &mut MmrScratch,
) -> Result<(), MmrError> {
    let capacity_u64 = u64::from(width)
        .checked_add(2)
        .ok_or(MmrError::IntegerOverflow)?;
    let transition_bytes = capacity_u64
        .checked_mul(4)
        .ok_or(MmrError::IntegerOverflow)?;
    let all_transition_bytes = transition_bytes
        .checked_mul(2)
        .ok_or(MmrError::IntegerOverflow)?;
    let Ok(bitmap_bytes) = u64::try_from(bitmap.bytes().len()) else {
        return Err(MmrError::IntegerOverflow);
    };
    let total = bitmap_bytes
        .checked_add(all_transition_bytes)
        .ok_or(MmrError::IntegerOverflow)?;
    if total > limits.max_decoder_managed_memory_bytes {
        return Err(MmrError::Bitmap(DecodeError::LimitExceeded(
            LimitExceeded {
                kind: LimitKind::Memory,
                limit: limits.max_decoder_managed_memory_bytes,
                actual: total,
            },
        )));
    }
    #[cfg(not(verus_keep_ghost))]
    {
        let retained_total = bitmap_bytes
            .checked_add(scratch.retained_bytes())
            .ok_or(MmrError::IntegerOverflow)?;
        if retained_total > limits.max_decoder_managed_memory_bytes {
            scratch.release();
        }
    }
    #[cfg(verus_keep_ghost)]
    production_mmr_memory_bound!(bitmap_bytes, transition_bytes, all_transition_bytes, limits);

    let Ok(capacity) = usize::try_from(capacity_u64) else {
        return Err(MmrError::IntegerOverflow);
    };
    scratch.clear();
    #[cfg(test)]
    {
        if scratch.transitions.capacity() < capacity {
            scratch.reserve_events += 1;
        }
        if scratch.row_transitions.capacity() < capacity {
            scratch.reserve_events += 1;
        }
    }
    if scratch.transitions.try_reserve(capacity).is_err() {
        return Err(MmrError::Allocation {
            requested_bytes: transition_bytes,
        });
    }
    if scratch.row_transitions.try_reserve(capacity).is_err() {
        #[cfg(not(verus_keep_ghost))]
        {
            scratch.retained_bytes = u64::try_from(scratch.transitions.capacity())
                .unwrap_or(u64::MAX)
                .saturating_add(
                    u64::try_from(scratch.row_transitions.capacity()).unwrap_or(u64::MAX),
                )
                .saturating_mul(core::mem::size_of::<u32>() as u64);
        }
        return Err(MmrError::Allocation {
            requested_bytes: transition_bytes,
        });
    }
    #[cfg(not(verus_keep_ghost))]
    {
        scratch.retained_bytes = u64::try_from(scratch.transitions.capacity())
            .unwrap_or(u64::MAX)
            .saturating_add(u64::try_from(scratch.row_transitions.capacity()).unwrap_or(u64::MAX))
            .saturating_mul(core::mem::size_of::<u32>() as u64);
        let actual_total = bitmap_bytes
            .checked_add(scratch.retained_bytes())
            .ok_or(MmrError::IntegerOverflow)?;
        if actual_total > limits.max_decoder_managed_memory_bytes {
            scratch.release();
            return Err(MmrError::Bitmap(DecodeError::LimitExceeded(
                LimitExceeded {
                    kind: LimitKind::Memory,
                    limit: limits.max_decoder_managed_memory_bytes,
                    actual: actual_total,
                },
            )));
        }
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out == if prefix as nat * 2 + bit as nat <= u16::MAX {
        Ok((prefix as nat * 2 + bit as nat) as u16)
    } else {
        Err(MmrError::IntegerOverflow)
    },
))]
fn append_code_bit(prefix: u16, bit: u16) -> Result<u16, MmrError> {
    let doubled = prefix.checked_mul(2).ok_or(MmrError::IntegerOverflow)?;
    doubled.checked_add(bit).ok_or(MmrError::IntegerOverflow)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires mmr_spec::reader_valid(old(reader)),
    ensures
        mmr_spec::reader_valid(final(reader)),
        final(reader).bytes@ == old(reader).bytes@,
        old(reader).bit_position <= final(reader).bit_position,
        final(reader).bit_position as nat
            <= old(reader).bit_position as nat + MAX_MODE_CODE_BITS as nat,
        match out {
            Ok(mode) => exists|code: ModeCode|
                mmr_spec::mode_code_in_implementation_table(code)
                && code.mode == mode
                && final(reader).bit_position
                    == old(reader).bit_position + code.length as usize
                && code.bits as nat == crate::spec::readers::bits_value(
                    old(reader).bytes@, old(reader).bit_position as nat, code.length as nat,
                ),
            Err(_) => true,
        },
))]
fn decode_mode(reader: &mut BitReader<'_>) -> Result<Mode, MmrError> {
    let start = reader.bit_position;
    let mut prefix = 0_u16;
    let mut length = 1_u8;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            mmr_spec::reader_valid(reader),
            reader.bytes@ == old(reader).bytes@,
            1 <= length <= MAX_MODE_CODE_BITS + 1,
            start == old(reader).bit_position,
            reader.bit_position
                == start + (length - 1) as usize,
            prefix as nat == crate::spec::readers::bits_value(
                reader.bytes@, start as nat, (length - 1) as nat,
            ),
        decreases MAX_MODE_CODE_BITS + 1 - length,
    ))]
    while length <= MAX_MODE_CODE_BITS {
        #[cfg(verus_keep_ghost)]
        proof_decl! {
            let ghost previous_prefix = prefix;
        }
        let bit =
            u16::try_from(reader.read_bits(1)?).map_err(|_error| MmrError::IntegerOverflow)?;
        prefix = append_code_bit(prefix, bit)?;
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::reader_properties::bits_value_append_one(
                reader.bytes@, start as nat, (length - 1) as nat,
            );
            assert(prefix as nat == previous_prefix as nat * 2 + bit as nat);
            assert(prefix as nat == crate::spec::readers::bits_value(
                reader.bytes@, start as nat, length as nat,
            ));
        }
        let mut index = 0_usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                index <= MODE_CODES@.len(),
                mmr_spec::reader_valid(reader),
                reader.bytes@ == old(reader).bytes@,
                start == old(reader).bit_position,
                reader.bit_position == start + length as usize,
                prefix as nat == crate::spec::readers::bits_value(
                    reader.bytes@, start as nat, length as nat,
                ),
            decreases MODE_CODES@.len() - index,
        ))]
        while index < MODE_CODES.len() {
            let code = MODE_CODES[index];
            if code.length == length && code.bits == prefix {
                #[cfg(verus_keep_ghost)]
                production_mmr_mode_entry!(code, index, prefix, length);
                return Ok(code.mode);
            }
            index += 1;
        }
        length = length.checked_add(1).ok_or(MmrError::IntegerOverflow)?;
    }
    Err(MmrError::InvalidMode { bit_offset: start })
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires mmr_spec::reader_valid(old(reader)),
    ensures
        mmr_spec::reader_valid(final(reader)),
        final(reader).bytes@ == old(reader).bytes@,
        old(reader).bit_position <= final(reader).bit_position,
        match out {
            Ok((after_first, end)) =>
                x <= after_first <= end <= width
                && x < end
                && mmr_spec::horizontal_pair(
                    black, old(reader).bytes@, old(reader).bit_position as nat,
                    final(reader).bit_position as nat, x, width, after_first, end,
                ),
            Err(_) => true,
        },
))]
fn decode_horizontal_runs(
    reader: &mut BitReader<'_>,
    black: bool,
    x: u32,
    width: u32,
) -> Result<(u32, u32), MmrError> {
    let remaining = width.checked_sub(x).ok_or(MmrError::InvalidTransition)?;
    let first = decode_run(reader, black, remaining)?;
    #[cfg(verus_keep_ghost)]
    proof_decl! {
        let ghost split = reader.bit_position as nat;
        let ghost first_codes = choose|codes: Seq<RunCode>|
            #[trigger] codes.len() > 0
            && mmr_spec::complete_run(
                black, old(reader).bytes@, old(reader).bit_position as nat,
                split, first, codes,
            );
    }
    let after_first = x.checked_add(first).ok_or(MmrError::IntegerOverflow)?;
    let second_remaining = width
        .checked_sub(after_first)
        .ok_or(MmrError::RunExceedsRow)?;
    let second = decode_run(reader, !black, second_remaining)?;
    #[cfg(verus_keep_ghost)]
    proof_decl! {
        let ghost second_codes = choose|codes: Seq<RunCode>|
            #[trigger] codes.len() > 0
            && mmr_spec::complete_run(
                !black, old(reader).bytes@, split, reader.bit_position as nat,
                second, codes,
            );
    }
    let end = after_first
        .checked_add(second)
        .ok_or(MmrError::IntegerOverflow)?;
    if end > width {
        return Err(MmrError::RunExceedsRow);
    }
    if end == x {
        return Err(MmrError::NonProgress);
    }
    #[cfg(verus_keep_ghost)]
    production_mmr_horizontal_progress!(x, width, first, second);
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(after_first - x == first);
        assert(end - after_first == second);
        assert(mmr_spec::complete_run(
            black, old(reader).bytes@, old(reader).bit_position as nat,
            old(reader).bit_position as nat + mmr_spec::run_bit_length(first_codes),
            first, first_codes,
        ));
        assert(reader.bit_position as nat == old(reader).bit_position as nat
            + mmr_spec::run_bit_length(first_codes)
            + mmr_spec::run_bit_length(second_codes));
        assert(mmr_spec::complete_run(
            !black, old(reader).bytes@,
            old(reader).bit_position as nat + mmr_spec::run_bit_length(first_codes),
            reader.bit_position as nat, second, second_codes,
        ));
        assert(mmr_spec::complete_run(
            black, old(reader).bytes@, old(reader).bit_position as nat,
            old(reader).bit_position as nat + mmr_spec::run_bit_length(first_codes),
            (after_first - x) as u32, first_codes,
        ));
        assert(mmr_spec::complete_run(
            !black, old(reader).bytes@,
            old(reader).bit_position as nat + mmr_spec::run_bit_length(first_codes),
            reader.bit_position as nat, (end - after_first) as u32, second_codes,
        ));
        assert(mmr_spec::horizontal_pair(
            black, old(reader).bytes@, old(reader).bit_position as nat,
            reader.bit_position as nat, x, width, after_first, end,
        ));
    }
    Ok((after_first, end))
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires mmr_spec::reader_valid(old(reader)),
    ensures
        mmr_spec::reader_valid(final(reader)),
        final(reader).bytes@ == old(reader).bytes@,
        old(reader).bit_position <= final(reader).bit_position,
        match out {
            Ok(total) => total <= remaining
                && exists|codes: Seq<RunCode>| mmr_spec::complete_run(
                    black, old(reader).bytes@, old(reader).bit_position as nat,
                    final(reader).bit_position as nat, total, codes,
                ),
            Err(_) => true,
        },
))]
fn decode_run(reader: &mut BitReader<'_>, black: bool, remaining: u32) -> Result<u32, MmrError> {
    let mut total = 0_u32;
    let mut makeup_count = 0_u32;
    #[cfg(verus_keep_ghost)]
    proof_decl! {
        let ghost mut codes = Seq::<RunCode>::empty();
    }
    let max_makeups = remaining
        .checked_div(64)
        .and_then(|value| value.checked_add(1))
        .ok_or(MmrError::IntegerOverflow)?;

    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            mmr_spec::reader_valid(reader),
            reader.bytes@ == old(reader).bytes@,
            old(reader).bit_position <= reader.bit_position,
            total <= remaining,
            makeup_count <= max_makeups,
            total as nat == mmr_spec::run_value_sum(codes),
            reader.bit_position as nat
                == old(reader).bit_position as nat + mmr_spec::run_bit_length(codes),
            mmr_spec::run_prefix_matches(
                black, old(reader).bytes@, old(reader).bit_position as nat, codes,
            ),
            mmr_spec::run_all_makeups(codes),
        decreases max_makeups + 1 - makeup_count,
    ))]
    loop {
        #[cfg(verus_keep_ghost)]
        proof_decl! {
            let ghost before_codes = codes;
            let ghost before_position = reader.bit_position;
        }
        let code = decode_run_code(reader, black)?;
        #[cfg(verus_keep_ghost)]
        proof! {
            codes = codes.push(code);
            assert(codes.drop_last() == before_codes);
            assert(codes.last() == code);
            assert(mmr_spec::run_prefix_matches(
                black, old(reader).bytes@, old(reader).bit_position as nat, codes,
            ));
        }
        total = total
            .checked_add(u32::from(code.value))
            .ok_or(MmrError::IntegerOverflow)?;
        if total > remaining {
            return Err(MmrError::RunExceedsRow);
        }
        if code.terminal {
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(mmr_spec::complete_run(
                    black, old(reader).bytes@, old(reader).bit_position as nat,
                    reader.bit_position as nat, total, codes,
                ));
            }
            return Ok(total);
        }
        #[cfg(verus_keep_ghost)]
        production_mmr_makeup_bounds!(black, code);
        makeup_count = makeup_count
            .checked_add(1)
            .ok_or(MmrError::IntegerOverflow)?;
        if makeup_count > max_makeups {
            return Err(MmrError::RunExceedsRow);
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires mmr_spec::reader_valid(old(reader)),
    ensures
        mmr_spec::reader_valid(final(reader)),
        final(reader).bytes@ == old(reader).bytes@,
        old(reader).bit_position <= final(reader).bit_position,
        final(reader).bit_position as nat
            <= old(reader).bit_position as nat + MAX_RUN_CODE_BITS as nat,
        match out {
            Ok(code) =>
                mmr_spec::run_code_in_implementation_tables(black, code)
                && final(reader).bit_position
                    == old(reader).bit_position + code.length as usize
                && code.bits as nat == crate::spec::readers::bits_value(
                    old(reader).bytes@, old(reader).bit_position as nat, code.length as nat,
                ),
            Err(_) => true,
        },
))]
fn decode_run_code(reader: &mut BitReader<'_>, black: bool) -> Result<RunCode, MmrError> {
    let start = reader.bit_position;
    let mut prefix = 0_u16;
    let mut length = 1_u8;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            mmr_spec::reader_valid(reader),
            reader.bytes@ == old(reader).bytes@,
            1 <= length <= MAX_RUN_CODE_BITS + 1,
            start == old(reader).bit_position,
            reader.bit_position
                == start + (length - 1) as usize,
            prefix as nat == crate::spec::readers::bits_value(
                reader.bytes@, start as nat, (length - 1) as nat,
            ),
        decreases MAX_RUN_CODE_BITS + 1 - length,
    ))]
    while length <= MAX_RUN_CODE_BITS {
        #[cfg(verus_keep_ghost)]
        proof_decl! {
            let ghost previous_prefix = prefix;
        }
        let bit =
            u16::try_from(reader.read_bits(1)?).map_err(|_error| MmrError::IntegerOverflow)?;
        prefix = append_code_bit(prefix, bit)?;
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::reader_properties::bits_value_append_one(
                reader.bytes@, start as nat, (length - 1) as nat,
            );
            assert(prefix as nat == previous_prefix as nat * 2 + bit as nat);
            assert(prefix as nat == crate::spec::readers::bits_value(
                reader.bytes@, start as nat, length as nat,
            ));
        }
        if let Some(code) = find_run_code(black, prefix, length) {
            #[cfg(verus_keep_ghost)]
            production_mmr_run_code_bounds!(black, code);
            return Ok(code);
        }
        length = length.checked_add(1).ok_or(MmrError::IntegerOverflow)?;
    }
    Err(MmrError::InvalidRunCode {
        bit_offset: start,
        black,
    })
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Some(code) =>
            mmr_spec::run_code_in_implementation_tables(black, code)
            && mmr_spec::run_key_matches(code, bits, length),
        None => true,
    },
))]
fn find_run_code(black: bool, bits: u16, length: u8) -> Option<RunCode> {
    let mut index = 0_usize;
    if black {
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                black,
                index <= BLACK_TERMINATING@.len(),
            decreases BLACK_TERMINATING@.len() - index,
        ))]
        while index < BLACK_TERMINATING.len() {
            let code = BLACK_TERMINATING[index];
            if code.length == length && code.bits == bits {
                #[cfg(verus_keep_ghost)]
                production_mmr_run_entry!(
                    BLACK_TERMINATING,
                    code,
                    index,
                    black,
                    true,
                    bits,
                    length
                );
                return Some(code);
            }
            index += 1;
        }
        index = 0;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                black,
                index <= BLACK_MAKEUP@.len(),
            decreases BLACK_MAKEUP@.len() - index,
        ))]
        while index < BLACK_MAKEUP.len() {
            let code = BLACK_MAKEUP[index];
            if code.length == length && code.bits == bits {
                #[cfg(verus_keep_ghost)]
                production_mmr_run_entry!(BLACK_MAKEUP, code, index, black, true, bits, length);
                return Some(code);
            }
            index += 1;
        }
    } else {
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                !black,
                index <= WHITE_TERMINATING@.len(),
            decreases WHITE_TERMINATING@.len() - index,
        ))]
        while index < WHITE_TERMINATING.len() {
            let code = WHITE_TERMINATING[index];
            if code.length == length && code.bits == bits {
                #[cfg(verus_keep_ghost)]
                production_mmr_run_entry!(
                    WHITE_TERMINATING,
                    code,
                    index,
                    black,
                    false,
                    bits,
                    length
                );
                return Some(code);
            }
            index += 1;
        }
        index = 0;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                !black,
                index <= WHITE_MAKEUP@.len(),
            decreases WHITE_MAKEUP@.len() - index,
        ))]
        while index < WHITE_MAKEUP.len() {
            let code = WHITE_MAKEUP[index];
            if code.length == length && code.bits == bits {
                #[cfg(verus_keep_ghost)]
                production_mmr_run_entry!(WHITE_MAKEUP, code, index, black, false, bits, length);
                return Some(code);
            }
            index += 1;
        }
    }
    index = 0;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= COMMON_MAKEUP@.len(),
        decreases COMMON_MAKEUP@.len() - index,
    ))]
    while index < COMMON_MAKEUP.len() {
        let code = COMMON_MAKEUP[index];
        if code.length == length && code.bits == bits {
            #[cfg(verus_keep_ghost)]
            production_mmr_run_entry!(COMMON_MAKEUP, code, index, black, black, bits, length);
            return Some(code);
        }
        index += 1;
    }
    None
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        mmr_spec::reference_line_valid(transitions@, width),
        x <= width,
    ensures match out {
        Ok((b1, b2)) => x <= b1 <= b2 <= width,
        Err(_) => true,
    },
))]
fn reference_elements(
    transitions: &[u32],
    x: u32,
    black: bool,
    width: u32,
    initial: bool,
) -> Result<(u32, u32), MmrError> {
    if transitions.len() < 2 {
        return Err(MmrError::InvalidTransition);
    }
    let search_end = transitions
        .len()
        .checked_sub(1)
        .ok_or(MmrError::IntegerOverflow)?;
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            mmr_spec::reference_line_valid(transitions@, width),
            x <= width,
            search_end == transitions@.len() - 1,
            index <= search_end,
        decreases search_end - index,
    ))]
    while index < search_end {
        let position = *transitions.get(index).ok_or(MmrError::InvalidTransition)?;
        if position > x || (initial && position == x) {
            // Reference transitions alternate from imaginary white. Therefore
            // the colour immediately after even-indexed transitions is black.
            let post_transition_black = index % 2 == 0;
            if post_transition_black != black {
                let next_index = index.checked_add(1).ok_or(MmrError::IntegerOverflow)?;
                let b2 = *transitions
                    .get(next_index)
                    .ok_or(MmrError::InvalidTransition)?;
                #[cfg(verus_keep_ghost)]
                production_mmr_reference_bounds!(
                    transitions,
                    index,
                    next_index,
                    position,
                    b2,
                    width
                );
                return Ok((position, b2));
            }
        }
        index = index.checked_add(1).ok_or(MmrError::IntegerOverflow)?;
    }
    // T.6 2.2.5.2 places either missing reference element at the
    // imaginary changing element immediately after the line.
    Ok((width, width))
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        mmr_spec::reference_line_valid(transitions@, width),
        x <= width,
        *old(search_start) < transitions@.len(),
    ensures
        *final(search_start) < transitions@.len(),
        match out {
            Ok((b1, b2)) => x <= b1 <= b2 <= width,
            Err(_) => true,
        },
))]
fn reference_elements_from(
    transitions: &[u32],
    x: u32,
    black: bool,
    width: u32,
    initial: bool,
    search_start: &mut usize,
) -> Result<(u32, u32), MmrError> {
    let search_end = transitions
        .len()
        .checked_sub(1)
        .ok_or(MmrError::InvalidTransition)?;
    if *search_start > search_end {
        return Err(MmrError::InvalidTransition);
    }
    let mut index = *search_start;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            mmr_spec::reference_line_valid(transitions@, width),
            x <= width,
            search_end == transitions@.len() - 1,
            index <= search_end,
        decreases search_end - index,
    ))]
    while index < search_end {
        let position = *transitions.get(index).ok_or(MmrError::InvalidTransition)?;
        if position > x || (initial && position == x) {
            break;
        }
        index = index.checked_add(1).ok_or(MmrError::IntegerOverflow)?;
    }
    *search_start = index;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            mmr_spec::reference_line_valid(transitions@, width),
            x <= width,
            search_end == transitions@.len() - 1,
            *search_start <= search_end,
            *search_start <= index <= search_end,
        decreases search_end - index,
    ))]
    while index < search_end {
        let position = *transitions.get(index).ok_or(MmrError::InvalidTransition)?;
        if position < x {
            return Err(MmrError::InvalidTransition);
        }
        let post_transition_black = index % 2 == 0;
        if post_transition_black != black {
            let next_index = index.checked_add(1).ok_or(MmrError::IntegerOverflow)?;
            let b2 = *transitions
                .get(next_index)
                .ok_or(MmrError::InvalidTransition)?;
            #[cfg(verus_keep_ghost)]
            production_mmr_reference_bounds!(transitions, index, next_index, position, b2, width);
            return Ok((position, b2));
        }
        index = index.checked_add(1).ok_or(MmrError::IntegerOverflow)?;
    }
    Ok((width, width))
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires mmr_spec::bitmap_storage_valid(old(bitmap)),
    ensures
        final(bitmap).width == old(bitmap).width,
        final(bitmap).height == old(bitmap).height,
        final(bitmap).stride == old(bitmap).stride,
        final(bitmap).data@.len() == old(bitmap).data@.len(),
        mmr_spec::bitmap_storage_valid(final(bitmap)),
        (start > end || end > old(bitmap).width || row >= old(bitmap).height)
            ==> matches!(out, Err(MmrError::InvalidTransition)),
        (start <= end <= old(bitmap).width && row < old(bitmap).height
            && (!black || start == end)) ==> out.is_ok(),
        (start > end || end > old(bitmap).width || row >= old(bitmap).height
            || !black || start == end)
            ==> final(bitmap).data@ == old(bitmap).data@,
        (crate::spec::bitmap::storage_valid(old(bitmap))
            && start <= end <= old(bitmap).width && row < old(bitmap).height)
            ==> out.is_ok(),
        (out.is_ok() && crate::spec::bitmap::storage_valid(old(bitmap))
            && start <= end <= old(bitmap).width && row < old(bitmap).height)
            ==> mmr_spec::span_result(old(bitmap), final(bitmap), row, start, end, black),
))]
fn fill_span(
    bitmap: &mut Bitmap,
    row: u32,
    start: u32,
    end: u32,
    black: bool,
) -> Result<(), MmrError> {
    if start > end || end > bitmap.width || row >= bitmap.height {
        return Err(MmrError::InvalidTransition);
    }
    if !black {
        return Ok(());
    }
    bitmap
        .fill_black_span(row, start, end)
        .map_err(|_error| MmrError::InvalidTransition)?;
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(
    requires
        mmr_spec::transition_prefix_valid(old(transitions)@, start, _width),
        start <= end <= _width,
    ensures
        mmr_spec::transition_prefix_valid(final(transitions)@, end, _width),
        final(transitions)@.len() <= old(transitions)@.len() + 1,
        start == end ==> final(transitions)@.len() == old(transitions)@.len(),
        mmr_spec::transition_span_result(
            old(transitions)@, final(transitions)@,
            *old(emitted_black), *final(emitted_black), start, end, black,
        ),
))]
fn record_transition_span(
    transitions: &mut Vec<u32>,
    emitted_black: &mut bool,
    start: u32,
    end: u32,
    _width: u32,
    black: bool,
) {
    if start == end {
        return;
    }
    let changed = *emitted_black != black;
    #[cfg(verus_keep_ghost)]
    proof_decl! {
        let ghost before = transitions@;
    }
    if changed {
        transitions.push(start);
    }
    *emitted_black = black;
    #[cfg(verus_keep_ghost)]
    production_mmr_transition_span!(before, start, end, _width, changed);
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        mmr_spec::bitmap_storage_valid(bitmap),
        row < bitmap.height,
        end <= bitmap.width,
        mmr_spec::transition_prefix_valid(old(transitions)@, start, width),
        start <= end <= width,
    ensures
    match out {
        Ok(()) =>
            final(bitmap).width == old(bitmap).width
            && final(bitmap).height == old(bitmap).height
            && final(bitmap).stride == old(bitmap).stride
            && final(bitmap).data@.len() == old(bitmap).data@.len()
            && mmr_spec::bitmap_storage_valid(final(bitmap))
            && mmr_spec::transition_prefix_valid(final(transitions)@, end, width)
            && final(transitions)@.len() <= old(transitions)@.len() + 1
            && (start == end ==>
                final(transitions)@.len() == old(transitions)@.len())
            && mmr_spec::transition_span_result(
                old(transitions)@, final(transitions)@,
                *old(emitted_black), *final(emitted_black), start, end, black,
            )
            && (crate::spec::bitmap::storage_valid(old(bitmap)) ==>
                mmr_spec::span_result(old(bitmap), final(bitmap), row, start, end, black)),
        Err(_) => final(transitions)@ == old(transitions)@
            && *final(emitted_black) == *old(emitted_black),
    },
    crate::spec::bitmap::storage_valid(old(bitmap)) ==> out.is_ok(),
))]
#[allow(clippy::too_many_arguments)]
fn fill_recorded_span(
    bitmap: &mut Bitmap,
    row: u32,
    start: u32,
    end: u32,
    black: bool,
    width: u32,
    transitions: &mut Vec<u32>,
    emitted_black: &mut bool,
) -> Result<(), MmrError> {
    fill_span(bitmap, row, start, end, black)?;
    record_transition_span(transitions, emitted_black, start, end, width, black);
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(
    requires mmr_spec::transition_prefix_valid(old(transitions)@, width, width),
    ensures
        mmr_spec::reference_line_valid(final(transitions)@, width),
        final(transitions)@.len() == old(transitions)@.len() + 2,
))]
fn finish_transition_row(transitions: &mut Vec<u32>, width: u32) {
    #[cfg(verus_keep_ghost)]
    proof_decl! {
        let ghost before = transitions@;
    }
    transitions.push(width);
    transitions.push(width);
    #[cfg(verus_keep_ghost)]
    production_mmr_transition_finish!(before, width);
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        mmr_spec::bitmap_storage_valid(bitmap),
        row < bitmap.height,
        width <= bitmap.width,
    ensures match out {
        Ok(()) =>
            mmr_spec::reference_line_valid(final(transitions)@, width)
            && final(transitions)@.len() <= width as nat + 2,
        Err(_) => true,
    },
))]
#[cfg(test)]
fn collect_transitions(
    bitmap: &Bitmap,
    row: u32,
    width: u32,
    transitions: &mut Vec<u32>,
) -> Result<(), MmrError> {
    transitions.clear();
    #[cfg(verus_keep_ghost)]
    production_mmr_transition_empty!(width);
    let mut previous_black = false;
    let row_index = usize::try_from(row).map_err(|_error| MmrError::IntegerOverflow)?;
    let row_base = row_index
        .checked_mul(bitmap.stride)
        .ok_or(MmrError::IntegerOverflow)?;
    let mut x = 0_u32;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            mmr_spec::bitmap_storage_valid(bitmap),
            row < bitmap.height,
            width <= bitmap.width,
            x <= width,
            mmr_spec::transition_prefix_valid(transitions@, x, width),
            transitions@.len() <= x as nat,
        decreases width - x,
    ))]
    while x < width {
        #[cfg(verus_keep_ghost)]
        production_mmr_span_byte!(bitmap, row, x);
        let byte_in_row = usize::try_from(x / 8).map_err(|_error| MmrError::IntegerOverflow)?;
        let byte_index = row_base
            .checked_add(byte_in_row)
            .ok_or(MmrError::IntegerOverflow)?;
        if byte_index >= bitmap.data.len() {
            return Err(MmrError::InvalidTransition);
        }
        let bit = 7_u32.checked_sub(x % 8).ok_or(MmrError::IntegerOverflow)?;
        let black = bitmap.data[byte_index] & (1_u8 << bit) != 0;
        let changed = black != previous_black;
        #[cfg(verus_keep_ghost)]
        proof_decl! {
            let ghost before = transitions@;
        }
        if changed {
            transitions.push(x);
            previous_black = black;
        }
        #[cfg(verus_keep_ghost)]
        production_mmr_transition_advance!(before, x, width, changed);
        x = x.checked_add(1).ok_or(MmrError::IntegerOverflow)?;
    }
    // One real imaginary end transition and one b2 sentinel.
    #[cfg(verus_keep_ghost)]
    proof_decl! {
        let ghost before = transitions@;
    }
    transitions.push(width);
    transitions.push(width);
    #[cfg(verus_keep_ghost)]
    production_mmr_transition_finish!(before, width);
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires mmr_spec::reader_valid(old(reader)),
    ensures
        mmr_spec::reader_valid(final(reader)),
        final(reader).bytes@ == old(reader).bytes@,
        old(reader).bit_position <= final(reader).bit_position,
        match out {
            Ok(()) => match policy {
                EofbPolicy::Optional =>
                    final(reader).bit_position == old(reader).bit_position,
                EofbPolicy::Required =>
                    final(reader).bit_position == old(reader).bytes@.len() * 8
                    && mmr_spec::exact_eofb(
                        old(reader).bytes@,
                        old(reader).bit_position as nat,
                    ),
            },
            Err(_) => true,
        },
))]
fn finish_block(reader: &mut BitReader<'_>, policy: EofbPolicy) -> Result<(), MmrError> {
    if let EofbPolicy::Optional = policy {
        return Ok(());
    }
    let remaining = remaining_bits(reader)?;

    if remaining < 24 {
        return Err(MmrError::MissingEofb);
    }
    if reader.read_bits(24)? != EOFB {
        return Err(MmrError::InvalidEofb);
    }

    let padding = remaining_bits(reader)?;
    if padding > 7 {
        return Err(MmrError::TrailingData);
    }
    if padding != 0 {
        let padding_count = u8::try_from(padding).map_err(|_error| MmrError::IntegerOverflow)?;
        let value = reader.read_bits(padding_count)?;
        if value != 0 {
            return Err(MmrError::NonZeroEofbPadding);
        }
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires mmr_spec::reader_valid(old(reader)),
    ensures
        mmr_spec::reader_valid(final(reader)),
        final(reader).bytes@ == old(reader).bytes@,
        old(reader).bit_position <= final(reader).bit_position,
        match out {
            Ok(()) =>
                old(reader).bit_position as nat + 24
                    <= final(reader).bit_position as nat
                && final(reader).bit_position as nat
                    <= old(reader).bit_position as nat + 31
                && final(reader).bit_position % 8 == 0
                && mmr_spec::exact_eofb(
                    old(reader).bytes@,
                    old(reader).bit_position as nat,
                ),
            Err(_) => true,
        },
))]
fn finish_required_prefix(reader: &mut BitReader<'_>) -> Result<(), MmrError> {
    if remaining_bits(reader)? < 24 {
        return Err(MmrError::MissingEofb);
    }
    if reader.read_bits(24)? != EOFB {
        return Err(MmrError::InvalidEofb);
    }

    let bit_remainder = reader.bit_position % 8;
    let padding = if bit_remainder == 0 {
        0
    } else {
        8_usize
            .checked_sub(bit_remainder)
            .ok_or(MmrError::IntegerOverflow)?
    };
    if remaining_bits(reader)? < padding {
        return Err(MmrError::MissingEofb);
    }
    if padding != 0 {
        let padding_count = u8::try_from(padding).map_err(|_error| MmrError::IntegerOverflow)?;
        if reader.read_bits(padding_count)? != 0 {
            return Err(MmrError::NonZeroEofbPadding);
        }
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires mmr_spec::reader_valid(reader),
    ensures match out {
        Ok(remaining) =>
            remaining == reader.bytes@.len() * 8 - reader.bit_position,
        Err(_) => true,
    },
))]
fn remaining_bits(reader: &BitReader<'_>) -> Result<usize, MmrError> {
    let total = reader
        .bytes
        .len()
        .checked_mul(8)
        .ok_or(MmrError::IntegerOverflow)?;
    total
        .checked_sub(reader.bit_position)
        .ok_or(MmrError::IntegerOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn previous_horizontal_runs(
        reader: &mut BitReader<'_>,
        black: bool,
        x: u32,
        width: u32,
    ) -> Result<(u32, u32), MmrError> {
        let remaining = width.checked_sub(x).ok_or(MmrError::InvalidTransition)?;
        let first = decode_run(reader, black, remaining)?;
        let after_first = x.checked_add(first).ok_or(MmrError::IntegerOverflow)?;
        let second_remaining = width
            .checked_sub(after_first)
            .ok_or(MmrError::RunExceedsRow)?;
        let second = decode_run(reader, !black, second_remaining)?;
        let end = after_first
            .checked_add(second)
            .ok_or(MmrError::IntegerOverflow)?;
        if end > width {
            return Err(MmrError::RunExceedsRow);
        }
        if end == x {
            return Err(MmrError::NonProgress);
        }
        Ok((after_first, end))
    }

    fn horizontal_error(error: MmrError) -> MalformedError {
        let DecodeError::Malformed(error) = DecodeError::from(error) else {
            panic!("horizontal runs produce typed malformed errors");
        };
        error
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn horizontal_pairs_match_numeric_endpoints_and_previous_inline_procedure() {
        let mut pairs = 0;
        let mut comparisons = 0;
        for black in [false, true] {
            let (first_table, second_table) = if black {
                (&BLACK_TERMINATING, &WHITE_TERMINATING)
            } else {
                (&WHITE_TERMINATING, &BLACK_TERMINATING)
            };
            for first_terminal in first_table {
                for second_terminal in second_table {
                    for start in [0, 1, 7] {
                        for (first_makeup, second_makeup) in [(0, 0), (64, 2560), (2560, 64)] {
                            let mut codes = Vec::new();
                            if first_makeup != 0 {
                                codes.push(
                                    (if black {
                                        BLACK_MAKEUP.iter()
                                    } else {
                                        WHITE_MAKEUP.iter()
                                    })
                                    .chain(COMMON_MAKEUP.iter())
                                    .copied()
                                    .find(|code| code.value == first_makeup)
                                    .unwrap(),
                                );
                            }
                            codes.push(*first_terminal);
                            if second_makeup != 0 {
                                codes.push(
                                    COMMON_MAKEUP
                                        .iter()
                                        .chain(if black {
                                            WHITE_MAKEUP.iter()
                                        } else {
                                            BLACK_MAKEUP.iter()
                                        })
                                        .copied()
                                        .find(|code| code.value == second_makeup)
                                        .unwrap(),
                                );
                            }
                            codes.push(*second_terminal);
                            let first = u32::from(first_makeup) + u32::from(first_terminal.value);
                            let second =
                                u32::from(second_makeup) + u32::from(second_terminal.value);
                            let total = first + second;
                            let (bytes, bits) = run_sequence_bytes(&codes, start);
                            pairs += 1;
                            for x in [0, u32::MAX - total - 1] {
                                for spare in 0..=1 {
                                    let width = x + total + spare;
                                    let mut reader = bit_reader(&bytes);
                                    reader.bit_position = start;
                                    let mut previous = bit_reader(&bytes);
                                    previous.bit_position = start;
                                    let actual =
                                        decode_horizontal_runs(&mut reader, black, x, width)
                                            .map_err(horizontal_error);
                                    let expected =
                                        previous_horizontal_runs(&mut previous, black, x, width)
                                            .map_err(horizontal_error);
                                    assert_eq!(actual, expected);
                                    if total == 0 {
                                        assert_eq!(
                                            actual,
                                            Err(MalformedError::InvalidMmr(
                                                MmrErrorKind::NonProgress
                                            ))
                                        );
                                    } else {
                                        assert_eq!(actual.unwrap(), (x + first, x + total));
                                    }
                                    assert_eq!(reader.bit_position, start + bits);
                                    assert_eq!(reader.bit_position, previous.bit_position);
                                    assert_eq!(reader.bytes, previous.bytes);
                                    comparisons += 1;
                                }
                            }
                            if total != 0 {
                                let mut reader = bit_reader(&bytes);
                                reader.bit_position = start;
                                let mut previous = bit_reader(&bytes);
                                previous.bit_position = start;
                                let error =
                                    decode_horizontal_runs(&mut reader, black, 0, total - 1)
                                        .unwrap_err();
                                let old_error =
                                    previous_horizontal_runs(&mut previous, black, 0, total - 1)
                                        .unwrap_err();
                                assert_eq!(horizontal_error(error), horizontal_error(old_error));
                                let mut sum = 0;
                                let mut consumed = start;
                                for code in &codes {
                                    sum += u32::from(code.value);
                                    consumed += usize::from(code.length);
                                    if sum >= total {
                                        break;
                                    }
                                }
                                assert_eq!(reader.bit_position, consumed);
                                assert_eq!(reader.bit_position, previous.bit_position);
                                assert_eq!(reader.bytes, previous.bytes);
                                comparisons += 1;
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(pairs, 73_728);
        assert_eq!(comparisons, 368_634);
    }

    #[test]
    fn horizontal_pair_partial_failures_preserve_previous_error_fields_and_cursor() {
        let inputs: &[&[u8]] = &[
            &[],
            &[0],
            &[0xff],
            &[0x35],
            &[0x35, 0],
            &[0x35, 0x01],
            &[0x03, 0xc0],
            &[0xde, 0xcd, 0x40],
        ];
        let mut errors = 0;
        let mut cases = 0;
        for bytes in inputs {
            for start in 0..=bytes.len() * 8 {
                for black in [false, true] {
                    for (x, width) in [
                        (0, 0),
                        (0, 1),
                        (0, 63),
                        (0, 64),
                        (0, 4096),
                        (u32::MAX, u32::MAX),
                        (1, 0),
                    ] {
                        let mut reader = bit_reader(bytes);
                        reader.bit_position = start;
                        let mut previous = bit_reader(bytes);
                        previous.bit_position = start;
                        let actual = decode_horizontal_runs(&mut reader, black, x, width)
                            .map_err(horizontal_error);
                        let expected = previous_horizontal_runs(&mut previous, black, x, width)
                            .map_err(horizontal_error);
                        errors += usize::from(actual.is_err());
                        cases += 1;
                        assert_eq!(actual, expected);
                        assert_eq!(reader.bit_position, previous.bit_position);
                        assert_eq!(reader.bytes, previous.bytes);
                        if x > width {
                            assert_eq!(reader.bit_position, start);
                        }
                    }
                }
            }
        }
        assert_eq!(cases, 1456);
        assert!(errors > 0);
    }

    fn run_sequence_bytes(codes: &[RunCode], start: usize) -> ([u8; 8], usize) {
        let mut packed = 0_u64;
        let mut count = 0;
        for code in codes {
            packed = (packed << code.length) | u64::from(code.bits);
            count += usize::from(code.length);
        }
        assert!(start + count < 64);
        let leading = if start == 0 {
            0
        } else {
            (0xa5_u64 << 56) & (u64::MAX << (64 - start))
        };
        (
            (leading | (packed << (64 - start - count)) | 1).to_be_bytes(),
            count,
        )
    }

    #[test]
    fn makeup_terminal_sequences_have_exact_sums_extent_and_overshoot_errors() {
        let mut cases = 0;
        for black in [false, true] {
            let terminating = if black {
                &BLACK_TERMINATING
            } else {
                &WHITE_TERMINATING
            };
            let makeup = if black { &BLACK_MAKEUP } else { &WHITE_MAKEUP };
            let choices = [64, 128, 1728, 2560].map(|value| {
                *makeup
                    .iter()
                    .chain(&COMMON_MAKEUP)
                    .find(|code| code.value == value)
                    .expect("project makeup entry")
            });
            for count in 0..=3 {
                for pattern in 0..4_usize.pow(count) {
                    let mut prefix = Vec::new();
                    let mut selected = pattern;
                    for _ in 0..count {
                        prefix.push(choices[selected % 4]);
                        selected /= 4;
                    }
                    for terminal in terminating {
                        let mut codes = prefix.clone();
                        codes.push(*terminal);
                        let expected: u32 = codes.iter().map(|code| u32::from(code.value)).sum();
                        for start in 0..8 {
                            let (encoded, bits) = run_sequence_bytes(&codes, start);
                            for remaining in [expected, expected + 1] {
                                let mut reader = BitReader {
                                    bytes: &encoded,
                                    bit_position: start,
                                };
                                assert_eq!(
                                    decode_run(&mut reader, black, remaining).unwrap(),
                                    expected
                                );
                                assert_eq!(reader.bit_position(), start + bits);
                                assert_eq!(reader.bytes.as_ptr(), encoded.as_ptr());
                            }
                            if expected > 0 {
                                let remaining = expected - 1;
                                let mut sum = 0;
                                let mut consumed = 0;
                                for code in &codes {
                                    sum += u32::from(code.value);
                                    consumed += usize::from(code.length);
                                    if sum > remaining {
                                        break;
                                    }
                                }
                                let mut reader = BitReader {
                                    bytes: &encoded,
                                    bit_position: start,
                                };
                                assert_kind(
                                    decode_run(&mut reader, black, remaining).unwrap_err(),
                                    MmrErrorKind::RunExceedsRow,
                                );
                                assert_eq!(reader.bit_position(), start + consumed);
                                assert_eq!(reader.bytes.as_ptr(), encoded.as_ptr());
                            }
                            cases += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(cases, 87_040);
    }

    #[test]
    fn makeup_without_terminal_reports_exact_physical_eof_after_the_code() {
        for black in [false, true] {
            let makeup = if black { &BLACK_MAKEUP } else { &WHITE_MAKEUP };
            for value in [64, 128, 1728, 2560] {
                let code = *makeup
                    .iter()
                    .chain(&COMMON_MAKEUP)
                    .find(|code| code.value == value)
                    .unwrap();
                let start = (8 - usize::from(code.length) % 8) % 8;
                let (encoded, count) = run_sequence_bytes(&[code], start);
                let end = start + count;
                let bytes = &encoded[..end / 8];
                let mut reader = BitReader {
                    bytes,
                    bit_position: start,
                };
                let MmrError::Reader(error) =
                    decode_run(&mut reader, black, u32::from(value)).unwrap_err()
                else {
                    panic!("terminating code is required")
                };
                assert_eq!(
                    error,
                    MalformedError::UnexpectedEof {
                        offset: end as u64,
                        needed: 1,
                        remaining: 0,
                    }
                );
                assert_eq!(reader.bit_position(), end);
                assert_eq!(reader.bytes.as_ptr(), bytes.as_ptr());
            }
        }
    }

    fn prefix_window_bytes(window: u16, width: u8, start: usize) -> [u8; 8] {
        let value = u64::from(window) << (64 - start - usize::from(width));
        let leading = if start == 0 {
            0
        } else {
            (0xa5_u64 << 56) & (u64::MAX << (64 - start))
        };
        (value | leading | 0x5a).to_be_bytes()
    }

    #[test]
    fn every_mode_window_and_alignment_matches_scalar_prefix_lookup() {
        for window in 0..(1_u16 << MAX_MODE_CODE_BITS) {
            let expected = MODE_CODES
                .iter()
                .filter(|code| window >> (MAX_MODE_CODE_BITS - code.length) == code.bits)
                .min_by_key(|code| code.length);
            for start in 0..8 {
                let encoded = prefix_window_bytes(window, MAX_MODE_CODE_BITS, start);
                let mut reader = BitReader {
                    bytes: &encoded,
                    bit_position: start,
                };
                if let Some(code) = expected {
                    assert_eq!(decode_mode(&mut reader).unwrap(), code.mode);
                    assert_eq!(reader.bit_position(), start + usize::from(code.length));
                    let packed = u64::from_be_bytes(encoded);
                    let bits =
                        (packed >> (64 - reader.bit_position())) & ((1_u64 << code.length) - 1);
                    assert_eq!(bits, u64::from(code.bits));
                } else {
                    assert_kind(
                        decode_mode(&mut reader).unwrap_err(),
                        MmrErrorKind::InvalidMode { bit_offset: start },
                    );
                    assert_eq!(
                        reader.bit_position(),
                        start + usize::from(MAX_MODE_CODE_BITS)
                    );
                }
                assert_eq!(reader.bytes.as_ptr(), encoded.as_ptr());
            }
        }
    }

    #[test]
    fn every_run_window_colour_and_alignment_matches_scalar_prefix_lookup() {
        for black in [false, true] {
            let terminating = if black {
                &BLACK_TERMINATING
            } else {
                &WHITE_TERMINATING
            };
            let makeup = if black { &BLACK_MAKEUP } else { &WHITE_MAKEUP };
            for window in 0..(1_u16 << MAX_RUN_CODE_BITS) {
                let expected = terminating
                    .iter()
                    .chain(makeup)
                    .chain(&COMMON_MAKEUP)
                    .filter(|code| window >> (MAX_RUN_CODE_BITS - code.length) == code.bits)
                    .min_by_key(|code| code.length);
                for start in 0..8 {
                    let encoded = prefix_window_bytes(window, MAX_RUN_CODE_BITS, start);
                    let mut reader = BitReader {
                        bytes: &encoded,
                        bit_position: start,
                    };
                    if let Some(code) = expected {
                        let actual = decode_run_code(&mut reader, black).unwrap();
                        assert_eq!(
                            (actual.bits, actual.length, actual.value, actual.terminal),
                            (code.bits, code.length, code.value, code.terminal)
                        );
                        assert_eq!(reader.bit_position(), start + usize::from(code.length));
                        let packed = u64::from_be_bytes(encoded);
                        let bits = (packed >> (64 - reader.bit_position()))
                            & ((1_u64 << actual.length) - 1);
                        assert_eq!(bits, u64::from(actual.bits));
                    } else {
                        assert_kind(
                            decode_run_code(&mut reader, black).unwrap_err(),
                            MmrErrorKind::InvalidRunCode {
                                bit_offset: start,
                                black,
                            },
                        );
                        assert_eq!(
                            reader.bit_position(),
                            start + usize::from(MAX_RUN_CODE_BITS)
                        );
                    }
                    assert_eq!(reader.bytes.as_ptr(), encoded.as_ptr());
                }
            }
        }
    }

    #[test]
    fn code_constructors_preserve_raw_fields_and_terminal_sense() {
        for bits in [0, 1, u16::MAX] {
            for length in [0, 1, MAX_RUN_CODE_BITS, u8::MAX] {
                for mode in [
                    Mode::Pass,
                    Mode::Horizontal,
                    Mode::Extension,
                    Mode::Vertical(i8::MIN),
                    Mode::Vertical(0),
                    Mode::Vertical(i8::MAX),
                ] {
                    let code = ModeCode::new(bits, length, mode);
                    assert_eq!((code.bits, code.length, code.mode), (bits, length, mode));
                }
                for value in [0, 63, 64, u16::MAX] {
                    let terminal = RunCode::terminal(value, bits, length);
                    let makeup = RunCode::makeup(value, bits, length);
                    assert_eq!(
                        (
                            terminal.bits,
                            terminal.length,
                            terminal.value,
                            terminal.terminal
                        ),
                        (bits, length, value, true)
                    );
                    assert_eq!(
                        (makeup.bits, makeup.length, makeup.value, makeup.terminal),
                        (bits, length, value, false)
                    );
                }
            }
        }
    }

    #[test]
    fn sequential_prefix_checks_equal_prior_closure_at_every_u16_value() {
        for prefix in 0..=u16::MAX {
            for bit in [0, 1, 2, u16::MAX] {
                let prior = prefix
                    .checked_mul(2)
                    .and_then(|value| value.checked_add(bit));
                let expanded = append_code_bit(prefix, bit).ok();
                assert_eq!(expanded, prior);
            }
        }
    }

    fn span_test_bitmap(width: u32) -> Bitmap {
        let mut bitmap = Bitmap::blank(width, 3, &Limits::default()).expect("bounded bitmap");
        for (index, byte) in bitmap.data.iter_mut().enumerate() {
            *byte = u8::try_from(index % 256)
                .expect("bounded pattern")
                .wrapping_mul(37)
                .wrapping_add(11);
        }
        bitmap
    }

    #[test]
    fn optimized_span_dispatch_matches_scalar_pixels_and_full_storage() {
        for width in [0, 1, 7, 8, 9, 63, 64, 65, 71, 72, 73, 127, 128, 129] {
            let original = span_test_bitmap(width);
            for row in 0..3 {
                for start in 0..=width {
                    for end in start..=width {
                        for black in [false, true] {
                            let mut actual = original.clone();
                            let mut expected = original.clone();
                            if black {
                                for x in start..end {
                                    let index = usize::try_from(row).expect("row")
                                        * expected.stride
                                        + usize::try_from(x / 8).expect("byte");
                                    expected.data[index] |= 0x80 >> (x % 8);
                                }
                            }
                            fill_span(&mut actual, row, start, end, black).expect("valid span");
                            assert_eq!(actual, expected, "{width}/{row}/{start}/{end}/{black}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn recorded_span_matches_scalar_storage_and_exact_transition_updates() {
        let mut cases = 0;
        for width in [0, 1, 7, 8, 9, 17] {
            let original = span_test_bitmap(width);
            for row in 0..3 {
                for start in 0..=width {
                    for end in start..=width {
                        for black in [false, true] {
                            for before_black in [false, true] {
                                for prefix_shape in 0..3 {
                                    let before: Vec<_> = match prefix_shape {
                                        0 => Vec::new(),
                                        1 => (0..start).collect(),
                                        _ => (0..start).flat_map(|x| [x, x]).collect(),
                                    };
                                    let mut expected_transitions = before.clone();
                                    if start < end && before_black != black {
                                        expected_transitions.push(start);
                                    }
                                    let mut expected = original.clone();
                                    if black {
                                        for x in start..end {
                                            let index = usize::try_from(row).unwrap()
                                                * expected.stride
                                                + usize::try_from(x / 8).unwrap();
                                            expected.data[index] |= 0x80 >> (x % 8);
                                        }
                                    }
                                    let mut actual = original.clone();
                                    let mut transitions = before;
                                    let mut emitted_black = before_black;
                                    fill_recorded_span(
                                        &mut actual,
                                        row,
                                        start,
                                        end,
                                        black,
                                        width,
                                        &mut transitions,
                                        &mut emitted_black,
                                    )
                                    .unwrap();
                                    assert_eq!(actual, expected);
                                    assert_eq!(transitions, expected_transitions);
                                    assert_eq!(
                                        emitted_black,
                                        if start == end { before_black } else { black },
                                    );
                                    cases += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(cases, 11_196);
    }

    #[test]
    fn transition_span_complete_output_matrix_preserves_empty_and_same_colour_frames() {
        let mut cases = 0;
        for width in 0..=9 {
            for start in 0..=width {
                for end in start..=width {
                    for mask in 0..(1_u32 << start) {
                        for duplicate in [false, true] {
                            let mut before = Vec::new();
                            for x in 0..start {
                                if mask & (1 << x) != 0 {
                                    before.push(x);
                                    if duplicate {
                                        before.push(x);
                                    }
                                }
                            }
                            for before_black in [false, true] {
                                for black in [false, true] {
                                    let mut expected = before.clone();
                                    if start < end && before_black != black {
                                        expected.push(start);
                                    }
                                    let mut transitions = before.clone();
                                    let mut emitted_black = before_black;
                                    record_transition_span(
                                        &mut transitions,
                                        &mut emitted_black,
                                        start,
                                        end,
                                        width,
                                        black,
                                    );
                                    assert_eq!(transitions, expected);
                                    assert_eq!(
                                        emitted_black,
                                        if start == end { before_black } else { black },
                                    );
                                    cases += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(cases, 32_136);
    }

    #[test]
    fn invalid_span_dispatch_preserves_exact_error_and_all_bytes() {
        for width in [0, 1, 8, 65, 129] {
            let original = span_test_bitmap(width);
            for (row, start, end) in [
                (3, 0, 0),
                (u32::MAX, 0, width),
                (0, 1, 0),
                (0, 0, width + 1),
                (0, u32::MAX, u32::MAX),
            ] {
                for black in [false, true] {
                    let mut bitmap = original.clone();
                    assert!(matches!(
                        fill_span(&mut bitmap, row, start, end, black),
                        Err(MmrError::InvalidTransition)
                    ));
                    assert_eq!(bitmap, original);
                }
            }
        }
    }

    fn assert_incremental_transitions_match_scan(bitmap: &Bitmap) {
        for row in 0..bitmap.height() {
            let mut scanned = Vec::new();
            collect_transitions(bitmap, row, bitmap.width(), &mut scanned)
                .expect("oracle scan must cover a valid bitmap row");

            let mut incremental = Vec::new();
            incremental
                .try_reserve(usize::try_from(bitmap.width()).expect("test width fits usize") + 2)
                .expect("test transition storage must allocate");
            let mut emitted_black = false;
            for x in 0..bitmap.width() {
                let black = bitmap.pixel(x, row).expect("test pixel is in bounds");
                record_transition_span(
                    &mut incremental,
                    &mut emitted_black,
                    x,
                    x + 1,
                    bitmap.width(),
                    black,
                );
            }
            finish_transition_row(&mut incremental, bitmap.width());
            assert_eq!(incremental, scanned, "transition mismatch on row {row}");
        }
    }

    fn decode(data: &[u8], width: u32, height: u32) -> Bitmap {
        decode_mmr(
            data,
            width,
            height,
            &Limits::default(),
            EofbPolicy::Optional,
        )
        .expect("independently derived MMR vector must decode")
    }

    fn assert_row(bitmap: &Bitmap, row: u32, expected: &[bool]) {
        assert_eq!(
            usize::try_from(bitmap.width()).expect("test width fits usize"),
            expected.len()
        );
        for (x, value) in expected.iter().copied().enumerate() {
            assert_eq!(
                bitmap.pixel(u32::try_from(x).expect("test x fits u32"), row,),
                Some(value),
                "pixel mismatch at ({x}, {row})"
            );
        }
    }

    #[test]
    fn all_white_uses_vertical_zero() {
        let bitmap = decode(&[0x80], 8, 1);
        assert_row(&bitmap, 0, &[false; 8]);
    }

    #[test]
    fn all_black_uses_zero_white_horizontal_run() {
        let bitmap = decode(&[0x26, 0xA2, 0x80], 8, 1);
        assert_row(&bitmap, 0, &[true; 8]);
    }

    #[test]
    fn horizontal_white_makeup_run_decodes() {
        let bitmap = decode(&[0x3B, 0x35, 0x0D, 0xC0], 64, 1);
        assert_row(&bitmap, 0, &[false; 64]);
    }

    #[test]
    fn horizontal_black_makeup_run_decodes() {
        let bitmap = decode(&[0x26, 0xA0, 0x78, 0x6E], 64, 1);
        assert_row(&bitmap, 0, &[true; 64]);
    }

    #[test]
    fn identical_black_rows_update_the_reference_line() {
        let bitmap = decode(&[0x26, 0xA2, 0xE0], 8, 2);
        assert_row(&bitmap, 0, &[true; 8]);
        assert_row(&bitmap, 1, &[true; 8]);
    }

    #[test]
    fn identical_islands_use_three_vertical_zero_codes() {
        let bitmap = decode(&[0x2F, 0xF8], 8, 2);
        let island = [false, false, true, true, false, false, false, false];
        assert_row(&bitmap, 0, &island);
        assert_row(&bitmap, 1, &island);
    }

    #[test]
    fn island_shift_right_uses_vertical_right_one() {
        let bitmap = decode(&[0x2F, 0xDB, 0x80], 8, 2);
        assert_row(
            &bitmap,
            0,
            &[false, false, true, true, false, false, false, false],
        );
        assert_row(
            &bitmap,
            1,
            &[false, false, false, true, true, false, false, false],
        );
    }

    #[test]
    fn island_shift_left_uses_vertical_left_one() {
        let bitmap = decode(&[0x2F, 0xD2, 0x80], 8, 2);
        assert_row(
            &bitmap,
            0,
            &[false, false, true, true, false, false, false, false],
        );
        assert_row(
            &bitmap,
            1,
            &[false, true, true, false, false, false, false, false],
        );
    }

    #[test]
    fn pass_skips_reference_island() {
        let bitmap = decode(&[0x2F, 0xC6], 8, 2);
        assert_row(
            &bitmap,
            0,
            &[false, false, true, true, false, false, false, false],
        );
        assert_row(&bitmap, 1, &[false; 8]);
    }

    #[test]
    fn incremental_transitions_match_scan_for_normative_modes() {
        for (data, width, height) in [
            (&[0x80][..], 8, 1),
            (&[0x26, 0xA2, 0x80][..], 8, 1),
            (&[0x2F, 0xF8][..], 8, 2),
            (&[0x2F, 0xDB, 0x80][..], 8, 2),
            (&[0x2F, 0xD2, 0x80][..], 8, 2),
            (&[0x2F, 0xC6][..], 8, 2),
        ] {
            assert_incremental_transitions_match_scan(&decode(data, width, height));
        }
    }

    #[test]
    fn zero_length_spans_and_same_x_transitions_match_scan() {
        let mut bitmap =
            Bitmap::blank(5, 1, &Limits::default()).expect("test bitmap must allocate");
        for x in 0..3 {
            bitmap
                .set_pixel(x, 0, true)
                .expect("test pixel is in bounds");
        }

        let mut incremental = Vec::with_capacity(7);
        let mut emitted_black = false;
        record_transition_span(&mut incremental, &mut emitted_black, 0, 0, 5, true);
        record_transition_span(&mut incremental, &mut emitted_black, 0, 0, 5, false);
        record_transition_span(&mut incremental, &mut emitted_black, 0, 3, 5, true);
        record_transition_span(&mut incremental, &mut emitted_black, 3, 3, 5, false);
        record_transition_span(&mut incremental, &mut emitted_black, 3, 3, 5, true);
        record_transition_span(&mut incremental, &mut emitted_black, 3, 5, 5, false);
        finish_transition_row(&mut incremental, 5);

        let mut scanned = Vec::new();
        collect_transitions(&bitmap, 0, 5, &mut scanned).expect("oracle scan must succeed");
        assert_eq!(incremental, scanned);
        assert_eq!(incremental, [0, 3, 5, 5]);
    }

    #[test]
    fn incremental_transitions_match_scan_for_random_small_bitmaps() {
        let mut state = 0xD1B5_4A32_u32;
        for width in 1..=17 {
            for height in 1..=7 {
                let mut bitmap = Bitmap::blank(width, height, &Limits::default())
                    .expect("small test bitmap must allocate");
                for row in 0..height {
                    for x in 0..width {
                        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                        if state & 0x8000_0000 != 0 {
                            bitmap
                                .set_pixel(x, row, true)
                                .expect("test pixel is in bounds");
                        }
                    }
                }
                assert_incremental_transitions_match_scan(&bitmap);
            }
        }
    }

    #[test]
    fn monotone_reference_cursor_matches_full_scan_exactly() {
        for width in 1..=9_u32 {
            let interior = width.saturating_sub(1);
            for mask in 0..(1_u32 << interior) {
                let mut transitions = Vec::new();
                for position in 0..width {
                    if mask & (1 << position) != 0 {
                        transitions.push(position);
                    }
                }
                transitions.push(width);
                transitions.push(width);

                let mut cursor = 0;
                let mut initial = true;
                for x in 0..width {
                    for black in [false, true, false] {
                        let expected = reference_elements(&transitions, x, black, width, initial)
                            .expect("full scan accepts valid transitions");
                        let actual = reference_elements_from(
                            &transitions,
                            x,
                            black,
                            width,
                            initial,
                            &mut cursor,
                        )
                        .expect("cursor accepts valid transitions");
                        assert_eq!(
                            actual, expected,
                            "width={width}, mask={mask:#x}, x={x}, black={black}, initial={initial}"
                        );
                        initial = false;
                    }
                }
            }
        }
    }

    #[test]
    fn required_eofb_can_start_off_byte_boundary() {
        let bitmap = decode_mmr(
            &[0x80, 0x08, 0x00, 0x80],
            8,
            1,
            &Limits::default(),
            EofbPolicy::Required,
        )
        .expect("EOFB vector must decode");
        assert_row(&bitmap, 0, &[false; 8]);
    }

    #[test]
    fn required_eofb_rejects_omission() {
        let error = decode_mmr(&[0x80], 8, 1, &Limits::default(), EofbPolicy::Required)
            .expect_err("required EOFB must not be omitted");
        assert!(matches!(error, MmrError::MissingEofb));
    }

    #[test]
    fn prefix_decoder_consumes_two_concatenated_eofb_planes() {
        let stream = [
            0x80, 0x08, 0x00, 0x80, // all-white plane and EOFB
            0x26, 0xA2, 0x80, 0x08, 0x00, 0x80, // all-black plane and EOFB
        ];

        let (white, white_bytes) =
            decode_mmr_prefix(&stream, 8, 1, &Limits::default()).expect("first prefix must decode");
        assert_eq!(white_bytes, 4);
        assert_row(&white, 0, &[false; 8]);

        let remainder = stream
            .get(white_bytes..)
            .expect("consumed byte count is within the test stream");
        let (black, black_bytes) = decode_mmr_prefix(remainder, 8, 1, &Limits::default())
            .expect("second prefix must decode");
        assert_eq!(black_bytes, 6);
        assert_eq!(white_bytes + black_bytes, stream.len());
        assert_row(&black, 0, &[true; 8]);
    }

    #[test]
    fn prefix_decoder_rejects_truncated_eofb() {
        let error = decode_mmr_prefix(&[0x80, 0x08, 0x00], 8, 1, &Limits::default())
            .expect_err("23 post-bitmap bits cannot contain EOFB");
        assert!(matches!(error, MmrError::MissingEofb));
    }

    #[test]
    fn prefix_decoder_rejects_nonzero_alignment_padding() {
        let error = decode_mmr_prefix(&[0x80, 0x08, 0x00, 0x81], 8, 1, &Limits::default())
            .expect_err("bits after EOFB and before the byte boundary must be zero");
        assert!(matches!(error, MmrError::NonZeroEofbPadding));
    }

    #[test]
    fn extension_mode_is_rejected() {
        let error = decode_mmr(
            &[0x02, 0x00],
            8,
            1,
            &Limits::default(),
            EofbPolicy::Optional,
        )
        .expect_err("T.6 extension mode is forbidden by T.88");
        assert!(matches!(error, MmrError::ExtensionMode { .. }));
    }

    #[test]
    fn truncated_horizontal_run_is_rejected() {
        let error = decode_mmr(&[0x26], 8, 1, &Limits::default(), EofbPolicy::Optional)
            .expect_err("truncated horizontal run must fail");
        assert!(matches!(
            error,
            MmrError::Reader(MalformedError::UnexpectedEof { .. })
        ));
    }

    #[test]
    fn run_overshoot_is_rejected() {
        let error = decode_mmr(&[0x33], 4, 1, &Limits::default(), EofbPolicy::Optional)
            .expect_err("white run 8 must not fit a width-four row");
        assert!(matches!(error, MmrError::RunExceedsRow));
    }

    #[test]
    fn work_limit_is_enforced_before_decoding() {
        let limits = Limits {
            max_work_units: 15,
            ..Limits::default()
        };
        let error = decode_mmr(&[0x80], 8, 1, &limits, EofbPolicy::Optional)
            .expect_err("two raster passes already exceed the work limit");
        assert!(matches!(
            error,
            MmrError::Bitmap(DecodeError::LimitExceeded(LimitExceeded {
                kind: LimitKind::Work,
                limit: 15,
                actual: 16
            }))
        ));
    }

    #[test]
    fn both_transition_vectors_are_counted_as_managed_memory() {
        let limits = Limits {
            max_decoder_managed_memory_bytes: 80,
            ..Limits::default()
        };
        let error = decode_mmr(&[0x80], 8, 1, &limits, EofbPolicy::Optional)
            .expect_err("bitmap plus two width+2 transition vectors need 81 bytes");
        assert!(matches!(
            error,
            MmrError::Bitmap(DecodeError::LimitExceeded(LimitExceeded {
                kind: LimitKind::Memory,
                limit: 80,
                actual: 81
            }))
        ));
    }

    #[test]
    fn optional_eofb_accepts_bounded_remainder() {
        let bitmap = decode_mmr(
            &[0x80, 0xFF],
            8,
            1,
            &Limits::default(),
            EofbPolicy::Optional,
        )
        .expect("known-length remainder may be skipped");
        assert_row(&bitmap, 0, &[false; 8]);
    }

    #[test]
    fn all_normative_run_codes_are_unique_per_colour() {
        fn assert_unique(codes: &[RunCode]) {
            for (index, left) in codes.iter().enumerate() {
                for right in &codes[index + 1..] {
                    assert!(
                        left.length != right.length || left.bits != right.bits,
                        "duplicate run code"
                    );
                }
            }
        }

        let mut white = Vec::new();
        white.extend_from_slice(&WHITE_TERMINATING);
        white.extend_from_slice(&WHITE_MAKEUP);
        white.extend_from_slice(&COMMON_MAKEUP);
        assert_unique(&white);

        let mut black = Vec::new();
        black.extend_from_slice(&BLACK_TERMINATING);
        black.extend_from_slice(&BLACK_MAKEUP);
        black.extend_from_slice(&COMMON_MAKEUP);
        assert_unique(&black);
    }

    #[test]
    fn every_mode_table_entry_decodes() {
        for expected in MODE_CODES {
            let shift = u32::from(16_u8 - expected.length);
            let encoded = expected
                .bits
                .checked_shl(shift)
                .expect("mode code fits two test bytes")
                .to_be_bytes();
            let mut reader = bit_reader(&encoded);
            assert_eq!(
                decode_mode(&mut reader).expect("table mode must decode"),
                expected.mode
            );
            assert_eq!(reader.bit_position(), usize::from(expected.length));
        }
    }

    #[test]
    fn every_run_table_entry_decodes_for_its_colour() {
        fn assert_codes_decode(codes: &[RunCode], black: bool) {
            for expected in codes {
                let shift = u32::from(16_u8 - expected.length);
                let encoded = expected
                    .bits
                    .checked_shl(shift)
                    .expect("run code fits two test bytes")
                    .to_be_bytes();
                let mut reader = bit_reader(&encoded);
                let actual = decode_run_code(&mut reader, black).expect("table run must decode");
                assert_eq!(actual.bits, expected.bits);
                assert_eq!(actual.length, expected.length);
                assert_eq!(actual.value, expected.value);
                assert_eq!(actual.terminal, expected.terminal);
                assert_eq!(reader.bit_position(), usize::from(expected.length));
            }
        }

        assert_codes_decode(&WHITE_TERMINATING, false);
        assert_codes_decode(&WHITE_MAKEUP, false);
        assert_codes_decode(&COMMON_MAKEUP, false);
        assert_codes_decode(&BLACK_TERMINATING, true);
        assert_codes_decode(&BLACK_MAKEUP, true);
        assert_codes_decode(&COMMON_MAKEUP, true);
    }

    fn capacity_charge(scratch: &MmrScratch) -> u64 {
        u64::try_from(
            (scratch.transitions.capacity() + scratch.row_transitions.capacity())
                * core::mem::size_of::<u32>(),
        )
        .unwrap()
    }

    fn fresh_capacity_charge(width: u32) -> u64 {
        let mut first = Vec::<u32>::new();
        let mut second = Vec::<u32>::new();
        let count = usize::try_from(width).unwrap() + 2;
        first.try_reserve(count).unwrap();
        second.try_reserve(count).unwrap();
        u64::try_from((first.capacity() + second.capacity()) * core::mem::size_of::<u32>()).unwrap()
    }

    fn assert_limit(error: MmrError, kind: LimitKind, limit: u64, actual: u64) {
        let DecodeError::LimitExceeded(found) = DecodeError::from(error) else {
            panic!("expected an unchanged resource-limit error");
        };
        assert_eq!(
            found,
            LimitExceeded {
                kind,
                limit,
                actual
            }
        );
    }

    fn assert_kind(error: MmrError, expected: MmrErrorKind) {
        let DecodeError::Malformed(found) = DecodeError::from(error) else {
            panic!("expected a typed malformed MMR error");
        };
        assert_eq!(found, MalformedError::InvalidMmr(expected));
    }

    fn assert_cleared(scratch: &MmrScratch, charge: u64) {
        assert!(scratch.transitions.is_empty());
        assert!(scratch.row_transitions.is_empty());
        assert_eq!(capacity_charge(scratch), charge);
        assert_eq!(scratch.retained_bytes(), charge);
    }

    #[test]
    fn scratch_clear_retains_and_release_drops_both_vectors_idempotently() {
        let bitmap = Bitmap::blank(128, 1, &Limits::default()).unwrap();
        let mut scratch = MmrScratch::default();
        prepare_transition_storage(128, &bitmap, &Limits::default(), &mut scratch).unwrap();
        let charge = capacity_charge(&scratch);
        assert_eq!(charge, fresh_capacity_charge(128));
        assert_eq!(scratch.reserve_events, 2);
        scratch.transitions.extend_from_slice(&[0, 128, 128]);
        scratch.row_transitions.extend_from_slice(&[64, 128, 128]);
        let capacities = (
            scratch.transitions.capacity(),
            scratch.row_transitions.capacity(),
        );
        scratch.clear();
        scratch.clear();
        assert_cleared(&scratch, charge);
        assert_eq!(
            capacities,
            (
                scratch.transitions.capacity(),
                scratch.row_transitions.capacity()
            )
        );
        assert_eq!(scratch.reserve_events, 2);
        scratch.release();
        scratch.release();
        assert_cleared(&scratch, 0);
        assert_eq!(scratch.transitions.capacity(), 0);
        assert_eq!(scratch.row_transitions.capacity(), 0);
        assert_eq!(scratch.reserve_events, 0);
    }

    #[test]
    fn reused_scratch_recovers_after_decode_and_framing_errors_then_shrinks() {
        let mut scratch = MmrScratch::default();
        let limits = Limits::default();
        let wide = decode_mmr_reused(&[0x80], 128, 1, &limits, EofbPolicy::Optional, &mut scratch)
            .unwrap();
        assert_eq!(wide.bytes(), &[0; 16]);
        let charge = capacity_charge(&scratch);
        assert_eq!(charge, fresh_capacity_charge(128));
        assert_cleared(&scratch, charge);
        for (data, expected) in [
            (&[0x02][..], MmrErrorKind::ExtensionMode { bit_offset: 0 }),
            (&[0x00][..], MmrErrorKind::InvalidMode { bit_offset: 0 }),
        ] {
            let error = decode_mmr_reused(data, 8, 1, &limits, EofbPolicy::Optional, &mut scratch)
                .unwrap_err();
            assert_kind(error, expected);
            assert_cleared(&scratch, charge);
            assert_eq!(scratch.reserve_events, 2);
        }
        let error = decode_mmr_reused(&[0x80], 8, 1, &limits, EofbPolicy::Required, &mut scratch)
            .unwrap_err();
        assert_kind(error, MmrErrorKind::MissingEofb);
        assert_cleared(&scratch, charge);
        let below_requested = Limits {
            max_decoder_managed_memory_bytes: 80,
            ..limits
        };
        let error = decode_mmr_reused(
            &[0x80],
            8,
            1,
            &below_requested,
            EofbPolicy::Optional,
            &mut scratch,
        )
        .unwrap_err();
        assert_limit(error, LimitKind::Memory, 80, 81);
        // The requested-size rejection precedes any retained-capacity release.
        assert_cleared(&scratch, charge);
        assert_eq!(scratch.reserve_events, 2);
        let tight = Limits {
            max_decoder_managed_memory_bytes: 1 + fresh_capacity_charge(8),
            ..limits
        };
        assert!(charge + 1 > tight.max_decoder_managed_memory_bytes);
        let bitmap = decode_mmr_reused(
            &[0x26, 0xA2, 0x80],
            8,
            1,
            &tight,
            EofbPolicy::Optional,
            &mut scratch,
        )
        .unwrap();
        assert_eq!(bitmap.bytes(), &[0xFF]);
        assert_cleared(&scratch, fresh_capacity_charge(8));
        // A retained-over-budget release resets the counter before two fresh reserves.
        assert_eq!(scratch.reserve_events, 2);
    }

    #[test]
    fn requested_and_actual_capacity_limits_are_distinct_inclusive_and_recoverable() {
        let width = 1;
        let requested = 1 + u64::from(width + 2) * 2 * 4;
        let actual = 1 + fresh_capacity_charge(width);
        assert!(
            requested < actual,
            "the minimum u32 Vec capacity rounds three elements up"
        );
        let mut scratch = MmrScratch::default();
        for (limit, expected_actual) in [
            (requested - 1, requested),
            (requested, actual),
            (actual - 1, actual),
        ] {
            let limits = Limits {
                max_decoder_managed_memory_bytes: limit,
                ..Limits::default()
            };
            let error = decode_mmr_reused(
                &[0x80],
                width,
                1,
                &limits,
                EofbPolicy::Optional,
                &mut scratch,
            )
            .unwrap_err();
            assert_limit(error, LimitKind::Memory, limit, expected_actual);
            assert_cleared(&scratch, 0);
            assert_eq!(scratch.reserve_events, 0);
        }
        let limits = Limits {
            max_decoder_managed_memory_bytes: actual,
            ..Limits::default()
        };
        let bitmap = decode_mmr_reused(
            &[0x80],
            width,
            1,
            &limits,
            EofbPolicy::Optional,
            &mut scratch,
        )
        .unwrap();
        assert_eq!((bitmap.width, bitmap.height, bitmap.stride), (1, 1, 1));
        assert_eq!(bitmap.bytes(), &[0]);
        assert_cleared(&scratch, actual - 1);
        assert_eq!(scratch.reserve_events, 2);
    }

    #[test]
    fn stream_and_each_mode_work_boundary_preserve_retained_scratch_and_recover() {
        let mut scratch = MmrScratch::default();
        let bitmap = decode_mmr_reused(
            &[0xC0],
            8,
            2,
            &Limits::default(),
            EofbPolicy::Optional,
            &mut scratch,
        )
        .unwrap();
        assert_eq!(bitmap.bytes(), &[0, 0]);
        let charge = capacity_charge(&scratch);
        for (stream, work, kind, limit, actual) in [
            (0, 0, LimitKind::StreamBytes, 0, 1),
            (1, 31, LimitKind::Work, 31, 32),
            (1, 32, LimitKind::Work, 32, 33),
            (1, 33, LimitKind::Work, 33, 34),
        ] {
            let limits = Limits {
                max_stream_bytes: stream,
                max_work_units: work,
                ..Limits::default()
            };
            let error =
                decode_mmr_reused(&[0xC0], 8, 2, &limits, EofbPolicy::Optional, &mut scratch)
                    .unwrap_err();
            assert_limit(error, kind, limit, actual);
            assert_cleared(&scratch, charge);
            assert_eq!(scratch.reserve_events, 2);
            let exact = Limits {
                max_stream_bytes: 1,
                max_work_units: 34,
                ..Limits::default()
            };
            let recovered =
                decode_mmr_reused(&[0xC0], 8, 2, &exact, EofbPolicy::Optional, &mut scratch)
                    .unwrap();
            assert_eq!(recovered.bytes(), &[0, 0]);
            assert_cleared(&scratch, charge);
        }
    }

    #[test]
    fn framing_at_every_alignment_has_exact_consumption_and_padding_errors() {
        let cases: &[(u8, &[u8])] = &[
            (0, &[0x00, 0x10, 0x01]),
            (1, &[0x00, 0x08, 0x00, 0x80]),
            (2, &[0x00, 0x04, 0x00, 0x40]),
            (3, &[0x00, 0x02, 0x00, 0x20]),
            (4, &[0x00, 0x01, 0x00, 0x10]),
            (5, &[0x00, 0x00, 0x80, 0x08]),
            (6, &[0x00, 0x00, 0x40, 0x04]),
            (7, &[0x00, 0x00, 0x20, 0x02]),
        ];
        for &(offset, bytes) in cases {
            let mut reader = bit_reader(bytes);
            reader.read_bits(offset).unwrap();
            finish_block(&mut reader, EofbPolicy::Required).unwrap();
            assert_eq!(reader.bit_position(), bytes.len() * 8);
            assert_eq!(reader.bytes, bytes);

            let mut with_suffix = bytes.to_vec();
            with_suffix.extend_from_slice(&[0xA5, 0x5A]);
            let mut prefix = bit_reader(&with_suffix);
            prefix.read_bits(offset).unwrap();
            finish_required_prefix(&mut prefix).unwrap();
            assert_eq!(prefix.bit_position(), bytes.len() * 8);
            assert_eq!(&prefix.bytes[prefix.bit_position() / 8..], &[0xA5, 0x5A]);
            let mut bounded = bit_reader(&with_suffix);
            bounded.read_bits(offset).unwrap();
            assert_kind(
                finish_block(&mut bounded, EofbPolicy::Required).unwrap_err(),
                MmrErrorKind::TrailingData,
            );
            assert_eq!(bounded.bit_position(), usize::from(offset) + 24);
            let mut optional = bit_reader(&with_suffix);
            optional.read_bits(offset).unwrap();
            finish_block(&mut optional, EofbPolicy::Optional).unwrap();
            assert_eq!(optional.bit_position(), usize::from(offset));

            if offset != 0 {
                let mut nonzero = bytes.to_vec();
                *nonzero.last_mut().unwrap() |= 1;
                for prefix_mode in [false, true] {
                    let mut reader = bit_reader(&nonzero);
                    reader.read_bits(offset).unwrap();
                    let error = if prefix_mode {
                        finish_required_prefix(&mut reader)
                    } else {
                        finish_block(&mut reader, EofbPolicy::Required)
                    }
                    .unwrap_err();
                    assert_kind(error, MmrErrorKind::NonZeroPadding);
                    assert_eq!(reader.bit_position(), nonzero.len() * 8);
                    assert_eq!(reader.bytes, nonzero);
                }
            }
        }
    }

    #[test]
    fn missing_and_invalid_eofb_have_distinct_exact_reader_effects() {
        for prefix_mode in [false, true] {
            for bytes in [&[][..], &[0x00][..], &[0x00, 0x10][..]] {
                let mut reader = bit_reader(bytes);
                let error = if prefix_mode {
                    finish_required_prefix(&mut reader)
                } else {
                    finish_block(&mut reader, EofbPolicy::Required)
                }
                .unwrap_err();
                assert_kind(error, MmrErrorKind::MissingEofb);
                assert_eq!(reader.bit_position(), 0);
                assert_eq!(reader.bytes, bytes);
            }
            let mut reader = bit_reader(&[0x00, 0x10, 0x00, 0xA5]);
            let error = if prefix_mode {
                finish_required_prefix(&mut reader)
            } else {
                finish_block(&mut reader, EofbPolicy::Required)
            }
            .unwrap_err();
            assert_kind(error, MmrErrorKind::InvalidEofb);
            assert_eq!(reader.bit_position(), 24);
            assert_eq!(reader.bytes, &[0x00, 0x10, 0x00, 0xA5]);
        }
        let (bitmap, consumed) =
            decode_mmr_prefix(&[0x00, 0x10, 0x01, 0xA5], 0, 0, &Limits::default()).unwrap();
        assert_eq!((bitmap.width, bitmap.height, bitmap.stride), (0, 0, 0));
        assert_eq!(bitmap.bytes(), &[]);
        assert_eq!(consumed, 3);
    }

    #[test]
    fn invalid_mode_and_run_codes_preserve_start_offsets_and_exact_eof_states() {
        let mut mode = bit_reader(&[0x80, 0x00]);
        assert_eq!(mode.read_bits(3), Ok(4));
        assert_kind(
            decode_mode(&mut mode).unwrap_err(),
            MmrErrorKind::InvalidMode { bit_offset: 3 },
        );
        assert_eq!(mode.bit_position(), 10);
        let mut truncated = bit_reader(&[0x00]);
        truncated.read_bits(2).unwrap();
        let DecodeError::Malformed(error) =
            DecodeError::from(decode_mode(&mut truncated).unwrap_err())
        else {
            panic!("mode truncation is a reader error");
        };
        assert_eq!(
            error,
            MalformedError::UnexpectedEof {
                offset: 8,
                needed: 1,
                remaining: 0,
            }
        );
        assert_eq!(truncated.bit_position(), 8);
        for black in [false, true] {
            let mut invalid = bit_reader(&[0x80, 0x00, 0x00]);
            invalid.read_bits(3).unwrap();
            assert_kind(
                decode_run_code(&mut invalid, black).unwrap_err(),
                MmrErrorKind::InvalidRunCode {
                    bit_offset: 3,
                    black,
                },
            );
            assert_eq!(invalid.bit_position(), 16);
            assert_eq!(invalid.bytes, &[0x80, 0x00, 0x00]);
            let mut truncated = bit_reader(&[0x80, 0x00]);
            truncated.read_bits(4).unwrap();
            let MmrError::Reader(error) = decode_run_code(&mut truncated, black).unwrap_err()
            else {
                panic!("run truncation is a reader error");
            };
            assert_eq!(
                error,
                MalformedError::UnexpectedEof {
                    offset: 16,
                    needed: 1,
                    remaining: 0,
                }
            );
            assert_eq!(truncated.bit_position(), 16);
        }
    }

    #[test]
    fn static_vertical_two_and_three_cases_have_literal_pixels_and_bounds() {
        for (data, expected) in [
            (&[0x0A][..], &[0x03][..]),
            (&[0x05][..], &[0x07][..]),
            (&[0x26, 0xA2, 0x87][..], &[0xFF, 0x3F][..]),
            (&[0x26, 0xA2, 0x83, 0x80][..], &[0xFF, 0x1F][..]),
        ] {
            let height = u32::try_from(expected.len()).unwrap();
            let bitmap = decode(data, 8, height);
            assert_eq!((bitmap.width, bitmap.height, bitmap.stride), (8, height, 1));
            assert_eq!(bitmap.bytes(), expected);
        }
        for (data, width, height) in [
            (&[0x0C][..], 8, 1),
            (&[0x06][..], 8, 1),
            (&[0x08][..], 1, 1),
            (&[0x04][..], 2, 1),
            (&[0x26, 0xA2, 0x84][..], 8, 2),
            (&[0x26, 0xA2, 0x82][..], 8, 2),
        ] {
            assert_kind(
                decode_mmr(
                    data,
                    width,
                    height,
                    &Limits::default(),
                    EofbPolicy::Required,
                )
                .unwrap_err(),
                MmrErrorKind::InvalidTransition,
            );
        }
    }

    #[test]
    fn horizontal_zero_pair_is_nonprogress_and_run_bounds_stop_at_the_code() {
        assert_kind(
            decode_mmr(
                &[0x26, 0xA1, 0xB8],
                8,
                1,
                &Limits::default(),
                EofbPolicy::Optional,
            )
            .unwrap_err(),
            MmrErrorKind::NonProgress,
        );
        for (bytes, black, consumed) in [(&[0x98][..], false, 5), (&[0x14][..], true, 6)] {
            let mut reader = bit_reader(bytes);
            assert_kind(
                decode_run(&mut reader, black, 7).unwrap_err(),
                MmrErrorKind::RunExceedsRow,
            );
            assert_eq!(reader.bit_position(), consumed);
            assert_eq!(reader.bytes, bytes);
            let mut exact = bit_reader(bytes);
            assert_eq!(decode_run(&mut exact, black, 8).unwrap(), 8);
            assert_eq!(exact.bit_position(), consumed);
        }
    }

    #[test]
    fn makeup_runs_require_a_terminal_and_enforce_the_exact_remaining_width() {
        for (bytes, black, value, consumed) in [
            (&[0xD9, 0xA8][..], false, 64, 13),
            (&[0x03, 0xC3, 0x70][..], true, 64, 20),
            (&[0xDE, 0xCD, 0x40][..], false, 128, 18),
            (&[0x03, 0xC0, 0xF0, 0xDC][..], true, 128, 30),
        ] {
            let mut exact = bit_reader(bytes);
            assert_eq!(decode_run(&mut exact, black, value).unwrap(), value);
            assert_eq!(exact.bit_position(), consumed);
            assert_eq!(exact.bytes, bytes);
            let mut short = bit_reader(bytes);
            assert_kind(
                decode_run(&mut short, black, 63).unwrap_err(),
                MmrErrorKind::RunExceedsRow,
            );
            assert_eq!(short.bit_position(), if black { 10 } else { 5 });
        }
        for (bytes, black) in [(&[0xD8][..], false), (&[0x03, 0xC0][..], true)] {
            let mut reader = bit_reader(bytes);
            let MmrError::Reader(error) = decode_run(&mut reader, black, 64).unwrap_err() else {
                panic!("make-up alone does not terminate a run");
            };
            assert_eq!(
                error,
                MalformedError::UnexpectedEof {
                    offset: (bytes.len() * 8) as u64,
                    needed: 1,
                    remaining: 0,
                }
            );
            assert_eq!(reader.bit_position(), bytes.len() * 8);
            assert_eq!(reader.bytes, bytes);
        }
    }

    #[test]
    fn defensive_reference_errors_preserve_inputs_and_exact_cursor_effects() {
        for transitions in [&[][..], &[8][..]] {
            assert_kind(
                reference_elements(transitions, 0, false, 8, true).unwrap_err(),
                MmrErrorKind::InvalidTransition,
            );
        }
        let mut cursor = 0;
        assert_kind(
            reference_elements_from(&[], 0, false, 8, true, &mut cursor).unwrap_err(),
            MmrErrorKind::InvalidTransition,
        );
        assert_eq!(cursor, 0);
        cursor = 2;
        assert_kind(
            reference_elements_from(&[8, 8], 0, false, 8, true, &mut cursor).unwrap_err(),
            MmrErrorKind::InvalidTransition,
        );
        assert_eq!(cursor, 2);
        let transitions = [0, 2, 1, 8, 8];
        cursor = 0;
        assert_kind(
            reference_elements_from(&transitions, 2, false, 8, true, &mut cursor).unwrap_err(),
            MmrErrorKind::InvalidTransition,
        );
        assert_eq!(cursor, 1);
        assert_eq!(transitions, [0, 2, 1, 8, 8]);
    }

    #[test]
    fn error_conversion_preserves_all_classifications_and_payloads() {
        for (error, expected) in [
            (
                MmrError::InvalidMode { bit_offset: 37 },
                MmrErrorKind::InvalidMode { bit_offset: 37 },
            ),
            (
                MmrError::ExtensionMode { bit_offset: 41 },
                MmrErrorKind::ExtensionMode { bit_offset: 41 },
            ),
            (
                MmrError::InvalidRunCode {
                    bit_offset: 43,
                    black: true,
                },
                MmrErrorKind::InvalidRunCode {
                    bit_offset: 43,
                    black: true,
                },
            ),
            (
                MmrError::InvalidRunCode {
                    bit_offset: 47,
                    black: false,
                },
                MmrErrorKind::InvalidRunCode {
                    bit_offset: 47,
                    black: false,
                },
            ),
            (MmrError::RunExceedsRow, MmrErrorKind::RunExceedsRow),
            (MmrError::InvalidTransition, MmrErrorKind::InvalidTransition),
            (MmrError::NonProgress, MmrErrorKind::NonProgress),
            (MmrError::TooManyModes, MmrErrorKind::TooManyModes),
            (MmrError::MissingEofb, MmrErrorKind::MissingEofb),
            (MmrError::InvalidEofb, MmrErrorKind::InvalidEofb),
            (MmrError::TrailingData, MmrErrorKind::TrailingData),
            (MmrError::NonZeroEofbPadding, MmrErrorKind::NonZeroPadding),
        ] {
            assert_kind(error, expected);
        }
        let DecodeError::Allocation { requested_bytes } = DecodeError::from(MmrError::Allocation {
            requested_bytes: 4096,
        }) else {
            panic!("allocation size must survive conversion");
        };
        assert_eq!(requested_bytes, 4096);
        let DecodeError::Malformed(error) = DecodeError::from(MmrError::IntegerOverflow) else {
            panic!("integer overflow must survive conversion");
        };
        assert_eq!(error, MalformedError::IntegerOverflow);
        let DecodeError::Malformed(error) = DecodeError::from(MmrError::Bitmap(
            DecodeError::Malformed(MalformedError::InvalidBitCount(33)),
        )) else {
            panic!("bitmap errors must not be reclassified");
        };
        assert_eq!(error, MalformedError::InvalidBitCount(33));
    }
}

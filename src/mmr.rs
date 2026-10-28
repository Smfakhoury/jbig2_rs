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
        Ok(bitmap) => crate::spec::bitmap::storage_valid(&bitmap)
            && mmr_spec::bounded_bitmap_decodes(
                data@, width, height, &bitmap, eofb_policy,
            ),
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
        Ok(bitmap) => crate::spec::bitmap::storage_valid(&bitmap)
            && mmr_spec::bounded_bitmap_decodes(
                data@, width, height, &bitmap, eofb_policy,
            ),
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
    #[cfg(verus_keep_ghost)]
    proof_decl! { let ghost decoded_end = reader.bit_position as nat; }
    let finish = finish_block(&mut reader, eofb_policy);
    scratch.clear();
    finish?;
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(mmr_spec::bitmap_decodes(data@, width, height, &bitmap, decoded_end));
        mmr_spec::bounded_bitmap_witness(
            data@, width, height, &bitmap, eofb_policy, decoded_end,
        );
    }
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
            && consumed_bytes <= data@.len()
            && mmr_spec::prefix_bitmap_decodes(
                data@, width, height, &bitmap, consumed_bytes,
            ),
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
    #[cfg(verus_keep_ghost)]
    proof_decl! { let ghost decoded_end = reader.bit_position as nat; }
    let finish = finish_required_prefix(&mut reader);
    scratch.clear();
    finish?;
    let consumed_bytes = reader
        .bit_position
        .checked_div(8)
        .ok_or(MmrError::IntegerOverflow)?;
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(mmr_spec::bitmap_decodes(data@, width, height, &bitmap, decoded_end));
        assert(consumed_bytes as nat * 8 == reader.bit_position as nat);
        mmr_spec::prefix_bitmap_witness(
            data@, width, height, &bitmap, decoded_end, consumed_bytes,
        );
    }
    Ok((bitmap, consumed_bytes))
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok((bitmap, reader)) =>
            crate::spec::bitmap::storage_valid(&bitmap)
            && mmr_spec::reader_valid(&reader)
            && reader.bytes@ == data@
            && mmr_spec::bitmap_decodes(
                data@, width, height, &bitmap, reader.bit_position as nat,
            ),
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
        mmr_properties::zeroed_rows_are_white(&bitmap);
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
        mmr_properties::initial_reference_cache(width);
        assert(transitions@ == Seq::empty().push(width).push(width));
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
    #[cfg(verus_keep_ghost)]
    production_mmr_trace_begin!(traced_rows);

    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            row <= height,
            bitmap.width == width,
            bitmap.height == height,
            crate::spec::bitmap::storage_valid(&bitmap),
            mmr_spec::bitmap_storage_valid(&bitmap),
            mmr_spec::future_rows_white(&bitmap, row as nat),
            row == 0 ==> mmr_spec::white_reference_cache(transitions@, width),
            row > 0 ==> mmr_spec::row_cache_matches(
                &bitmap, (row - 1) as u32, transitions@,
            ),
            mmr_spec::reference_line_valid(transitions@, width),
            transitions@.len() <= width as nat + 2,
            mmr_spec::reader_valid(&reader),
            reader.bytes@ == data@,
            mmr_spec::raster_outer(
                data@, width, row, &bitmap, traced_rows, transitions@,
                reader.bit_position as nat,
            ),
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
            mmr_properties::empty_row_cache(&bitmap, row);
            mmr_properties::white_future_advance(&bitmap, row as nat, row as nat + 1);
        }
        #[cfg(verus_keep_ghost)]
        production_mmr_trace_row_start!(traced_start, traced_steps, reader);

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
                crate::spec::bitmap::storage_valid(&bitmap),
                mmr_spec::future_rows_white(&bitmap, row as nat + 1),
                mmr_spec::row_cache_prefix(
                    &bitmap, row, row_transitions@, emitted_black, x,
                ),
                row > 0 ==> mmr_spec::row_cache_matches(
                    &bitmap, (row - 1) as u32, transitions@,
                ),
                mmr_spec::reference_line_valid(transitions@, width),
                transitions@.len() <= width as nat + 2,
                reference_index < transitions@.len(),
                mmr_properties::reference_prefix_skipped(
                    transitions@, x, initial, reference_index as int,
                ),
                mmr_spec::transition_prefix_valid(row_transitions@, x, width),
                row_transitions@.len() <= x as nat,
                mmr_spec::reader_valid(&reader),
                reader.bytes@ == data@,
                mmr_spec::raster_inner(
                    data@, width, row, &bitmap, traced_rows, transitions@,
                    traced_start, traced_steps, initial,
                    mmr_spec::raster_state(
                        reader.bit_position as nat, x, black,
                        row_transitions@, emitted_black,
                    ),
                ),
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

            #[cfg(verus_keep_ghost)]
            proof_decl! {
                let ghost previous_reference_index = reference_index;
                let ghost reference_x = x;
            }
            let (b1, b2) = reference_elements_from(
                transitions,
                x,
                black,
                width,
                initial,
                &mut reference_index,
            )?;
            #[cfg(verus_keep_ghost)]
            proof! {
                assert forall|previous: int| 0 <= previous < reference_index implies
                    !mmr_spec::reference_position_eligible(
                        #[trigger] transitions@[previous], x, initial,
                    ) by {
                    if previous < previous_reference_index {
                        assert(!mmr_spec::reference_position_eligible(
                            transitions@[previous], x, initial,
                        ));
                    }
                }
                assert(mmr_spec::reference_pair_matches(
                    transitions@, x, black, width, initial, 0, b1, b2,
                ));
                assert forall|previous: int| 0 <= previous < reference_index implies
                    #[trigger] transitions@[previous] <= reference_x by {}
            }
            #[cfg(verus_keep_ghost)]
            production_mmr_trace_before!(
                traced_before,
                reader,
                x,
                black,
                row_transitions,
                emitted_black
            );
            let mode_offset = reader.bit_position;
            match decode_mode(&mut reader)? {
                Mode::Extension => {
                    return Err(MmrError::ExtensionMode {
                        bit_offset: mode_offset,
                    });
                }
                Mode::Pass => {
                    #[cfg(verus_keep_ghost)]
                    production_mmr_trace_mode_end!(traced_mode_end, reader);
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
                    #[cfg(verus_keep_ghost)]
                    production_mmr_trace_step!(
                        data,
                        transitions,
                        width,
                        traced_start,
                        traced_steps,
                        traced_before,
                        Mode::Pass,
                        traced_mode_end,
                        b1,
                        b2,
                        x,
                        reader,
                        x,
                        black,
                        row_transitions,
                        emitted_black
                    );
                }
                Mode::Horizontal => {
                    #[cfg(verus_keep_ghost)]
                    production_mmr_trace_mode_end!(traced_mode_end, reader);
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
                    #[cfg(verus_keep_ghost)]
                    production_mmr_trace_step!(
                        data,
                        transitions,
                        width,
                        traced_start,
                        traced_steps,
                        traced_before,
                        Mode::Horizontal,
                        traced_mode_end,
                        b1,
                        b2,
                        after_first,
                        reader,
                        x,
                        black,
                        row_transitions,
                        emitted_black
                    );
                }
                Mode::Vertical(displacement) => {
                    #[cfg(verus_keep_ghost)]
                    production_mmr_trace_mode_end!(traced_mode_end, reader);
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
                    #[cfg(verus_keep_ghost)]
                    production_mmr_trace_step!(
                        data,
                        transitions,
                        width,
                        traced_start,
                        traced_steps,
                        traced_before,
                        Mode::Vertical(displacement),
                        traced_mode_end,
                        b1,
                        b2,
                        x,
                        reader,
                        x,
                        black,
                        row_transitions,
                        emitted_black
                    );
                }
            }
            #[cfg(verus_keep_ghost)]
            proof! {
                crate::views::bitmap_fields(&bitmap);
                reveal(crate::spec::bitmap::storage_valid);
                assert(reference_x <= x);
            }
            initial = false;
            #[cfg(verus_keep_ghost)]
            proof! {
                assert forall|previous: int| 0 <= previous < reference_index implies
                    !mmr_spec::reference_position_eligible(
                        #[trigger] transitions@[previous], x, initial,
                    ) by {
                    assert(transitions@[previous] <= reference_x);
                }
            }
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::views::bitmap_fields(&bitmap);
            reveal(crate::spec::bitmap::storage_valid);
            reveal(mmr_spec::bitmap_storage_valid);
            mmr_properties::complete_row_cache(&bitmap, row, row_transitions@, emitted_black);
        }
        #[cfg(verus_keep_ghost)]
        production_mmr_trace_row_complete!(
            traced_row,
            traced_start,
            traced_steps,
            reader,
            x,
            black,
            row_transitions,
            emitted_black
        );

        #[cfg(verus_keep_ghost)]
        proof_decl! { let ghost completed_cache = row_transitions@; }
        finish_transition_row(row_transitions, width);
        #[cfg(verus_keep_ghost)]
        proof! { mmr_properties::finished_row_cache(&bitmap, row, completed_cache, width); }
        core::mem::swap(&mut transitions, &mut row_transitions);
        #[cfg(verus_keep_ghost)]
        proof! { assert(mmr_spec::row_cache_matches(&bitmap, row, transitions@)); }
        #[cfg(verus_keep_ghost)]
        production_mmr_trace_row_push!(data, width, bitmap, traced_rows, traced_row);
        #[cfg(verus_keep_ghost)]
        production_mmr_row_termination!(row, height);
    }

    #[cfg(verus_keep_ghost)]
    production_mmr_trace_output!(data, width, height, bitmap, reader, traced_rows);
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
    ensures
    out.is_ok(),
    match out {
        Ok((b1, b2)) => x <= b1 <= b2 <= width
            && mmr_spec::reference_pair_matches(
                transitions@, x, black, width, initial, 0, b1, b2,
            ),
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
            forall|previous: int| 0 <= previous < index ==>
                !mmr_spec::reference_position_eligible(
                    #[trigger] transitions@[previous], x, initial,
                ) || (previous % 2 == 0) == black,
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
                #[cfg(verus_keep_ghost)]
                proof! {
                    assert(mmr_spec::first_reference_candidate(
                        transitions@, x, black, initial, 0, index as int,
                    ));
                    assert(mmr_spec::reference_pair_matches(
                        transitions@, x, black, width, initial, 0, position, b2,
                    ));
                }
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
        out.is_ok(),
        match out {
            Ok((b1, b2)) => x <= b1 <= b2 <= width
                && mmr_spec::reference_cursor_matches(
                    transitions@, x, initial,
                    *old(search_start) as int, *final(search_start) as int,
                )
                && mmr_spec::reference_pair_matches(
                    transitions@, x, black, width, initial,
                    *old(search_start) as int, b1, b2,
                )
                && (mmr_properties::reference_prefix_skipped(
                    transitions@, x, initial, *old(search_start) as int,
                ) ==> mmr_spec::reference_pair_matches(
                        transitions@, x, black, width, initial, 0, b1, b2,
                    )),
            Err(_) => true,
        },
))]
#[allow(clippy::too_many_lines)]
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
            *old(search_start) <= index <= search_end,
            forall|previous: int| *old(search_start) <= previous < index ==>
                !mmr_spec::reference_position_eligible(
                    #[trigger] transitions@[previous], x, initial,
                ),
        ensures
            mmr_spec::reference_cursor_matches(
                transitions@, x, initial, *old(search_start) as int, index as int,
            ),
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
            mmr_spec::reference_cursor_matches(
                transitions@, x, initial, *old(search_start) as int, *search_start as int,
            ),
            forall|previous: int| *old(search_start) <= previous < index ==>
                !mmr_spec::reference_position_eligible(
                    #[trigger] transitions@[previous], x, initial,
                ) || (previous % 2 == 0) == black,
        decreases search_end - index,
    ))]
    while index < search_end {
        let position = *transitions.get(index).ok_or(MmrError::InvalidTransition)?;
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal(mmr_spec::reference_line_valid);
            reveal(mmr_spec::transitions_ordered);
            assert(transitions@[*search_start as int] >= x);
            if *search_start < index {
                assert(transitions@[*search_start as int] <= transitions@[index as int]);
            }
        }
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
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(mmr_spec::first_reference_candidate(
                    transitions@, x, black, initial, *old(search_start) as int, index as int,
                ));
                assert(mmr_spec::reference_pair_matches(
                    transitions@, x, black, width, initial,
                    *old(search_start) as int, position, b2,
                ));
                if mmr_properties::reference_prefix_skipped(
                    transitions@, x, initial, *old(search_start) as int,
                ) {
                    mmr_properties::reference_suffix_is_global(
                        transitions@, x, black, width, initial,
                        *old(search_start) as int, position, b2,
                    );
                }
            }
            return Ok((position, b2));
        }
        index = index.checked_add(1).ok_or(MmrError::IntegerOverflow)?;
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(mmr_spec::reference_pair_matches(
            transitions@, x, black, width, initial,
            *old(search_start) as int, width, width,
        ));
        if mmr_properties::reference_prefix_skipped(
            transitions@, x, initial, *old(search_start) as int,
        ) {
            mmr_properties::reference_suffix_is_global(
                transitions@, x, black, width, initial,
                *old(search_start) as int, width, width,
            );
        }
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
                mmr_spec::span_result(old(bitmap), final(bitmap), row, start, end, black)
                && mmr_spec::span_pixels(old(bitmap), final(bitmap), row, start, end, black)),
        Err(_) => final(transitions)@ == old(transitions)@
            && *final(emitted_black) == *old(emitted_black),
    },
    crate::spec::bitmap::storage_valid(old(bitmap)) ==> out.is_ok(),
    mmr_spec::row_cache_prefix(
        old(bitmap), row, old(transitions)@, *old(emitted_black), start,
    ) ==> mmr_spec::row_cache_prefix(
        final(bitmap), row, final(transitions)@, *final(emitted_black), end,
    ),
    mmr_spec::future_rows_white(old(bitmap), row as nat + 1) ==>
        mmr_spec::future_rows_white(final(bitmap), row as nat + 1),
    crate::spec::bitmap::storage_valid(old(bitmap)) ==>
        forall|other: u32, cache: Seq<u32>|
            other != row && mmr_spec::row_cache_matches(old(bitmap), other, cache) ==>
                mmr_spec::row_cache_matches(final(bitmap), other, cache),
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
    #[cfg(verus_keep_ghost)]
    proof_decl! {
        let ghost old_cache = transitions@;
        let ghost old_emitted = *emitted_black;
    }
    fill_span(bitmap, row, start, end, black)?;
    record_transition_span(transitions, emitted_black, start, end, width, black);
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::views::bitmap_fields(old(bitmap));
        crate::views::bitmap_fields(bitmap);
        reveal(crate::spec::bitmap::storage_valid);
        if crate::spec::bitmap::storage_valid(old(bitmap)) {
            mmr_properties::span_pixel_frame(old(bitmap), bitmap, row, start, end, black);
            mmr_properties::span_row_cache_frame(old(bitmap), bitmap, row, start, end, black);
            if mmr_spec::future_rows_white(old(bitmap), row as nat + 1) {
                mmr_properties::span_keeps_future_white(
                    old(bitmap), bitmap, row, start, end, black, row as nat + 1,
                );
            }
        }
        if mmr_spec::row_cache_prefix(old(bitmap), row, old_cache, old_emitted, start) {
            mmr_properties::recorded_span_cache_pixels(
                old(bitmap), bitmap, row, start, end, black, old_cache, transitions@,
                old_emitted, *emitted_black, width,
            );
        }
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(
    requires mmr_spec::transition_prefix_valid(old(transitions)@, width, width),
    ensures
        mmr_spec::reference_line_valid(final(transitions)@, width),
        final(transitions)@.len() == old(transitions)@.len() + 2,
        final(transitions)@ == old(transitions)@.push(width).push(width),
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
                    )
                    && mmr_spec::required_eofb_prefix(
                        old(reader).bytes@, old(reader).bit_position as nat,
                        final(reader).bit_position as nat,
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
                )
                && mmr_spec::required_eofb_prefix(
                    old(reader).bytes@, old(reader).bit_position as nat,
                    final(reader).bit_position as nat,
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
mod tests;

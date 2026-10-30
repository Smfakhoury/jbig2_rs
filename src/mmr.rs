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
use vstd::std_specs::convert::FromSpecImpl;

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

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl From<MalformedError> for MmrError {
    fn from(error: MalformedError) -> Self {
        Self::Reader(error)
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
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

#[cfg(verus_keep_ghost)]
verus! {

impl FromSpecImpl<MalformedError> for MmrError {
    open spec fn obeys_from_spec() -> bool { true }

    closed spec fn from_spec(error: MalformedError) -> MmrError {
        MmrError::Reader(error)
    }
}

// Project error policy, discharged by the executing From body through the pinned trait contract.
impl FromSpecImpl<MmrError> for DecodeError {
    open spec fn obeys_from_spec() -> bool {
        true
    }

    open spec fn from_spec(error: MmrError) -> DecodeError {
        mmr_error_result(error)
    }
}

// Keep the project mapping opaque externally; the open trait bridge exposes no private fields.
pub closed spec fn mmr_error_result(error: MmrError) -> DecodeError {
    match error {
            MmrError::Reader(error) => DecodeError::Malformed(error),
            MmrError::Bitmap(error) => error,
            MmrError::Allocation { requested_bytes } => DecodeError::Allocation { requested_bytes },
            MmrError::IntegerOverflow => DecodeError::Malformed(MalformedError::IntegerOverflow),
            MmrError::InvalidMode { bit_offset } => DecodeError::Malformed(
                MalformedError::InvalidMmr(MmrErrorKind::InvalidMode { bit_offset })),
            MmrError::ExtensionMode { bit_offset } => DecodeError::Malformed(
                MalformedError::InvalidMmr(MmrErrorKind::ExtensionMode { bit_offset })),
            MmrError::InvalidRunCode { bit_offset, black } => DecodeError::Malformed(
                MalformedError::InvalidMmr(MmrErrorKind::InvalidRunCode { bit_offset, black })),
            MmrError::RunExceedsRow => DecodeError::Malformed(
                MalformedError::InvalidMmr(MmrErrorKind::RunExceedsRow)),
            MmrError::InvalidTransition => DecodeError::Malformed(
                MalformedError::InvalidMmr(MmrErrorKind::InvalidTransition)),
            MmrError::NonProgress => DecodeError::Malformed(
                MalformedError::InvalidMmr(MmrErrorKind::NonProgress)),
            MmrError::TooManyModes => DecodeError::Malformed(
                MalformedError::InvalidMmr(MmrErrorKind::TooManyModes)),
            MmrError::MissingEofb => DecodeError::Malformed(
                MalformedError::InvalidMmr(MmrErrorKind::MissingEofb)),
            MmrError::InvalidEofb => DecodeError::Malformed(
                MalformedError::InvalidMmr(MmrErrorKind::InvalidEofb)),
            MmrError::TrailingData => DecodeError::Malformed(
                MalformedError::InvalidMmr(MmrErrorKind::TrailingData)),
            MmrError::NonZeroEofbPadding => DecodeError::Malformed(
                MalformedError::InvalidMmr(MmrErrorKind::NonZeroPadding)),
    }
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

mod codes;
mod memory;
mod raster;
mod reference;
mod spans;
mod tables;
mod termination;

#[cfg(test)]
use spans::collect_transitions;
// Private aliases also serve cfg-gated proof and test callers.
#[allow(unused_imports)]
use self::{
    codes::{
        append_code_bit, decode_horizontal_runs, decode_mode, decode_run, decode_run_code,
        find_run_code,
    },
    memory::{
        enforce_input_limit, enforce_work, observed_transition_bytes, prepare_transition_storage,
    },
    raster::decode_bitmap,
    reference::{reference_elements, reference_elements_from},
    spans::{fill_recorded_span, fill_span, finish_transition_row, record_transition_span},
    tables::{
        BLACK_MAKEUP, BLACK_TERMINATING, COMMON_MAKEUP, MODE_CODES, WHITE_MAKEUP, WHITE_TERMINATING,
    },
    termination::{finish_block, finish_required_prefix, remaining_bits},
};

#[cfg(test)]
mod tests;

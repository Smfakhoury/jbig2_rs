//! Adaptive binary arithmetic decoding for JBIG2.
//!
//! This module implements the normative decoder in T.88 Annex E, the
//! arithmetic integer procedures in Annex A, and strict bounded input
//! handling. The Annex G complemented-code-register convention is not used:
//! it is output-equivalent but non-normative, while the Annex E convention is
//! easier to audit directly against the standard.

use core::fmt;

use crate::{DecodeError, MalformedError};
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg(verus_keep_ghost)]
#[path = "../specs/arithmetic.rs"]
mod arithmetic_spec;

#[cfg(verus_keep_ghost)]
#[macro_use]
#[path = "../proofs/production_impl/arithmetic.rs"]
mod arithmetic_proofs;

#[cfg(verus_keep_ghost)]
#[path = "../proofs/shared/arithmetic.rs"]
mod arithmetic_shared;

#[cfg(verus_keep_ghost)]
#[path = "../proofs/spec_properties/arithmetic.rs"]
mod arithmetic_properties;

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) const MIN_INTERVAL: u32 = 0x8000;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) const INTEGER_CONTEXT_COUNT: usize = 512;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) const MAX_IAID_CODE_LENGTH: u8 = 31;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
struct QeState {
    qe: u16,
    next_mps: u8,
    next_lps: u8,
    switch_mps: bool,
}

// T.88, Table E.1. Keep the order identical to the probability-state index.
#[cfg_attr(verus_keep_ghost, verus_verify)]
const QE_STATES: [QeState; 47] = [
    QeState {
        qe: 0x5601,
        next_mps: 1,
        next_lps: 1,
        switch_mps: true,
    },
    QeState {
        qe: 0x3401,
        next_mps: 2,
        next_lps: 6,
        switch_mps: false,
    },
    QeState {
        qe: 0x1801,
        next_mps: 3,
        next_lps: 9,
        switch_mps: false,
    },
    QeState {
        qe: 0x0ac1,
        next_mps: 4,
        next_lps: 12,
        switch_mps: false,
    },
    QeState {
        qe: 0x0521,
        next_mps: 5,
        next_lps: 29,
        switch_mps: false,
    },
    QeState {
        qe: 0x0221,
        next_mps: 38,
        next_lps: 33,
        switch_mps: false,
    },
    QeState {
        qe: 0x5601,
        next_mps: 7,
        next_lps: 6,
        switch_mps: true,
    },
    QeState {
        qe: 0x5401,
        next_mps: 8,
        next_lps: 14,
        switch_mps: false,
    },
    QeState {
        qe: 0x4801,
        next_mps: 9,
        next_lps: 14,
        switch_mps: false,
    },
    QeState {
        qe: 0x3801,
        next_mps: 10,
        next_lps: 14,
        switch_mps: false,
    },
    QeState {
        qe: 0x3001,
        next_mps: 11,
        next_lps: 17,
        switch_mps: false,
    },
    QeState {
        qe: 0x2401,
        next_mps: 12,
        next_lps: 18,
        switch_mps: false,
    },
    QeState {
        qe: 0x1c01,
        next_mps: 13,
        next_lps: 20,
        switch_mps: false,
    },
    QeState {
        qe: 0x1601,
        next_mps: 29,
        next_lps: 21,
        switch_mps: false,
    },
    QeState {
        qe: 0x5601,
        next_mps: 15,
        next_lps: 14,
        switch_mps: true,
    },
    QeState {
        qe: 0x5401,
        next_mps: 16,
        next_lps: 14,
        switch_mps: false,
    },
    QeState {
        qe: 0x5101,
        next_mps: 17,
        next_lps: 15,
        switch_mps: false,
    },
    QeState {
        qe: 0x4801,
        next_mps: 18,
        next_lps: 16,
        switch_mps: false,
    },
    QeState {
        qe: 0x3801,
        next_mps: 19,
        next_lps: 17,
        switch_mps: false,
    },
    QeState {
        qe: 0x3401,
        next_mps: 20,
        next_lps: 18,
        switch_mps: false,
    },
    QeState {
        qe: 0x3001,
        next_mps: 21,
        next_lps: 19,
        switch_mps: false,
    },
    QeState {
        qe: 0x2801,
        next_mps: 22,
        next_lps: 19,
        switch_mps: false,
    },
    QeState {
        qe: 0x2401,
        next_mps: 23,
        next_lps: 20,
        switch_mps: false,
    },
    QeState {
        qe: 0x2201,
        next_mps: 24,
        next_lps: 21,
        switch_mps: false,
    },
    QeState {
        qe: 0x1c01,
        next_mps: 25,
        next_lps: 22,
        switch_mps: false,
    },
    QeState {
        qe: 0x1801,
        next_mps: 26,
        next_lps: 23,
        switch_mps: false,
    },
    QeState {
        qe: 0x1601,
        next_mps: 27,
        next_lps: 24,
        switch_mps: false,
    },
    QeState {
        qe: 0x1401,
        next_mps: 28,
        next_lps: 25,
        switch_mps: false,
    },
    QeState {
        qe: 0x1201,
        next_mps: 29,
        next_lps: 26,
        switch_mps: false,
    },
    QeState {
        qe: 0x1101,
        next_mps: 30,
        next_lps: 27,
        switch_mps: false,
    },
    QeState {
        qe: 0x0ac1,
        next_mps: 31,
        next_lps: 28,
        switch_mps: false,
    },
    QeState {
        qe: 0x09c1,
        next_mps: 32,
        next_lps: 29,
        switch_mps: false,
    },
    QeState {
        qe: 0x08a1,
        next_mps: 33,
        next_lps: 30,
        switch_mps: false,
    },
    QeState {
        qe: 0x0521,
        next_mps: 34,
        next_lps: 31,
        switch_mps: false,
    },
    QeState {
        qe: 0x0441,
        next_mps: 35,
        next_lps: 32,
        switch_mps: false,
    },
    QeState {
        qe: 0x02a1,
        next_mps: 36,
        next_lps: 33,
        switch_mps: false,
    },
    QeState {
        qe: 0x0221,
        next_mps: 37,
        next_lps: 34,
        switch_mps: false,
    },
    QeState {
        qe: 0x0141,
        next_mps: 38,
        next_lps: 35,
        switch_mps: false,
    },
    QeState {
        qe: 0x0111,
        next_mps: 39,
        next_lps: 36,
        switch_mps: false,
    },
    QeState {
        qe: 0x0085,
        next_mps: 40,
        next_lps: 37,
        switch_mps: false,
    },
    QeState {
        qe: 0x0049,
        next_mps: 41,
        next_lps: 38,
        switch_mps: false,
    },
    QeState {
        qe: 0x0025,
        next_mps: 42,
        next_lps: 39,
        switch_mps: false,
    },
    QeState {
        qe: 0x0015,
        next_mps: 43,
        next_lps: 40,
        switch_mps: false,
    },
    QeState {
        qe: 0x0009,
        next_mps: 44,
        next_lps: 41,
        switch_mps: false,
    },
    QeState {
        qe: 0x0005,
        next_mps: 45,
        next_lps: 42,
        switch_mps: false,
    },
    QeState {
        qe: 0x0001,
        next_mps: 45,
        next_lps: 43,
        switch_mps: false,
    },
    QeState {
        qe: 0x5601,
        next_mps: 46,
        next_lps: 46,
        switch_mps: false,
    },
];

/// An arithmetic probability context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct ArithmeticContext {
    pub(crate) state_index: u8,
    pub(crate) mps: u8,
}

impl ArithmeticContext {
    /// Returns a context in the default `I = 0, MPS = 0` state.
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures
            out.state_index == 0,
            out.mps == 0,
            arithmetic_spec::context_valid(out),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) const fn new() -> Self {
        Self {
            state_index: 0,
            mps: 0,
        }
    }

    /// Constructs a validated context, for restoring saved statistics.
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures match out {
            Ok(context) =>
                state_index < 47
                && mps <= 1
                && context.state_index == state_index
                && context.mps == mps,
            Err(_) => state_index >= 47 || mps > 1,
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn from_state(state_index: u8, mps: u8) -> Result<Self, ArithmeticError> {
        if state_index >= 47 || mps > 1 {
            return Err(ArithmeticError::InvalidContextState { state_index, mps });
        }
        Ok(Self { state_index, mps })
    }

    /// Restores the default probability state.
    #[cfg_attr(verus_keep_ghost, verus_spec(
        ensures
            final(self).state_index == 0,
            final(self).mps == 0,
            arithmetic_spec::context_valid(*final(self)),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn reset(&mut self) {
        *self = Self::new();
    }

    /// Returns the Table E.1 state index.
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == self.state_index,
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) const fn state_index(self) -> u8 {
        self.state_index
    }

    /// Returns the current more-probable symbol.
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == self.mps,
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) const fn mps(self) -> u8 {
        self.mps
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl Default for ArithmeticContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Resets every context in a statistics set.
#[cfg_attr(verus_keep_ghost, verus_spec(
    ensures forall|index: int| 0 <= index < final(contexts)@.len() ==>
        arithmetic_spec::context_valid(final(contexts)@[index]),
))]
pub(crate) fn reset_contexts(contexts: &mut [ArithmeticContext]) {
    let mut index = 0;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            0 <= index <= contexts@.len(),
            contexts@.len() == old(contexts)@.len(),
            forall|completed: int| 0 <= completed < index ==>
                arithmetic_spec::context_valid(contexts@[completed]),
        decreases contexts@.len() - index,
    ))]
    while index < contexts.len() {
        contexts[index] = ArithmeticContext::new();
        index += 1;
    }
}

/// A marker observed without consuming its two bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct ArithmeticMarker {
    /// Offset of the `0xff` marker prefix.
    pub(crate) offset: usize,
    /// Marker byte following the prefix.
    pub(crate) code: u8,
}

/// Arithmetic decoding failure.
#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) enum ArithmeticError {
    /// Existing crate-wide malformed-input classification.
    Malformed(MalformedError),
    /// A restored arithmetic context was outside Table E.1.
    InvalidContextState { state_index: u8, mps: u8 },
    /// Too few probability contexts were supplied.
    InsufficientContexts { required: usize, actual: usize },
    /// IAID cannot safely represent or index this code length.
    InvalidIaidCodeLength(u8),
}

impl fmt::Display for ArithmeticError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(error) => error.fmt(formatter),
            Self::InvalidContextState { state_index, mps } => write!(
                formatter,
                "invalid arithmetic context state I={state_index}, MPS={mps}"
            ),
            Self::InsufficientContexts { required, actual } => write!(
                formatter,
                "arithmetic decoding requires {required} contexts, but {actual} were supplied"
            ),
            Self::InvalidIaidCodeLength(length) => {
                write!(formatter, "unsupported IAID code length {length}")
            }
        }
    }
}

impl std::error::Error for ArithmeticError {}

impl From<MalformedError> for ArithmeticError {
    fn from(error: MalformedError) -> Self {
        Self::Malformed(error)
    }
}

impl From<ArithmeticError> for DecodeError {
    fn from(error: ArithmeticError) -> Self {
        match error {
            ArithmeticError::Malformed(error) => Self::Malformed(error),
            ArithmeticError::InvalidContextState { state_index, mps } => {
                Self::Malformed(MalformedError::InvalidArithmeticContextState { state_index, mps })
            }
            ArithmeticError::InsufficientContexts { required, actual } => {
                Self::Malformed(MalformedError::InsufficientArithmeticContexts { required, actual })
            }
            ArithmeticError::InvalidIaidCodeLength(length) => {
                Self::Malformed(MalformedError::InvalidIaidCodeLength(length))
            }
        }
    }
}

/// Result of an Annex A arithmetic integer decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) enum ArithmeticInteger {
    /// A signed integer value.
    Value(i64),
    /// The special negative-zero out-of-band code.
    OutOfBand,
}

/// A bounded Annex E arithmetic decoder.
#[derive(Debug)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct ArithmeticDecoder<'a> {
    pub(crate) bytes: &'a [u8],
    pub(crate) byte_position: usize,
    pub(crate) current_byte: u8,
    pub(crate) code: u32,
    pub(crate) interval: u32,
    pub(crate) bit_count: u8,
    pub(crate) marker: Option<ArithmeticMarker>,
}

impl<'a> ArithmeticDecoder<'a> {
    /// Initializes the Annex E decoder at the start of one arithmetic stream.
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures match out {
            Ok(decoder) => arithmetic_spec::decoder_valid(&decoder),
            Err(_) => true,
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn new(bytes: &'a [u8]) -> Result<Self, ArithmeticError> {
        #[cfg(verus_keep_ghost)]
        production_reveal_decoder_valid!();
        if bytes.is_empty() {
            return Err(unexpected_eof(0, 1, 0));
        }
        let first = bytes[0];
        let mut decoder = Self {
            bytes,
            byte_position: 0,
            current_byte: first,
            code: u32::from(first) << 16,
            interval: MIN_INTERVAL,
            bit_count: 0,
            marker: None,
        };
        decoder.byte_in()?;
        decoder.code = decoder
            .code
            .checked_mul(1 << 7)
            .ok_or(MalformedError::IntegerOverflow)?;
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(decoder.interval == MIN_INTERVAL);
            assert(decoder.bit_count >= 7);
            assert(decoder.bit_count <= 8);
        }
        decoder.bit_count = decoder
            .bit_count
            .checked_sub(7)
            .ok_or(MalformedError::IntegerOverflow)?;
        Ok(decoder)
    }

    /// Decodes one binary decision and updates its probability context.
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            arithmetic_spec::decoder_valid(old(self)),
            arithmetic_spec::context_valid(*old(context)),
        ensures
            arithmetic_spec::decoder_state_valid(final(self)),
            arithmetic_spec::context_valid(*final(context)),
            match out {
                Ok(decision) =>
                    decision <= 1
                    && arithmetic_spec::decoder_valid(final(self)),
                Err(_) => true,
            },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn decode(
        &mut self,
        context: &mut ArithmeticContext,
    ) -> Result<u8, ArithmeticError> {
        #[cfg(verus_keep_ghost)]
        production_reveal_decoder_valid!();
        let state = QE_STATES[usize::from(context.state_index)];
        #[cfg(verus_keep_ghost)]
        production_qe_transition_valid!(context.state_index, state);
        let qe = u32::from(state.qe);
        let old_mps = context.mps;

        #[cfg(verus_keep_ghost)]
        production_decode_interval_subtraction!(self, qe);
        self.interval = self
            .interval
            .checked_sub(qe)
            .ok_or(MalformedError::IntegerOverflow)?;

        if self.code >> 16 < qe {
            let decision;
            if self.interval < qe {
                decision = old_mps;
                context.state_index = state.next_mps;
            } else {
                decision = 1 - old_mps;
                context.state_index = state.next_lps;
                if state.switch_mps {
                    context.mps = decision;
                }
            }
            self.interval = qe;
            self.renormalize()?;
            return Ok(decision);
        }

        self.code = self
            .code
            .checked_sub(qe << 16)
            .ok_or(MalformedError::IntegerOverflow)?;
        if self.interval >= MIN_INTERVAL {
            return Ok(old_mps);
        }

        let decision;
        if self.interval < qe {
            decision = 1 - old_mps;
            context.state_index = state.next_lps;
            if state.switch_mps {
                context.mps = decision;
            }
        } else {
            decision = old_mps;
            context.state_index = state.next_mps;
        }
        self.renormalize()?;
        Ok(decision)
    }

    /// Decodes one Annex A integer using exactly 512 procedure-specific contexts.
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            arithmetic_spec::decoder_valid(old(self)),
            forall|index: int| 0 <= index < old(contexts)@.len() ==>
                arithmetic_spec::context_valid(old(contexts)@[index]),
        ensures
            arithmetic_spec::decoder_state_valid(final(self)),
            forall|index: int| 0 <= index < final(contexts)@.len() ==>
                arithmetic_spec::context_valid(final(contexts)@[index]),
            match out {
                Ok(_) => arithmetic_spec::decoder_valid(final(self)),
                Err(_) => true,
            },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    #[allow(clippy::cast_possible_wrap)]
    pub(crate) fn decode_integer(
        &mut self,
        contexts: &mut [ArithmeticContext; INTEGER_CONTEXT_COUNT],
    ) -> Result<ArithmeticInteger, ArithmeticError> {
        let mut previous = 1_u16;
        let sign = self.decode_integer_bit(&mut previous, contexts)?;

        let (bit_count, offset) = if self.decode_integer_bit(&mut previous, contexts)? == 0 {
            (2, 0_u64)
        } else if self.decode_integer_bit(&mut previous, contexts)? == 0 {
            (4, 4)
        } else if self.decode_integer_bit(&mut previous, contexts)? == 0 {
            (6, 20)
        } else if self.decode_integer_bit(&mut previous, contexts)? == 0 {
            (8, 84)
        } else if self.decode_integer_bit(&mut previous, contexts)? == 0 {
            (12, 340)
        } else {
            (32, 4436)
        };

        let magnitude_bits = self.decode_integer_bits(&mut previous, bit_count, contexts)?;
        let magnitude = offset
            .checked_add(u64::from(magnitude_bits))
            .ok_or(MalformedError::IntegerOverflow)?;
        if sign == 1 && magnitude == 0 {
            return Ok(ArithmeticInteger::OutOfBand);
        }

        #[cfg(verus_keep_ghost)]
        production_integer_magnitude!(magnitude_bits, offset);
        let magnitude = magnitude as i64;
        if sign == 0 {
            Ok(ArithmeticInteger::Value(magnitude))
        } else {
            let negative = -magnitude;
            Ok(ArithmeticInteger::Value(negative))
        }
    }

    /// Decodes one fixed-length Annex A IAID value.
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            arithmetic_spec::decoder_valid(old(self)),
            forall|index: int| 0 <= index < old(contexts)@.len() ==>
                arithmetic_spec::context_valid(old(contexts)@[index]),
        ensures
            arithmetic_spec::decoder_state_valid(final(self)),
            forall|index: int| 0 <= index < final(contexts)@.len() ==>
                arithmetic_spec::context_valid(final(contexts)@[index]),
            match out {
                Ok(_) => arithmetic_spec::decoder_valid(final(self)),
                Err(_) => true,
            },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn decode_iaid(
        &mut self,
        code_length: u8,
        contexts: &mut [ArithmeticContext],
    ) -> Result<u32, ArithmeticError> {
        if code_length > MAX_IAID_CODE_LENGTH {
            return Err(ArithmeticError::InvalidIaidCodeLength(code_length));
        }
        let required = 1_usize << code_length;
        let context_count = contexts.len();
        if context_count < required {
            return Err(ArithmeticError::InsufficientContexts {
                required,
                actual: context_count,
            });
        }

        let mut previous = 1_usize;
        let mut remaining = code_length;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                arithmetic_spec::decoder_valid(self),
                contexts@.len() == context_count,
                forall|index: int| 0 <= index < contexts@.len() ==>
                    arithmetic_spec::context_valid(contexts@[index]),
                remaining <= code_length,
                required <= context_count,
            decreases remaining,
        ))]
        while remaining != 0 {
            if previous >= context_count {
                return Err(ArithmeticError::InsufficientContexts {
                    required,
                    actual: context_count,
                });
            }
            let decision = self.decode(&mut contexts[previous])?;
            previous = previous
                .checked_mul(2)
                .and_then(|partial| partial.checked_add(usize::from(decision)))
                .ok_or(MalformedError::IntegerOverflow)?;
            remaining -= 1;
        }
        let value = previous
            .checked_sub(required)
            .ok_or(MalformedError::IntegerOverflow)?;
        if value > u32::MAX as usize {
            return Err(MalformedError::IntegerOverflow.into());
        }
        Ok(value as u32)
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            arithmetic_spec::decoder_valid(old(self)),
            *old(previous) <= 511,
            forall|index: int| 0 <= index < old(contexts)@.len() ==>
                arithmetic_spec::context_valid(old(contexts)@[index]),
        ensures
            arithmetic_spec::decoder_state_valid(final(self)),
            *final(previous) <= 511,
            forall|index: int| 0 <= index < final(contexts)@.len() ==>
                arithmetic_spec::context_valid(final(contexts)@[index]),
            match out {
                Ok(decision) =>
                    decision <= 1
                    && arithmetic_spec::decoder_valid(final(self)),
                Err(_) => true,
            },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    fn decode_integer_bit(
        &mut self,
        previous: &mut u16,
        contexts: &mut [ArithmeticContext; INTEGER_CONTEXT_COUNT],
    ) -> Result<u8, ArithmeticError> {
        let prior = *previous;
        let context_index = usize::from(prior & 0x01ff);
        #[cfg(verus_keep_ghost)]
        production_integer_history_index!(prior);
        let decision = self.decode(&mut contexts[context_index])?;
        let shifted = prior * 2 + u16::from(decision);
        *previous = if prior < 256 {
            shifted
        } else {
            (shifted & 511) | 256
        };
        #[cfg(verus_keep_ghost)]
        proof! {
            if prior < 256 {
                assert(prior <= 255);
                assert(decision <= 1);
                assert(shifted == prior * 2 + decision as u16);
                assert(shifted <= 511);
            } else {
                assert((shifted & 511) | 256 <= 511) by (bit_vector);
            }
        }
        Ok(decision)
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            arithmetic_spec::decoder_valid(old(self)),
            *old(previous) <= 511,
            bit_count <= 32,
            forall|index: int| 0 <= index < old(contexts)@.len() ==>
                arithmetic_spec::context_valid(old(contexts)@[index]),
        ensures
            arithmetic_spec::decoder_state_valid(final(self)),
            *final(previous) <= 511,
            forall|index: int| 0 <= index < final(contexts)@.len() ==>
                arithmetic_spec::context_valid(final(contexts)@[index]),
            match out {
                Ok(_) => arithmetic_spec::decoder_valid(final(self)),
                Err(_) => true,
            },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    fn decode_integer_bits(
        &mut self,
        previous: &mut u16,
        bit_count: u8,
        contexts: &mut [ArithmeticContext; INTEGER_CONTEXT_COUNT],
    ) -> Result<u32, ArithmeticError> {
        let mut value = 0_u32;
        let mut remaining = bit_count;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                arithmetic_spec::decoder_valid(self),
                *previous <= 511,
                forall|index: int| 0 <= index < contexts@.len() ==>
                    arithmetic_spec::context_valid(contexts@[index]),
                remaining <= bit_count,
            decreases remaining,
        ))]
        while remaining != 0 {
            let bit = self.decode_integer_bit(previous, contexts)?;
            value = value
                .checked_mul(2)
                .and_then(|partial| partial.checked_add(u32::from(bit)))
                .ok_or(MalformedError::IntegerOverflow)?;
            remaining -= 1;
        }
        Ok(value)
    }

    /// Returns a recognized marker without consuming it.
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == arithmetic_spec::decoder_marker(self),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) const fn marker(&self) -> Option<ArithmeticMarker> {
        self.marker
    }

    /// Returns the offset of the current byte.
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == arithmetic_spec::decoder_byte_position(self),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) const fn byte_position(&self) -> usize {
        self.byte_position
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires arithmetic_spec::decoder_state_valid(old(self)),
        ensures
            arithmetic_spec::decoder_state_valid(final(self)),
            match out {
                Ok(()) => arithmetic_spec::decoder_valid(final(self)),
                Err(_) => true,
            },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    fn renormalize(&mut self) -> Result<(), ArithmeticError> {
        #[cfg(verus_keep_ghost)]
        production_reveal_decoder_valid!();
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant arithmetic_spec::decoder_state_valid(self),
            decreases arithmetic_spec::renormalization_steps(
                self.interval as nat,
            ),
        ))]
        while self.interval < MIN_INTERVAL {
            #[cfg(verus_keep_ghost)]
            proof_decl! {
                let ghost previous_interval = self.interval;
            }
            if self.bit_count == 0 {
                self.byte_in()?;
            }
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(self.interval < 0x8000u32);
                assert(self.interval <= u32::MAX / 2);
                assert(self.interval * 2 <= 0xffffu32);
                assert(self.bit_count > 0);
                assert(self.bit_count <= 8);
                assert(
                    MIN_INTERVAL - self.interval * 2
                        < MIN_INTERVAL - self.interval
                );
            }
            self.interval = self
                .interval
                .checked_mul(2)
                .ok_or(MalformedError::IntegerOverflow)?;
            self.code = self
                .code
                .checked_mul(2)
                .ok_or(MalformedError::IntegerOverflow)?;
            self.bit_count = self
                .bit_count
                .checked_sub(1)
                .ok_or(MalformedError::IntegerOverflow)?;
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(self.interval == previous_interval * 2);
                reveal(arithmetic_spec::renormalization_steps);
                assert(
                    arithmetic_spec::renormalization_steps(
                        self.interval as nat,
                    ) < arithmetic_spec::renormalization_steps(
                        previous_interval as nat,
                    )
                );
            }
        }
        Ok(())
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires arithmetic_spec::decoder_state_valid(old(self)),
        ensures
            arithmetic_spec::decoder_state_valid(final(self)),
            final(self).bytes@ == old(self).bytes@,
            final(self).interval == old(self).interval,
            final(self).byte_position == old(self).byte_position
                || final(self).byte_position == old(self).byte_position + 1,
            match out {
                Ok(()) =>
                    old(self).byte_position + 1 < old(self).bytes@.len()
                    && final(self).bit_count >= 7
                    && (
                        old(self).marker.is_none()
                            && final(self).marker.is_some()
                        ==> final(self).byte_position
                            == old(self).byte_position
                    ),
                Err(_) => true,
            },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    fn byte_in(&mut self) -> Result<(), ArithmeticError> {
        #[cfg(verus_keep_ghost)]
        production_reveal_decoder_valid!();
        let next_position = self
            .byte_position
            .checked_add(1)
            .ok_or(MalformedError::IntegerOverflow)?;
        if next_position >= self.bytes.len() {
            return Err(unexpected_eof(next_position, 1, 0));
        }
        let next_byte = self.bytes[next_position];

        if self.current_byte == 0xff {
            if next_byte > 0x8f {
                self.code = self
                    .code
                    .checked_add(0xff00)
                    .ok_or(MalformedError::IntegerOverflow)?;
                self.bit_count = 8;
                self.marker = Some(ArithmeticMarker {
                    offset: self.byte_position,
                    code: next_byte,
                });
                return Ok(());
            }

            self.byte_position = next_position;
            self.current_byte = next_byte;
            self.code = self
                .code
                .checked_add(u32::from(next_byte) << 9)
                .ok_or(MalformedError::IntegerOverflow)?;
            self.bit_count = 7;
            return Ok(());
        }

        self.byte_position = next_position;
        self.current_byte = next_byte;
        self.code = self
            .code
            .checked_add(u32::from(next_byte) << 8)
            .ok_or(MalformedError::IntegerOverflow)?;
        self.bit_count = 8;
        Ok(())
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn unexpected_eof(offset: usize, needed: usize, remaining: usize) -> ArithmeticError {
    let Ok(offset) = u64::try_from(offset) else {
        return ArithmeticError::Malformed(MalformedError::IntegerOverflow);
    };
    let Ok(needed) = u64::try_from(needed) else {
        return ArithmeticError::Malformed(MalformedError::IntegerOverflow);
    };
    let Ok(remaining) = u64::try_from(remaining) else {
        return ArithmeticError::Malformed(MalformedError::IntegerOverflow);
    };
    ArithmeticError::Malformed(MalformedError::UnexpectedEof {
        offset,
        needed,
        remaining,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANNEX_H_DECISIONS: [u8; 32] = [
        0x00, 0x02, 0x00, 0x51, 0x00, 0x00, 0x00, 0xc0, 0x03, 0x52, 0x87, 0x2a, 0xaa, 0xaa, 0xaa,
        0xaa, 0x82, 0xc0, 0x20, 0x00, 0xfc, 0xd7, 0x9e, 0xf6, 0xbf, 0x7f, 0xed, 0x90, 0x4f, 0x46,
        0xa3, 0xbf,
    ];
    const ANNEX_H_ENCODED: [u8; 30] = [
        0x84, 0xc7, 0x3b, 0xfc, 0xe1, 0xa1, 0x43, 0x04, 0x02, 0x20, 0x00, 0x00, 0x41, 0x0d, 0xbb,
        0x86, 0xf4, 0x31, 0x7f, 0xff, 0x88, 0xff, 0x37, 0x47, 0x1a, 0xdb, 0x6a, 0xdf, 0xff, 0xac,
    ];

    fn decode_integer_with<F>(mut decode: F) -> Result<ArithmeticInteger, ArithmeticError>
    where
        F: FnMut(usize) -> Result<u8, ArithmeticError>,
    {
        let mut previous = 1_u16;
        let sign = integer_bit(&mut previous, &mut decode)?;

        let (bit_count, offset) = if integer_bit(&mut previous, &mut decode)? == 0 {
            (2, 0_u64)
        } else if integer_bit(&mut previous, &mut decode)? == 0 {
            (4, 4)
        } else if integer_bit(&mut previous, &mut decode)? == 0 {
            (6, 20)
        } else if integer_bit(&mut previous, &mut decode)? == 0 {
            (8, 84)
        } else if integer_bit(&mut previous, &mut decode)? == 0 {
            (12, 340)
        } else {
            (32, 4436)
        };

        let mut magnitude_bits = 0_u32;
        for _ in 0..bit_count {
            let bit = integer_bit(&mut previous, &mut decode)?;
            magnitude_bits = magnitude_bits
                .checked_mul(2)
                .and_then(|partial| partial.checked_add(u32::from(bit)))
                .ok_or(MalformedError::IntegerOverflow)?;
        }
        let magnitude = offset
            .checked_add(u64::from(magnitude_bits))
            .ok_or(MalformedError::IntegerOverflow)?;
        if sign == 1 && magnitude == 0 {
            return Ok(ArithmeticInteger::OutOfBand);
        }

        let magnitude = i64::try_from(magnitude).map_err(|_| MalformedError::IntegerOverflow)?;
        if sign == 0 {
            Ok(ArithmeticInteger::Value(magnitude))
        } else {
            Ok(ArithmeticInteger::Value(
                magnitude
                    .checked_neg()
                    .ok_or(MalformedError::IntegerOverflow)?,
            ))
        }
    }

    fn integer_bit<F>(previous: &mut u16, decode: &mut F) -> Result<u8, ArithmeticError>
    where
        F: FnMut(usize) -> Result<u8, ArithmeticError>,
    {
        let context_index = usize::from(*previous & 0x01ff);
        let decision = decode(context_index)?;
        let shifted = (*previous << 1) | u16::from(decision);
        *previous = if *previous < 256 {
            shifted
        } else {
            (shifted & 511) | 256
        };
        Ok(decision)
    }

    fn decode_iaid_with<F>(
        code_length: u8,
        context_count: usize,
        mut decode: F,
    ) -> Result<u32, ArithmeticError>
    where
        F: FnMut(usize) -> Result<u8, ArithmeticError>,
    {
        if code_length > MAX_IAID_CODE_LENGTH {
            return Err(ArithmeticError::InvalidIaidCodeLength(code_length));
        }
        let required = 1_usize
            .checked_shl(u32::from(code_length))
            .ok_or(ArithmeticError::InvalidIaidCodeLength(code_length))?;
        if context_count < required {
            return Err(ArithmeticError::InsufficientContexts {
                required,
                actual: context_count,
            });
        }

        let mut previous = 1_usize;
        for _ in 0..code_length {
            let decision = decode(previous)?;
            previous = previous
                .checked_mul(2)
                .and_then(|partial| partial.checked_add(usize::from(decision)))
                .ok_or(MalformedError::IntegerOverflow)?;
        }
        let value = previous
            .checked_sub(required)
            .ok_or(MalformedError::IntegerOverflow)?;
        u32::try_from(value)
            .map_err(|_| ArithmeticError::Malformed(MalformedError::IntegerOverflow))
    }

    #[test]
    fn qe_table_matches_annex_e_table_e_1() {
        let expected = [
            (0x5601, 1, 1, true),
            (0x3401, 2, 6, false),
            (0x1801, 3, 9, false),
            (0x0ac1, 4, 12, false),
            (0x0521, 5, 29, false),
            (0x0221, 38, 33, false),
            (0x5601, 7, 6, true),
            (0x5401, 8, 14, false),
            (0x4801, 9, 14, false),
            (0x3801, 10, 14, false),
            (0x3001, 11, 17, false),
            (0x2401, 12, 18, false),
            (0x1c01, 13, 20, false),
            (0x1601, 29, 21, false),
            (0x5601, 15, 14, true),
            (0x5401, 16, 14, false),
            (0x5101, 17, 15, false),
            (0x4801, 18, 16, false),
            (0x3801, 19, 17, false),
            (0x3401, 20, 18, false),
            (0x3001, 21, 19, false),
            (0x2801, 22, 19, false),
            (0x2401, 23, 20, false),
            (0x2201, 24, 21, false),
            (0x1c01, 25, 22, false),
            (0x1801, 26, 23, false),
            (0x1601, 27, 24, false),
            (0x1401, 28, 25, false),
            (0x1201, 29, 26, false),
            (0x1101, 30, 27, false),
            (0x0ac1, 31, 28, false),
            (0x09c1, 32, 29, false),
            (0x08a1, 33, 30, false),
            (0x0521, 34, 31, false),
            (0x0441, 35, 32, false),
            (0x02a1, 36, 33, false),
            (0x0221, 37, 34, false),
            (0x0141, 38, 35, false),
            (0x0111, 39, 36, false),
            (0x0085, 40, 37, false),
            (0x0049, 41, 38, false),
            (0x0025, 42, 39, false),
            (0x0015, 43, 40, false),
            (0x0009, 44, 41, false),
            (0x0005, 45, 42, false),
            (0x0001, 45, 43, false),
            (0x5601, 46, 46, false),
        ];
        let actual =
            QE_STATES.map(|state| (state.qe, state.next_mps, state.next_lps, state.switch_mps));
        assert_eq!(actual, expected);
    }

    #[test]
    fn annex_h_decodes_all_256_decisions() {
        let mut decoder = ArithmeticDecoder::new(&ANNEX_H_ENCODED).expect("valid initialization");
        let mut context = ArithmeticContext::new();

        for (decision_index, expected) in ANNEX_H_DECISIONS
            .iter()
            .flat_map(|byte| (0..8).rev().map(move |shift| (byte >> shift) & 1))
            .enumerate()
        {
            let actual = decoder
                .decode(&mut context)
                .expect("valid Annex H decision");
            assert_eq!(actual, expected, "decision {}", decision_index + 1);
        }

        assert_eq!(
            decoder.marker(),
            Some(ArithmeticMarker {
                offset: 28,
                code: 0xac,
            })
        );
        assert_eq!(decoder.byte_position(), 28);
        assert_eq!(
            context,
            ArithmeticContext::from_state(18, 1).expect("valid state")
        );
    }

    #[test]
    fn concrete_integer_and_iaid_match_callback_reference_procedures() {
        let mut integer_decoder =
            ArithmeticDecoder::new(&ANNEX_H_ENCODED).expect("integer decoder");
        let mut reference_integer_decoder =
            ArithmeticDecoder::new(&ANNEX_H_ENCODED).expect("reference integer decoder");
        let mut integer_contexts = [ArithmeticContext::new(); INTEGER_CONTEXT_COUNT];
        let mut reference_integer_contexts = integer_contexts;

        let actual_integer = integer_decoder
            .decode_integer(&mut integer_contexts)
            .expect("concrete integer");
        let expected_integer = decode_integer_with(|context_index| {
            reference_integer_decoder.decode(&mut reference_integer_contexts[context_index])
        })
        .expect("reference integer");
        assert_eq!(actual_integer, expected_integer);
        assert_eq!(integer_contexts, reference_integer_contexts);
        assert_eq!(
            (
                integer_decoder.byte_position,
                integer_decoder.current_byte,
                integer_decoder.code,
                integer_decoder.interval,
                integer_decoder.bit_count,
                integer_decoder.marker,
            ),
            (
                reference_integer_decoder.byte_position,
                reference_integer_decoder.current_byte,
                reference_integer_decoder.code,
                reference_integer_decoder.interval,
                reference_integer_decoder.bit_count,
                reference_integer_decoder.marker,
            )
        );

        let mut iaid_decoder = ArithmeticDecoder::new(&ANNEX_H_ENCODED).expect("IAID decoder");
        let mut reference_iaid_decoder =
            ArithmeticDecoder::new(&ANNEX_H_ENCODED).expect("reference IAID decoder");
        let mut iaid_contexts = [ArithmeticContext::new(); 8];
        let mut reference_iaid_contexts = iaid_contexts;

        let actual_iaid = iaid_decoder
            .decode_iaid(3, &mut iaid_contexts)
            .expect("concrete IAID");
        let expected_iaid = decode_iaid_with(3, reference_iaid_contexts.len(), |context_index| {
            reference_iaid_decoder.decode(&mut reference_iaid_contexts[context_index])
        })
        .expect("reference IAID");
        assert_eq!(actual_iaid, expected_iaid);
        assert_eq!(iaid_contexts, reference_iaid_contexts);
        assert_eq!(
            (
                iaid_decoder.byte_position,
                iaid_decoder.current_byte,
                iaid_decoder.code,
                iaid_decoder.interval,
                iaid_decoder.bit_count,
                iaid_decoder.marker,
            ),
            (
                reference_iaid_decoder.byte_position,
                reference_iaid_decoder.current_byte,
                reference_iaid_decoder.code,
                reference_iaid_decoder.interval,
                reference_iaid_decoder.bit_count,
                reference_iaid_decoder.marker,
            )
        );
    }

    #[test]
    fn first_annex_h_decisions_follow_mps_state_transitions() {
        let mut decoder = ArithmeticDecoder::new(&ANNEX_H_ENCODED).expect("valid initialization");
        let mut context = ArithmeticContext::new();
        assert_eq!(decoder.code, 0x4263_8000);

        assert_eq!(decoder.decode(&mut context).expect("decision"), 0);
        assert_eq!((context.state_index(), context.mps()), (1, 0));
        assert_eq!(decoder.decode(&mut context).expect("decision"), 0);
        assert_eq!((context.state_index(), context.mps()), (2, 0));
        assert_eq!(decoder.decode(&mut context).expect("decision"), 0);
        assert_eq!((context.state_index(), context.mps()), (2, 0));
    }

    #[test]
    fn marker_is_strictly_greater_than_8f_and_is_not_consumed() {
        let stuffed = [0xff, 0x8f, 0x00];
        let decoder = ArithmeticDecoder::new(&stuffed).expect("0x8f is stuffed data");
        assert_eq!(decoder.marker(), None);
        assert_eq!(decoder.byte_position(), 1);
        assert_eq!(decoder.bit_count, 0);

        let marker = [0xff, 0x90];
        let decoder = ArithmeticDecoder::new(&marker).expect("0x90 is a marker");
        assert_eq!(
            decoder.marker(),
            Some(ArithmeticMarker {
                offset: 0,
                code: 0x90,
            })
        );
        assert_eq!(decoder.byte_position(), 0);
        assert_eq!(decoder.bit_count, 1);
    }

    #[test]
    fn initialization_and_byte_in_reject_truncation() {
        for bytes in [&[][..], &[0x00][..], &[0xff][..]] {
            let error = ArithmeticDecoder::new(bytes).expect_err("must reject truncation");
            assert!(matches!(
                error,
                ArithmeticError::Malformed(MalformedError::UnexpectedEof { .. })
            ));
        }
    }

    #[test]
    fn marker_fill_requires_a_real_marker() {
        let mut decoder = ArithmeticDecoder::new(&[0xff, 0xac]).expect("complete marker");
        decoder.bit_count = 0;
        let old_code = decoder.code;
        decoder.byte_in().expect("marker supplies fill bits");
        assert_eq!(decoder.code, old_code + 0xff00);
        assert_eq!(decoder.byte_position(), 0);

        let mut truncated = ArithmeticDecoder {
            bytes: &[0x00, 0x01],
            byte_position: 1,
            current_byte: 0x01,
            code: 0,
            interval: 0x4000,
            bit_count: 0,
            marker: None,
        };
        let error = truncated
            .renormalize()
            .expect_err("raw EOF is not marker fill");
        assert!(matches!(
            error,
            ArithmeticError::Malformed(MalformedError::UnexpectedEof { .. })
        ));
    }

    #[test]
    fn renormalization_shifts_until_interval_is_normalized() {
        let mut decoder = ArithmeticDecoder {
            bytes: &[0x00, 0x00],
            byte_position: 1,
            current_byte: 0,
            code: 3,
            interval: 0x2000,
            bit_count: 3,
            marker: None,
        };
        decoder.renormalize().expect("bounded shifts");
        assert_eq!(decoder.interval, 0x8000);
        assert_eq!(decoder.code, 12);
        assert_eq!(decoder.bit_count, 1);
    }

    #[test]
    fn contexts_reset_and_restore_safely() {
        let mut contexts = [
            ArithmeticContext::from_state(14, 1).expect("valid"),
            ArithmeticContext::from_state(46, 0).expect("valid"),
        ];
        reset_contexts(&mut contexts);
        assert_eq!(contexts, [ArithmeticContext::new(); 2]);
        assert!(matches!(
            ArithmeticContext::from_state(47, 0),
            Err(ArithmeticError::InvalidContextState { .. })
        ));
        assert!(matches!(
            ArithmeticContext::from_state(0, 2),
            Err(ArithmeticError::InvalidContextState { .. })
        ));
    }

    #[test]
    fn integer_tree_decodes_positive_negative_oob_and_wide_tail() {
        assert_eq!(
            scripted_integer(&[0, 1, 0, 1, 0, 0, 0]),
            ArithmeticInteger::Value(12)
        );
        assert_eq!(
            scripted_integer(&[1, 0, 0, 0]),
            ArithmeticInteger::OutOfBand
        );
        assert_eq!(
            scripted_integer(&[1, 0, 0, 1]),
            ArithmeticInteger::Value(-1)
        );

        let mut wide = vec![0, 1, 1, 1, 1, 1];
        wide.extend([1; 32]);
        assert_eq!(
            scripted_integer(&wide),
            ArithmeticInteger::Value(4_294_971_731)
        );
    }

    #[test]
    fn integer_context_history_uses_annex_a_indices() {
        let bits = [0, 1, 0, 1, 0, 0, 0];
        let mut offset = 0;
        let mut visited = Vec::new();
        let result = decode_integer_with(|context_index| {
            visited.push(context_index);
            let bit = bits[offset];
            offset += 1;
            Ok(bit)
        })
        .expect("scripted integer");
        assert_eq!(result, ArithmeticInteger::Value(12));
        assert_eq!(visited, [1, 2, 5, 10, 21, 42, 84]);
    }

    #[test]
    fn iaid_decodes_value_and_validates_context_storage() {
        let bits = [0, 1, 0];
        let mut offset = 0;
        let mut visited = Vec::new();
        let value = decode_iaid_with(3, 8, |context_index| {
            visited.push(context_index);
            let bit = bits[offset];
            offset += 1;
            Ok(bit)
        })
        .expect("scripted IAID");
        assert_eq!(value, 2);
        assert_eq!(visited, [1, 2, 5]);

        let error = decode_iaid_with(3, 7, |_| Ok(0)).expect_err("seven contexts are insufficient");
        assert_eq!(
            error,
            ArithmeticError::InsufficientContexts {
                required: 8,
                actual: 7,
            }
        );
        assert_eq!(
            decode_iaid_with(32, 0, |_| Ok(0)),
            Err(ArithmeticError::InvalidIaidCodeLength(32))
        );
    }

    fn scripted_integer(bits: &[u8]) -> ArithmeticInteger {
        let mut offset = 0;
        let result = decode_integer_with(|_| {
            let bit = bits
                .get(offset)
                .copied()
                .ok_or_else(|| unexpected_eof(offset, 1, 0))?;
            offset += 1;
            Ok(bit)
        })
        .expect("complete scripted integer");
        assert_eq!(offset, bits.len());
        result
    }
}

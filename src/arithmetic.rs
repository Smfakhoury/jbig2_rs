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
pub(crate) mod arithmetic_spec;

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
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) const RASTER_SIGNAL_EOF: u8 = 2;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) const RASTER_SIGNAL_OVERFLOW: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C, align(8))]
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
#[repr(C)]
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
    ensures
        final(contexts)@.len() == old(contexts)@.len(),
        forall|index: int| 0 <= index < final(contexts)@.len() ==>
            arithmetic_spec::context_valid(final(contexts)@[index]),
        forall|index: int| 0 <= index < final(contexts)@.len() ==>
            final(contexts)@[index].state_index == 0
            && final(contexts)@[index].mps == 0,
))]
pub(crate) fn reset_contexts(contexts: &mut [ArithmeticContext]) {
    let mut index = 0;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            0 <= index <= contexts@.len(),
            contexts@.len() == old(contexts)@.len(),
            forall|completed: int| 0 <= completed < index ==>
                arithmetic_spec::context_valid(contexts@[completed]),
            forall|completed: int| 0 <= completed < index ==>
                contexts@[completed].state_index == 0
                && contexts@[completed].mps == 0,
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

impl ArithmeticError {
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub(crate) fn into_decode(self) -> DecodeError {
        match self {
            Self::Malformed(error) => DecodeError::Malformed(error),
            Self::InvalidContextState { state_index, mps } => {
                DecodeError::Malformed(MalformedError::InvalidArithmeticContextState {
                    state_index,
                    mps,
                })
            }
            Self::InsufficientContexts { required, actual } => {
                DecodeError::Malformed(MalformedError::InsufficientArithmeticContexts {
                    required,
                    actual,
                })
            }
            Self::InvalidIaidCodeLength(length) => {
                DecodeError::Malformed(MalformedError::InvalidIaidCodeLength(length))
            }
        }
    }
}

impl From<ArithmeticError> for DecodeError {
    fn from(error: ArithmeticError) -> Self {
        error.into_decode()
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
        ensures
        out.is_ok() == (bytes@.len() >= 2),
        match out {
            Ok(decoder) =>
                arithmetic_spec::decoder_valid(&decoder)
                && decoder.bytes@ == bytes@
                && decoder.interval == MIN_INTERVAL
                && decoder.byte_position == arithmetic_spec::initialized_state(bytes).unwrap().0
                && decoder.current_byte == arithmetic_spec::initialized_state(bytes).unwrap().1
                && decoder.code == arithmetic_spec::initialized_state(bytes).unwrap().2
                && decoder.bit_count == arithmetic_spec::initialized_state(bytes).unwrap().3
                && decoder.marker == arithmetic_spec::initialized_state(bytes).unwrap().4,
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
        #[cfg(verus_keep_ghost)]
        proof! {
            if bytes@.len() >= 2 {
                arithmetic_spec::initialization_code_bound(bytes@[0], bytes@[1]);
                assert(arithmetic_spec::refill_step(&decoder).0 == 0);
                assert(arithmetic_spec::refill_step(&decoder).3 <= u32::MAX / 128);
            }
        }
        decoder.byte_in()?;
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(bytes@.len() >= 2);
            assert(decoder.code <= u32::MAX / 128);
            // Relate the native shift constant to the model's arithmetic scale.
            assert((1u32 << 7) == 128u32) by(bit_vector);
        }
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
            final(self).bytes@ == old(self).bytes@,
            final(self).bytes == old(self).bytes,
            final(context).state_index == arithmetic_spec::entry_index(
                old(self).interval, old(self).code, old(context).state_index,
            ),
            final(context).mps == arithmetic_spec::entry_mps(
                old(self).interval, old(self).code, old(context).state_index, old(context).mps,
            ),
            match out {
                Ok(decision) =>
                    decision <= 1
                    && arithmetic_spec::decoder_valid(final(self))
                    && decision == arithmetic_spec::entry_decision(
                        old(self).interval, old(self).code, old(context).state_index, old(context).mps,
                    )
                    && decision == arithmetic_spec::decision_state(old(self), *old(context)).0,
                Err(_) => true,
            },
            out.is_ok() == (arithmetic_spec::decision_state(old(self), *old(context)).0 <= 1),
            final(self).interval == arithmetic_spec::decision_state(old(self), *old(context)).1,
            final(self).byte_position == arithmetic_spec::decision_state(old(self), *old(context)).2,
            final(self).current_byte == arithmetic_spec::decision_state(old(self), *old(context)).3,
            final(self).code == arithmetic_spec::decision_state(old(self), *old(context)).4,
            final(self).bit_count == arithmetic_spec::decision_state(old(self), *old(context)).5,
            final(self).marker == arithmetic_spec::decision_state(old(self), *old(context)).6,
            match out {
                Err(error) => error == arithmetic_spec::error_for_signal(
                    final(self), arithmetic_spec::decision_state(old(self), *old(context)).0,
                ),
                Ok(_) => true,
            },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    #[inline]
    pub(crate) fn decode(
        &mut self,
        context: &mut ArithmeticContext,
    ) -> Result<u8, ArithmeticError> {
        let signal = self.decode_signal(context);
        if signal <= 1 {
            Ok(signal)
        } else {
            Err(self.signal_error(signal))
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            arithmetic_spec::decoder_valid(old(self)),
            arithmetic_spec::context_valid(*old(context)),
        ensures
            out <= RASTER_SIGNAL_OVERFLOW,
            arithmetic_spec::decoder_state_valid(final(self)),
            arithmetic_spec::context_valid(*final(context)),
            final(self).bytes@ == old(self).bytes@,
            final(self).bytes == old(self).bytes,
            out <= 1 ==> arithmetic_spec::decoder_valid(final(self)),
            out <= 1 ==> out == arithmetic_spec::entry_decision(
                old(self).interval, old(self).code, old(context).state_index, old(context).mps,
            ),
            final(context).state_index == arithmetic_spec::entry_index(
                old(self).interval, old(self).code, old(context).state_index,
            ),
            final(context).mps == arithmetic_spec::entry_mps(
                old(self).interval, old(self).code, old(context).state_index, old(context).mps,
            ),
            out == arithmetic_spec::decision_state(old(self), *old(context)).0,
            final(self).interval == arithmetic_spec::decision_state(old(self), *old(context)).1,
            final(self).byte_position == arithmetic_spec::decision_state(old(self), *old(context)).2,
            final(self).current_byte == arithmetic_spec::decision_state(old(self), *old(context)).3,
            final(self).code == arithmetic_spec::decision_state(old(self), *old(context)).4,
            final(self).bit_count == arithmetic_spec::decision_state(old(self), *old(context)).5,
            final(self).marker == arithmetic_spec::decision_state(old(self), *old(context)).6,
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    #[inline]
    pub(crate) fn decode_signal(&mut self, context: &mut ArithmeticContext) -> u8 {
        #[cfg(verus_keep_ghost)]
        production_reveal_decoder_valid!();
        #[cfg(verus_keep_ghost)]
        proof! {
            arithmetic_spec::decision_model_equations(self, *context);
            arithmetic_spec::entry_model_equations(
                self.interval, self.code, context.state_index, context.mps,
            );
        }
        let state_index = usize::from(context.state_index);
        let qe = u32::from(QE_STATES[state_index].qe);
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::arithmetic::arithmetic_shared::qe_transition_valid(
                context.state_index as int,
            );
            assert(QE_STATES@[context.state_index as int].qe > 0);
            assert(QE_STATES@[context.state_index as int].qe <= 0x5601);
        }
        let old_mps = context.mps;

        #[cfg(verus_keep_ghost)]
        production_decode_interval_subtraction!(self, qe);
        self.interval -= qe;

        if self.code >> 16 < qe {
            let state = QE_STATES[state_index];
            #[cfg(verus_keep_ghost)]
            production_qe_transition_valid!(context.state_index, state);
            return self.decode_lower_signal(context, state, qe, old_mps);
        }

        #[cfg(verus_keep_ghost)]
        proof! {
            let value = self.code;
            assert(value >= qe << 16) by (bit_vector)
                requires value >> 16 >= qe, qe <= 0xffff;
        }
        let Some(code) = self.code.checked_sub(qe << 16) else {
            return RASTER_SIGNAL_OVERFLOW;
        };
        self.code = code;
        if self.interval >= MIN_INTERVAL {
            return old_mps;
        }

        let state = QE_STATES[state_index];
        #[cfg(verus_keep_ghost)]
        production_qe_transition_valid!(context.state_index, state);
        self.decode_upper_signal(context, state, qe, old_mps)
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            arithmetic_spec::decoder_state_valid(old(self)),
            arithmetic_spec::context_valid(*old(context)),
            state.next_mps < 47,
            state.next_lps < 47,
            0 < qe <= 0x5601,
            old_mps <= 1,
        ensures
            out <= RASTER_SIGNAL_OVERFLOW,
            arithmetic_spec::decoder_state_valid(final(self)),
            arithmetic_spec::context_valid(*final(context)),
            final(self).bytes@ == old(self).bytes@,
            final(self).bytes == old(self).bytes,
            out <= 1 ==> arithmetic_spec::decoder_valid(final(self)),
            final(context).state_index == arithmetic_spec::exchange_index(
                true, old(self).interval, qe, state.next_mps, state.next_lps,
            ),
            final(context).mps == arithmetic_spec::exchange_mps(
                true, old(self).interval, qe, state.switch_mps, old(context).mps, old_mps,
            ),
            out <= 1 ==> out == arithmetic_spec::exchange_decision(
                true, old(self).interval, qe, old_mps,
            ),
            out == if arithmetic_spec::exchange_normalization(old(self), true, qe).0 == 0 {
                arithmetic_spec::exchange_decision(true, old(self).interval, qe, old_mps)
            } else {
                arithmetic_spec::exchange_normalization(old(self), true, qe).0
            },
            final(self).interval == arithmetic_spec::exchange_normalization(old(self), true, qe).1,
            final(self).byte_position == arithmetic_spec::exchange_normalization(old(self), true, qe).2,
            final(self).current_byte == arithmetic_spec::exchange_normalization(old(self), true, qe).3,
            final(self).code == arithmetic_spec::exchange_normalization(old(self), true, qe).4,
            final(self).bit_count == arithmetic_spec::exchange_normalization(old(self), true, qe).5,
            final(self).marker == arithmetic_spec::exchange_normalization(old(self), true, qe).6,
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    #[inline(never)]
    fn decode_lower_signal(
        &mut self,
        context: &mut ArithmeticContext,
        state: QeState,
        qe: u32,
        old_mps: u8,
    ) -> u8 {
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal(arithmetic_spec::exchange_decision);
            reveal(arithmetic_spec::exchange_is_mps);
            reveal(arithmetic_spec::exchange_index);
            reveal(arithmetic_spec::exchange_mps);
        }
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
        let signal = self.renormalize_signal();
        if signal == 0 {
            decision
        } else {
            signal
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            arithmetic_spec::decoder_state_valid(old(self)),
            arithmetic_spec::context_valid(*old(context)),
            state.next_mps < 47,
            state.next_lps < 47,
            0 < qe <= 0x5601,
            old_mps <= 1,
        ensures
            out <= RASTER_SIGNAL_OVERFLOW,
            arithmetic_spec::decoder_state_valid(final(self)),
            arithmetic_spec::context_valid(*final(context)),
            final(self).bytes@ == old(self).bytes@,
            final(self).bytes == old(self).bytes,
            out <= 1 ==> arithmetic_spec::decoder_valid(final(self)),
            final(context).state_index == arithmetic_spec::exchange_index(
                false, old(self).interval, qe, state.next_mps, state.next_lps,
            ),
            final(context).mps == arithmetic_spec::exchange_mps(
                false, old(self).interval, qe, state.switch_mps, old(context).mps, old_mps,
            ),
            out <= 1 ==> out == arithmetic_spec::exchange_decision(
                false, old(self).interval, qe, old_mps,
            ),
            out == if arithmetic_spec::exchange_normalization(old(self), false, qe).0 == 0 {
                arithmetic_spec::exchange_decision(false, old(self).interval, qe, old_mps)
            } else {
                arithmetic_spec::exchange_normalization(old(self), false, qe).0
            },
            final(self).interval == arithmetic_spec::exchange_normalization(old(self), false, qe).1,
            final(self).byte_position == arithmetic_spec::exchange_normalization(old(self), false, qe).2,
            final(self).current_byte == arithmetic_spec::exchange_normalization(old(self), false, qe).3,
            final(self).code == arithmetic_spec::exchange_normalization(old(self), false, qe).4,
            final(self).bit_count == arithmetic_spec::exchange_normalization(old(self), false, qe).5,
            final(self).marker == arithmetic_spec::exchange_normalization(old(self), false, qe).6,
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    #[inline(never)]
    fn decode_upper_signal(
        &mut self,
        context: &mut ArithmeticContext,
        state: QeState,
        qe: u32,
        old_mps: u8,
    ) -> u8 {
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal(arithmetic_spec::exchange_decision);
            reveal(arithmetic_spec::exchange_is_mps);
            reveal(arithmetic_spec::exchange_index);
            reveal(arithmetic_spec::exchange_mps);
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
        let signal = self.renormalize_signal();
        if signal == 0 {
            decision
        } else {
            signal
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == arithmetic_spec::error_for_signal(self, signal),
    ))]
    pub(crate) fn signal_error(&self, signal: u8) -> ArithmeticError {
        if signal == RASTER_SIGNAL_EOF {
            match self.byte_position.checked_add(1) {
                Some(offset) => unexpected_eof(offset, 1, 0),
                None => ArithmeticError::Malformed(MalformedError::IntegerOverflow),
            }
        } else {
            ArithmeticError::Malformed(MalformedError::IntegerOverflow)
        }
    }

    #[cfg(test)]
    fn decode_checked_oracle(
        &mut self,
        context: &mut ArithmeticContext,
    ) -> Result<u8, ArithmeticError> {
        let state = QE_STATES[usize::from(context.state_index)];
        let qe = u32::from(state.qe);
        let old_mps = context.mps;
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
            self.renormalize_checked_oracle()?;
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
        self.renormalize_checked_oracle()?;
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
            code_length > MAX_IAID_CODE_LENGTH ==> {
                &&& out == Err(ArithmeticError::InvalidIaidCodeLength(code_length))
                &&& *final(self) == *old(self)
                &&& final(contexts)@ == old(contexts)@
            },
            code_length <= MAX_IAID_CODE_LENGTH
                && old(contexts)@.len() < (1usize << code_length) ==> {
                &&& out == Err(ArithmeticError::InsufficientContexts {
                    required: 1usize << code_length, actual: old(contexts)@.len() as usize,
                })
                &&& *final(self) == *old(self)
                &&& final(contexts)@ == old(contexts)@
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
                code_length <= MAX_IAID_CODE_LENGTH,
                required == 1usize << code_length,
                context_count == old(contexts)@.len(),
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
                    && arithmetic_spec::decoder_valid(final(self))
                    && decision == arithmetic_spec::integer_bit_state(
                        old(self), old(contexts)@, *old(previous),
                    ).0
                    && *final(previous) == arithmetic_spec::integer_history_step(*old(previous), decision),
                Err(error) =>
                    *final(previous) == *old(previous)
                    && error == arithmetic_spec::error_for_signal(
                        final(self), arithmetic_spec::integer_bit_state(
                            old(self), old(contexts)@, *old(previous),
                        ).0,
                    ),
            },
            out.is_ok() == (arithmetic_spec::integer_bit_state(old(self), old(contexts)@, *old(previous)).0 <= 1),
            final(self).bytes@ == old(self).bytes@,
            final(self).bytes == old(self).bytes,
            final(self).interval == arithmetic_spec::integer_bit_state(old(self), old(contexts)@, *old(previous)).1,
            final(self).byte_position == arithmetic_spec::integer_bit_state(old(self), old(contexts)@, *old(previous)).2,
            final(self).current_byte == arithmetic_spec::integer_bit_state(old(self), old(contexts)@, *old(previous)).3,
            final(self).code == arithmetic_spec::integer_bit_state(old(self), old(contexts)@, *old(previous)).4,
            final(self).bit_count == arithmetic_spec::integer_bit_state(old(self), old(contexts)@, *old(previous)).5,
            final(self).marker == arithmetic_spec::integer_bit_state(old(self), old(contexts)@, *old(previous)).6,
            final(contexts)@[( *old(previous) & 511) as int]
                == arithmetic_spec::context_after_decision(
                    old(self), old(contexts)@[( *old(previous) & 511) as int],
                ),
            forall|index: int| 0 <= index < old(contexts)@.len()
                && index != (*old(previous) & 511) as int ==>
                    final(contexts)@[index] == old(contexts)@[index],
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
            final(self).bytes@ == old(self).bytes@,
            final(self).bytes == old(self).bytes,
            out.is_ok() == (arithmetic_spec::normalized_state(old(self)).0 == 0),
            final(self).interval == arithmetic_spec::normalized_state(old(self)).1,
            final(self).byte_position == arithmetic_spec::normalized_state(old(self)).2,
            final(self).current_byte == arithmetic_spec::normalized_state(old(self)).3,
            final(self).code == arithmetic_spec::normalized_state(old(self)).4,
            final(self).bit_count == arithmetic_spec::normalized_state(old(self)).5,
            final(self).marker == arithmetic_spec::normalized_state(old(self)).6,
    ))]
    fn renormalize(&mut self) -> Result<(), ArithmeticError> {
        let signal = self.renormalize_signal();
        if signal == 0 {
            Ok(())
        } else {
            Err(self.signal_error(signal))
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires arithmetic_spec::decoder_state_valid(old(self)),
        ensures
            out <= RASTER_SIGNAL_OVERFLOW,
            out == 0 || out == RASTER_SIGNAL_EOF || out == RASTER_SIGNAL_OVERFLOW,
            arithmetic_spec::decoder_state_valid(final(self)),
            out == 0 ==> arithmetic_spec::decoder_valid(final(self)),
            final(self).bytes@ == old(self).bytes@,
            final(self).bytes == old(self).bytes,
            out == arithmetic_spec::normalized_state(old(self)).0,
            final(self).interval == arithmetic_spec::normalized_state(old(self)).1,
            final(self).byte_position == arithmetic_spec::normalized_state(old(self)).2,
            final(self).current_byte == arithmetic_spec::normalized_state(old(self)).3,
            final(self).code == arithmetic_spec::normalized_state(old(self)).4,
            final(self).bit_count == arithmetic_spec::normalized_state(old(self)).5,
            final(self).marker == arithmetic_spec::normalized_state(old(self)).6,
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    #[inline]
    fn renormalize_signal(&mut self) -> u8 {
        #[cfg(verus_keep_ghost)]
        production_reveal_decoder_valid!();
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                arithmetic_spec::decoder_state_valid(self),
                arithmetic_spec::decoder_state_valid(old(self)),
                self.bytes@ == old(self).bytes@,
                self.bytes == old(self).bytes,
                arithmetic_spec::normalized_state(old(self))
                    == arithmetic_spec::normalized_state(self),
            decreases arithmetic_spec::renormalization_steps(
                self.interval as nat,
            ),
        ))]
        while self.interval < MIN_INTERVAL {
            #[cfg(verus_keep_ghost)]
            proof_decl! {
                let ghost previous_interval = self.interval;
                let ghost before = *self;
            }
            if self.bit_count == 0 {
                let signal = self.byte_in_signal();
                if signal != 0 {
                    return signal;
                }
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
                        <                         MIN_INTERVAL - self.interval
                );
            }
            self.interval *= 2;
            let Some(code) = self.code.checked_mul(2) else {
                return RASTER_SIGNAL_OVERFLOW;
            };
            self.code = code;
            self.bit_count -= 1;
            #[cfg(verus_keep_ghost)]
            proof! {
                reveal_with_fuel(arithmetic_spec::normalized_state, 1);
                assert(
                    arithmetic_spec::normalized_state(&before)
                        == arithmetic_spec::normalized_state(self)
                );
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
        0
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires arithmetic_spec::decoder_state_valid(old(self)),
        ensures
            arithmetic_spec::decoder_state_valid(final(self)),
            final(self).bytes@ == old(self).bytes@,
            final(self).interval == old(self).interval,
            final(self).byte_position == old(self).byte_position
                || final(self).byte_position == old(self).byte_position + 1,
            final(self).bytes == old(self).bytes,
            out.is_ok() == (arithmetic_spec::refill_step(old(self)).0 == 0),
            final(self).byte_position == arithmetic_spec::refill_step(old(self)).1,
            final(self).current_byte == arithmetic_spec::refill_step(old(self)).2,
            final(self).code == arithmetic_spec::refill_step(old(self)).3,
            final(self).bit_count == arithmetic_spec::refill_step(old(self)).4,
            final(self).marker == arithmetic_spec::refill_step(old(self)).5,
            match out {
                Ok(()) =>
                    old(self).byte_position + 1 < old(self).bytes@.len()
                    && final(self).bit_count >= 7,
                Err(_) => true,
            },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    fn byte_in(&mut self) -> Result<(), ArithmeticError> {
        let signal = self.byte_in_signal();
        if signal == 0 {
            Ok(())
        } else {
            Err(self.signal_error(signal))
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires arithmetic_spec::decoder_state_valid(old(self)),
        ensures
            out <= RASTER_SIGNAL_OVERFLOW,
            out == 0 || out == RASTER_SIGNAL_EOF || out == RASTER_SIGNAL_OVERFLOW,
            arithmetic_spec::decoder_state_valid(final(self)),
            final(self).bytes@ == old(self).bytes@,
            final(self).interval == old(self).interval,
            final(self).byte_position == old(self).byte_position
                || final(self).byte_position == old(self).byte_position + 1,
            final(self).bytes == old(self).bytes,
            out == arithmetic_spec::refill_step(old(self)).0,
            final(self).byte_position == arithmetic_spec::refill_step(old(self)).1,
            final(self).current_byte == arithmetic_spec::refill_step(old(self)).2,
            final(self).code == arithmetic_spec::refill_step(old(self)).3,
            final(self).bit_count == arithmetic_spec::refill_step(old(self)).4,
            final(self).marker == arithmetic_spec::refill_step(old(self)).5,
            out == 0 ==>
                    old(self).byte_position + 1 < old(self).bytes@.len()
                    && final(self).bit_count >= 7
                    && (
                        old(self).marker.is_none()
                            && final(self).marker.is_some()
                        ==> final(self).byte_position
                            == old(self).byte_position
                    ),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    #[inline]
    fn byte_in_signal(&mut self) -> u8 {
        #[cfg(verus_keep_ghost)]
        production_reveal_decoder_valid!();
        let next_position = self.byte_position + 1;
        if next_position >= self.bytes.len() {
            return RASTER_SIGNAL_EOF;
        }
        let next_byte = self.bytes[next_position];

        if self.current_byte == 0xff {
            if next_byte > 0x8f {
                let Some(code) = self.code.checked_add(0xff00) else {
                    return RASTER_SIGNAL_OVERFLOW;
                };
                self.code = code;
                self.bit_count = 8;
                self.marker = Some(ArithmeticMarker {
                    offset: self.byte_position,
                    code: next_byte,
                });
                return 0;
            }

            self.byte_position = next_position;
            self.current_byte = next_byte;
            let Some(code) = self.code.checked_add(u32::from(next_byte) << 9) else {
                return RASTER_SIGNAL_OVERFLOW;
            };
            self.code = code;
            self.bit_count = 7;
            return 0;
        }

        self.byte_position = next_position;
        self.current_byte = next_byte;
        let Some(code) = self.code.checked_add(u32::from(next_byte) << 8) else {
            return RASTER_SIGNAL_OVERFLOW;
        };
        self.code = code;
        self.bit_count = 8;
        0
    }

    #[cfg(test)]
    fn renormalize_checked_oracle(&mut self) -> Result<(), ArithmeticError> {
        while self.interval < MIN_INTERVAL {
            if self.bit_count == 0 {
                self.byte_in_checked_oracle()?;
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
        }
        Ok(())
    }

    #[cfg(test)]
    fn byte_in_checked_oracle(&mut self) -> Result<(), ArithmeticError> {
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

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out == arithmetic_spec::eof_error(offset, needed, remaining),
))]
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

    fn next_random(state: &mut u64) -> u32 {
        *state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (*state >> 32) as u32
    }

    fn assert_decoder_state_eq(left: &ArithmeticDecoder<'_>, right: &ArithmeticDecoder<'_>) {
        assert_eq!(left.byte_position, right.byte_position);
        assert_eq!(left.current_byte, right.current_byte);
        assert_eq!(left.code, right.code);
        assert_eq!(left.interval, right.interval);
        assert_eq!(left.bit_count, right.bit_count);
        assert_eq!(left.marker, right.marker);
    }

    #[test]
    fn initialization_accepts_every_byte_pair_and_preserves_suffixes() {
        for first in 0_u8..=u8::MAX {
            for next in 0_u8..=u8::MAX {
                let bytes = [first, next];
                let marker = first == 0xff && next >= 0x90;
                let stuffed = first == 0xff && !marker;
                let increment = if marker {
                    65_280
                } else {
                    u64::from(next) * if stuffed { 512 } else { 256 }
                };
                let expected = ArithmeticDecoder {
                    bytes: &bytes,
                    byte_position: usize::from(!marker),
                    current_byte: if marker { first } else { next },
                    code: u32::try_from((u64::from(first) * 65_536 + increment) * 128)
                        .expect("two-byte initialization fits u32"),
                    interval: MIN_INTERVAL,
                    bit_count: u8::from(!stuffed),
                    marker: if marker {
                        Some(ArithmeticMarker {
                            offset: 0,
                            code: next,
                        })
                    } else {
                        None
                    },
                };
                let actual = ArithmeticDecoder::new(&bytes).expect("every two-byte prefix");
                assert_decoder_state_eq(&actual, &expected);
                assert_eq!(actual.bytes, bytes);
                let suffix = [first, next, 0xff, 0xac];
                let extended = ArithmeticDecoder::new(&suffix).expect("unchanged prefix");
                assert_decoder_state_eq(&extended, &expected);
                assert_eq!(extended.bytes, suffix);
            }
        }
    }

    #[test]
    fn initialization_short_input_errors_preserve_exact_offsets() {
        assert!(matches!(
            ArithmeticDecoder::new(&[]),
            Err(ArithmeticError::Malformed(MalformedError::UnexpectedEof {
                offset: 0,
                needed: 1,
                remaining: 0
            }))
        ));
        for first in 0_u8..=u8::MAX {
            assert!(matches!(
                ArithmeticDecoder::new(&[first]),
                Err(ArithmeticError::Malformed(MalformedError::UnexpectedEof {
                    offset: 1,
                    needed: 1,
                    remaining: 0
                }))
            ));
        }
    }

    fn refill_fixture(
        bytes: &[u8],
        position: usize,
        code: u32,
        bit_count: u8,
        marker: Option<ArithmeticMarker>,
    ) -> ArithmeticDecoder<'_> {
        ArithmeticDecoder {
            bytes,
            byte_position: position,
            current_byte: bytes[position],
            code,
            interval: MIN_INTERVAL,
            bit_count,
            marker,
        }
    }

    fn assert_refill_matches_numeric_oracle(
        bytes: &[u8],
        position: usize,
        code: u32,
        bit_count: u8,
        marker: Option<ArithmeticMarker>,
    ) {
        let mut expected = refill_fixture(bytes, position, code, bit_count, marker);
        let signal = if let Some(&next) = bytes.get(position + 1) {
            let marker_byte = bytes[position] == 0xff && next >= 0x90;
            let stuffed = bytes[position] == 0xff && !marker_byte;
            let increment = if marker_byte {
                65_280
            } else {
                u64::from(next) * if stuffed { 512 } else { 256 }
            };
            if !marker_byte {
                expected.byte_position += 1;
                expected.current_byte = next;
            }
            if let Ok(sum) = u32::try_from(u64::from(code) + increment) {
                expected.code = sum;
                expected.bit_count = if stuffed { 7 } else { 8 };
                if marker_byte {
                    expected.marker = Some(ArithmeticMarker {
                        offset: position,
                        code: next,
                    });
                }
                0
            } else {
                RASTER_SIGNAL_OVERFLOW
            }
        } else {
            RASTER_SIGNAL_EOF
        };
        let mut actual = refill_fixture(bytes, position, code, bit_count, marker);
        assert_eq!(actual.byte_in_signal(), signal);
        assert_decoder_state_eq(&actual, &expected);
        assert_eq!(actual.bytes, bytes);

        let mut checked = refill_fixture(bytes, position, code, bit_count, marker);
        match (signal, checked.byte_in()) {
            (0, Ok(()))
            | (
                RASTER_SIGNAL_OVERFLOW,
                Err(ArithmeticError::Malformed(MalformedError::IntegerOverflow)),
            ) => {}
            (
                RASTER_SIGNAL_EOF,
                Err(ArithmeticError::Malformed(MalformedError::UnexpectedEof {
                    offset,
                    needed,
                    remaining,
                })),
            ) => {
                assert_eq!(offset, u64::try_from(position + 1).unwrap());
                assert_eq!(needed, 1);
                assert_eq!(remaining, 0);
            }
            (_, result) => panic!("wrong checked refill result for signal {signal}: {result:?}"),
        }
        assert_decoder_state_eq(&checked, &expected);
        assert_eq!(checked.bytes, bytes);
    }

    #[test]
    fn refill_byte_pairs_match_numeric_code_consumption_and_counter_boundaries() {
        for current in 0_u8..=u8::MAX {
            for next in 0_u8..=u8::MAX {
                let bytes = [current, next];
                let increment = if current == 0xff && next >= 0x90 {
                    65_280
                } else {
                    u32::from(next) * if current == 0xff { 512 } else { 256 }
                };
                let maximum = u32::MAX - increment;
                for bit_count in [0, 7, 8] {
                    for code in [0, maximum, u32::MAX] {
                        assert_refill_matches_numeric_oracle(&bytes, 0, code, bit_count, None);
                    }
                    if increment != 0 {
                        assert_refill_matches_numeric_oracle(
                            &bytes,
                            0,
                            maximum + 1,
                            bit_count,
                            None,
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn refill_eof_and_existing_markers_preserve_exact_error_order_and_frames() {
        for current in 0_u8..=u8::MAX {
            for code in [0, u32::MAX] {
                for bit_count in [0, 3, 8] {
                    assert_refill_matches_numeric_oracle(&[current], 0, code, bit_count, None);
                    assert_refill_matches_numeric_oracle(
                        &[0x12, current],
                        1,
                        code,
                        bit_count,
                        None,
                    );
                }
            }
        }
        for next in 0x90_u8..=u8::MAX {
            let bytes = [0x12, 0xff, next];
            for code in [0, u32::MAX - 65_280, u32::MAX - 65_279, u32::MAX] {
                for bit_count in [0, 3, 8] {
                    assert_refill_matches_numeric_oracle(
                        &bytes,
                        1,
                        code,
                        bit_count,
                        Some(ArithmeticMarker {
                            offset: 1,
                            code: next,
                        }),
                    );
                }
            }
        }
    }

    fn decision_fixture(
        bytes: &[u8],
        interval: u32,
        code: u32,
        bit_count: u8,
    ) -> ArithmeticDecoder<'_> {
        ArithmeticDecoder {
            bytes,
            byte_position: 0,
            current_byte: bytes[0],
            code,
            interval,
            bit_count,
            marker: None,
        }
    }

    #[test]
    fn successful_decisions_and_contexts_cover_every_probability_row_and_route() {
        let bytes = [0; 32];
        for (index, state) in QE_STATES.iter().copied().enumerate() {
            let qe = u32::from(state.qe);
            for old_mps in 0..=1 {
                for interval in [
                    MIN_INTERVAL,
                    0xffff,
                    (2 * qe).max(MIN_INTERVAL),
                    (MIN_INTERVAL + qe).min(0xffff),
                ] {
                    for upper in [0, qe - 1, qe, qe + 1] {
                        let code = upper << 16;
                        let reduced = interval - qe;
                        let lower = upper < qe;
                        let fast = !lower && reduced >= MIN_INTERVAL;
                        let mps_route = (lower && reduced < qe) || (!lower && reduced >= qe);
                        let decision = if fast || mps_route {
                            old_mps
                        } else {
                            1 - old_mps
                        };
                        let expected = ArithmeticContext {
                            state_index: if fast {
                                u8::try_from(index).unwrap()
                            } else if mps_route {
                                state.next_mps
                            } else {
                                state.next_lps
                            },
                            mps: if !fast && !mps_route && state.switch_mps {
                                1 - old_mps
                            } else {
                                old_mps
                            },
                        };
                        let mut decoder = decision_fixture(&bytes, interval, code, 8);
                        let mut context =
                            ArithmeticContext::from_state(u8::try_from(index).unwrap(), old_mps)
                                .unwrap();
                        assert_eq!(decoder.decode_signal(&mut context), decision);
                        assert_eq!(context, expected);
                    }
                }
            }
        }
    }

    #[test]
    fn exchange_context_update_precedes_truncation_and_keeps_supplied_mps_distinct() {
        let bytes = [0];
        for state in QE_STATES {
            let qe = u32::from(state.qe);
            for lower in [false, true] {
                for prior_mps in 0..=1 {
                    for supplied_mps in 0..=1 {
                        let mut decoder = decision_fixture(&bytes, 1, 0, 0);
                        let mut context = ArithmeticContext::from_state(0, prior_mps).unwrap();
                        let mps_route = (1 < qe) == lower;
                        let expected_index = if mps_route {
                            state.next_mps
                        } else {
                            state.next_lps
                        };
                        let expected_mps = if !mps_route && state.switch_mps {
                            1 - supplied_mps
                        } else {
                            prior_mps
                        };
                        let result = if lower {
                            decoder.decode_lower_signal(&mut context, state, qe, supplied_mps)
                        } else {
                            decoder.decode_upper_signal(&mut context, state, qe, supplied_mps)
                        };
                        assert_eq!(result, RASTER_SIGNAL_EOF);
                        assert_eq!(context.state_index, expected_index);
                        assert_eq!(context.mps, expected_mps);
                    }
                }
            }
        }
    }

    #[test]
    fn entry_errors_preserve_context_updates_input_and_exact_partial_registers() {
        let bytes = [0x11];
        for (index, state) in QE_STATES.iter().copied().enumerate() {
            let qe = u32::from(state.qe);
            for old_mps in 0..=1 {
                for interval in [MIN_INTERVAL, 0xffff] {
                    for upper in [0, qe - 1, qe, qe + 1] {
                        for low in [0, 0xffff] {
                            let code = (upper << 16) | low;
                            let reduced = interval - qe;
                            let lower = upper < qe;
                            let fast = !lower && reduced >= MIN_INTERVAL;
                            let mps_route = (lower && reduced < qe) || (!lower && reduced >= qe);
                            let expected_context = ArithmeticContext {
                                state_index: if fast {
                                    u8::try_from(index).unwrap()
                                } else if mps_route {
                                    state.next_mps
                                } else {
                                    state.next_lps
                                },
                                mps: if !fast && !mps_route && state.switch_mps {
                                    1 - old_mps
                                } else {
                                    old_mps
                                },
                            };
                            for checked in [false, true] {
                                let mut decoder = decision_fixture(&bytes, interval, code, 0);
                                let mut context = ArithmeticContext::from_state(
                                    u8::try_from(index).unwrap(),
                                    old_mps,
                                )
                                .unwrap();
                                if checked {
                                    assert_eq!(
                                        decoder.decode(&mut context),
                                        if fast {
                                            Ok(old_mps)
                                        } else {
                                            Err(unexpected_eof(1, 1, 0))
                                        },
                                    );
                                } else {
                                    assert_eq!(
                                        decoder.decode_signal(&mut context),
                                        if fast { old_mps } else { RASTER_SIGNAL_EOF },
                                    );
                                }
                                assert_eq!(context, expected_context);
                                assert_eq!(decoder.interval, if lower { qe } else { reduced });
                                let expected_code = if lower {
                                    u64::from(code)
                                } else {
                                    u64::from(code) - u64::from(qe) * 65536
                                };
                                assert_eq!(u64::from(decoder.code), expected_code);
                                assert_eq!(decoder.bit_count, 0);
                                assert_eq!(decoder.byte_position, 0);
                                assert_eq!(decoder.current_byte, 0x11);
                                assert_eq!(decoder.marker, None);
                                assert_eq!(decoder.bytes, bytes);
                                assert_eq!(decoder.bytes.as_ptr(), bytes.as_ptr());
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn single_context_restore_reset_and_default_cover_every_stored_byte_pair() {
        for state_index in 0..=u8::MAX {
            for mps in 0..=u8::MAX {
                match ArithmeticContext::from_state(state_index, mps) {
                    Ok(mut context) => {
                        assert!(state_index < 47 && mps <= 1);
                        assert_eq!((context.state_index(), context.mps()), (state_index, mps));
                        context.reset();
                        assert_eq!(context, ArithmeticContext::default());
                        context.reset();
                        assert_eq!((context.state_index(), context.mps()), (0, 0));
                    }
                    Err(error) => {
                        assert!(state_index >= 47 || mps > 1);
                        assert_eq!(
                            error,
                            ArithmeticError::InvalidContextState { state_index, mps },
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn arithmetic_error_adapters_and_formatters_preserve_every_variant_and_field() {
        use std::fmt::Write as _;

        struct RejectWrites;
        impl fmt::Write for RejectWrites {
            fn write_str(&mut self, _: &str) -> fmt::Result {
                Err(fmt::Error)
            }
        }

        for via_from in [false, true] {
            for (error, expected, message) in [
                (
                    ArithmeticError::from(MalformedError::IntegerOverflow),
                    MalformedError::IntegerOverflow,
                    "input-derived integer overflow".to_owned(),
                ),
                (
                    ArithmeticError::InvalidContextState {
                        state_index: u8::MAX,
                        mps: u8::MAX,
                    },
                    MalformedError::InvalidArithmeticContextState {
                        state_index: u8::MAX,
                        mps: u8::MAX,
                    },
                    "invalid arithmetic context state I=255, MPS=255".to_owned(),
                ),
                (
                    ArithmeticError::InsufficientContexts {
                        required: usize::MAX,
                        actual: 0,
                    },
                    MalformedError::InsufficientArithmeticContexts {
                        required: usize::MAX,
                        actual: 0,
                    },
                    format!(
                        "arithmetic decoding requires {} contexts, but 0 were supplied",
                        usize::MAX
                    ),
                ),
                (
                    ArithmeticError::InvalidIaidCodeLength(u8::MAX),
                    MalformedError::InvalidIaidCodeLength(u8::MAX),
                    "unsupported IAID code length 255".to_owned(),
                ),
            ] {
                assert_eq!(error.to_string(), message);
                assert!(std::error::Error::source(&error).is_none());
                assert!(write!(&mut RejectWrites, "{error}").is_err());
                let converted = if via_from {
                    DecodeError::from(error)
                } else {
                    error.into_decode()
                };
                match converted {
                    DecodeError::Malformed(actual) => assert_eq!(actual, expected),
                    other => panic!("unexpected arithmetic error conversion: {other:?}"),
                }
            }
        }
    }

    #[test]
    fn unchecked_hot_decoder_matches_checked_oracle_randomized() {
        let mut state = 0x45ea_30c7_98b1_f26d;
        for case in 0..256 {
            let length = 2 + usize::try_from(next_random(&mut state) % 127).expect("bounded");
            let mut bytes = Vec::with_capacity(length);
            for _ in 0..length {
                bytes.push(u8::try_from(next_random(&mut state) & 0xff).expect("bounded byte"));
            }
            if case % 3 == 0 {
                bytes[length - 2] = 0xff;
                bytes[length - 1] = 0xac;
            }
            let mut optimized = ArithmeticDecoder::new(&bytes).expect("decoder");
            let mut oracle = ArithmeticDecoder::new(&bytes).expect("decoder");
            let mut optimized_contexts = Vec::new();
            let mut oracle_contexts = Vec::new();
            for _ in 0..64 {
                let context = ArithmeticContext::from_state(
                    (next_random(&mut state) % 47) as u8,
                    (next_random(&mut state) % 2) as u8,
                )
                .expect("valid state");
                optimized_contexts.push(context);
                oracle_contexts.push(context);
            }

            for _ in 0..512 {
                let index = usize::try_from(next_random(&mut state) % 64).expect("bounded context");
                let signal = optimized.decode_signal(&mut optimized_contexts[index]);
                let optimized_result = if signal <= 1 {
                    Ok(signal)
                } else {
                    Err(optimized.signal_error(signal))
                };
                let oracle_result = oracle.decode_checked_oracle(&mut oracle_contexts[index]);
                assert_eq!(optimized_result, oracle_result, "case {case}");
                assert_eq!(optimized_contexts, oracle_contexts, "case {case}");
                assert_decoder_state_eq(&optimized, &oracle);
                if optimized_result.is_err() {
                    break;
                }
            }
        }
    }

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

    fn normalization_numeric_oracle<'a>(
        initial: &ArithmeticDecoder<'a>,
    ) -> (u8, ArithmeticDecoder<'a>) {
        let mut expected = refill_fixture(
            initial.bytes,
            initial.byte_position,
            initial.code,
            initial.bit_count,
            initial.marker,
        );
        expected.interval = initial.interval;
        let shifts = initial.interval.leading_zeros().saturating_sub(16);
        for shift in 0..shifts {
            if expected.bit_count == 0 {
                let Some(&next) = expected.bytes.get(expected.byte_position + 1) else {
                    return (RASTER_SIGNAL_EOF, expected);
                };
                let marker = expected.current_byte == 255 && next >= 144;
                let stuffed = expected.current_byte == 255 && !marker;
                let increment = if marker {
                    65_280
                } else {
                    u64::from(next) * if stuffed { 512 } else { 256 }
                };
                if !marker {
                    expected.byte_position += 1;
                    expected.current_byte = next;
                }
                let sum = u64::from(expected.code) + increment;
                if sum > u64::from(u32::MAX) {
                    return (RASTER_SIGNAL_OVERFLOW, expected);
                }
                expected.code = u32::try_from(sum).unwrap();
                expected.bit_count = if stuffed { 7 } else { 8 };
                if marker {
                    expected.marker = Some(ArithmeticMarker {
                        offset: expected.byte_position,
                        code: next,
                    });
                }
            }
            expected.interval = initial.interval << (shift + 1);
            let product = u64::from(expected.code) * 2;
            if product > u64::from(u32::MAX) {
                return (RASTER_SIGNAL_OVERFLOW, expected);
            }
            expected.code = u32::try_from(product).unwrap();
            expected.bit_count -= 1;
        }
        (0, expected)
    }

    fn decision_numeric_oracle<'a>(
        initial: &ArithmeticDecoder<'a>,
        context: ArithmeticContext,
    ) -> (u8, ArithmeticDecoder<'a>, ArithmeticContext) {
        let row = QE_STATES[usize::from(context.state_index)];
        let qe = u32::from(row.qe);
        let reduced = initial.interval - qe;
        let lower = u64::from(initial.code) / 65_536 < u64::from(qe);
        let fast = !lower && reduced >= MIN_INTERVAL;
        let mps_route = (lower && reduced < qe) || (!lower && reduced >= qe);
        let decision = if fast || mps_route {
            context.mps
        } else {
            1 - context.mps
        };
        let next_context = ArithmeticContext {
            state_index: if fast {
                context.state_index
            } else if mps_route {
                row.next_mps
            } else {
                row.next_lps
            },
            mps: if !fast && !mps_route && row.switch_mps {
                decision
            } else {
                context.mps
            },
        };
        let code = u64::from(initial.code) - if lower { 0 } else { u64::from(qe) * 65_536 };
        let mut next = refill_fixture(
            initial.bytes,
            initial.byte_position,
            u32::try_from(code).unwrap(),
            initial.bit_count,
            initial.marker,
        );
        next.interval = if lower { qe } else { reduced };
        if fast {
            (decision, next, next_context)
        } else {
            let (signal, normalized) = normalization_numeric_oracle(&next);
            (
                if signal == 0 { decision } else { signal },
                normalized,
                next_context,
            )
        }
    }

    fn assert_decision_matches_numeric_oracle(
        mut initial: ArithmeticDecoder<'_>,
        context: ArithmeticContext,
    ) -> u8 {
        let bytes = initial.bytes;
        let (signal, expected, expected_context) = decision_numeric_oracle(&initial, context);
        let mut actual = refill_fixture(
            bytes,
            initial.byte_position,
            initial.code,
            initial.bit_count,
            initial.marker,
        );
        actual.interval = initial.interval;
        let mut actual_context = context;
        assert_eq!(actual.decode_signal(&mut actual_context), signal);
        assert_eq!(actual_context, expected_context);
        assert_decoder_state_eq(&actual, &expected);
        assert_eq!(actual.bytes, bytes);
        assert_eq!(actual.bytes.as_ptr(), bytes.as_ptr());

        let mut checked_context = context;
        let expected_result = if signal <= 1 {
            Ok(signal)
        } else {
            Err(ArithmeticError::Malformed(if signal == RASTER_SIGNAL_EOF {
                MalformedError::UnexpectedEof {
                    offset: u64::try_from(expected.byte_position).unwrap() + 1,
                    needed: 1,
                    remaining: 0,
                }
            } else {
                MalformedError::IntegerOverflow
            }))
        };
        assert_eq!(initial.decode(&mut checked_context), expected_result);
        assert_eq!(checked_context, expected_context);
        assert_decoder_state_eq(&initial, &expected);
        assert_eq!(initial.bytes.as_ptr(), bytes.as_ptr());
        signal
    }

    #[test]
    fn complete_decision_registers_match_wide_numeric_oracle_on_success_and_partial_errors() {
        let samples: [&[u8]; 6] = [
            &[0x13],
            &[0, 0x11],
            &[0xff, 0x8f, 0],
            &[0xff, 0x90, 0],
            &[0x5a, 0xff, 0x90],
            &[0; 32],
        ];
        let mut outcomes = [0usize; 3];
        for bytes in samples {
            for position in [0, bytes.len() - 1] {
                let marker = bytes
                    .get(position + 1)
                    .copied()
                    .filter(|&next| bytes[position] == 0xff && next >= 144)
                    .map(|code| ArithmeticMarker {
                        offset: position,
                        code,
                    });
                for marker in [None, marker] {
                    for (index, row) in QE_STATES.iter().copied().enumerate() {
                        let qe = u32::from(row.qe);
                        for mps in 0..=1 {
                            let context =
                                ArithmeticContext::from_state(u8::try_from(index).unwrap(), mps)
                                    .unwrap();
                            for interval in [
                                MIN_INTERVAL,
                                0xffff,
                                (2 * qe).max(MIN_INTERVAL),
                                (MIN_INTERVAL + qe).min(0xffff),
                            ] {
                                for code in [
                                    0,
                                    (qe - 1) << 16,
                                    qe << 16,
                                    (qe + 1) << 16,
                                    u32::MAX / 2,
                                    u32::MAX / 2 + 1,
                                    u32::MAX,
                                ] {
                                    for count in [0, 1, 7, 8] {
                                        let mut initial =
                                            refill_fixture(bytes, position, code, count, marker);
                                        initial.interval = interval;
                                        let signal = assert_decision_matches_numeric_oracle(
                                            initial, context,
                                        );
                                        let outcome = if signal <= 1 {
                                            0
                                        } else if signal == RASTER_SIGNAL_EOF {
                                            1
                                        } else {
                                            assert_eq!(signal, RASTER_SIGNAL_OVERFLOW);
                                            2
                                        };
                                        outcomes[outcome] += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(outcomes.into_iter().all(|count| count > 0));
    }

    fn patterned_integer_contexts() -> [ArithmeticContext; INTEGER_CONTEXT_COUNT] {
        let mut contexts = [ArithmeticContext::new(); INTEGER_CONTEXT_COUNT];
        for (index, context) in contexts.iter_mut().enumerate() {
            *context = ArithmeticContext::from_state(
                u8::try_from(index % 47).unwrap(),
                u8::try_from(index % 2).unwrap(),
            )
            .unwrap();
        }
        contexts
    }

    fn assert_integer_bit_matches_numeric_oracle(
        mut decoder: ArithmeticDecoder<'_>,
        prior: u16,
        mut contexts: [ArithmeticContext; INTEGER_CONTEXT_COUNT],
    ) -> u8 {
        let index = usize::from(prior);
        let before = contexts;
        let (signal, expected_decoder, expected_context) =
            decision_numeric_oracle(&decoder, before[index]);
        let mut expected_contexts = before;
        expected_contexts[index] = expected_context;
        let mut previous = prior;
        let result = decoder.decode_integer_bit(&mut previous, &mut contexts);
        if signal <= 1 {
            assert_eq!(result, Ok(signal));
            let wide = u32::from(prior) * 2 + u32::from(signal);
            let expected = if prior < 256 {
                u16::try_from(wide).unwrap()
            } else {
                256 + u16::try_from(wide % 256).unwrap()
            };
            assert_eq!(previous, expected);
        } else {
            let expected = if signal == RASTER_SIGNAL_EOF {
                eof_numeric_error(expected_decoder.byte_position + 1, 1, 0)
            } else {
                assert_eq!(signal, RASTER_SIGNAL_OVERFLOW);
                ArithmeticError::Malformed(MalformedError::IntegerOverflow)
            };
            assert_eq!(result, Err(expected));
            assert_eq!(previous, prior);
        }
        assert_eq!(contexts, expected_contexts);
        assert_decoder_state_eq(&decoder, &expected_decoder);
        assert_eq!(decoder.bytes.as_ptr(), expected_decoder.bytes.as_ptr());
        signal
    }

    #[test]
    fn integer_bit_history_and_context_frames_match_every_history_and_probability_row() {
        let base = patterned_integer_contexts();
        let cases: [(&[u8], u32, u8); 3] =
            [(&[0, 0, 0, 0], 0, 8), (&[0], 0, 0), (&[0, 0], u32::MAX, 8)];
        let mut outcomes = [0usize; 3];
        for prior in 0..512_u16 {
            for state_index in 0..47 {
                for mps in 0..=1 {
                    for (bytes, code, count) in cases {
                        let mut contexts = base;
                        contexts[usize::from(prior)] =
                            ArithmeticContext::from_state(state_index, mps).unwrap();
                        let decoder = decision_fixture(bytes, MIN_INTERVAL, code, count);
                        let signal =
                            assert_integer_bit_matches_numeric_oracle(decoder, prior, contexts);
                        let outcome = if signal <= 1 {
                            0
                        } else if signal == RASTER_SIGNAL_EOF {
                            1
                        } else {
                            2
                        };
                        outcomes[outcome] += 1;
                    }
                }
            }
        }
        assert_eq!(outcomes.iter().sum::<usize>(), 512 * 47 * 2 * 3);
        assert!(outcomes.into_iter().all(|count| count > 0));
    }

    #[test]
    fn iaid_preflight_errors_preserve_every_field_without_decoding() {
        let bytes = [0x13];
        for code_length in 0..=u8::MAX {
            let required = (code_length <= MAX_IAID_CODE_LENGTH).then(|| 1_u64 << code_length);
            for count in [0, 1, 2, 3, 7, 31, 511] {
                if required.is_some_and(|required| count as u64 >= required) {
                    continue;
                }
                let mut contexts = vec![ArithmeticContext::from_state(1, 1).unwrap(); count];
                let before_contexts = contexts.clone();
                let mut decoder = decision_fixture(&bytes, MIN_INTERVAL, u32::MAX, 8);
                let before = decision_fixture(&bytes, MIN_INTERVAL, u32::MAX, 8);
                let error = match required {
                    Some(required) => ArithmeticError::InsufficientContexts {
                        required: usize::try_from(required).unwrap(),
                        actual: count,
                    },
                    None => ArithmeticError::InvalidIaidCodeLength(code_length),
                };
                assert_eq!(decoder.decode_iaid(code_length, &mut contexts), Err(error));
                assert_eq!(contexts, before_contexts);
                assert_decoder_state_eq(&decoder, &before);
                assert_eq!(decoder.bytes.as_ptr(), before.bytes.as_ptr());
            }
        }
    }

    fn eof_numeric_error(offset: usize, needed: usize, remaining: usize) -> ArithmeticError {
        if [offset, needed, remaining]
            .into_iter()
            .any(|value| value as u128 > u128::from(u64::MAX))
        {
            ArithmeticError::Malformed(MalformedError::IntegerOverflow)
        } else {
            ArithmeticError::Malformed(MalformedError::UnexpectedEof {
                offset: offset as u64,
                needed: needed as u64,
                remaining: remaining as u64,
            })
        }
    }

    #[test]
    fn eof_projection_preserves_full_width_fields() {
        let values = [
            0,
            1,
            usize::from(u16::MAX),
            usize::MAX / 2,
            usize::MAX - 1,
            usize::MAX,
        ];
        for offset in values {
            for needed in values {
                for remaining in values {
                    assert_eq!(
                        unexpected_eof(offset, needed, remaining),
                        eof_numeric_error(offset, needed, remaining)
                    );
                }
            }
        }
    }

    #[test]
    fn signal_error_projection_matches_numeric_policy_and_prior_conversion_path() {
        let bytes = [0];
        for position in [0, 1, usize::MAX / 2, usize::MAX - 1, usize::MAX] {
            let mut decoder = decision_fixture(&bytes, MIN_INTERVAL, 0, 0);
            decoder.byte_position = position;
            for signal in 0..=u8::MAX {
                let expected = if signal == RASTER_SIGNAL_EOF && position < usize::MAX {
                    eof_numeric_error(position + 1, 1, 0)
                } else {
                    ArithmeticError::Malformed(MalformedError::IntegerOverflow)
                };
                assert_eq!(decoder.signal_error(signal), expected);
                let prior = if signal == RASTER_SIGNAL_EOF {
                    match position.checked_add(1) {
                        Some(offset) => unexpected_eof(offset, 1, 0),
                        None => MalformedError::IntegerOverflow.into(),
                    }
                } else {
                    MalformedError::IntegerOverflow.into()
                };
                assert_eq!(decoder.signal_error(signal), prior);
            }
        }
    }

    fn assert_normalization_matches_numeric_oracle(initial: ArithmeticDecoder<'_>) {
        let (signal, expected) = normalization_numeric_oracle(&initial);
        let mut actual = refill_fixture(
            initial.bytes,
            initial.byte_position,
            initial.code,
            initial.bit_count,
            initial.marker,
        );
        actual.interval = initial.interval;
        assert_eq!(actual.renormalize_signal(), signal);
        assert_decoder_state_eq(&actual, &expected);
        assert!(std::ptr::eq(actual.bytes.as_ptr(), initial.bytes.as_ptr()));

        let mut checked = initial;
        match (signal, checked.renormalize()) {
            (0, Ok(()))
            | (
                RASTER_SIGNAL_OVERFLOW,
                Err(ArithmeticError::Malformed(MalformedError::IntegerOverflow)),
            ) => {}
            (
                RASTER_SIGNAL_EOF,
                Err(ArithmeticError::Malformed(MalformedError::UnexpectedEof {
                    offset,
                    needed,
                    remaining,
                })),
            ) => {
                assert_eq!(offset, u64::try_from(expected.byte_position + 1).unwrap());
                assert_eq!((needed, remaining), (1, 0));
            }
            (_, result) => panic!("wrong normalization result for signal {signal}: {result:?}"),
        }
        assert_decoder_state_eq(&checked, &expected);
        assert!(std::ptr::eq(
            checked.bytes.as_ptr(),
            expected.bytes.as_ptr()
        ));
    }

    #[test]
    fn normalization_no_refill_matches_closed_form_and_exact_shift_boundaries() {
        let bytes = [0xff, 0xac];
        let marker = Some(ArithmeticMarker {
            offset: 0,
            code: 0xac,
        });
        for interval in 128_u32..=0xffff {
            let shifts = interval.leading_zeros().saturating_sub(16);
            let available = u8::try_from(shifts).unwrap();
            let maximum = u32::MAX >> shifts;
            for bit_count in [available, 8] {
                for code in [0, 1, maximum, maximum.saturating_add(1), u32::MAX] {
                    let mut initial = refill_fixture(&bytes, 0, code, bit_count, marker);
                    initial.interval = interval;
                    if code <= maximum {
                        let mut actual = refill_fixture(&bytes, 0, code, bit_count, marker);
                        actual.interval = interval;
                        actual.renormalize().unwrap();
                        assert_eq!(actual.interval, interval << shifts);
                        assert_eq!(actual.code, code << shifts);
                        assert_eq!(actual.bit_count, bit_count - available);
                        assert_eq!(actual.byte_position, 0);
                        assert_eq!(actual.current_byte, 0xff);
                        assert_eq!(actual.marker, marker);
                    }
                    assert_normalization_matches_numeric_oracle(initial);
                }
            }
        }
    }

    #[test]
    fn normalization_byte_pairs_and_partial_errors_match_wide_numeric_oracle() {
        for first in 0_u8..=u8::MAX {
            for next in 0_u8..=u8::MAX {
                let bytes = [first, next, 0x55];
                let marker = (first == 255 && next >= 144).then_some(ArithmeticMarker {
                    offset: 0,
                    code: next,
                });
                for interval in [1, 255, 0x4000, 0x7fff, MIN_INTERVAL] {
                    for bit_count in [0, 1, 8] {
                        for code in [0, u32::MAX / 2, u32::MAX] {
                            let mut initial = refill_fixture(&bytes, 0, code, bit_count, marker);
                            initial.interval = interval;
                            assert_normalization_matches_numeric_oracle(initial);
                        }
                    }
                }
            }
        }
        for bytes in [&[0_u8][..], &[0xff][..], &[0, 1][..], &[0xff, 0x8f][..]] {
            for position in 0..bytes.len() {
                for interval in [1, 0x4000, MIN_INTERVAL] {
                    for bit_count in [0, 1, 8] {
                        for code in [0, u32::MAX / 2, u32::MAX] {
                            let mut initial =
                                refill_fixture(bytes, position, code, bit_count, None);
                            initial.interval = interval;
                            assert_normalization_matches_numeric_oracle(initial);
                        }
                    }
                }
            }
        }
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
    fn batch_reset_zeros_all_stored_byte_pairs_and_is_idempotent() {
        reset_contexts(&mut []);
        let mut contexts = Vec::new();
        for state_index in 0..=u8::MAX {
            for mps in 0..=u8::MAX {
                contexts.push(ArithmeticContext { state_index, mps });
            }
        }
        let length = contexts.len();
        reset_contexts(&mut contexts);
        assert_eq!(contexts.len(), length);
        assert!(contexts
            .iter()
            .all(|context| context.state_index() == 0 && context.mps() == 0));
        reset_contexts(&mut contexts);
        assert!(contexts
            .iter()
            .all(|context| context.state_index() == 0 && context.mps() == 0));
    }

    #[test]
    fn batch_reset_only_changes_the_selected_slice() {
        let original = [(1, 0), (2, 1), (3, 0), (4, 1), (5, 0), (46, 1)]
            .map(|(index, mps)| ArithmeticContext::from_state(index, mps).expect("valid"));
        for start in 0..=original.len() {
            for end in start..=original.len() {
                let mut contexts = original;
                reset_contexts(&mut contexts[start..end]);
                for (index, actual) in contexts.iter().enumerate() {
                    let expected = if (start..end).contains(&index) {
                        ArithmeticContext::new()
                    } else {
                        original[index]
                    };
                    assert_eq!(*actual, expected, "range {start}..{end}, slot {index}");
                }
            }
        }
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

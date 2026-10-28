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
            Err(error) => (state_index >= 47 || mps > 1)
                && error == ArithmeticError::InvalidContextState { state_index, mps },
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
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == arithmetic_spec::decode_error(self),
    ))]
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
            out == arithmetic_spec::integer_state(*old(self), old(contexts)@).0,
            *final(self) == arithmetic_spec::integer_state(*old(self), old(contexts)@).1,
            final(contexts)@ == arithmetic_spec::integer_state(*old(self), old(contexts)@).2,
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    #[allow(clippy::cast_possible_wrap)]
    pub(crate) fn decode_integer(
        &mut self,
        contexts: &mut [ArithmeticContext; INTEGER_CONTEXT_COUNT],
    ) -> Result<ArithmeticInteger, ArithmeticError> {
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal_with_fuel(arithmetic_spec::integer_selection_state, 6);
        }
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
            .ok_or(ArithmeticError::Malformed(MalformedError::IntegerOverflow))?;
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
            out == arithmetic_spec::iaid_state(*old(self), old(contexts)@, code_length).0,
            *final(self) == arithmetic_spec::iaid_state(*old(self), old(contexts)@, code_length).1,
            final(contexts)@ == arithmetic_spec::iaid_state(*old(self), old(contexts)@, code_length).2,
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
                arithmetic_spec::iaid_run(*old(self), old(contexts)@, 1, code_length, required)
                    == arithmetic_spec::iaid_run(*self, contexts@, previous, remaining, required),
            decreases remaining,
        ))]
        while remaining != 0 {
            #[cfg(verus_keep_ghost)]
            proof_decl! {
                let ghost before = *self;
                let ghost before_contexts = contexts@;
                let ghost before_previous = previous;
                let ghost before_remaining = remaining;
            }
            #[cfg(verus_keep_ghost)]
            proof! {
                reveal_with_fuel(arithmetic_spec::iaid_run, 1);
            }
            if previous >= context_count {
                return Err(ArithmeticError::InsufficientContexts {
                    required,
                    actual: context_count,
                });
            }
            let decision = self.decode(&mut contexts[previous])?;
            let shifted = previous
                .checked_mul(2)
                .ok_or(ArithmeticError::Malformed(MalformedError::IntegerOverflow))?;
            previous = shifted
                .checked_add(usize::from(decision))
                .ok_or(ArithmeticError::Malformed(MalformedError::IntegerOverflow))?;
            remaining -= 1;
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(
                    arithmetic_spec::iaid_run(
                        before, before_contexts, before_previous, before_remaining, required,
                    ) == arithmetic_spec::iaid_run(*self, contexts@, previous, remaining, required)
                );
            }
        }
        let value = previous
            .checked_sub(required)
            .ok_or(ArithmeticError::Malformed(MalformedError::IntegerOverflow))?;
        if value > u32::MAX as usize {
            return Err(ArithmeticError::Malformed(MalformedError::IntegerOverflow));
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
            out == arithmetic_spec::integer_step_state(
                *old(self), old(contexts)@, *old(previous),
            ).0,
            *final(self) == arithmetic_spec::integer_step_state(
                *old(self), old(contexts)@, *old(previous),
            ).1,
            final(contexts)@ == arithmetic_spec::integer_step_state(
                *old(self), old(contexts)@, *old(previous),
            ).2,
            *final(previous) == arithmetic_spec::integer_step_state(
                *old(self), old(contexts)@, *old(previous),
            ).3,
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    fn decode_integer_bit(
        &mut self,
        previous: &mut u16,
        contexts: &mut [ArithmeticContext; INTEGER_CONTEXT_COUNT],
    ) -> Result<u8, ArithmeticError> {
        let prior = *previous;
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(prior & 511 == prior) by (bit_vector)
                requires prior <= 511;
        }
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
            out == arithmetic_spec::integer_bits_run(
                *old(self), old(contexts)@, *old(previous), bit_count, 0,
            ).0,
            *final(self) == arithmetic_spec::integer_bits_run(
                *old(self), old(contexts)@, *old(previous), bit_count, 0,
            ).1,
            final(contexts)@ == arithmetic_spec::integer_bits_run(
                *old(self), old(contexts)@, *old(previous), bit_count, 0,
            ).2,
            *final(previous) == arithmetic_spec::integer_bits_run(
                *old(self), old(contexts)@, *old(previous), bit_count, 0,
            ).3,
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
                arithmetic_spec::integer_bits_run(
                    *old(self), old(contexts)@, *old(previous), bit_count, 0,
                ) == arithmetic_spec::integer_bits_run(
                    *self, contexts@, *previous, remaining, value,
                ),
            decreases remaining,
        ))]
        while remaining != 0 {
            #[cfg(verus_keep_ghost)]
            proof_decl! {
                let ghost before = *self;
                let ghost before_contexts = contexts@;
                let ghost before_previous = *previous;
                let ghost before_remaining = remaining;
                let ghost before_value = value;
            }
            #[cfg(verus_keep_ghost)]
            proof! {
                reveal_with_fuel(arithmetic_spec::integer_bits_run, 1);
            }
            let bit = self.decode_integer_bit(previous, contexts)?;
            let shifted = value
                .checked_mul(2)
                .ok_or(ArithmeticError::Malformed(MalformedError::IntegerOverflow))?;
            value = shifted
                .checked_add(u32::from(bit))
                .ok_or(ArithmeticError::Malformed(MalformedError::IntegerOverflow))?;
            remaining -= 1;
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(
                    arithmetic_spec::integer_bits_run(
                        before, before_contexts, before_previous, before_remaining, before_value,
                    ) == arithmetic_spec::integer_bits_run(
                        *self, contexts@, *previous, remaining, value,
                    )
                );
            }
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
mod tests;

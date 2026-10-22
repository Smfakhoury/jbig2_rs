use vstd::prelude::*;

verus! {

pub(crate) open spec fn context_valid(
    context: super::ArithmeticContext,
) -> bool {
    context.state_index < 47 && context.mps <= 1
}

pub(crate) open spec fn exchange_is_mps(lower: bool, interval: u32, qe: u32) -> bool {
    (interval < qe) == lower
}

pub(crate) open spec fn exchange_decision(
    lower: bool, interval: u32, qe: u32, old_mps: u8,
) -> u8
    recommends old_mps <= 1,
{
    if exchange_is_mps(lower, interval, qe) {
        old_mps
    } else {
        (1 - old_mps) as u8
    }
}

pub(crate) open spec fn exchange_index(
    lower: bool, interval: u32, qe: u32, next_mps: u8, next_lps: u8,
) -> u8 {
    if exchange_is_mps(lower, interval, qe) { next_mps } else { next_lps }
}

pub(crate) open spec fn exchange_mps(
    lower: bool, interval: u32, qe: u32, switch_mps: bool,
    prior_context_mps: u8, old_mps: u8,
) -> u8
    recommends old_mps <= 1,
{
    if !exchange_is_mps(lower, interval, qe) && switch_mps {
        exchange_decision(lower, interval, qe, old_mps)
    } else {
        prior_context_mps
    }
}

pub(crate) closed spec fn entry_decision(
    interval: u32, code: u32, index: u8, mps: u8,
) -> u8
    recommends index < 47, mps <= 1, interval >= super::MIN_INTERVAL,
{
    let state = super::QE_STATES@[index as int];
    let qe = state.qe as u32;
    let reduced = (interval as int - qe as int) as u32;
    if code >> 16 >= qe && reduced >= super::MIN_INTERVAL {
        mps
    } else {
        exchange_decision(code >> 16 < qe, reduced, qe, mps)
    }
}

pub(crate) closed spec fn entry_index(
    interval: u32, code: u32, index: u8,
) -> u8
    recommends index < 47, interval >= super::MIN_INTERVAL,
{
    let state = super::QE_STATES@[index as int];
    let qe = state.qe as u32;
    let reduced = (interval as int - qe as int) as u32;
    if code >> 16 >= qe && reduced >= super::MIN_INTERVAL {
        index
    } else {
        exchange_index(code >> 16 < qe, reduced, qe, state.next_mps, state.next_lps)
    }
}

pub(crate) closed spec fn entry_mps(
    interval: u32, code: u32, index: u8, mps: u8,
) -> u8
    recommends index < 47, mps <= 1, interval >= super::MIN_INTERVAL,
{
    let state = super::QE_STATES@[index as int];
    let qe = state.qe as u32;
    let reduced = (interval as int - qe as int) as u32;
    if code >> 16 >= qe && reduced >= super::MIN_INTERVAL {
        mps
    } else {
        exchange_mps(code >> 16 < qe, reduced, qe, state.switch_mps, mps, mps)
    }
}

// Representation preconditions used by production contracts, not normative rules.
pub(crate) open spec fn decoder_position_valid(
    decoder: &super::ArithmeticDecoder,
) -> bool {
    decoder.byte_position < decoder.bytes@.len()
        && decoder.current_byte == decoder.bytes@[decoder.byte_position as int]
}

pub(crate) open spec fn decoder_state_valid(
    decoder: &super::ArithmeticDecoder,
) -> bool {
    decoder_position_valid(decoder)
        && 0 < decoder.interval
        && decoder.interval <= 0xffff
        && decoder.bit_count <= 8
        && match decoder.marker {
            Some(marker) =>
                marker.offset == decoder.byte_position
                    && marker.offset + 1 < decoder.bytes@.len()
                    && decoder.bytes@[marker.offset as int] == 0xff
                    && decoder.bytes@[(marker.offset + 1) as int] == marker.code
                    && marker.code > 0x8f,
            None => true,
        }
}

pub(crate) open spec fn decoder_valid(
    decoder: &super::ArithmeticDecoder,
) -> bool {
    decoder_state_valid(decoder)
        && decoder.interval >= super::MIN_INTERVAL
}

pub(crate) open spec fn refill_step(
    decoder: &super::ArithmeticDecoder,
) -> (u8, usize, u8, u32, u8, Option<super::ArithmeticMarker>)
    recommends decoder_state_valid(decoder),
{
    let next_position = decoder.byte_position as nat + 1;
    if next_position >= decoder.bytes@.len() {
        (
            super::RASTER_SIGNAL_EOF, decoder.byte_position, decoder.current_byte,
            decoder.code, decoder.bit_count, decoder.marker,
        )
    } else {
        let next = decoder.bytes@[next_position as int];
        let marker = decoder.current_byte == 0xff && next > 0x8f;
        let increment: u32 = if marker {
            0xff00
        } else if decoder.current_byte == 0xff {
            (next as u32) << 9
        } else {
            (next as u32) << 8
        };
        let position = if marker { decoder.byte_position } else { next_position as usize };
        let current = if marker { decoder.current_byte } else { next };
        let code = decoder.code as nat + increment as nat;
        if code > u32::MAX {
            (
                super::RASTER_SIGNAL_OVERFLOW, position, current,
                decoder.code, decoder.bit_count, decoder.marker,
            )
        } else {
            (
                0, position, current, code as u32,
                if decoder.current_byte == 0xff && !marker { 7u8 } else { 8u8 },
                if marker {
                    Some(super::ArithmeticMarker { offset: decoder.byte_position, code: next })
                } else {
                    decoder.marker
                },
            )
        }
    }
}

pub(crate) open spec fn initialized_state(
    bytes: &[u8],
) -> Option<(usize, u8, u32, u8, Option<super::ArithmeticMarker>)> {
    if bytes@.len() < 2 {
        None
    } else {
        let seed = super::ArithmeticDecoder {
            bytes,
            byte_position: 0,
            current_byte: bytes@[0],
            code: (bytes@[0] as u32) << 16,
            interval: super::MIN_INTERVAL,
            bit_count: 0,
            marker: None,
        };
        let refill = refill_step(&seed);
        Some((
            refill.1, refill.2, (refill.3 as nat * 128) as u32,
            (refill.4 as int - 7) as u8, refill.5,
        ))
    }
}

pub(crate) open spec fn renormalization_steps(interval: nat) -> nat
    decreases if 0 < interval < super::MIN_INTERVAL {
        super::MIN_INTERVAL - interval
    } else {
        0
    },
{
    if interval == 0 || interval >= super::MIN_INTERVAL {
        0
    } else {
        1 + renormalization_steps(interval * 2)
    }
}

pub(crate) open spec fn normalized_state(
    decoder: &super::ArithmeticDecoder,
) -> (u8, u32, usize, u8, u32, u8, Option<super::ArithmeticMarker>)
    recommends decoder_state_valid(decoder),
    decreases if 0 < decoder.interval < super::MIN_INTERVAL {
        super::MIN_INTERVAL as int - decoder.interval as int
    } else {
        0
    },
{
    if decoder.interval == 0 || decoder.interval >= super::MIN_INTERVAL {
        (
            0, decoder.interval, decoder.byte_position, decoder.current_byte,
            decoder.code, decoder.bit_count, decoder.marker,
        )
    } else {
        let refill = if decoder.bit_count == 0 {
            refill_step(decoder)
        } else {
            (
                0u8, decoder.byte_position, decoder.current_byte,
                decoder.code, decoder.bit_count, decoder.marker,
            )
        };
        if refill.0 != 0 {
            (
                refill.0, decoder.interval, refill.1, refill.2,
                refill.3, refill.4, refill.5,
            )
        } else {
            let interval = (decoder.interval as nat * 2) as u32;
            if refill.3 as nat * 2 > u32::MAX {
                (
                    super::RASTER_SIGNAL_OVERFLOW, interval, refill.1,
                    refill.2, refill.3, refill.4, refill.5,
                )
            } else {
                normalized_state(&super::ArithmeticDecoder {
                    bytes: decoder.bytes,
                    byte_position: refill.1,
                    current_byte: refill.2,
                    code: (refill.3 as nat * 2) as u32,
                    interval,
                    bit_count: (refill.4 as int - 1) as u8,
                    marker: refill.5,
                })
            }
        }
    }
}

pub(crate) open spec fn exchange_normalization(
    decoder: &super::ArithmeticDecoder, lower: bool, qe: u32,
) -> (u8, u32, usize, u8, u32, u8, Option<super::ArithmeticMarker>)
    recommends decoder_state_valid(decoder), 0 < qe <= 0x5601,
{
    normalized_state(&super::ArithmeticDecoder {
        bytes: decoder.bytes,
        byte_position: decoder.byte_position,
        current_byte: decoder.current_byte,
        code: decoder.code,
        interval: if lower { qe } else { decoder.interval },
        bit_count: decoder.bit_count,
        marker: decoder.marker,
    })
}

pub(crate) open spec fn decision_registers(
    decoder: &super::ArithmeticDecoder, qe: u32, decision: u8,
) -> (u8, u32, usize, u8, u32, u8, Option<super::ArithmeticMarker>)
    recommends decoder_valid(decoder), 0 < qe <= 0x5601, decision <= 1,
{
    let interval = (decoder.interval as int - qe as int) as u32;
    let lower = decoder.code >> 16 < qe;
    let code = if lower {
        decoder.code
    } else {
        (decoder.code as int - (qe << 16) as int) as u32
    };
    if !lower && interval >= super::MIN_INTERVAL {
        (
            decision, interval, decoder.byte_position, decoder.current_byte,
            code, decoder.bit_count, decoder.marker,
        )
    } else {
        let normalized = exchange_normalization(&super::ArithmeticDecoder {
            bytes: decoder.bytes,
            byte_position: decoder.byte_position,
            current_byte: decoder.current_byte,
            code,
            interval,
            bit_count: decoder.bit_count,
            marker: decoder.marker,
        }, lower, qe);
        (
            if normalized.0 == 0 {
                decision
            } else {
                normalized.0
            },
            normalized.1, normalized.2, normalized.3, normalized.4,
            normalized.5, normalized.6,
        )
    }
}

pub(crate) closed spec fn decision_state(
    decoder: &super::ArithmeticDecoder, context: super::ArithmeticContext,
) -> (u8, u32, usize, u8, u32, u8, Option<super::ArithmeticMarker>)
    recommends decoder_valid(decoder), context_valid(context),
{
    decision_registers(
        decoder,
        super::QE_STATES@[context.state_index as int].qe as u32,
        entry_decision(decoder.interval, decoder.code, context.state_index, context.mps),
    )
}

pub(crate) open spec fn eof_error(
    offset: usize, needed: usize, remaining: usize,
) -> super::ArithmeticError {
    if offset as nat > u64::MAX || needed as nat > u64::MAX || remaining as nat > u64::MAX {
        super::ArithmeticError::Malformed(super::MalformedError::IntegerOverflow)
    } else {
        super::ArithmeticError::Malformed(super::MalformedError::UnexpectedEof {
            offset: offset as u64, needed: needed as u64, remaining: remaining as u64,
        })
    }
}

pub(crate) open spec fn error_for_signal(
    decoder: &super::ArithmeticDecoder, signal: u8,
) -> super::ArithmeticError {
    let next = decoder.byte_position as nat + 1;
    if signal == super::RASTER_SIGNAL_EOF && next <= usize::MAX {
        eof_error(next as usize, 1, 0)
    } else {
        super::ArithmeticError::Malformed(super::MalformedError::IntegerOverflow)
    }
}

pub(crate) open spec fn decoder_marker(
    decoder: &super::ArithmeticDecoder,
) -> Option<super::ArithmeticMarker> {
    decoder.marker
}

pub(crate) open spec fn decoder_byte_position(
    decoder: &super::ArithmeticDecoder,
) -> usize {
    decoder.byte_position
}

}

// Keep closed-model reveal ownership here while separating implementation proofs.
include!("../proofs/model_support/arithmetic.rs");

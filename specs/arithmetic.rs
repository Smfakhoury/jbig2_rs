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

pub(super) proof fn entry_model_equations(
    interval: u32, code: u32, index: u8, mps: u8,
)
    requires index < 47, mps <= 1, interval >= super::MIN_INTERVAL,
    ensures ({
        let state = super::QE_STATES@[index as int];
        let qe = state.qe as u32;
        let reduced = (interval as int - qe as int) as u32;
        let fast = code >> 16 >= qe && reduced >= super::MIN_INTERVAL;
        entry_decision(interval, code, index, mps)
            == if fast { mps } else {
                exchange_decision(code >> 16 < qe, reduced, qe, mps)
            }
        && entry_index(interval, code, index)
            == if fast { index } else {
                exchange_index(code >> 16 < qe, reduced, qe, state.next_mps, state.next_lps)
            }
        && entry_mps(interval, code, index, mps)
            == if fast { mps } else {
                exchange_mps(code >> 16 < qe, reduced, qe, state.switch_mps, mps, mps)
            }
    }),
{
    reveal(entry_decision);
    reveal(entry_index);
    reveal(entry_mps);
}

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

pub(crate) proof fn initialization_code_bound(first: u8, next: u8)
    ensures
        (((first as u32) << 16) as nat)
            + (if first == 0xff && next > 0x8f {
                0xff00u32
            } else if first == 0xff {
                (next as u32) << 9
            } else {
                (next as u32) << 8
            }) as nat <= u32::MAX / 128,
{
    assert(((first as u32) << 16) <= 0x00ff_0000u32) by(bit_vector);
    assert(((next as u32) << 9) <= 0x0001_fe00u32) by(bit_vector);
    assert(((next as u32) << 8) <= 0x0000_ff00u32) by(bit_vector);
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

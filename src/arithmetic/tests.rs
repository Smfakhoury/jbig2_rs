#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

use super::*;

const ANNEX_H_DECISIONS: [u8; 32] = [
    0x00, 0x02, 0x00, 0x51, 0x00, 0x00, 0x00, 0xc0, 0x03, 0x52, 0x87, 0x2a, 0xaa, 0xaa, 0xaa, 0xaa,
    0x82, 0xc0, 0x20, 0x00, 0xfc, 0xd7, 0x9e, 0xf6, 0xbf, 0x7f, 0xed, 0x90, 0x4f, 0x46, 0xa3, 0xbf,
];
const ANNEX_H_ENCODED: [u8; 30] = [
    0x84, 0xc7, 0x3b, 0xfc, 0xe1, 0xa1, 0x43, 0x04, 0x02, 0x20, 0x00, 0x00, 0x41, 0x0d, 0xbb, 0x86,
    0xf4, 0x31, 0x7f, 0xff, 0x88, 0xff, 0x37, 0x47, 0x1a, 0xdb, 0x6a, 0xdf, 0xff, 0xac,
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
                    assert_refill_matches_numeric_oracle(&bytes, 0, maximum + 1, bit_count, None);
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
                assert_refill_matches_numeric_oracle(&[0x12, current], 1, code, bit_count, None);
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
    u32::try_from(value).map_err(|_| ArithmeticError::Malformed(MalformedError::IntegerOverflow))
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
    let mut integer_decoder = ArithmeticDecoder::new(&ANNEX_H_ENCODED).expect("integer decoder");
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
                                    let signal =
                                        assert_decision_matches_numeric_oracle(initial, context);
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

fn integer_numeric_bit(
    decoder: &mut ArithmeticDecoder<'_>,
    contexts: &mut [ArithmeticContext; INTEGER_CONTEXT_COUNT],
    previous: &mut u16,
) -> Result<u8, ArithmeticError> {
    let index = usize::from(*previous);
    let (signal, next, context) = decision_numeric_oracle(decoder, contexts[index]);
    *decoder = next;
    contexts[index] = context;
    if signal > 1 {
        return Err(if signal == RASTER_SIGNAL_EOF {
            eof_numeric_error(decoder.byte_position + 1, 1, 0)
        } else {
            assert_eq!(signal, RASTER_SIGNAL_OVERFLOW);
            ArithmeticError::Malformed(MalformedError::IntegerOverflow)
        });
    }
    let next = *previous * 2 + u16::from(signal);
    *previous = if *previous < 256 {
        next
    } else {
        256 + next % 256
    };
    Ok(signal)
}

fn integer_numeric_oracle<'a>(
    initial: &ArithmeticDecoder<'a>,
    contexts: &[ArithmeticContext; INTEGER_CONTEXT_COUNT],
) -> (
    Result<ArithmeticInteger, ArithmeticError>,
    ArithmeticDecoder<'a>,
    [ArithmeticContext; INTEGER_CONTEXT_COUNT],
    Option<usize>,
) {
    let mut decoder = refill_fixture(
        initial.bytes,
        initial.byte_position,
        initial.code,
        initial.bit_count,
        initial.marker,
    );
    decoder.interval = initial.interval;
    let mut bank = *contexts;
    let mut previous = 1;
    let mut selected_leaf = None;
    let result = (|| {
        let sign = integer_numeric_bit(&mut decoder, &mut bank, &mut previous)?;
        let mut ones = 0;
        while ones < 5 {
            if integer_numeric_bit(&mut decoder, &mut bank, &mut previous)? == 0 {
                break;
            }
            ones += 1;
        }
        selected_leaf = Some(ones);
        let (count, offset) = [(2, 0), (4, 4), (6, 20), (8, 84), (12, 340), (32, 4436)][ones];
        let mut value = 0_u64;
        for position in (0..count).rev() {
            let bit = integer_numeric_bit(&mut decoder, &mut bank, &mut previous)?;
            value |= u64::from(bit) << position;
        }
        let magnitude = i64::try_from(value + offset).unwrap();
        Ok(if sign == 1 && magnitude == 0 {
            ArithmeticInteger::OutOfBand
        } else {
            ArithmeticInteger::Value(if sign == 0 { magnitude } else { -magnitude })
        })
    })();
    (result, decoder, bank, selected_leaf)
}

#[test]
fn complete_integer_state_matches_numeric_leaf_oracle_and_prior_procedure() {
    let samples: [&[u8]; 5] = [
        &[0],
        &[0, 0],
        &[0; 64],
        &[0xff, 0x90, 0],
        &[0x13, 0xff, 0x8f, 0],
    ];
    let mut leaves = [0; 6];
    let mut outcomes = [0; 6];
    for salt in 0..47 {
        let contexts = std::array::from_fn(|index| {
            ArithmeticContext::from_state(
                u8::try_from((index * 17 + salt) % 47).unwrap(),
                u8::try_from(((index >> (salt % 3)) + salt) % 2).unwrap(),
            )
            .unwrap()
        });
        for bytes in samples {
            for position in [0, bytes.len() - 1] {
                for code in [0, 0x1234_5678, 0x4000_0000, u32::MAX] {
                    for count in [0, 1, 8] {
                        for interval in [MIN_INTERVAL, 0xffff] {
                            let mut actual = refill_fixture(bytes, position, code, count, None);
                            actual.interval = interval;
                            let (expected, decoder, bank, leaf) =
                                integer_numeric_oracle(&actual, &contexts);
                            let mut prior = refill_fixture(bytes, position, code, count, None);
                            prior.interval = interval;
                            let mut prior_bank = contexts;
                            let result =
                                decode_integer_with(|index| prior.decode(&mut prior_bank[index]));
                            assert_eq!(result, expected);
                            assert_decoder_state_eq(&prior, &decoder);
                            assert_eq!(prior_bank, bank);
                            let mut actual_bank = contexts;
                            assert_eq!(actual.decode_integer(&mut actual_bank), expected);
                            assert_decoder_state_eq(&actual, &decoder);
                            assert_eq!(actual.bytes.as_ptr(), decoder.bytes.as_ptr());
                            assert_eq!(actual_bank, bank);
                            if let Some(index) = leaf {
                                leaves[index] += 1;
                            }
                            let index = match expected {
                                Ok(ArithmeticInteger::Value(0)) => 0,
                                Ok(ArithmeticInteger::Value(value)) if value > 0 => 1,
                                Ok(ArithmeticInteger::Value(_)) => 2,
                                Ok(ArithmeticInteger::OutOfBand) => 3,
                                Err(ArithmeticError::Malformed(
                                    MalformedError::UnexpectedEof { .. },
                                )) => 4,
                                Err(ArithmeticError::Malformed(
                                    MalformedError::IntegerOverflow,
                                )) => 5,
                                other => panic!("unexpected integer outcome: {other:?}"),
                            };
                            outcomes[index] += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(outcomes.iter().sum::<usize>(), 11_280);
    assert!(outcomes.into_iter().all(|count| count > 0), "{outcomes:?}");
    assert!(leaves.into_iter().all(|count| count > 0), "{leaves:?}");
}

fn integer_payload_numeric<'a>(
    initial: &ArithmeticDecoder<'a>,
    contexts: &[ArithmeticContext; INTEGER_CONTEXT_COUNT],
    mut previous: u16,
    bit_count: u8,
) -> (
    Result<u32, ArithmeticError>,
    ArithmeticDecoder<'a>,
    [ArithmeticContext; INTEGER_CONTEXT_COUNT],
    u16,
) {
    let mut decoder = refill_fixture(
        initial.bytes,
        initial.byte_position,
        initial.code,
        initial.bit_count,
        initial.marker,
    );
    decoder.interval = initial.interval;
    let mut bank = *contexts;
    let result = (|| {
        let mut value = 0_u64;
        for position in (0..bit_count).rev() {
            let bit = integer_numeric_bit(&mut decoder, &mut bank, &mut previous)?;
            value |= u64::from(bit) << position;
        }
        Ok(u32::try_from(value).unwrap())
    })();
    (result, decoder, bank, previous)
}

#[test]
fn integer_payload_fold_matches_numeric_weights_history_and_prior_checked_operations() {
    let contexts = std::array::from_fn(|index| {
        ArithmeticContext::from_state(
            u8::try_from((index * 17 + 11) % 47).unwrap(),
            u8::try_from((index / 2) % 2).unwrap(),
        )
        .unwrap()
    });
    let samples: [&[u8]; 5] = [
        &[0],
        &[0, 0],
        &[0; 64],
        &[0xff, 0x90, 0],
        &[0x13, 0xff, 0x8f, 0],
    ];
    let mut cases = 0;
    for bit_count in 0..=32 {
        for history in [0, 1, 255, 256, 511] {
            for bytes in samples {
                for position in [0, bytes.len() - 1] {
                    for code in [0, 0x4000_0000, u32::MAX] {
                        for count in [0, 8] {
                            for interval in [MIN_INTERVAL, 0xffff] {
                                let mut actual = refill_fixture(bytes, position, code, count, None);
                                actual.interval = interval;
                                let (expected, decoder, bank, previous) =
                                    integer_payload_numeric(&actual, &contexts, history, bit_count);
                                let mut prior = refill_fixture(bytes, position, code, count, None);
                                prior.interval = interval;
                                let mut prior_bank = contexts;
                                let mut prior_history = history;
                                let mut decode = |index| prior.decode(&mut prior_bank[index]);
                                let prior_result = (|| {
                                    let mut value = 0_u32;
                                    for _ in 0..bit_count {
                                        let bit = integer_bit(&mut prior_history, &mut decode)?;
                                        value = value
                                            .checked_mul(2)
                                            .and_then(|partial| partial.checked_add(u32::from(bit)))
                                            .ok_or(MalformedError::IntegerOverflow)?;
                                    }
                                    Ok(value)
                                })();
                                assert_eq!(prior_result, expected);
                                assert_decoder_state_eq(&prior, &decoder);
                                assert_eq!(prior_bank, bank);
                                assert_eq!(prior_history, previous);
                                let mut actual_bank = contexts;
                                let mut actual_history = history;
                                assert_eq!(
                                    actual.decode_integer_bits(
                                        &mut actual_history,
                                        bit_count,
                                        &mut actual_bank
                                    ),
                                    expected,
                                );
                                assert_decoder_state_eq(&actual, &decoder);
                                assert_eq!(actual.bytes.as_ptr(), decoder.bytes.as_ptr());
                                assert_eq!(actual_bank, bank);
                                assert_eq!(actual_history, previous);
                                cases += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 19_800);
}

fn iaid_numeric_oracle<'a>(
    initial: &ArithmeticDecoder<'a>,
    contexts: &[ArithmeticContext],
    code_length: u8,
) -> (
    Result<u32, ArithmeticError>,
    ArithmeticDecoder<'a>,
    Vec<ArithmeticContext>,
) {
    let mut decoder = refill_fixture(
        initial.bytes,
        initial.byte_position,
        initial.code,
        initial.bit_count,
        initial.marker,
    );
    decoder.interval = initial.interval;
    let mut bank = contexts.to_vec();
    let mut value = 0_u64;
    for used in 0..code_length {
        let index = usize::try_from((1_u64 << used) | value).unwrap();
        assert!(index < bank.len());
        let (signal, next, context) = decision_numeric_oracle(&decoder, bank[index]);
        decoder = next;
        bank[index] = context;
        if signal > 1 {
            let error = if signal == RASTER_SIGNAL_EOF {
                eof_numeric_error(decoder.byte_position + 1, 1, 0)
            } else {
                assert_eq!(signal, RASTER_SIGNAL_OVERFLOW);
                ArithmeticError::Malformed(MalformedError::IntegerOverflow)
            };
            return (Err(error), decoder, bank);
        }
        value = value * 2 + u64::from(signal);
    }
    (Ok(u32::try_from(value).unwrap()), decoder, bank)
}

fn assert_iaid_matches_numeric_and_prior_procedure(
    mut initial: ArithmeticDecoder<'_>,
    code_length: u8,
    contexts: &[ArithmeticContext],
) -> Result<u32, ArithmeticError> {
    let (expected, decoder, bank) = iaid_numeric_oracle(&initial, contexts, code_length);
    let mut reference = refill_fixture(
        initial.bytes,
        initial.byte_position,
        initial.code,
        initial.bit_count,
        initial.marker,
    );
    reference.interval = initial.interval;
    let mut reference_bank = contexts.to_vec();
    let prior = decode_iaid_with(code_length, reference_bank.len(), |index| {
        reference.decode(&mut reference_bank[index])
    });
    assert_eq!(prior, expected);
    assert_decoder_state_eq(&reference, &decoder);
    assert_eq!(reference_bank, bank);
    let mut actual_bank = contexts.to_vec();
    let actual = initial.decode_iaid(code_length, &mut actual_bank);
    assert_eq!(actual, expected);
    assert_decoder_state_eq(&initial, &decoder);
    assert_eq!(initial.bytes.as_ptr(), decoder.bytes.as_ptr());
    assert_eq!(actual_bank, bank);
    actual
}

#[test]
fn complete_iaid_state_matches_wide_prefix_oracle_and_prior_checked_procedure() {
    let samples: [&[u8]; 5] = [
        &[0],
        &[0, 0],
        &[0; 32],
        &[0xff, 0x90, 0],
        &[0x13, 0xff, 0x8f, 0],
    ];
    let mut outcomes = [0usize; 4];
    let mut empty = 0;
    for code_length in 0..=9 {
        for extra in [0, 3] {
            let count = (1_usize << code_length) + extra;
            let contexts: Vec<_> = (0..count)
                .map(|index| {
                    ArithmeticContext::from_state(
                        u8::try_from((index + 1) % 47).unwrap(),
                        u8::try_from((index + 1) % 2).unwrap(),
                    )
                    .unwrap()
                })
                .collect();
            for bytes in samples {
                for position in [0, bytes.len() - 1] {
                    for code in [0, 0x4000_0000, u32::MAX] {
                        for count in [0, 1, 8] {
                            for interval in [MIN_INTERVAL, 0xffff] {
                                let mut initial =
                                    refill_fixture(bytes, position, code, count, None);
                                initial.interval = interval;
                                match assert_iaid_matches_numeric_and_prior_procedure(
                                    initial,
                                    code_length,
                                    &contexts,
                                ) {
                                    Ok(0) => {
                                        outcomes[0] += 1;
                                        empty += usize::from(code_length == 0);
                                    }
                                    Ok(_) => outcomes[1] += 1,
                                    Err(ArithmeticError::Malformed(
                                        MalformedError::UnexpectedEof { .. },
                                    )) => outcomes[2] += 1,
                                    Err(ArithmeticError::Malformed(
                                        MalformedError::IntegerOverflow,
                                    )) => outcomes[3] += 1,
                                    other => panic!("unexpected IAID outcome: {other:?}"),
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(outcomes.iter().sum::<usize>(), 3600);
    assert!(outcomes.into_iter().all(|count| count > 0));
    assert!(empty > 0);
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
                        let mut initial = refill_fixture(bytes, position, code, bit_count, None);
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

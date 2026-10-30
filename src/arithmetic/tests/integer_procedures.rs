#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

use super::*;

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

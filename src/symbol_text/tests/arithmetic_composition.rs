#![cfg(test)]

use super::*;
use crate::arithmetic::ArithmeticMarker;

type InitialRegisters = (usize, u8, u32, u8, Option<ArithmeticMarker>);

fn assert_checked_result(
    actual: Result<u32, SymbolTextError>,
    expected: Result<u32, SymbolTextError>,
) {
    match (actual, expected) {
        (Ok(actual), Ok(expected)) => assert_eq!(actual, expected),
        (
            Err(SymbolTextError::InvalidCount(actual)),
            Err(SymbolTextError::InvalidCount(expected)),
        ) => assert_eq!(actual, expected),
        (
            Err(SymbolTextError::SymbolIdOutOfRange { id, available }),
            Err(SymbolTextError::SymbolIdOutOfRange {
                id: expected_id,
                available: expected_available,
            }),
        ) => assert_eq!((id, available), (expected_id, expected_available)),
        (
            Err(SymbolTextError::Decode(DecodeError::Malformed(actual))),
            Err(SymbolTextError::Decode(DecodeError::Malformed(expected))),
        ) => assert_eq!(actual, expected),
        (actual, expected) => panic!("checked ID mismatch: {actual:?}, expected {expected:?}"),
    }
}

#[test]
fn checked_symbol_ids_preserve_u32_values_and_exact_rejection_payloads() {
    for available in [0_u32, 1, 2, 3, 7, 31, u32::MAX] {
        for id in [
            0,
            1,
            available.saturating_sub(1),
            available,
            available.saturating_add(1),
            u32::MAX,
        ] {
            let expected = if id < available {
                Ok(id)
            } else {
                Err(SymbolTextError::SymbolIdOutOfRange { id, available })
            };
            assert_checked_result(checked_symbol_id(id, available), expected);
        }
    }
}

#[test]
fn empty_codebook_rejects_before_arithmetic_or_literal_bit_decoding() {
    let bytes = [0x13, 0x80];
    for length in [0, 1, 8, 31, 32, 255] {
        let before = arithmetic_codec_fixture(&bytes, 8, 23);
        let mut actual = SymbolCodec::Arithmetic(arithmetic_codec_fixture(&bytes, 8, 23));
        let request = SymbolIdRequest {
            available_symbols: 0,
            code_length: length,
            huffman: false,
        };
        assert_checked_result(
            crate::symbol_text::codec::decode_checked_symbol_id(&mut actual, request),
            Err(SymbolTextError::InvalidCount(
                "symbol ID with no available symbols",
            )),
        );
        let SymbolCodec::Arithmetic(ref mut after) = actual else {
            panic!("wrong variant");
        };
        assert_eq!(after.integers.slots, before.integers.slots);
        assert_eq!(after.iaid, before.iaid);
        assert_eq!(after.refinement_work, before.refinement_work);
        assert_eq!(
            (
                after.decoder.byte_position,
                after.decoder.current_byte,
                after.decoder.code,
                after.decoder.interval,
                after.decoder.bit_count,
                after.decoder.marker
            ),
            (
                before.decoder.byte_position,
                before.decoder.current_byte,
                before.decoder.code,
                before.decoder.interval,
                before.decoder.bit_count,
                before.decoder.marker
            ),
        );
        let mut literal = SymbolCodec::Huffman(super::huffman_fields::codec(&bytes, 1, 2));
        assert_checked_result(
            crate::symbol_text::codec::decode_checked_symbol_id(
                &mut literal,
                SymbolIdRequest {
                    huffman: true,
                    ..request
                },
            ),
            Err(SymbolTextError::InvalidCount(
                "symbol ID with no available symbols",
            )),
        );
        let SymbolCodec::Huffman(after) = literal else {
            panic!("wrong variant");
        };
        assert_eq!(after.reader.bytes, &bytes);
        assert_eq!(after.reader.bit_position, 1);
        assert_eq!(after.log_strip_size, 2);
        assert_eq!(after.refinement_work, 71);
        assert!(after.symbol_id_table.is_none() && after.refinement.is_none());
    }
}

#[test]
fn checked_arithmetic_ids_preserve_raw_results_errors_and_consumed_state() {
    let samples: [&[u8]; 6] = [
        &[0, 0],
        &[0xff, 0x90],
        &[0; 64],
        &[0x13; 64],
        &[0xaa; 64],
        &[0xff, 0x8f, 0, 0],
    ];
    let mut outcomes = [0; 4];
    for available in [0, 1, 2, 3, 8] {
        for length in [0, 1, 2, 3, 5, 8, 31, 32, 255] {
            for count in [0, 1, 2, 3, 4, 16, 32, 35] {
                for bytes in samples {
                    let before = arithmetic_codec_fixture(bytes, count, usize::from(length) % 47);
                    let banks = before.integers.slots;
                    let mut expected_iaid = before.iaid;
                    let mut expected_decoder = before.decoder;
                    let (expected, decoded_ok) = if available == 0 {
                        outcomes[0] += 1;
                        (
                            Err(SymbolTextError::InvalidCount(
                                "symbol ID with no available symbols",
                            )),
                            false,
                        )
                    } else {
                        match expected_decoder.decode_iaid(length, &mut expected_iaid) {
                            Ok(id) if id < available => {
                                outcomes[1] += 1;
                                (Ok(id), true)
                            }
                            Ok(id) => {
                                outcomes[2] += 1;
                                (
                                    Err(SymbolTextError::SymbolIdOutOfRange { id, available }),
                                    true,
                                )
                            }
                            Err(error) => {
                                outcomes[3] += 1;
                                (Err(error.into()), false)
                            }
                        }
                    };
                    let mut actual = SymbolCodec::Arithmetic(arithmetic_codec_fixture(
                        bytes,
                        count,
                        usize::from(length) % 47,
                    ));
                    assert_checked_result(
                        crate::symbol_text::codec::decode_checked_symbol_id(
                            &mut actual,
                            SymbolIdRequest {
                                available_symbols: available,
                                code_length: length,
                                huffman: false,
                            },
                        ),
                        expected,
                    );
                    let SymbolCodec::Arithmetic(ref mut after) = actual else {
                        panic!("wrong variant");
                    };
                    assert_eq!(after.integers.slots, banks);
                    assert_eq!(after.iaid, expected_iaid);
                    assert_eq!(after.refinement_work, 73);
                    assert!(after.refinement.is_none());
                    assert_arithmetic_cursor_and_continuation(
                        &mut after.decoder,
                        &mut expected_decoder,
                        decoded_ok,
                    );
                }
            }
        }
    }
    assert!(outcomes.into_iter().all(|count| count > 0));
}

#[test]
fn checked_literal_ids_preserve_scalar_bits_errors_and_consumed_positions() {
    let samples: [&[u8]; 5] = [
        &[],
        &[0],
        &[0xff],
        &[0xa5, 0x32, 0xdc, 0x4f, 0x80],
        &[0xff; 8],
    ];
    let mut outcomes = [0; 4];
    for bytes in samples {
        for position in 0..=bytes.len() * 8 {
            for length in (0..=33).chain([255]) {
                for available in [0, 1, 3, 8, u32::MAX] {
                    let raw_bits = super::huffman_fields::scalar(bytes, position, length);
                    let mut actual =
                        SymbolCodec::Huffman(super::huffman_fields::codec(bytes, position, 2));
                    let decoded = crate::symbol_text::codec::decode_checked_symbol_id(
                        &mut actual,
                        SymbolIdRequest {
                            available_symbols: available,
                            code_length: length,
                            huffman: true,
                        },
                    );
                    if available == 0 {
                        outcomes[0] += 1;
                        assert_checked_result(
                            decoded,
                            Err(SymbolTextError::InvalidCount(
                                "symbol ID with no available symbols",
                            )),
                        );
                    } else {
                        match &raw_bits {
                            Ok((id, _)) if *id < available => {
                                outcomes[1] += 1;
                                assert_checked_result(decoded, Ok(*id));
                            }
                            Ok((id, _)) => {
                                outcomes[2] += 1;
                                assert_checked_result(
                                    decoded,
                                    Err(SymbolTextError::SymbolIdOutOfRange { id: *id, available }),
                                );
                            }
                            Err(error) => {
                                outcomes[3] += 1;
                                let Err(SymbolTextError::Decode(DecodeError::Malformed(
                                    actual_error,
                                ))) = decoded
                                else {
                                    panic!("wrong literal decoding error");
                                };
                                assert_eq!(&actual_error, error);
                            }
                        }
                    }
                    let SymbolCodec::Huffman(ref mut after) = actual else {
                        panic!("wrong variant");
                    };
                    let untouched = Ok((0, position));
                    super::huffman_fields::frame_and_continuation(
                        after,
                        bytes,
                        position,
                        2,
                        if available == 0 {
                            &untouched
                        } else {
                            &raw_bits
                        },
                    );
                }
            }
        }
    }
    assert!(outcomes.into_iter().all(|count| count > 0));
}

#[test]
fn fresh_arithmetic_codec_has_zero_banks_exact_size_and_literal_initial_state() {
    let samples: [(&[u8], InitialRegisters); 4] = [
        (&[0, 0], (1, 0, 0, 1, None)),
        (&[0x13, 0x80], (1, 0x80, 0x09c0_0000, 1, None)),
        (
            &[0xff, 0x90],
            (
                0,
                0xff,
                0x7fff_8000,
                1,
                Some(ArithmeticMarker {
                    offset: 0,
                    code: 0x90,
                }),
            ),
        ),
        (&[0xff, 0x8f, 0], (1, 0x8f, 0x800f_0000, 0, None)),
    ];
    let templates = [
        None,
        Some(RefinementTemplate::Template0),
        Some(RefinementTemplate::Template1),
    ];
    for (bytes, expected) in samples {
        for length in 0..=8 {
            for template in templates {
                let codec = crate::symbol_text::codec::new_arithmetic_codec(
                    bytes,
                    length,
                    template,
                    &Limits::default(),
                )
                .unwrap();
                assert_eq!(codec.iaid.len(), 1_usize << length);
                assert_eq!(
                    codec.iaid,
                    vec![ArithmeticContext::new(); 1_usize << length]
                );
                assert_eq!(
                    codec.integers.slots,
                    [[ArithmeticContext::new(); 512]; INTEGER_FIELD_COUNT]
                );
                assert_eq!(codec.refinement_work, 0);
                assert_eq!(codec.refinement.is_some(), template.is_some());
                assert_eq!(codec.decoder.bytes, bytes);
                assert_eq!(codec.decoder.bytes.as_ptr(), bytes.as_ptr());
                assert_eq!(codec.decoder.interval, 0x8000);
                assert_eq!(
                    (
                        codec.decoder.byte_position,
                        codec.decoder.current_byte,
                        codec.decoder.code,
                        codec.decoder.bit_count,
                        codec.decoder.marker
                    ),
                    expected,
                );
            }
        }
    }
    assert_eq!(
        IntegerContexts::new().slots,
        [[ArithmeticContext::new(); 512]; INTEGER_FIELD_COUNT],
    );
}

#[test]
fn arithmetic_enum_integer_preserves_direct_result_state_and_other_banks() {
    let fields = [
        IntegerField::AggregationInstances,
        IntegerField::HeightDelta,
        IntegerField::WidthDelta,
        IntegerField::ExportRun,
        IntegerField::StripDelta,
        IntegerField::FirstS,
        IntegerField::SubsequentS,
        IntegerField::WithinStripT,
        IntegerField::RefinementWidth,
        IntegerField::RefinementHeight,
        IntegerField::RefinementX,
        IntegerField::RefinementY,
        IntegerField::RefinementSize,
        IntegerField::RefinementIndicator,
        IntegerField::DictionaryBitmapSize,
    ];
    let samples: [&[u8]; 6] = [
        &[0, 0],
        &[0xff, 0x90],
        &[0; 64],
        &[0x13; 64],
        &[0xaa; 64],
        &[0xff, 0x8f, 0, 0],
    ];
    let mut outcomes = [0; 2];
    for (slot, field) in fields.into_iter().enumerate() {
        for bytes in samples {
            for salt in [0, 1, 23, 46] {
                let initial = arithmetic_codec_fixture(bytes, 7, salt);
                let before_banks = initial.integers.slots;
                let before_iaid = initial.iaid.clone();
                let mut expected_bank = before_banks[slot];
                let mut expected_decoder = ArithmeticDecoder::new(bytes).unwrap();
                let expected = expected_decoder
                    .decode_integer(&mut expected_bank)
                    .map(|value| match value {
                        ArithmeticInteger::Value(value) => EntropyInteger::Value(value),
                        ArithmeticInteger::OutOfBand => EntropyInteger::OutOfBand,
                    })
                    .map_err(SymbolTextError::from);
                let mut actual = SymbolCodec::Arithmetic(initial);
                let success = assert_arithmetic_forwarding(actual.decode_integer(field), expected);
                outcomes[usize::from(success)] += 1;
                let SymbolCodec::Arithmetic(ref mut after) = actual else {
                    panic!("arithmetic dispatch changed its variant");
                };
                assert_eq!(after.integers.slots[slot], expected_bank);
                for (other, bank) in before_banks.iter().enumerate() {
                    if other != slot {
                        assert_eq!(after.integers.slots[other], *bank);
                    }
                }
                assert_eq!(after.iaid, before_iaid);
                assert!(after.refinement.is_none());
                assert_eq!(after.refinement_work, 73);
                assert_arithmetic_cursor_and_continuation(
                    &mut after.decoder,
                    &mut expected_decoder,
                    success,
                );
            }
        }
    }
    assert_eq!(outcomes.iter().sum::<usize>(), 360);
    assert!(outcomes.into_iter().all(|count| count > 0));
}

#[test]
fn arithmetic_enum_iaid_preserves_direct_result_range_and_full_frames() {
    let samples: [&[u8]; 6] = [
        &[0, 0],
        &[0xff, 0x90],
        &[0; 64],
        &[0x13; 64],
        &[0xaa; 64],
        &[0xff, 0x8f, 0, 0],
    ];
    let mut outcomes = [0; 2];
    for length in [0, 1, 2, 3, 5, 8, 31, 32, 255] {
        for count in [0, 1, 2, 3, 4, 16, 32, 35] {
            for bytes in samples {
                let initial = arithmetic_codec_fixture(bytes, count, usize::from(length) % 47);
                let before_banks = initial.integers.slots;
                let mut expected_bank = initial.iaid.clone();
                let mut expected_decoder = ArithmeticDecoder::new(bytes).unwrap();
                let expected = expected_decoder
                    .decode_iaid(length, &mut expected_bank)
                    .map_err(SymbolTextError::from);
                if let Ok(value) = &expected {
                    assert!(*value < (1_u32 << length));
                }
                let mut actual = SymbolCodec::Arithmetic(initial);
                let success = assert_arithmetic_forwarding(
                    actual.decode_symbol_id(SymbolIdRequest {
                        available_symbols: 0,
                        code_length: length,
                        huffman: false,
                    }),
                    expected,
                );
                outcomes[usize::from(success)] += 1;
                let SymbolCodec::Arithmetic(ref mut after) = actual else {
                    panic!("arithmetic dispatch changed its variant");
                };
                assert_eq!(after.integers.slots, before_banks);
                assert_eq!(after.iaid, expected_bank);
                assert!(after.refinement.is_none());
                assert_eq!(after.refinement_work, 73);
                assert_arithmetic_cursor_and_continuation(
                    &mut after.decoder,
                    &mut expected_decoder,
                    success,
                );
            }
        }
    }
    assert_eq!(outcomes.iter().sum::<usize>(), 432);
    assert!(outcomes.into_iter().all(|count| count > 0));
}

#![cfg(test)]

use super::*;
use crate::arithmetic::ArithmeticMarker;

type InitialRegisters = (usize, u8, u32, u8, Option<ArithmeticMarker>);

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

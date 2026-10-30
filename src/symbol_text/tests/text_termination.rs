#![cfg(test)]

use super::super::text::decode_text_region_body;
use super::*;

fn config(huffman: bool, log_strips: u16) -> TextRegionConfig {
    parse_text_region_flags(
        u16::from(huffman) | (log_strips << 2),
        huffman.then_some(0),
        false,
    )
    .unwrap()
}

#[test]
fn final_strip_terminator_precedes_following_dictionary_fields() {
    let limits = Limits::default();
    let symbol = Bitmap::filled(1, 1, true, &limits).unwrap();
    let symbols = [&symbol];
    for huffman in [false, true] {
        for instances in [1, 2] {
            let mut integers = VecDeque::from([
                EntropyInteger::Value(0),
                EntropyInteger::Value(0),
                EntropyInteger::Value(0),
            ]);
            if instances == 2 {
                integers.push_back(EntropyInteger::Value(1));
            }
            integers.push_back(EntropyInteger::OutOfBand);
            integers.push_back(EntropyInteger::Value(37));
            let mut codec = SymbolCodec::Scripted(ScriptedCodec {
                integers,
                symbol_id: 0,
            });
            let mut resources = ResourceTracker::default();
            let bitmap = decode_text_region_body(
                &mut codec,
                SymbolCodebook::Borrowed(&symbols),
                0,
                instances,
                1,
                instances,
                &config(huffman, 0),
                [(0, 0); 2],
                &limits,
                &mut resources,
            )
            .unwrap();
            assert_eq!(
                bitmap.bytes(),
                if instances == 1 {
                    &[0x80][..]
                } else {
                    &[0xc0][..]
                }
            );
            assert_eq!(resources.work_units, u64::from(instances) * 2);
            assert_eq!(resources.decoded_bitmap_bytes, 1);
            assert_eq!(
                required_integer(&mut codec, IntegerField::WidthDelta).unwrap(),
                37
            );
            let SymbolCodec::Scripted(codec) = codec else {
                unreachable!()
            };
            assert!(codec.integers.is_empty());
        }
    }
}

#[test]
fn declared_instance_count_does_not_hide_a_missing_or_overfull_strip_end() {
    let limits = Limits::default();
    let symbol = Bitmap::filled(1, 1, true, &limits).unwrap();
    let symbols = [&symbol];
    for huffman in [false, true] {
        let mut codec = SymbolCodec::Scripted(ScriptedCodec {
            integers: VecDeque::from([
                EntropyInteger::Value(0),
                EntropyInteger::Value(0),
                EntropyInteger::Value(0),
            ]),
            symbol_id: 0,
        });
        let mut resources = ResourceTracker::default();
        let result = decode_text_region_body(
            &mut codec,
            SymbolCodebook::Borrowed(&symbols),
            0,
            1,
            1,
            1,
            &config(huffman, 0),
            [(0, 0); 2],
            &limits,
            &mut resources,
        );
        assert!(matches!(
            result,
            Err(SymbolTextError::InvalidState("missing scripted integer"))
        ));
        assert_eq!(resources.work_units, 2);

        let mut codec = SymbolCodec::Scripted(ScriptedCodec {
            integers: VecDeque::from([
                EntropyInteger::Value(0),
                EntropyInteger::Value(0),
                EntropyInteger::Value(0),
                EntropyInteger::Value(0),
                EntropyInteger::Value(4),
                EntropyInteger::OutOfBand,
                EntropyInteger::Value(37),
            ]),
            symbol_id: 0,
        });
        let mut resources = ResourceTracker::default();
        let result = decode_text_region_body(
            &mut codec,
            SymbolCodebook::Borrowed(&symbols),
            0,
            1,
            1,
            1,
            &config(huffman, 2),
            [(0, 0); 2],
            &limits,
            &mut resources,
        );
        assert!(matches!(
            result,
            Err(SymbolTextError::InvalidCount("too many text instances"))
        ));
        let SymbolCodec::Scripted(codec) = codec else {
            unreachable!()
        };
        assert_eq!(
            codec.integers,
            VecDeque::from([EntropyInteger::OutOfBand, EntropyInteger::Value(37),])
        );
        assert_eq!(resources.work_units, 2);
        assert_eq!(resources.decoded_bitmap_bytes, 1);
    }
}

#[test]
fn every_strip_consumes_its_own_terminator_without_eating_the_next_field() {
    let limits = Limits::default();
    let symbol = Bitmap::filled(1, 1, true, &limits).unwrap();
    let symbols = [&symbol];
    let mut codec = SymbolCodec::Scripted(ScriptedCodec {
        integers: VecDeque::from([
            EntropyInteger::Value(0),
            EntropyInteger::Value(0),
            EntropyInteger::Value(0),
            EntropyInteger::OutOfBand,
            EntropyInteger::Value(1),
            EntropyInteger::Value(1),
            EntropyInteger::OutOfBand,
            EntropyInteger::Value(37),
        ]),
        symbol_id: 0,
    });
    let mut resources = ResourceTracker::default();
    let bitmap = decode_text_region_body(
        &mut codec,
        SymbolCodebook::Borrowed(&symbols),
        0,
        2,
        2,
        2,
        &config(false, 0),
        [(0, 0); 2],
        &limits,
        &mut resources,
    )
    .unwrap();
    assert_eq!(bitmap.bytes(), &[0x80, 0x40]);
    assert_eq!(resources.work_units, 4);
    assert_eq!(resources.decoded_bitmap_bytes, 2);
    assert_eq!(
        required_integer(&mut codec, IntegerField::WidthDelta).unwrap(),
        37
    );
}

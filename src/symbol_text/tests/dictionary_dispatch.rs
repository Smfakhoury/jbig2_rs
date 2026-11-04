#![cfg(test)]

use super::super::dictionary::decode_one_new_symbol;
use super::*;

fn header(huffman: bool, refinement: bool) -> SymbolDictionaryHeader {
    let flags = u16::from(huffman) | (u16::from(refinement) << 1);
    SymbolDictionaryHeader {
        config: parse_symbol_dictionary_flags(flags).unwrap(),
        adaptive: [TemplateOffset::new(0, 0); 4],
        adaptive_count: 0,
        refinement_adaptive: [TemplateOffset::new(0, 0); 2],
        exported_symbols: 1,
        new_symbols: 1,
    }
}

#[test]
fn actual_dictionary_id_request_counts_imported_and_new_with_enclosing_width() {
    let limits = Limits::default();
    let bytes = [0xff; 2];
    for width in 1_u8..=4 {
        let count = (1_u32 << width) - 1;
        let symbols: Vec<Bitmap> = (0..count)
            .map(|_| Bitmap::blank(1, 1, &limits).unwrap())
            .collect();
        for imported_count in 0..=symbols.len() {
            let (imported, new_symbols) = symbols.split_at(imported_count);
            let borrowed: Vec<&Bitmap> = imported.iter().collect();
            for book in [
                SymbolCodebook::Contiguous(imported),
                SymbolCodebook::Borrowed(&borrowed),
            ] {
                for position in 0..=8 {
                    for huffman in [false, true] {
                        let concrete = super::huffman_fields::codec(&bytes, position, 0);
                        let mut codec = SymbolCodec::Huffman(concrete);
                        let mut resources = ResourceTracker::default();
                        let result =
                            super::super::dictionary::decode_dictionary_single_instance_refinement(
                                &mut codec,
                                book,
                                new_symbols,
                                width,
                                1,
                                1,
                                0,
                                [(0, 0); 2],
                                huffman,
                                &limits,
                                &mut resources,
                            );
                        assert!(matches!(
                            result,
                            Err(SymbolTextError::SymbolIdOutOfRange { id, available })
                                if id == count && available == count
                        ));
                        assert_eq!(resources, ResourceTracker::default());
                        let SymbolCodec::Huffman(after) = codec else {
                            panic!("changed codec variant");
                        };
                        assert_eq!(after.reader.bit_position, position + usize::from(width));
                        assert_eq!(after.reader.bytes, bytes);
                        assert!(after.symbol_id_table.is_none());
                        assert_eq!(after.refinement_work, 71);
                    }
                }
            }
        }
    }
}

#[test]
fn single_instance_refinement_reaches_the_existing_raw_callback() {
    let limits = Limits::default();
    let reference = Bitmap::blank(1, 1, &limits).unwrap();
    let references = [&reference];
    for huffman in [false, true] {
        let mut integers = VecDeque::from([EntropyInteger::Value(0), EntropyInteger::Value(0)]);
        if huffman {
            integers.push_back(EntropyInteger::Value(2));
        }
        let mut codec = SymbolCodec::Scripted(ScriptedCodec {
            integers,
            symbol_id: 0,
        });
        let mut resources = ResourceTracker::default();
        let mut generic_work = 0;
        let bitmap = decode_one_new_symbol(
            &header(huffman, true),
            &mut codec,
            SymbolCodebook::Borrowed(&references),
            &[],
            None,
            &mut generic_work,
            Some(1),
            1,
            1,
            0,
            [(0, 0); 2],
            &limits,
            &mut resources,
        )
        .expect("one-instance refinement must not reject its required count");
        assert_eq!(
            (bitmap.width(), bitmap.height(), bitmap.bytes()),
            (1, 1, &[0][..])
        );
        assert_eq!(resources.work_units, 0);
        assert_eq!(resources.decoded_bitmap_bytes, 1);
        assert_eq!(generic_work, 0);
        let SymbolCodec::Scripted(codec) = codec else {
            unreachable!()
        };
        assert!(codec.integers.is_empty());
    }
}

#[test]
fn direct_coded_symbol_rejects_an_aggregation_count_before_decoding() {
    let limits = Limits::default();
    let mut codec = SymbolCodec::Scripted(ScriptedCodec {
        integers: VecDeque::from([EntropyInteger::Value(9)]),
        symbol_id: 0,
    });
    let mut resources = ResourceTracker::default();
    let mut generic_work = 0;
    let result = decode_one_new_symbol(
        &header(false, false),
        &mut codec,
        SymbolCodebook::Contiguous(&[]),
        &[],
        None,
        &mut generic_work,
        Some(1),
        1,
        1,
        0,
        [(0, 0); 2],
        &limits,
        &mut resources,
    );
    assert!(matches!(
        result,
        Err(SymbolTextError::InvalidState(
            "aggregation count on direct-coded symbol",
        ))
    ));
    assert_eq!(resources, ResourceTracker::default());
    assert_eq!(generic_work, 0);
    let SymbolCodec::Scripted(codec) = codec else {
        unreachable!()
    };
    assert_eq!(codec.integers.len(), 1);
}

#[test]
fn refinement_still_rejects_missing_and_zero_instance_counts() {
    let limits = Limits::default();
    for count in [None, Some(0)] {
        let mut codec = SymbolCodec::Scripted(ScriptedCodec {
            integers: VecDeque::new(),
            symbol_id: 0,
        });
        let mut resources = ResourceTracker::default();
        let mut generic_work = 0;
        let result = decode_one_new_symbol(
            &header(false, true),
            &mut codec,
            SymbolCodebook::Contiguous(&[]),
            &[],
            None,
            &mut generic_work,
            count,
            1,
            1,
            0,
            [(0, 0); 2],
            &limits,
            &mut resources,
        );
        assert!(matches!(
            (count, result),
            (
                None,
                Err(SymbolTextError::InvalidState(
                    "missing aggregation-instance count"
                ))
            ) | (
                Some(0),
                Err(SymbolTextError::InvalidCount(
                    "zero-instance aggregate symbol"
                ))
            )
        ));
        assert_eq!(resources, ResourceTracker::default());
    }
}

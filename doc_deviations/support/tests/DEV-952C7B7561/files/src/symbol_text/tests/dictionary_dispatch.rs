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

fn dictionary_bits(bits: &str) -> Vec<u8> {
    let mut bytes = vec![0; bits.len().div_ceil(8)];
    for (index, bit) in bits.bytes().enumerate() {
        assert!(bit == b'0' || bit == b'1');
        bytes[index / 8] |= (bit - b'0') << (7 - index % 8);
    }
    bytes
}

fn dictionary_export_run(value: usize) -> String {
    assert!(value <= 32);
    if value < 16 {
        format!("0{value:04b}")
    } else {
        format!("10{:08b}", value - 16)
    }
}

fn fixed_id_dictionary_payload(imported_count: usize, ids: [usize; 2]) -> Vec<u8> {
    assert!(imported_count > 0 && imported_count <= 32);
    assert!(ids.iter().all(|id| *id < imported_count));
    // T.88 6.5.8.2.3: include the declared NEW symbol in the fixed width,
    // even though it is not available to either instance of its own aggregate.
    let width = (usize::BITS - imported_count.leading_zeros()) as usize;
    let mut payload = 0x1003_u16.to_be_bytes().to_vec();
    payload.extend_from_slice(&1_u32.to_be_bytes());
    payload.extend_from_slice(&1_u32.to_be_bytes());

    // B.4 DH=1; B.2 DW=16; B.1 REFAGGNINST=2.
    let mut bits = String::from("0");
    bits.push_str("11110000101");
    bits.push_str("00010");
    // Table 17: B.11 initial DT=1 and DT=1, B.6 FS=0.
    // No 7.4.3.1.7 symbol-ID table header or byte alignment belongs here.
    bits.push_str("00");
    bits.push_str("000000000");
    bits.push_str(&format!("{:0width$b}0", ids[0]));
    // B.8 DS=1 puts the next eight-pixel symbol at x=8; RI=0 for both.
    bits.push_str("001");
    bits.push_str(&format!("{:0width$b}0", ids[1]));
    bits.push_str("01"); // B.8 strip OOB, including after the final instance.
    bits.push_str("111111"); // B.2 height-class OOB.
    bits.push_str(&dictionary_export_run(imported_count));
    bits.push_str(&dictionary_export_run(1));
    payload.extend_from_slice(&dictionary_bits(&bits));
    payload
}

#[test]
fn huffman_dictionary_aggregation_preserves_standard_fixed_symbol_ids() {
    single_instance_refinement_reaches_the_existing_raw_callback();
    let limits = Limits::default();
    let control_symbol = bitmap_from_packed_bytes(8, 1, &[0xa5], &limits).unwrap();
    let mut control_payload = 0x1003_u16.to_be_bytes().to_vec();
    control_payload.extend_from_slice(&1_u32.to_be_bytes());
    control_payload.extend_from_slice(&0_u32.to_be_bytes());
    control_payload.extend_from_slice(&dictionary_bits("0000000001"));
    let control = decode_symbol_dictionary(&SymbolDictionaryInput {
        payload: &control_payload,
        imported_symbols: SymbolCodebook::Contiguous(std::slice::from_ref(&control_symbol)),
        custom_tables: &[],
        used_contexts: None,
        limits: &limits,
    })
    .expect("valid import-only Huffman refinement dictionary control");
    assert_eq!(control.exported_symbols.len(), 1);
    assert_eq!(control.exported_symbols[0].bytes(), &[0xa5]);
    assert_eq!(control.consumed_bytes, control_payload.len());
    assert!(control.retained_contexts.is_none());

    let mut failures = Vec::new();
    let mut seed = 0x88_65_82_03_u32;
    let mut checked = 0;
    for count in [1_usize, 2, 3, 4, 7, 8, 15, 16, 31, 32] {
        let pixels: Vec<u8> = (0..count)
            .map(|id| u8::try_from(id).unwrap().wrapping_mul(37) ^ 0xa5)
            .collect();
        let imported: Vec<Bitmap> = pixels
            .iter()
            .map(|pixel| bitmap_from_packed_bytes(8, 1, &[*pixel], &limits).unwrap())
            .collect();
        for bitmap in &imported {
            assert_eq!(
                (bitmap.width(), bitmap.height(), bitmap.stride()),
                (8, 1, 1)
            );
            assert_eq!(bitmap.bytes().len(), 1);
        }
        for first in 0..count {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let second = if first == 0 {
                count - 1
            } else {
                usize::try_from(seed).unwrap() % count
            };
            let ids = [first, second];
            let payload = fixed_id_dictionary_payload(count, ids);
            let (header, header_len) = parse_symbol_dictionary_header(&payload, &limits).unwrap();
            assert_eq!(header_len, 10);
            assert!(header.config.huffman && header.config.refinement_aggregation);
            assert_eq!(header.config.refinement_template, 1);
            assert!(!header.config.bitmap_context_used && !header.config.bitmap_context_retained);
            assert_eq!((header.new_symbols, header.exported_symbols), (1, 1));
            assert_eq!(payload[0..2], [0x10, 0x03]);
            eprintln!(
                "DEEPSPEC_FIXED_DICTIONARY_IDS count={count} ids={ids:?} payload={payload:02x?}"
            );
            let result = decode_symbol_dictionary(&SymbolDictionaryInput {
                payload: &payload,
                imported_symbols: SymbolCodebook::Contiguous(&imported),
                custom_tables: &[],
                used_contexts: None,
                limits: &limits,
            });
            let expected = [pixels[first], pixels[second]];
            match result {
                Ok(output) => {
                    assert_eq!(
                        output.exported_symbols.len(),
                        1,
                        "count={count} ids={ids:?}"
                    );
                    let bitmap = &output.exported_symbols[0];
                    assert_eq!(
                        (bitmap.width(), bitmap.height(), bitmap.stride()),
                        (16, 1, 2)
                    );
                    assert_eq!(bitmap.bytes(), &expected, "count={count} ids={ids:?}");
                    assert_eq!(output.consumed_bytes, payload.len());
                    assert!(output.retained_contexts.is_none());
                }
                Err(error) => failures.push(format!(
                    "count={count} ids={ids:?}: expected 16x1 {expected:02x?}, got {error:?}"
                )),
            }
            for (bitmap, expected) in imported.iter().zip(&pixels) {
                assert_eq!(bitmap.bytes(), &[*expected], "imported bitmap mutated");
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 119);
    assert!(
        failures.is_empty(),
        "T.88 6.5.8.2.3 fixed-width dictionary IDs must decode without a transmitted table:\n{}",
        failures.join("\n")
    );
}

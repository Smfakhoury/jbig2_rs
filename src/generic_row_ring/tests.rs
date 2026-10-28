#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

use super::*;
use crate::error::{DecodeError, MalformedError};

fn assert_overflow<T: std::fmt::Debug>(result: Result<T, GenericRegionError>) {
    match result {
        Err(GenericRegionError::Decode(DecodeError::Malformed(
            MalformedError::IntegerOverflow,
        ))) => {}
        other => panic!("expected exact integer-overflow classification, got {other:?}"),
    }
}

#[test]
fn allocation_shapes_slots_and_memory_budgets_are_exact() {
    for width in [0, 1, 7, 31, 32, 33, 63, 64, 65, 127] {
        for rows in [0, 1, 2, 3, 4, u32::MAX] {
            let stride = u64::from(width).div_ceil(32);
            let slots = u64::from(rows.min(3));
            let bytes = stride * slots * 4;
            assert_eq!(storage_bytes(width, rows), Some(bytes));
            let ring = GenericRowRing::try_new(width, rows, bytes).unwrap();
            assert_eq!(ring.width, width);
            assert_eq!(ring.stride_words as u64, stride);
            assert_eq!(ring.row_slots as u64, slots);
            assert_eq!(ring.words.len() as u64, stride * slots);
            assert_eq!(ring.charged_bytes, bytes);
            assert!(ring.words.iter().all(|&word| word == 0));
            if bytes > 0 {
                assert!(GenericRowRing::try_new(width, rows, bytes - 1).is_none());
            }
            for row in [0, 1, 2, 3, u32::MAX] {
                let expected = if slots == 0 {
                    None
                } else {
                    Some(usize::try_from(u64::from(row) % slots).unwrap())
                };
                assert_eq!(ring.slot_for_row(row), expected);
            }
            assert_eq!(ring.row_start(None), None);
            assert_eq!(ring.row_start(Some(ring.row_slots)), None);
            assert_eq!(ring.row_start(Some(usize::MAX)), None);
            for slot in 0..ring.row_slots {
                assert_eq!(ring.row_start(Some(slot)), Some(slot * ring.stride_words));
            }
            let limits = Limits {
                max_decoder_managed_memory_bytes: bytes + 30,
                ..Limits::default()
            };
            assert!(fits_managed_memory(width, rows, 10, 20, &limits).unwrap());
            let smaller = Limits {
                max_decoder_managed_memory_bytes: bytes + 29,
                ..limits
            };
            assert!(!fits_managed_memory(width, rows, 10, 20, &smaller).unwrap());
        }
    }
    assert!(GenericRowRing::try_new(u32::MAX, 0, u64::MAX).is_none());
    assert_eq!(storage_bytes(u32::MAX, u32::MAX), Some(1_610_612_736));
    assert_overflow(fits_managed_memory(1, 1, u64::MAX, 0, &Limits::default()));
    assert_overflow(fits_managed_memory(0, 0, u64::MAX, 1, &Limits::default()));
}

fn storage_bits(words: &[u32]) -> Vec<bool> {
    words
        .iter()
        .flat_map(|word| word.to_be_bytes())
        .flat_map(|byte| (0..8).map(move |bit| byte & (128 >> bit) != 0))
        .collect()
}

#[test]
fn packed_windows_match_scalar_storage_and_clipped_pixel_oracles() {
    let mut ring = GenericRowRing::try_new(65, 3, 36).unwrap();
    ring.words = vec![
        0xa50f_f05a,
        0x807f_00ff,
        0xaabb_ccdd,
        0x1234_5678,
        0x80ff_0081,
        0x55aa_55aa,
        0xdead_beef,
        0xfedc_ba98,
        0x9876_5432,
    ];
    let bits = storage_bits(&ring.words);
    for start in [
        None,
        Some(0),
        Some(3),
        Some(6),
        Some(8),
        Some(usize::MAX - 1),
        Some(usize::MAX),
    ] {
        for x in [0, 1, 7, 31, 32, 33, 63, 64, 65, 95, 96, u64::MAX] {
            let mut expected = 0_u32;
            for offset in 0..32_u128 {
                let local = u128::from(x) + offset;
                let set = start.is_some_and(|start| {
                    let absolute = start as u128 * 32 + local;
                    local < ring.stride_words as u128 * 32
                        && absolute < bits.len() as u128
                        && bits[absolute as usize]
                });
                expected = expected.wrapping_mul(2) | u32::from(set);
            }
            assert_eq!(ring.row_word(start, x), expected);
        }
    }
    for slot in [None, Some(0), Some(1), Some(2), Some(3), Some(usize::MAX)] {
        for x in [
            i64::MIN,
            -3,
            -1,
            0,
            1,
            31,
            32,
            63,
            64,
            65,
            i64::MAX - 1,
            i64::MAX,
        ] {
            for count in [0, 1, 7, 16] {
                let mut expected = 0_usize;
                if let Some(slot) = slot.filter(|&slot| slot < 3) {
                    for offset in 0..count {
                        let position = i128::from(x) + i128::from(offset);
                        if position > i128::from(i64::MAX) {
                            break;
                        }
                        let set = (0..65).contains(&position)
                            && bits[slot * 96 + usize::try_from(position).unwrap()];
                        expected = expected.wrapping_mul(2) | usize::from(set);
                    }
                }
                assert_eq!(ring.row_bits(slot, x, count), expected);
            }
        }
    }
}

#[test]
fn row_mutations_preserve_other_slots_and_flush_exact_big_endian_bytes() {
    for width in [0, 1, 7, 8, 9, 31, 32, 33, 63, 64, 65, 95, 96, 97] {
        let mut ring = GenericRowRing::try_new(width, 3, u64::MAX).unwrap();
        for slot in 0..3 {
            for x in 0..width {
                if (u64::from(x) * 3 + slot as u64) % 7 < 3 {
                    ring.set_black(slot * ring.stride_words, x);
                }
            }
        }
        let original = ring.words.clone();
        let metadata = (
            ring.width,
            ring.stride_words,
            ring.row_slots,
            ring.charged_bytes,
        );
        for source in 0..3 {
            for destination in 0..3 {
                ring.words.clone_from(&original);
                ring.copy_row(source, destination).unwrap();
                for slot in 0..3 {
                    let expected = if slot == destination { source } else { slot };
                    assert_eq!(
                        &ring.words[slot * ring.stride_words..(slot + 1) * ring.stride_words],
                        &original[expected * ring.stride_words..(expected + 1) * ring.stride_words],
                    );
                }
            }
            ring.words.clone_from(&original);
            assert_eq!(ring.clear_row(source).unwrap(), source * ring.stride_words);
            for slot in 0..3 {
                for column in 0..ring.stride_words {
                    let index = slot * ring.stride_words + column;
                    assert_eq!(
                        ring.words[index],
                        if slot == source { 0 } else { original[index] }
                    );
                }
            }
            ring.words.clone_from(&original);
            let mut bitmap = Bitmap::filled(width, 4, true, &Limits::default()).unwrap();
            let before = bitmap.bytes().to_vec();
            ring.flush_row(source, &mut bitmap, 2).unwrap();
            for y in 0..4 {
                for x in 0..width {
                    let expected = if y == 2 {
                        (u64::from(x) * 3 + source as u64) % 7 < 3
                    } else {
                        true
                    };
                    assert_eq!(bitmap.pixel(x, y), Some(expected));
                }
                if y != 2 {
                    let start = y as usize * bitmap.stride();
                    assert_eq!(
                        &bitmap.bytes()[start..start + bitmap.stride()],
                        &before[start..start + bitmap.stride()]
                    );
                }
            }
        }
        for invalid in [3, usize::MAX] {
            ring.words.clone_from(&original);
            assert_overflow(ring.clear_row(invalid));
            assert_overflow(ring.copy_row(invalid, 0));
            assert_overflow(ring.copy_row(0, invalid));
            let mut bitmap = Bitmap::blank(width, 1, &Limits::default()).unwrap();
            assert_overflow(ring.flush_row(invalid, &mut bitmap, 0));
            assert_eq!(ring.words, original);
            assert!(bitmap.bytes().iter().all(|&byte| byte == 0));
        }
        assert_eq!(
            (
                ring.width,
                ring.stride_words,
                ring.row_slots,
                ring.charged_bytes
            ),
            metadata
        );
    }
}

#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

use super::*;

#[test]
fn raw_wide_code_arithmetic_matches_independent_bit_serial_oracles() {
    for low in [0, 1, 1_u64 << 63, u64::MAX] {
        for middle in [0, 1, u64::MAX] {
            for high in [0, 1, u64::MAX] {
                for top in [0, 1, 1_u64 << 63, u64::MAX] {
                    let value = WideCode {
                        limbs: [low, middle, high, top],
                    };
                    for increment in [0, 1, 2, u32::MAX] {
                        let mut expected = [0_u64; 4];
                        let mut carry = false;
                        for bit in 0..256 {
                            let a = value.limbs[bit / 64] & (1 << (bit % 64)) != 0;
                            let b = bit < 32 && increment & (1 << bit) != 0;
                            if a ^ b ^ carry {
                                expected[bit / 64] |= 1 << (bit % 64);
                            }
                            carry = u8::from(a) + u8::from(b) + u8::from(carry) >= 2;
                        }
                        assert_eq!(
                            value.checked_add_small(increment),
                            if carry {
                                None
                            } else {
                                Some(WideCode { limbs: expected })
                            }
                        );
                    }
                    let expected = [
                        low.wrapping_mul(2),
                        middle.wrapping_mul(2) | u64::from(low >= 1 << 63),
                        high.wrapping_mul(2) | u64::from(middle >= 1 << 63),
                        top.wrapping_mul(2) | u64::from(high >= 1 << 63),
                    ];
                    assert_eq!(
                        value.checked_shift_left_one(),
                        if top >= 1 << 63 {
                            None
                        } else {
                            Some(WideCode { limbs: expected })
                        }
                    );
                    for bit in [0, 1] {
                        let mut appended = expected;
                        appended[0] |= u64::from(bit);
                        assert_eq!(
                            value.append_bit(bit),
                            if top >= 1 << 63 {
                                None
                            } else {
                                Some(WideCode { limbs: appended })
                            }
                        );
                    }
                }
            }
        }
    }
    for left in [0, 1, 2, 1_u64 << 63, u64::MAX] {
        for right in [0, 1, 2, 1_u64 << 63, u64::MAX] {
            for borrow in [false, true] {
                let subtrahend = u128::from(right) + u128::from(borrow);
                let difference = u128::from(left) + (1_u128 << 64) - subtrahend;
                assert_eq!(
                    subtract_limb(left, right, borrow),
                    (
                        u64::try_from(difference & u128::from(u64::MAX)).unwrap(),
                        u128::from(left) < subtrahend,
                    )
                );
            }
        }
    }
}

#[test]
fn raw_wide_distances_match_bit_serial_subtraction_and_exclusive_bounds() {
    let values = [
        WideCode {
            limbs: [0, 0, 0, 0],
        },
        WideCode {
            limbs: [1, 0, 0, 0],
        },
        WideCode {
            limbs: [u64::from(u32::MAX), 0, 0, 0],
        },
        WideCode {
            limbs: [u64::from(u32::MAX) + 1, 0, 0, 0],
        },
        WideCode {
            limbs: [u64::MAX, 0, 0, 0],
        },
        WideCode {
            limbs: [0, 1, 0, 0],
        },
        WideCode {
            limbs: [u64::MAX, u64::MAX, 0, 0],
        },
        WideCode {
            limbs: [0, 0, 1, 0],
        },
        WideCode {
            limbs: [0, 0, 0, 1],
        },
        WideCode {
            limbs: [u64::MAX; 4],
        },
    ];
    for upper in values {
        for lower in values {
            let mut difference = [false; 256];
            let mut borrow = false;
            for (bit, output) in difference.iter_mut().enumerate() {
                let a = upper.limbs[bit / 64] & (1 << (bit % 64)) != 0;
                let b = lower.limbs[bit / 64] & (1 << (bit % 64)) != 0;
                *output = a ^ b ^ borrow;
                borrow = u8::from(a) < u8::from(b) + u8::from(borrow);
            }
            let small = if borrow || difference[32..].iter().any(|bit| *bit) {
                None
            } else {
                Some(
                    difference[..32]
                        .iter()
                        .enumerate()
                        .fold(0_u32, |value, (index, bit)| {
                            value | (u32::from(*bit) << index)
                        }),
                )
            };
            for bound in [0, 1, 2, u32::MAX] {
                assert_eq!(
                    upper.small_distance_from(lower, bound),
                    small.filter(|value| *value < bound)
                );
            }
        }
    }
}

#[test]
fn wide_code_fit_matches_independent_significant_bit_counts() {
    let mut cases = vec![
        WideCode::default(),
        WideCode {
            limbs: [u64::MAX; 4],
        },
        WideCode {
            limbs: [0xaaaa_aaaa_aaaa_aaaa, 0x5555_5555_5555_5555, 0, 1],
        },
    ];
    for position in 0..256 {
        let mut limbs = [0; 4];
        limbs[position / 64] = 1_u64 << (position % 64);
        cases.push(WideCode { limbs });
    }
    for code in cases {
        let significant_bits = code
            .limbs
            .iter()
            .enumerate()
            .filter(|(_, limb)| **limb != 0)
            .map(|(index, limb)| {
                index * 64 + usize::try_from(u64::BITS - limb.leading_zeros()).expect("bits")
            })
            .max()
            .unwrap_or(0);
        for width in 0..=257 {
            assert_eq!(
                code.fits_in_bits(width),
                significant_bits <= width,
                "{code:?}, width={width}"
            );
        }
    }
}

#[test]
fn canonical_width_boundaries_reject_oversubscription_and_preserve_valid_codes() {
    for boundary in [1_u8, 63, 64, 65, 127, 128, 129, 191, 192, 193, 255] {
        let mut lines: Vec<_> = (1..boundary)
            .map(|length| HuffmanLine::normal(i32::from(length - 1), length, 0))
            .collect();
        for offset in 0..2 {
            lines.push(HuffmanLine::normal(
                i32::from(boundary - 1) + offset,
                boundary,
                0,
            ));
            let table = HuffmanTable::new(&lines, &Limits::default()).expect("valid tree");
            let last = lines.len() - 1;
            assert_eq!(
                decode_bits(&table, &encoded_line(&table, last, 0)),
                HuffmanValue::Integer(i32::try_from(last).expect("small line index"))
            );
        }
        lines.push(HuffmanLine::normal(i32::from(boundary) + 1, boundary, 0));
        assert!(
            matches!(
                HuffmanTable::new(&lines, &Limits::default()),
                Err(HuffmanError::InvalidTable(
                    HuffmanTableError::Oversubscribed
                ))
            ),
            "oversubscribed {boundary}-bit tree was accepted"
        );
    }
}

#[test]
fn supports_prefixes_longer_than_machine_words() {
    let mut lines = Vec::new();
    for value in 0..254_i32 {
        lines.push(HuffmanLine::normal(
            value,
            u8::try_from(value + 1).expect("length"),
            0,
        ));
    }
    lines.push(HuffmanLine::normal(254, 255, 0));
    lines.push(HuffmanLine::normal(255, 255, 0));
    let table = HuffmanTable::new(&lines, &Limits::default()).expect("255-bit table");
    let encoded = encoded_line(&table, 255, 0);
    assert_eq!(encoded.len(), 255);
    assert_eq!(decode_bits(&table, &encoded), HuffmanValue::Integer(255));
}

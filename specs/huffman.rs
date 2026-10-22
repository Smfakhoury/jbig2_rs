use vstd::prelude::*;

verus! {

pub(super) open spec fn pow2(exponent: nat) -> nat
    decreases exponent,
{
    if exponent == 0 {
        1
    } else {
        2 * pow2((exponent - 1) as nat)
    }
}

pub(super) open spec fn wide_value(code: super::WideCode) -> nat {
    code.limbs@[0] as nat
        + code.limbs@[1] as nat * pow2(64)
        + code.limbs@[2] as nat * pow2(128)
        + code.limbs@[3] as nat * pow2(192)
}

pub(super) open spec fn wide_fits(
    code: super::WideCode,
    bit_count: usize,
) -> bool {
    bit_count >= 256 || wide_value(code) < pow2(bit_count as nat)
}

pub(super) open spec fn add_limb(left: u64, right: u64) -> (u64, bool) {
    if right <= u64::MAX - left {
        ((left + right) as u64, false)
    } else {
        ((right - (u64::MAX - left) - 1) as u64, true)
    }
}

pub(super) open spec fn checked_add_small(
    code: super::WideCode,
    value: u32,
) -> Option<super::WideCode> {
    let first = add_limb(code.limbs@[0], value as u64);
    if !first.1 {
        Some(super::WideCode {
            limbs: [first.0, code.limbs@[1], code.limbs@[2], code.limbs@[3]],
        })
    } else if code.limbs@[1] != u64::MAX {
        Some(super::WideCode {
            limbs: [first.0, (code.limbs@[1] + 1) as u64, code.limbs@[2], code.limbs@[3]],
        })
    } else if code.limbs@[2] != u64::MAX {
        Some(super::WideCode {
            limbs: [first.0, 0, (code.limbs@[2] + 1) as u64, code.limbs@[3]],
        })
    } else if code.limbs@[3] != u64::MAX {
        Some(super::WideCode {
            limbs: [first.0, 0, 0, (code.limbs@[3] + 1) as u64],
        })
    } else {
        None
    }
}

pub(super) open spec fn checked_shift_left_one(
    code: super::WideCode,
) -> Option<super::WideCode> {
    if code.limbs@[3] >> 63 != 0 {
        None
    } else {
        Some(super::WideCode {
            limbs: [
                code.limbs@[0] << 1,
                (code.limbs@[1] << 1) | (code.limbs@[0] >> 63),
                (code.limbs@[2] << 1) | (code.limbs@[1] >> 63),
                (code.limbs@[3] << 1) | (code.limbs@[2] >> 63),
            ],
        })
    }
}

pub(super) open spec fn append_bit(
    code: super::WideCode,
    bit: u32,
) -> Option<super::WideCode> {
    match checked_shift_left_one(code) {
        Some(shifted) => checked_add_small(shifted, bit),
        None => None,
    }
}

pub(super) open spec fn fits_in_bits(
    code: super::WideCode,
    bit_count: usize,
) -> bool {
    if bit_count >= 256 {
        true
    } else {
        let limb = bit_count / 64;
        let bit = bit_count % 64;
        !(limb < 3 && code.limbs@[3] != 0)
            && !(limb < 2 && code.limbs@[2] != 0)
            && !(limb < 1 && code.limbs@[1] != 0)
            && code.limbs@[limb as int] >> bit == 0
    }
}

pub(super) open spec fn subtract_limb(
    left: u64,
    right: u64,
    borrow: bool,
) -> (u64, bool) {
    let borrow_value = if borrow { 1u64 } else { 0u64 };
    let first = if left >= right {
        ((left - right) as u64, false)
    } else {
        ((u64::MAX - (right - left) + 1) as u64, true)
    };
    let second = if first.0 >= borrow_value {
        ((first.0 - borrow_value) as u64, false)
    } else {
        (u64::MAX, true)
    };
    (second.0, first.1 || second.1)
}

pub(super) open spec fn small_distance_from(
    left: super::WideCode,
    right: super::WideCode,
    upper_bound: u32,
) -> Option<u32> {
    let d0 = subtract_limb(left.limbs@[0], right.limbs@[0], false);
    let d1 = subtract_limb(left.limbs@[1], right.limbs@[1], d0.1);
    let d2 = subtract_limb(left.limbs@[2], right.limbs@[2], d1.1);
    let d3 = subtract_limb(left.limbs@[3], right.limbs@[3], d2.1);
    if d3.1 || d1.0 != 0 || d2.0 != 0 || d3.0 != 0 || d0.0 > u32::MAX {
        None
    } else if d0.0 < upper_bound {
        Some(d0.0 as u32)
    } else {
        None
    }
}

pub(crate) open spec fn reader_valid(reader: &crate::reader::BitReader) -> bool {
    reader.bytes@.len() <= usize::MAX / 8
        && reader.bit_position <= reader.bytes@.len() * 8
}

pub(super) open spec fn line_decodable(line: super::HuffmanLine) -> bool {
    line.range_len <= 32
        && match line.kind {
            super::LineKind::Normal | super::LineKind::Upper =>
                i32::MIN <= line.range_low
                && line.range_low
                    + (pow2(line.range_len as nat) - 1) as int <= i32::MAX,
            super::LineKind::Lower =>
                i32::MIN <= line.range_low
                    - (pow2(line.range_len as nat) - 1) as int
                && line.range_low <= i32::MAX,
            super::LineKind::OutOfBand => line.range_len == 0,
        }
}

pub(super) open spec fn decoded_line_value(
    line: super::HuffmanLine,
    offset: nat,
    value: i32,
) -> bool {
    match line.kind {
        super::LineKind::Lower =>
            value as int == line.range_low - offset as int,
        super::LineKind::Normal | super::LineKind::Upper =>
            value as int == line.range_low + offset as int,
        super::LineKind::OutOfBand => false,
    }
}

pub(super) open spec fn canonical_first_code(
    previous: super::WideCode,
    count: u32,
) -> Option<super::WideCode> {
    match checked_add_small(previous, count) {
        Some(sum) => checked_shift_left_one(sum),
        None => None,
    }
}

pub(super) open spec fn canonical_range_fits(
    first: super::WideCode,
    count: u32,
    length: usize,
) -> bool {
    match checked_add_small(first, (count - 1) as u32) {
        Some(last) => fits_in_bits(last, length),
        None => false,
    }
}

pub(super) open spec fn line_lengths_valid(lines: Seq<super::HuffmanLine>) -> bool {
    forall|index: int| 0 <= index < lines.len() ==>
        lines[index].range_len <= 32
}

pub(crate) closed spec fn table_has_oob(table: &super::HuffmanTable) -> bool {
    exists|index: int| 0 <= index < table.lines@.len()
        && table.lines@[index].kind == super::LineKind::OutOfBand
}

}

// Keep the closed-model reveal in its owning module.
include!("../proofs/model_support/huffman.rs");

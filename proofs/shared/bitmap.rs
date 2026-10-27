// Source-only author navigation; named purposes are not normative approval.
// Guide, official acquisition and source identity: ../../specs/README.md
// All requirement links and inherited locators: ../../validation/traceability/requirements.json
// REQUIREMENT CONTEXT is not a direct normative equation, including for proof/policy helpers.

use vstd::prelude::*;

verus! {

// Model navigation: spec::black_fill_prefix [proof-support]
// Purpose: Loop partition: filled rows precede the untouched byte suffix.
// Audit: filled-storage-prefix
// REQUIREMENT CONTEXT: T88-PAGE-DEFAULT-PIXEL | ITU-T T.88 (08/2018) | 7.4.8.5 bit 2; 8.2 step 3
// Fidelity: unresolved; no independent normative approval.
pub(super) open spec fn black_fill_prefix(
    data: Seq<u8>, width: u32, stride: usize, height: u32, completed: nat,
) -> bool {
    forall|row: int, column: int|
        0 <= row < height && 0 <= column < stride ==>
            #[trigger] data[row * stride as int + column]
                == if row < completed {
                    crate::spec::bitmap::filled_byte(width, column as nat, true)
                } else {
                    0xFFu8
                }
}

pub(super) proof fn row_byte_bounds(row: int, column: int, stride: int, height: int)
    requires 0 <= row < height, 0 <= column < stride,
    ensures
        0 <= row * stride + column < height * stride,
        row * stride + column < stride * height,
{
    vstd::arithmetic::mul::lemma_mul_inequality(0, row, stride);
    vstd::arithmetic::mul::lemma_mul_inequality(row + 1, height, stride);
    vstd::arithmetic::mul::lemma_mul_is_distributive_add_other_way(stride, row, 1);
    vstd::arithmetic::mul::lemma_mul_is_commutative(row, stride);
    vstd::arithmetic::mul::lemma_mul_is_commutative(row + 1, stride);
    vstd::arithmetic::mul::lemma_mul_is_commutative(height, stride);
}

pub(super) proof fn distinct_row_tail(row: int, column: int, current: int, stride: int)
    requires
        0 <= row, 0 <= current, 0 <= column < stride,
        row != current || column + 1 != stride,
    ensures row * stride + column != current * stride + stride - 1,
{
    if row < current {
        vstd::arithmetic::mul::lemma_mul_is_distributive_add_other_way(stride, row, 1);
        vstd::arithmetic::mul::lemma_mul_inequality(row + 1, current, stride);
    } else if row > current {
        vstd::arithmetic::mul::lemma_mul_is_distributive_add_other_way(stride, current, 1);
        vstd::arithmetic::mul::lemma_mul_inequality(current + 1, row, stride);
    }
}

pub(super) proof fn initial_black_fill(
    data: Seq<u8>, width: u32, stride: usize, height: u32,
)
    requires
        data.len() == stride as nat * height as nat,
        forall|index: int| 0 <= index < data.len() ==> data[index] == 0xFF,
    ensures black_fill_prefix(data, width, stride, height, 0),
{
    vstd::arithmetic::mul::lemma_mul_is_commutative(height as int, stride as int);
    reveal(black_fill_prefix);
    assert forall|row: int, column: int|
        0 <= row < height && 0 <= column < stride implies
        #[trigger] data[row * stride as int + column] == 0xFFu8 by {
        row_byte_bounds(row, column, stride as int, height as int);
    }
}

pub(super) proof fn black_fill_step(
    before: Seq<u8>, after: Seq<u8>,
    width: u32, stride: usize, height: u32, current: usize,
)
    requires
        width % 8 != 0,
        stride as nat == crate::spec::bitmap::stride_for_width(width as nat),
        0 < stride, current < height,
        before.len() == stride as nat * height as nat,
        after.len() == before.len(),
        black_fill_prefix(before, width, stride, height, current as nat),
        after[current as int * stride as int + stride as int - 1]
            == crate::spec::bitmap::filled_byte(width, (stride - 1) as nat, true),
        forall|index: int| 0 <= index < before.len()
            && index != current as int * stride as int + stride as int - 1
            ==> after[index] == before[index],
    ensures black_fill_prefix(after, width, stride, height, current as nat + 1),
{
    reveal(black_fill_prefix);
    assert forall|row: int, column: int|
        0 <= row < height && 0 <= column < stride implies
        #[trigger] after[row * stride as int + column]
            == if row < current as int + 1 {
                crate::spec::bitmap::filled_byte(width, column as nat, true)
            } else {
                0xFFu8
            } by {
        row_byte_bounds(row, column, stride as int, height as int);
        assert(before[row * stride as int + column] == if row < current {
            crate::spec::bitmap::filled_byte(width, column as nat, true)
        } else { 0xFFu8 });
        if row != current as int || column + 1 != stride as int {
            distinct_row_tail(row, column, current as int, stride as int);
        }
    }
}

pub(super) proof fn filled_storage_result(bitmap: &crate::bitmap::Bitmap, value: bool)
    requires
        crate::spec::bitmap::storage_valid(bitmap),
        if value && bitmap.width % 8 != 0 {
            black_fill_prefix(
                bitmap.data@, bitmap.width, bitmap.stride, bitmap.height, bitmap.height as nat,
            )
        } else {
            forall|index: int| 0 <= index < bitmap.data@.len() ==>
                bitmap.data@[index] == if value { 0xFFu8 } else { 0u8 }
        },
    ensures crate::spec::bitmap::filled_storage(bitmap, value),
{
    crate::views::bitmap_fields(bitmap);
    reveal(crate::spec::bitmap::storage_valid);
    reveal(crate::spec::bitmap::filled_storage);
    reveal(black_fill_prefix);
    assert forall|row: int, column: int|
        0 <= row < bitmap.height && 0 <= column < bitmap.stride implies
        #[trigger] bitmap.data@[row * bitmap.stride as int + column]
            == crate::spec::bitmap::filled_byte(bitmap.width, column as nat, value) by {
        row_byte_bounds(row, column, bitmap.stride as int, bitmap.height as int);
    }
}

pub(super) proof fn black_span_mask(byte: u8, shift: u32, count: u32)
    requires shift < 8, 0 < count <= 8 - shift,
    ensures forall|bit: u32| bit < 8 ==>
        #[trigger] crate::spec::bitmap::byte_bit(
            byte | ((0xFFu8 >> shift) & (0xFFu8 << ((8 - shift - count) as u32))), bit,
        ) == (shift <= bit < shift + count || crate::spec::bitmap::byte_bit(byte, bit)),
{
    assert forall|bit: u32| bit < 8 implies
        #[trigger] crate::spec::bitmap::byte_bit(
            byte | ((0xFFu8 >> shift) & (0xFFu8 << ((8 - shift - count) as u32))), bit,
        ) == (shift <= bit < shift + count || crate::spec::bitmap::byte_bit(byte, bit)) by {
        reveal(crate::spec::bitmap::byte_bit);
        reveal(crate::spec::bitmap::bit_mask);
        assert(
            (byte | ((0xFFu8 >> shift) & (0xFFu8 << ((8 - shift - count) as u32))))
                & crate::spec::bitmap::bit_mask(bit) != 0
                <==> (shift <= bit < shift + count
                    || byte & crate::spec::bitmap::bit_mask(bit) != 0)
        ) by (bit_vector)
            requires shift < 8, 0 < count <= 8 - shift, bit < 8;
    }
}

pub(super) proof fn black_span_partial_step(
    original: Seq<u8>, before: Seq<u8>, after: Seq<u8>,
    row_start: int, start: u32, x: u32, count: u32,
)
    requires
        0 <= row_start,
        start <= x,
        0 < count <= 8 - x % 8,
        x as int + count as int <= u32::MAX,
        row_start + x as int / 8 < before.len(),
        crate::spec::bitmap::black_span_bits(original, before, row_start, start, x),
        after.len() == before.len(),
        after[row_start + x as int / 8] == before[row_start + x as int / 8]
            | ((0xFFu8 >> (x % 8)) & (0xFFu8 << ((8 - x % 8 - count) as u32))),
        forall|i: int| 0 <= i < before.len() && i != row_start + x as int / 8
            ==> after[i] == before[i],
    ensures
        crate::spec::bitmap::black_span_bits(original, after, row_start, start, (x + count) as u32),
{
    reveal(crate::spec::bitmap::black_span_bits);
    vstd::arithmetic::div_mod::lemma_fundamental_div_mod(x as int, 8);
    vstd::arithmetic::div_mod::lemma_mod_bound(x as int, 8);
    black_span_mask(before[row_start + x as int / 8], x % 8, count);
    assert forall|i: int, bit: u32| 0 <= i < original.len() && bit < 8 implies
        crate::spec::bitmap::byte_bit(after[i], bit)
            == (row_start <= i
                && start as int <= (i - row_start) * 8 + bit as int
                && (i - row_start) * 8 + (bit as int) < x as int + count as int
                || crate::spec::bitmap::byte_bit(original[i], bit)) by {
        assert(crate::spec::bitmap::byte_bit(before[i], bit)
            == (row_start <= i
                && start as int <= (i - row_start) * 8 + bit as int
                && (i - row_start) * 8 + (bit as int) < x as int
                || crate::spec::bitmap::byte_bit(original[i], bit)));
        if i == row_start + x as int / 8 {
            assert((i - row_start) * 8 + x as int % 8 == x as int);
        } else {
            assert(!((x as int) <= (i - row_start) * 8 + bit as int
                && (i - row_start) * 8 + (bit as int) < x as int + count as int));
        }
    }
}

pub(super) proof fn black_span_full_step(
    original: Seq<u8>, before: Seq<u8>, after: Seq<u8>,
    row_start: int, start: u32, x: u32, count: u32,
)
    requires
        0 <= row_start,
        start <= x,
        x % 8 == 0,
        count == 8 || count == 64,
        x as int + count as int <= u32::MAX,
        crate::spec::bitmap::black_span_bits(original, before, row_start, start, x),
        after.len() == before.len(),
        forall|i: int| 0 <= i < before.len() ==>
            after[i] == if row_start + x as int / 8 <= i
                < row_start + (x as int + count as int) / 8 { 0xFFu8 } else { before[i] },
    ensures
        crate::spec::bitmap::black_span_bits(original, after, row_start, start, (x + count) as u32),
{
    reveal(crate::spec::bitmap::black_span_bits);
    vstd::arithmetic::div_mod::lemma_fundamental_div_mod(x as int, 8);
    vstd::arithmetic::div_mod::lemma_fundamental_div_mod(x as int + count as int, 8);
    vstd::arithmetic::div_mod::lemma_mod_bound(x as int + count as int, 8);
    assert forall|i: int, bit: u32| 0 <= i < original.len() && bit < 8 implies
        crate::spec::bitmap::byte_bit(after[i], bit)
            == (row_start <= i
                && start as int <= (i - row_start) * 8 + bit as int
                && (i - row_start) * 8 + (bit as int) < x as int + count as int
                || crate::spec::bitmap::byte_bit(original[i], bit)) by {
        assert(crate::spec::bitmap::byte_bit(before[i], bit)
            == (row_start <= i
                && start as int <= (i - row_start) * 8 + bit as int
                && (i - row_start) * 8 + (bit as int) < x as int
                || crate::spec::bitmap::byte_bit(original[i], bit)));
        if row_start + x as int / 8 <= i < row_start + (x as int + count as int) / 8 {
            reveal(crate::spec::bitmap::byte_bit);
            reveal(crate::spec::bitmap::bit_mask);
            assert(0xFFu8 & crate::spec::bitmap::bit_mask(bit) != 0) by (bit_vector)
                requires bit < 8;
        }
    }
}

pub(super) proof fn byte_bits_extensional(a: u8, b: u8)
    requires forall|bit: u32| bit < 8 ==>
        #[trigger] crate::spec::bitmap::byte_bit(a, bit)
            == crate::spec::bitmap::byte_bit(b, bit),
    ensures a == b,
{
    assert(crate::spec::bitmap::byte_bit(a, 0) == crate::spec::bitmap::byte_bit(b, 0));
    assert(crate::spec::bitmap::byte_bit(a, 1) == crate::spec::bitmap::byte_bit(b, 1));
    assert(crate::spec::bitmap::byte_bit(a, 2) == crate::spec::bitmap::byte_bit(b, 2));
    assert(crate::spec::bitmap::byte_bit(a, 3) == crate::spec::bitmap::byte_bit(b, 3));
    assert(crate::spec::bitmap::byte_bit(a, 4) == crate::spec::bitmap::byte_bit(b, 4));
    assert(crate::spec::bitmap::byte_bit(a, 5) == crate::spec::bitmap::byte_bit(b, 5));
    assert(crate::spec::bitmap::byte_bit(a, 6) == crate::spec::bitmap::byte_bit(b, 6));
    assert(crate::spec::bitmap::byte_bit(a, 7) == crate::spec::bitmap::byte_bit(b, 7));
    reveal(crate::spec::bitmap::byte_bit);
    reveal(crate::spec::bitmap::bit_mask);
    assert(a == b) by (bit_vector)
        requires
            (a & 0x80u8 != 0) == (b & 0x80u8 != 0),
            (a & 0x40u8 != 0) == (b & 0x40u8 != 0),
            (a & 0x20u8 != 0) == (b & 0x20u8 != 0),
            (a & 0x10u8 != 0) == (b & 0x10u8 != 0),
            (a & 0x08u8 != 0) == (b & 0x08u8 != 0),
            (a & 0x04u8 != 0) == (b & 0x04u8 != 0),
            (a & 0x02u8 != 0) == (b & 0x02u8 != 0),
            (a & 0x01u8 != 0) == (b & 0x01u8 != 0);
}

pub(super) proof fn black_span_result(
    before: &crate::bitmap::Bitmap, after: &crate::bitmap::Bitmap,
    row: u32, start: u32, end: u32,
)
    requires
        crate::spec::bitmap::storage_valid(before),
        after.width == before.width,
        after.height == before.height,
        after.stride == before.stride,
        row < before.height,
        start <= end <= before.width,
        crate::spec::bitmap::black_span_bits(
            before.data@, after.data@, row as int * before.stride as int, start, end,
        ),
    ensures crate::spec::bitmap::black_span_result(before, after, row, start, end),
{
    crate::views::bitmap_fields(before);
    crate::views::bitmap_fields(after);
    reveal(crate::spec::bitmap::black_span_result);
    reveal(crate::spec::bitmap::black_span_bits);
    reveal(crate::spec::bitmap::storage_valid);
    let base = row as int * before.stride as int;
    vstd::arithmetic::div_mod::lemma_fundamental_div_mod(start as int, 8);
    vstd::arithmetic::div_mod::lemma_mod_bound(start as int, 8);
    vstd::arithmetic::div_mod::lemma_fundamental_div_mod(end as int + 7, 8);
    vstd::arithmetic::div_mod::lemma_mod_bound(end as int + 7, 8);
    assert forall|i: int| 0 <= i < before.data@.len()
        && (start == end || i < base + start as int / 8 || base + (end as int + 7) / 8 <= i)
        implies after.data@[i] == before.data@[i] by {
        assert forall|bit: u32| bit < 8 implies
            crate::spec::bitmap::byte_bit(after.data@[i], bit)
                == crate::spec::bitmap::byte_bit(before.data@[i], bit) by {
            assert(crate::spec::bitmap::byte_bit(after.data@[i], bit)
                == (base <= i && start as int <= (i - base) * 8 + bit as int
                    && (i - base) * 8 + (bit as int) < end as int
                    || crate::spec::bitmap::byte_bit(before.data@[i], bit)));
        }
        byte_bits_extensional(after.data@[i], before.data@[i]);
    }
    stride_covers_width(before.width);
    assert forall|x: u32, y: u32|
        crate::spec::bitmap::coordinates_in_bounds(before.width, before.height, x, y)
        implies crate::spec::bitmap::stored_pixel(after, x, y)
            == (y == row && start <= x < end || crate::spec::bitmap::stored_pixel(before, x, y)) by {
        in_bounds_byte(before, x, y);
        reveal(crate::spec::bitmap::stored_pixel);
        reveal(crate::spec::bitmap::byte_index);
        reveal(crate::spec::bitmap::byte_bit);
        reveal(crate::spec::bitmap::bit_mask);
        vstd::arithmetic::div_mod::lemma_fundamental_div_mod(x as int, 8);
        vstd::arithmetic::div_mod::lemma_mod_bound(x as int, 8);
        let i = y as int * before.stride as int + x as int / 8;
        assert(x as int / 8 < before.stride as int);
        assert(crate::spec::bitmap::byte_bit(after.data@[i], x % 8)
            == (base <= i && start as int <= (i - base) * 8 + (x % 8) as int
                && (i - base) * 8 + ((x % 8) as int) < end as int
                || crate::spec::bitmap::byte_bit(before.data@[i], x % 8)));
        if y < row {
            vstd::arithmetic::mul::lemma_mul_inequality(y as int + 1, row as int, before.stride as int);
            vstd::arithmetic::mul::lemma_mul_is_distributive_add_other_way(before.stride as int, y as int, 1);
        } else if row < y {
            vstd::arithmetic::mul::lemma_mul_inequality(row as int + 1, y as int, before.stride as int);
            vstd::arithmetic::mul::lemma_mul_is_distributive_add_other_way(before.stride as int, row as int, 1);
        }
    }
}

pub(super) proof fn byte_operator_semantics(
    operator: crate::bitmap::CombinationOperator, destination: u8, source: u8, bit: u32,
)
    requires bit < 8,
    ensures crate::spec::bitmap::byte_operator_bit(
        crate::spec::bitmap::operator_byte(operator, destination, source), bit,
    ) == crate::spec::bitmap::combine(
        operator, crate::spec::bitmap::byte_operator_bit(destination, bit),
        crate::spec::bitmap::byte_operator_bit(source, bit),
    ),
{
    reveal(crate::spec::bitmap::operator_byte);
    reveal(crate::spec::bitmap::byte_operator_bit);
    reveal(crate::spec::bitmap::combine);
    let shift = bit as u8;
    assert(shift < 8);
    match operator {
        crate::bitmap::CombinationOperator::Or => {
            assert(((((destination | source) >> shift) & 1u8) != 0)
                == ((((destination >> shift) & 1u8) != 0)
                    || (((source >> shift) & 1u8) != 0))) by (bit_vector);
        },
        crate::bitmap::CombinationOperator::And => {
            assert(((((destination & source) >> shift) & 1u8) != 0)
                == ((((destination >> shift) & 1u8) != 0)
                    && (((source >> shift) & 1u8) != 0))) by (bit_vector);
        },
        crate::bitmap::CombinationOperator::Xor => {
            assert(((((destination ^ source) >> shift) & 1u8) != 0)
                == ((((destination >> shift) & 1u8) != 0)
                    != (((source >> shift) & 1u8) != 0))) by (bit_vector);
        },
        crate::bitmap::CombinationOperator::Xnor => {
            assert(shift < 8 ==> (((!(destination ^ source) >> shift) & 1u8) != 0)
                == ((((destination >> shift) & 1u8) != 0)
                    == (((source >> shift) & 1u8) != 0))) by (bit_vector);
        },
        crate::bitmap::CombinationOperator::Replace => {},
    }
}

pub(super) proof fn word_operator_semantics(
    operator: crate::bitmap::CombinationOperator, destination: u64, source: u64, bit: u32,
)
    requires bit < 64,
    ensures crate::spec::bitmap::word_operator_bit(
        crate::spec::bitmap::operator_word(operator, destination, source), bit,
    ) == crate::spec::bitmap::combine(
        operator, crate::spec::bitmap::word_operator_bit(destination, bit),
        crate::spec::bitmap::word_operator_bit(source, bit),
    ),
{
    reveal(crate::spec::bitmap::operator_word);
    reveal(crate::spec::bitmap::word_operator_bit);
    reveal(crate::spec::bitmap::combine);
    let shift = bit as u64;
    assert(shift < 64);
    match operator {
        crate::bitmap::CombinationOperator::Or => {
            assert(((((destination | source) >> shift) & 1u64) != 0)
                == ((((destination >> shift) & 1u64) != 0)
                    || (((source >> shift) & 1u64) != 0))) by (bit_vector);
        },
        crate::bitmap::CombinationOperator::And => {
            assert(((((destination & source) >> shift) & 1u64) != 0)
                == ((((destination >> shift) & 1u64) != 0)
                    && (((source >> shift) & 1u64) != 0))) by (bit_vector);
        },
        crate::bitmap::CombinationOperator::Xor => {
            assert(((((destination ^ source) >> shift) & 1u64) != 0)
                == ((((destination >> shift) & 1u64) != 0)
                    != (((source >> shift) & 1u64) != 0))) by (bit_vector);
        },
        crate::bitmap::CombinationOperator::Xnor => {
            assert(shift < 64 ==> (((!(destination ^ source) >> shift) & 1u64) != 0)
                == ((((destination >> shift) & 1u64) != 0)
                    == (((source >> shift) & 1u64) != 0))) by (bit_vector);
        },
        crate::bitmap::CombinationOperator::Replace => {},
    }
}

pub(super) proof fn row_copy_bounds(
    bitmap: &crate::bitmap::Bitmap,
    source: u32,
    destination: u32,
)
    requires
        crate::spec::bitmap::storage_valid(bitmap),
        source < bitmap.height,
        destination < bitmap.height,
    ensures
        source as nat * bitmap.stride as nat + bitmap.stride as nat <= bitmap.data@.len(),
        destination as nat * bitmap.stride as nat + bitmap.stride as nat <= bitmap.data@.len(),
        source < destination ==>
            source as nat * bitmap.stride as nat + bitmap.stride as nat
                <= destination as nat * bitmap.stride as nat,
        destination < source ==>
            destination as nat * bitmap.stride as nat + bitmap.stride as nat
                <= source as nat * bitmap.stride as nat,
{
    crate::views::bitmap_fields(bitmap);
    reveal(crate::spec::bitmap::storage_valid);
    vstd::arithmetic::mul::lemma_mul_is_commutative(
        bitmap.stride as int, bitmap.height as int,
    );
    vstd::arithmetic::mul::lemma_mul_inequality(
        source as int + 1, bitmap.height as int, bitmap.stride as int,
    );
    vstd::arithmetic::mul::lemma_mul_inequality(
        destination as int + 1, bitmap.height as int, bitmap.stride as int,
    );
    vstd::arithmetic::mul::lemma_mul_is_distributive_add_other_way(
        bitmap.stride as int, source as int, 1,
    );
    vstd::arithmetic::mul::lemma_mul_is_distributive_add_other_way(
        bitmap.stride as int, destination as int, 1,
    );
    if source < destination {
        vstd::arithmetic::mul::lemma_mul_inequality(
            source as int + 1, destination as int, bitmap.stride as int,
        );
    } else if destination < source {
        vstd::arithmetic::mul::lemma_mul_inequality(
            destination as int + 1, source as int, bitmap.stride as int,
        );
    }
}

pub(crate) proof fn row_copy_pixels(
    before: &crate::bitmap::Bitmap,
    after: &crate::bitmap::Bitmap,
    source: u32,
    destination: u32,
)
    requires
        crate::spec::bitmap::storage_valid(before),
        crate::spec::bitmap::storage_valid(after),
        source < before.height,
        destination < before.height,
        crate::spec::bitmap::row_copy_result(before, after, source, destination),
    ensures
        forall|x: u32| x < before.width ==>
            crate::spec::bitmap::stored_pixel(after, x, destination)
                == crate::spec::bitmap::stored_pixel(before, x, source),
{
    crate::views::bitmap_fields(before);
    crate::views::bitmap_fields(after);
    reveal(crate::spec::bitmap::row_copy_result);
    row_copy_bounds(before, source, destination);
    stride_covers_width(before.width);
    assert forall|x: u32| x < before.width implies
        crate::spec::bitmap::stored_pixel(after, x, destination)
            == crate::spec::bitmap::stored_pixel(before, x, source) by {
        vstd::arithmetic::div_mod::lemma_fundamental_div_mod(x as int, 8);
        vstd::arithmetic::div_mod::lemma_mod_bound(x as int, 8);
        assert(x as int / 8 < before.stride as int);
        reveal(crate::spec::bitmap::stored_pixel);
        reveal(crate::spec::bitmap::byte_index);
    }
}

pub(crate) proof fn stride_covers_width(width: u32)
    ensures
        crate::spec::bitmap::stride_for_width(width as nat) * 8
            >= width as nat,
        width > 0 ==> crate::spec::bitmap::stride_for_width(width as nat) > 0,
{
    reveal(crate::spec::bitmap::stride_for_width);
    vstd::arithmetic::div_mod::lemma_fundamental_div_mod(
        width as int + 7,
        8,
    );
    vstd::arithmetic::div_mod::lemma_mod_bound(width as int + 7, 8);
}

pub(crate) proof fn in_bounds_byte(
    bitmap: &crate::bitmap::Bitmap,
    x: u32,
    y: u32,
)
    requires
        crate::spec::bitmap::storage_valid(bitmap),
        crate::spec::bitmap::coordinates_in_bounds(
            bitmap.width,
            bitmap.height,
            x,
            y,
        ),
    ensures
        crate::spec::bitmap::byte_index(bitmap.stride, x, y)
            < bitmap.data@.len(),
{
    crate::views::bitmap_fields(bitmap);
    reveal(crate::spec::bitmap::storage_valid);
    reveal(crate::spec::bitmap::coordinates_in_bounds);
    reveal(crate::spec::bitmap::byte_index);
    reveal(crate::spec::bitmap::stride_for_width);
    stride_covers_width(bitmap.width);
    vstd::arithmetic::div_mod::lemma_fundamental_div_mod(
        bitmap.width as int + 7,
        8,
    );
    vstd::arithmetic::div_mod::lemma_mod_bound(
        bitmap.width as int + 7,
        8,
    );
    vstd::arithmetic::div_mod::lemma_fundamental_div_mod(x as int, 8);
    vstd::arithmetic::div_mod::lemma_mod_bound(x as int, 8);
    assert((x as int) < (bitmap.stride as int) * 8);
    assert((x as int) / 8 < bitmap.stride as int);
    assert(bitmap.stride > 0);
    let s = bitmap.stride as int;
    let q = x as int / 8;
    let r = y as int;
    let h = bitmap.height as int;
    vstd::arithmetic::mul::lemma_mul_is_distributive_add_other_way(s, r, 1);
    assert(r * s + q < (r + 1) * s);
    vstd::arithmetic::mul::lemma_mul_inequality(r + 1, h, s);
    assert((r + 1) * s <= h * s);
    vstd::arithmetic::mul::lemma_mul_is_commutative(
        bitmap.height as int,
        bitmap.stride as int,
    );
}

pub(super) proof fn u32_bitmap_arithmetic_is_checked(width: u32, height: u32)
    ensures
        crate::spec::bitmap::dimensions_checked(width, height),
        crate::spec::bitmap::stride_checked(width),
{
    reveal(crate::spec::bitmap::dimensions_checked);
    reveal(crate::spec::bitmap::stride_checked);
    reveal(crate::spec::bitmap::stride_for_width);
    assert(width as nat <= u32::MAX as nat);
    assert(height as nat <= u32::MAX as nat);
    vstd::arithmetic::mul::lemma_mul_inequality(
        width as int,
        u32::MAX as int,
        height as int,
    );
    vstd::arithmetic::mul::lemma_mul_inequality(
        height as int,
        u32::MAX as int,
        u32::MAX as int,
    );
    assert(u32::MAX as nat * u32::MAX as nat <= u64::MAX) by (compute);
    assert(u32::MAX as nat + 7 <= u64::MAX) by (compute);
    assert((u32::MAX as nat + 7) / 8 <= usize::MAX) by (compute);
}

pub(super) proof fn packed_span_bounds(width: u32, stride: usize, bit: u32, count: u32)
    requires
        stride as int == (width as int + 7) / 8,
        count > 0,
        bit as int + count as int <= width as int,
    ensures
        bit as int / 8 + (bit as int % 8 + count as int + 7) / 8 <= stride as int,
        count == 8 ==> (bit as int % 8 + count as int + 7) / 8
            == if bit % 8 == 0 { 1int } else { 2int },
        count == 64 ==> (bit as int % 8 + count as int + 7) / 8
            == if bit % 8 == 0 { 8int } else { 9int },
        count <= 8 ==> (bit as int % 8 + count as int + 7) / 8
            == if count as int <= 8 - bit as int % 8 { 1int } else { 2int },
{
    let b = bit as int;
    let c = count as int;
    let w = width as int;
    vstd::arithmetic::div_mod::lemma_fundamental_div_mod(b, 8);
    vstd::arithmetic::div_mod::lemma_mod_bound(b, 8);
    vstd::arithmetic::div_mod::lemma_fundamental_div_mod(w + 7, 8);
    vstd::arithmetic::div_mod::lemma_mod_bound(w + 7, 8);
    vstd::arithmetic::div_mod::lemma_fundamental_div_mod(b % 8 + c + 7, 8);
    vstd::arithmetic::div_mod::lemma_mod_bound(b % 8 + c + 7, 8);
}

pub(super) proof fn clipped_bounds(
    destination_width: u32,
    destination_height: u32,
    source_width: u32,
    source_height: u32,
    x: u32,
    y: u32,
)
    ensures
        crate::spec::bitmap::clipped_width(
            destination_width,
            source_width,
            x,
        ) <= source_width,
        crate::spec::bitmap::clipped_height(
            destination_height,
            source_height,
            y,
        ) <= source_height,
        crate::spec::bitmap::clipped_width(
            destination_width,
            source_width,
            x,
        ) > 0 ==> x < destination_width,
        crate::spec::bitmap::clipped_height(
            destination_height,
            source_height,
            y,
        ) > 0 ==> y < destination_height,
{
    reveal(crate::spec::bitmap::clipped_width);
    reveal(crate::spec::bitmap::clipped_height);
}

pub(super) proof fn aligned_byte_position(x: u32, head: u32, width: u32, bit: u32)
    requires
        0 < x % 8 < 8,
        head as int == if width <= 8 - x % 8 { width as int } else { 8 - x % 8 },
        head <= bit < width,
        (bit as int - head as int) % 8 == 0,
    ensures
        (x as int + bit as int) % 8 == 0,
        (x as int + bit as int) / 8
            == x as int / 8 + 1 + (bit as int - head as int) / 8,
{
    vstd::arithmetic::div_mod::lemma_fundamental_div_mod(x as int, 8);
    vstd::arithmetic::div_mod::lemma_mod_bound(x as int, 8);
    vstd::arithmetic::div_mod::lemma_fundamental_div_mod(bit as int - head as int, 8);
    vstd::arithmetic::div_mod::lemma_fundamental_div_mod(x as int + bit as int, 8);
    vstd::arithmetic::div_mod::lemma_mod_bound(x as int + bit as int, 8);
}

pub(super) proof fn compose_row_byte(
    index: int, stride: usize, dw: u32, dh: u32, sw: u32, sh: u32,
    x: u32, y: u32, row: u32,
)
    requires
        crate::spec::bitmap::clipped_width(dw, sw, x) > 0,
        row < crate::spec::bitmap::clipped_height(dh, sh, y),
        (y as int + row as int) * stride as int + x as int / 8 <= index,
        index < (y as int + row as int) * stride as int
            + (x as int + crate::spec::bitmap::clipped_width(dw, sw, x) as int + 7) / 8,
    ensures
        crate::spec::bitmap::compose_byte_may_change(
            index, stride, dw, dh, sw, sh, x, y,
        ),
{
    reveal(crate::spec::bitmap::clipped_width);
    reveal(crate::spec::bitmap::clipped_height);
    let w = crate::spec::bitmap::clipped_width(dw, sw, x) as int;
    let base = (y as int + row as int) * stride as int;
    let column = index - base;
    let px = if column == x as int / 8 { x as int } else { column * 8 };
    let sx = px - x as int;
    let sy = row as int;
    vstd::arithmetic::div_mod::lemma_fundamental_div_mod(x as int, 8);
    vstd::arithmetic::div_mod::lemma_mod_bound(x as int, 8);
    vstd::arithmetic::div_mod::lemma_fundamental_div_mod(x as int + w + 7, 8);
    vstd::arithmetic::div_mod::lemma_mod_bound(x as int + w + 7, 8);
    vstd::arithmetic::div_mod::lemma_fundamental_div_mod(px, 8);
    vstd::arithmetic::div_mod::lemma_mod_bound(px, 8);
    assert(0 <= sx < w);
    assert(px < dw);
    assert(y as int + sy < dh);
    reveal(crate::spec::bitmap::compose_byte_may_change);
    reveal(crate::spec::bitmap::byte_index);
    assert(index == crate::spec::bitmap::byte_index(
        stride, (x as int + sx) as u32, (y as int + sy) as u32,
    ) as int);
    assert(exists|candidate_x: int, candidate_y: int|
        0 <= candidate_x < crate::spec::bitmap::clipped_width(dw, sw, x)
        && 0 <= candidate_y < crate::spec::bitmap::clipped_height(dh, sh, y)
        && index == #[trigger] crate::spec::bitmap::byte_index(
            stride, (x as int + candidate_x) as u32, (y as int + candidate_y) as u32,
        ) as int
    );
}

pub(super) proof fn compose_step_frame(
    before: Seq<u8>, after: Seq<u8>, original: Seq<u8>,
    stride: usize, dw: u32, dh: u32, sw: u32, sh: u32,
    x: u32, y: u32, row: u32, start: usize, count: int,
)
    requires
        crate::spec::bitmap::compose_frame(original, before, stride, dw, dh, sw, sh, x, y),
        after.len() == before.len(),
        forall|i: int| 0 <= i < before.len()
            && (i < start as int || start as int + count <= i) ==> after[i] == before[i],
        count > 0,
        crate::spec::bitmap::clipped_width(dw, sw, x) > 0,
        row < crate::spec::bitmap::clipped_height(dh, sh, y),
        (y as int + row as int) * stride as int + x as int / 8 <= start as int,
        start as int + count <= (y as int + row as int) * stride as int
            + (x as int + crate::spec::bitmap::clipped_width(dw, sw, x) as int + 7) / 8,
    ensures
        crate::spec::bitmap::compose_frame(original, after, stride, dw, dh, sw, sh, x, y),
{
    reveal(crate::spec::bitmap::compose_frame);
    assert forall|i: int| 0 <= i < original.len()
        && !crate::spec::bitmap::compose_byte_may_change(i, stride, dw, dh, sw, sh, x, y)
        implies after[i] == original[i] by {
        if start as int <= i < start as int + count {
            compose_row_byte(i, stride, dw, dh, sw, sh, x, y, row);
        }
    }
}

pub(super) proof fn set_byte_reads_written_value(
    byte: u8,
    x: u32,
    value: bool,
)
    ensures
        crate::spec::bitmap::byte_bit(
            crate::spec::bitmap::set_byte_bit(byte, x, value),
            x,
        ) == value,
{
    reveal(crate::spec::bitmap::byte_bit);
    reveal(crate::spec::bitmap::set_byte_bit);
    reveal(crate::spec::bitmap::bit_mask);
    assert(x % 8 < 8);
    if x % 8 == 0 {
        if value {
            assert(((byte | 0x80u8) & 0x80u8) != 0) by (bit_vector);
        } else {
            assert(((byte & !0x80u8) & 0x80u8) == 0) by (bit_vector);
        }
    } else if x % 8 == 1 {
        if value {
            assert(((byte | 0x40u8) & 0x40u8) != 0) by (bit_vector);
        } else {
            assert(((byte & !0x40u8) & 0x40u8) == 0) by (bit_vector);
        }
    } else if x % 8 == 2 {
        if value {
            assert(((byte | 0x20u8) & 0x20u8) != 0) by (bit_vector);
        } else {
            assert(((byte & !0x20u8) & 0x20u8) == 0) by (bit_vector);
        }
    } else if x % 8 == 3 {
        if value {
            assert(((byte | 0x10u8) & 0x10u8) != 0) by (bit_vector);
        } else {
            assert(((byte & !0x10u8) & 0x10u8) == 0) by (bit_vector);
        }
    } else if x % 8 == 4 {
        if value {
            assert(((byte | 0x08u8) & 0x08u8) != 0) by (bit_vector);
        } else {
            assert(((byte & !0x08u8) & 0x08u8) == 0) by (bit_vector);
        }
    } else if x % 8 == 5 {
        if value {
            assert(((byte | 0x04u8) & 0x04u8) != 0) by (bit_vector);
        } else {
            assert(((byte & !0x04u8) & 0x04u8) == 0) by (bit_vector);
        }
    } else if x % 8 == 6 {
        if value {
            assert(((byte | 0x02u8) & 0x02u8) != 0) by (bit_vector);
        } else {
            assert(((byte & !0x02u8) & 0x02u8) == 0) by (bit_vector);
        }
    } else {
        assert(x % 8 == 7);
        if value {
            assert(((byte | 0x01u8) & 0x01u8) != 0) by (bit_vector);
        } else {
            assert(((byte & !0x01u8) & 0x01u8) == 0) by (bit_vector);
        }
    }
}

}

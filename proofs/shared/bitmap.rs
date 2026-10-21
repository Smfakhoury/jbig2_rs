use vstd::prelude::*;

verus! {

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

pub(super) proof fn in_bounds_byte(
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

use vstd::prelude::*;

verus! {

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

use vstd::prelude::*;

macro_rules! production_filled_tail_bounds {
    ($data:ident, $height:ident, $stride:ident, $row:ident, $offset:ident, $last:ident, $index:ident) => {
        proof! {
            assert($row < $height as usize);
            assert($offset as nat == $row as nat * $stride as nat);
            assert($last as nat + 1 == $stride as nat);
            assert($index as nat == $offset as nat + $last as nat);
            vstd::arithmetic::mul::lemma_mul_inequality(
                $row as int + 1, $height as int, $stride as int,
            );
            vstd::arithmetic::mul::lemma_mul_is_distributive_add_other_way(
                $stride as int, $row as int, 1,
            );
            vstd::arithmetic::mul::lemma_mul_is_commutative($height as int, $stride as int);
            assert(($row as nat + 1) * $stride as nat <= $height as nat * $stride as nat);
            assert($index as nat + 1 == ($row as nat + 1) * $stride as nat);
            assert($index < $data@.len());
        }
    };
}

macro_rules! production_filled_storage_step {
    ($before:ident, $data:ident, $width:ident, $stride:ident, $height:ident, $row:ident, $mask:ident) => {
        proof! {
            assert($before[$row as int * $stride as int + $stride as int - 1] == 0xFFu8);
            assert((0xFFu8 & $mask) == $mask) by(bit_vector);
            assert(crate::spec::bitmap::filled_byte($width, ($stride - 1) as nat, true) == $mask);
            crate::bitmap::bitmap_shared::black_fill_step(
                $before, $data@, $width, $stride, $height, $row,
            );
        }
    };
}

macro_rules! production_filled_storage_result {
    ($bitmap:ident, $value:ident) => {
        proof! {
            crate::bitmap::bitmap_shared::filled_storage_result(&$bitmap, $value);
        }
    };
}

macro_rules! production_black_span_init {
    ($bitmap:ident, $row:ident, $start:ident, $end:ident, $original:ident, $valid:ident) => {
        proof! {
            crate::views::bitmap_fields($bitmap);
            crate::views::bitmap_fields(old($bitmap));
            assert($bitmap.data@.len() == $bitmap.data.len());
            assert($bitmap.data@.len() <= usize::MAX);
        }
        proof_decl! { let ghost $original = $bitmap.data@; }
        proof_decl! { let ghost $valid = crate::spec::bitmap::storage_valid($bitmap); }
        proof! {
            assert(crate::spec::bitmap::black_span_bits(
                $original, $original, $row as int * $bitmap.stride as int, $start, $start,
            ));
            if $start == $end && $valid {
                crate::bitmap::bitmap_shared::black_span_result(old($bitmap), $bitmap, $row, $start, $end);
            }
        }
    };
}

macro_rules! production_black_span_bounds {
    ($bitmap:ident, $row:ident, $end:ident, $valid:ident) => {
        proof! {
            assert(u32::MAX as nat <= usize::MAX) by (compute);
            if $valid {
                crate::bitmap::bitmap_shared::in_bounds_byte($bitmap, ($end - 1) as u32, $row);
                vstd::arithmetic::div_mod::lemma_fundamental_div_mod($end as int - 1, 8);
                vstd::arithmetic::div_mod::lemma_mod_bound($end as int - 1, 8);
                vstd::arithmetic::div_mod::lemma_fundamental_div_mod($end as int + 7, 8);
                vstd::arithmetic::div_mod::lemma_mod_bound($end as int + 7, 8);
                assert($row as int * $bitmap.stride as int + ($end as int + 7) / 8 <= $bitmap.data@.len());
                assert($bitmap.data@.len() <= usize::MAX);
            }
        }
    };
}

macro_rules! production_black_span_first_arithmetic {
    ($start:ident, $first_bits:ident) => {
        proof! {
            vstd::arithmetic::div_mod::lemma_fundamental_div_mod($start as int, 8);
            vstd::arithmetic::div_mod::lemma_mod_bound($start as int, 8);
            vstd::arithmetic::div_mod::lemma_fundamental_div_mod($start as int + $first_bits as int + 7, 8);
            vstd::arithmetic::div_mod::lemma_mod_bound($start as int + $first_bits as int + 7, 8);
        }
    };
}

macro_rules! production_black_span_first {
    ($bitmap:ident, $valid:ident, $original:ident, $row_start:ident, $start:ident, $first_bits:ident) => {
        production_black_span_first_arithmetic!($start, $first_bits);
        proof! {
            if $valid {
                crate::bitmap::bitmap_shared::black_span_partial_step(
                    $original, $original, $bitmap.data@, $row_start as int, $start, $start, $first_bits,
                );
            }
        }
    };
}

macro_rules! production_black_span_access {
    ($bitmap:ident, $index:ident, $row_start:ident, $x:ident, $count:literal, $valid:ident, $before:ident) => {
        proof! {
            vstd::arithmetic::div_mod::lemma_fundamental_div_mod($x as int, 8);
            vstd::arithmetic::div_mod::lemma_fundamental_div_mod($x as int + 7, 8);
            vstd::arithmetic::div_mod::lemma_mod_bound($x as int + 7, 8);
            assert($index as int == $row_start as int + $x as int / 8);
            if $valid {
                assert($index as int + $count <= $bitmap.data@.len());
            }
        }
        proof_decl! { let ghost $before = $bitmap.data@; }
    };
}

macro_rules! production_black_span_word {
    ($bitmap:ident, $valid:ident, $original:ident, $before:ident,
     $row_start:ident, $start:ident, $x:ident, $index:ident) => {
        proof! {
            if $valid {
                assert forall|offset: int| 0 <= offset < 8 implies
                    crate::spec::bitmap::chunk_byte(u64::MAX, offset) == 0xFFu8 by {
                    reveal(crate::spec::bitmap::chunk_byte);
                    assert((u64::MAX >> 56) as u8 == 0xFFu8) by (bit_vector);
                    assert((u64::MAX >> 48) as u8 == 0xFFu8) by (bit_vector);
                    assert((u64::MAX >> 40) as u8 == 0xFFu8) by (bit_vector);
                    assert((u64::MAX >> 32) as u8 == 0xFFu8) by (bit_vector);
                    assert((u64::MAX >> 24) as u8 == 0xFFu8) by (bit_vector);
                    assert((u64::MAX >> 16) as u8 == 0xFFu8) by (bit_vector);
                    assert((u64::MAX >> 8) as u8 == 0xFFu8) by (bit_vector);
                    assert(u64::MAX as u8 == 0xFFu8) by (bit_vector);
                }
                vstd::arithmetic::div_mod::lemma_fundamental_div_mod($x as int + 64, 8);
                vstd::arithmetic::div_mod::lemma_mod_bound($x as int + 64, 8);
                assert(($x as int + 64) / 8 == $x as int / 8 + 8);
                assert forall|i: int| 0 <= i < $before.len() implies
                    $bitmap.data@[i] == if $row_start as int + $x as int / 8 <= i
                        < $row_start as int + ($x as int + 64) / 8 { 0xFFu8 } else { $before[i] } by {
                    if $index as int <= i < $index as int + 8 {
                        assert($bitmap.data@[$index as int + (i - $index as int)]
                            == crate::spec::bitmap::chunk_byte(u64::MAX, i - $index as int));
                    }
                }
                crate::bitmap::bitmap_shared::black_span_full_step(
                    $original, $before, $bitmap.data@, $row_start as int, $start, $x, 64,
                );
            }
            vstd::arithmetic::div_mod::lemma_fundamental_div_mod($x as int, 8);
            vstd::arithmetic::div_mod::lemma_fundamental_div_mod($x as int + 71, 8);
            vstd::arithmetic::div_mod::lemma_mod_bound($x as int + 71, 8);
        }
    };
}

macro_rules! production_black_span_byte {
    ($bitmap:ident, $valid:ident, $original:ident, $before:ident,
     $row_start:ident, $start:ident, $x:ident) => {
        proof! {
            if $valid {
                crate::bitmap::bitmap_shared::black_span_full_step(
                    $original, $before, $bitmap.data@, $row_start as int, $start, $x, 8,
                );
            }
            vstd::arithmetic::div_mod::lemma_fundamental_div_mod($x as int, 8);
            vstd::arithmetic::div_mod::lemma_fundamental_div_mod($x as int + 15, 8);
            vstd::arithmetic::div_mod::lemma_mod_bound($x as int + 15, 8);
        }
    };
}

macro_rules! production_black_span_tail {
    ($bitmap:ident, $original:ident, $before:ident,
     $row_start:ident, $start:ident, $x:ident, $end:ident, $index:ident, $mask:ident) => {
        proof! {
            vstd::arithmetic::div_mod::lemma_fundamental_div_mod($x as int, 8);
            vstd::arithmetic::div_mod::lemma_fundamental_div_mod($x as int + 7, 8);
            vstd::arithmetic::div_mod::lemma_mod_bound($x as int + 7, 8);
            assert($index as int == $row_start as int + $x as int / 8);
            assert(0xFFu8 & $mask == $mask) by (bit_vector);
            if crate::spec::bitmap::storage_valid(old($bitmap)) {
                assert($bitmap.data@[$index as int] == $before[$index as int] | $mask);
                assert(0xFFu8 >> ($x % 8) == 0xFFu8) by (bit_vector)
                    requires $x % 8 == 0;
                assert($mask == ((0xFFu8 >> ($x % 8))
                    & (0xFFu8 << ((8 - $x % 8 - ($end - $x)) as u32))));
                crate::bitmap::bitmap_shared::black_span_partial_step(
                    $original, $before, $bitmap.data@, $row_start as int, $start, $x, ($end - $x) as u32,
                );
            }
        }
    };
}

macro_rules! production_black_span_result {
    ($bitmap:ident, $valid:ident, $row:ident, $start:ident, $end:ident) => {
        proof! {
            if $valid {
                crate::bitmap::bitmap_shared::black_span_result(old($bitmap), $bitmap, $row, $start, $end);
            }
        }
    };
}

macro_rules! production_bitmap_row_copy_bounds {
    ($bitmap:ident, $source:ident, $destination:ident) => {
        proof! {
            crate::bitmap::bitmap_shared::row_copy_bounds($bitmap, $source, $destination);
            crate::views::bitmap_fields($bitmap);
            crate::views::bitmap_fields(old($bitmap));
            reveal(crate::spec::bitmap::storage_valid);
            reveal(crate::spec::bitmap::row_copy_result);
        }
    };
}

macro_rules! production_bitmap_fields {
    ($bitmap:ident) => {
        proof! {
            crate::views::bitmap_fields($bitmap);
        }
    };
}

macro_rules! production_bitmap_in_bounds_byte {
    ($bitmap:ident, $x:ident, $y:ident) => {
        proof! {
            crate::bitmap::bitmap_shared::in_bounds_byte($bitmap, $x, $y);
        }
    };
}

macro_rules! production_bitmap_checked_dimensions {
    ($width:ident, $height:ident) => {
        proof! {
            crate::bitmap::bitmap_shared::u32_bitmap_arithmetic_is_checked(
                $width,
                $height,
            );
        }
    };
}

macro_rules! production_bitmap_clipped_bounds {
    ($destination:ident, $source:ident, $x:ident, $y:ident) => {
        proof! {
            crate::bitmap::bitmap_shared::clipped_bounds(
                $destination.width,
                $destination.height,
                $source.width,
                $source.height,
                $x,
                $y,
            );
        }

    };
}

macro_rules! production_bitmap_compose_frame {
    ($bitmap:ident, $source:ident, $x:ident, $y:ident, $row:ident,
     $start:ident, $count:expr, $before:ident, $original:ident) => {
        proof! {
            let clipped = crate::spec::bitmap::clipped_width(
                $bitmap.width, $source.width, $x,
            ) as int;
            vstd::arithmetic::div_mod::lemma_fundamental_div_mod($x as int, 8);
            vstd::arithmetic::div_mod::lemma_mod_bound($x as int, 8);
            vstd::arithmetic::div_mod::lemma_fundamental_div_mod(clipped, 8);
            vstd::arithmetic::div_mod::lemma_mod_bound(clipped, 8);
            vstd::arithmetic::div_mod::lemma_fundamental_div_mod($x as int + clipped + 7, 8);
            vstd::arithmetic::div_mod::lemma_mod_bound($x as int + clipped + 7, 8);
            crate::bitmap::bitmap_shared::compose_step_frame(
                $before, $bitmap.data@, $original,
                $bitmap.stride, $bitmap.width, $bitmap.height,
                $source.width, $source.height, $x, $y, $row, $start, $count,
            );
        }
    };
}

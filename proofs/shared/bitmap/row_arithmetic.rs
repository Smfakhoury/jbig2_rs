// Pure row-arithmetic proof support, not a normative rule.
// Guide: ../../../specs/README.md

use super::*;

verus! {

pub(in crate::bitmap) proof fn row_byte_bounds(row: int, column: int, stride: int, height: int)
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

pub(in crate::bitmap) proof fn distinct_row_tail(row: int, column: int, current: int, stride: int)
    requires
        0 <= row, 0 <= current, 0 <= column < stride,
        row != current || column + 1 != stride,
    ensures row * stride + column != current * stride + stride - 1,
{
    vstd::arithmetic::mul::lemma_mul_is_distributive_add_other_way(stride, row, 1);
    vstd::arithmetic::mul::lemma_mul_is_distributive_add_other_way(stride, current, 1);
    if row < current {
        vstd::arithmetic::mul::lemma_mul_inequality(row + 1, current, stride);
    } else if row > current {
        vstd::arithmetic::mul::lemma_mul_inequality(current + 1, row, stride);
    }
}


}

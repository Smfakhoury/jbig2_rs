use vstd::prelude::*;

verus! {

pub(super) proof fn one_pattern_has_zero_planes()
    ensures super::halftone_spec::gray_depth_valid(1, 0),
{
    assert(super::halftone_spec::pow2(32) == 4_294_967_296) by (compute);
}

pub(super) proof fn gray_binary_step_is_xor(
    coded: bool,
    previous: Option<bool>,
)
    ensures
        super::halftone_spec::gray_binary_bit(coded, previous)
            == match previous {
                Some(bit) => coded != bit,
                None => coded,
            },
{}

pub(super) proof fn parsed_operator_uses_bitmap_model(
    code: u8,
    destination: bool,
    source: bool,
)
    ensures match super::halftone_spec::operator_for_code(code) {
        Some(operator) =>
            crate::spec::bitmap::combine(operator, destination, source)
                == match code {
                    0 => destination || source,
                    1 => destination && source,
                    2 => destination != source,
                    3 => destination == source,
                    _ => source,
                },
        None => true,
    },
{
    match code {
        0 => {},
        1 => {},
        2 => {},
        3 => {},
        4 => {},
        _ => {},
    }
}

pub(super) proof fn row_major_order_is_strict(
    width: u32,
    first_x: u32,
    first_y: u32,
    second_x: u32,
    second_y: u32,
)
    requires
        first_x < width,
        second_x < width,
        super::halftone_spec::render_precedes(
            first_x,
            first_y,
            second_x,
            second_y,
        ),
    ensures
        super::halftone_spec::row_major_offset(width, first_x, first_y)
            < super::halftone_spec::row_major_offset(
                width,
                second_x,
                second_y,
            ),
{
    if first_y < second_y {
        let w = width as nat;
        let fx = first_x as nat;
        let fy = first_y as nat;
        let sx = second_x as nat;
        let sy = second_y as nat;
        let difference = (sy - fy) as nat;
        assert(0 < w);
        assert(fx < w);
        assert(0 < difference);
        assert(sy == fy + difference);
        let remainder = (difference - 1) as nat;
        assert(difference == 1 + remainder);
        assert(0 <= w * remainder);
        assert(w * difference == w * (1 + remainder));
        assert(
            w * (1 + remainder) == w + w * remainder
        ) by (nonlinear_arith);
        assert(w * sy == w * (fy + difference));
        assert(
            w * (fy + difference) == w * fy + w * difference
        ) by (nonlinear_arith);
        assert(w * difference >= w);
        assert(w * fy + fx < w * fy + w);
        assert(w * fy + w <= w * sy + sx);
        assert(fy * w == w * fy) by (nonlinear_arith);
        assert(sy * w == w * sy) by (nonlinear_arith);
        assert(fy * w + fx < sy * w + sx);
    }
}

pub(super) proof fn checked_sum_is_monotonic(left: u64, right: u64)
    requires left as nat + right as nat <= u64::MAX as nat,
    ensures
        left <= left + right,
        right <= left + right,
{}

pub(super) proof fn checked_product_is_monotonic(left: u64, right: u64)
    requires left > 0, right > 0, left as nat * right as nat <= u64::MAX as nat,
    ensures left <= left * right, right <= left * right,
{
    let l = left as nat;
    let r = right as nat;
    let r_remainder = (r - 1) as nat;
    let l_remainder = (l - 1) as nat;
    assert(r == 1 + r_remainder);
    assert(l == 1 + l_remainder);
    assert(0 <= l * r_remainder);
    assert(0 <= l_remainder * r);
    assert(l * r == l * (1 + r_remainder));
    assert(l * (1 + r_remainder) == l + l * r_remainder)
        by (nonlinear_arith);
    assert(l * r == (1 + l_remainder) * r);
    assert((1 + l_remainder) * r == r + l_remainder * r)
        by (nonlinear_arith);
}

pub(super) proof fn multiply_monotonic(
    factor: nat,
    first: nat,
    second: nat,
)
    requires first <= second,
    ensures factor * first <= factor * second,
{
    let difference = (second - first) as nat;
    assert(second == first + difference);
    assert(0 <= factor * difference);
    assert(factor * second == factor * (first + difference));
    assert(
        factor * (first + difference)
            == factor * first + factor * difference
    ) by (nonlinear_arith);
}

pub(super) proof fn product_upper_bound(
    left: nat,
    right: nat,
    left_max: nat,
    right_max: nat,
)
    requires left <= left_max, right <= right_max,
    ensures left * right <= left_max * right_max,
{
    multiply_monotonic(left, right, right_max);
    multiply_monotonic(right_max, left, left_max);
    assert(left * right == right * left) by (nonlinear_arith);
    assert(left_max * right_max == right_max * left_max)
        by (nonlinear_arith);
}

pub(super) proof fn half_open_outside_rules(
    x: i64,
    y: i64,
    pattern_width: u32,
    pattern_height: u32,
    region_width: u32,
    region_height: u32,
)
    ensures
        super::halftone_spec::wholly_outside(
            x,
            y,
            pattern_width,
            pattern_height,
            region_width,
            region_height,
        )
        == (
            x as int + pattern_width as int <= 0
            || x as int >= region_width as int
            || y as int + pattern_height as int <= 0
            || y as int >= region_height as int
        ),
{}

pub(super) proof fn collective_slice_coordinate_is_bounded(
    pattern_index: u64,
    pattern_count: u64,
    x: u32,
    pattern_width: u8,
)
    requires
        pattern_index < pattern_count,
        x < pattern_width,
        (pattern_count as nat) * (pattern_width as nat) <= (u32::MAX as nat),
    ensures
        (pattern_index as nat) * (pattern_width as nat) + (x as nat)
            < (pattern_count as nat) * (pattern_width as nat),
{
    let index = pattern_index as nat;
    let count = pattern_count as nat;
    let offset = x as nat;
    let width = pattern_width as nat;
    let difference = (count - index) as nat;
    assert(0 < width);
    assert(offset < width);
    assert(0 < difference);
    assert(count == index + difference);
    let remainder = (difference - 1) as nat;
    assert(difference == 1 + remainder);
    assert(0 <= width * remainder);
    assert(width * difference == width * (1 + remainder));
    assert(width * (1 + remainder) == width + width * remainder)
        by (nonlinear_arith);
    assert(width * count == width * (index + difference));
    assert(width * (index + difference) == width * index + width * difference)
        by (nonlinear_arith);
    assert(width * difference >= width);
    assert(width * index + offset < width * index + width);
    assert(width * index + width <= width * count);
    assert(index * width == width * index) by (nonlinear_arith);
    assert(count * width == width * count) by (nonlinear_arith);
    assert(index * width + offset < count * width);
}

}

use vstd::prelude::*;

verus! {

pub(super) proof fn one_pattern_runtime_fact()
    ensures super::halftone_spec::gray_depth_valid(1, 0),
{
    super::halftone_properties::one_pattern_has_zero_planes();
}

pub(super) proof fn row_major_runtime_fact(
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
    super::halftone_properties::row_major_order_is_strict(
        width,
        first_x,
        first_y,
        second_x,
        second_y,
    );
}

}

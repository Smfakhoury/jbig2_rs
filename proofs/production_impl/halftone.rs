use vstd::prelude::*;

macro_rules! production_grid_position_bounds {
    ($ng:ident, $mg:ident, $vector_x:ident, $vector_y:ident) => {
        proof! {
            assert(0 <= $ng <= u32::MAX as int);
            assert(0 <= $mg <= u32::MAX as int);
            assert(0 <= $vector_x <= u16::MAX as int);
            assert(0 <= $vector_y <= u16::MAX as int);
            crate::halftone::halftone_properties::product_upper_bound(
                $mg as nat,
                $vector_y as nat,
                u32::MAX as nat,
                u16::MAX as nat,
            );
            crate::halftone::halftone_properties::product_upper_bound(
                $ng as nat,
                $vector_x as nat,
                u32::MAX as nat,
                u16::MAX as nat,
            );
            crate::halftone::halftone_properties::product_upper_bound(
                $mg as nat,
                $vector_x as nat,
                u32::MAX as nat,
                u16::MAX as nat,
            );
            crate::halftone::halftone_properties::product_upper_bound(
                $ng as nat,
                $vector_y as nat,
                u32::MAX as nat,
                u16::MAX as nat,
            );
            assert(
                (u32::MAX as nat) * (u16::MAX as nat)
                    == 281_470_681_677_825
            ) by (compute);
            assert(
                2 * 281_470_681_677_825 + (i32::MAX as int)
                    < i64::MAX as int
            ) by (compute);
            assert(
                (i32::MIN as int) - 281_470_681_677_825
                    > i64::MIN as int
            ) by (compute);
        }
    };
}

macro_rules! production_row_major_bounds {
    ($y:ident, $width:ident) => {
        proof! {
            crate::halftone::halftone_properties::product_upper_bound(
                $y as nat,
                $width as nat,
                u32::MAX as nat,
                u32::MAX as nat,
            );
            assert(
                (u32::MAX as nat) * (u32::MAX as nat)
                    + (u32::MAX as nat)
                    < u64::MAX as nat
            ) by (compute);
        }
    };
}

macro_rules! production_one_pattern_zero_plane {
    () => {
        proof! {
            crate::halftone::halftone_shared::one_pattern_runtime_fact();
        }
    };
}

macro_rules! production_skip_boundary {
    (
        $x:ident,
        $y:ident,
        $pattern_width:ident,
        $pattern_height:ident,
        $region_width:ident,
        $region_height:ident
    ) => {
        proof! {
            crate::halftone::halftone_properties::half_open_outside_rules(
                $x,
                $y,
                $pattern_width,
                $pattern_height,
                $region_width,
                $region_height,
            );
        }
    };
}

macro_rules! production_slice_coordinate {
    (
        $pattern_index:ident,
        $pattern_count:ident,
        $x:ident,
        $header:ident,
        $source_x:ident,
        $expected_width:ident
    ) => {
        proof! {
            crate::halftone::halftone_properties::collective_slice_coordinate_is_bounded(
                $pattern_index,
                $pattern_count,
                $x,
                $header.pattern_width,
            );
            assert(
                ($source_x as nat) + ($x as nat)
                    < ($expected_width as nat)
            );
        }
    };
}

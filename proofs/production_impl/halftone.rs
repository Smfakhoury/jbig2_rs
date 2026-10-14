use vstd::prelude::*;

macro_rules! production_one_pattern_zero_plane {
    () => {
        proof! {
            crate::halftone::halftone_shared::one_pattern_runtime_fact();
        }
    };
}

macro_rules! production_gray_binary_step {
    ($coded:ident, $previous:ident) => {
        proof! {
            crate::halftone::halftone_properties::gray_binary_step_is_xor(
                $coded,
                $previous,
            );
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

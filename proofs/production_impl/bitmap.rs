use vstd::prelude::*;

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

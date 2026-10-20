use vstd::prelude::*;

macro_rules! production_generic_copied_row_pixels {
    ($bitmap:ident, $source:ident, $destination:ident) => {
        proof! {
            crate::views::bitmap_fields(old($bitmap));
            crate::views::bitmap_fields($bitmap);
            crate::bitmap::bitmap_shared::row_copy_pixels(
                old($bitmap), $bitmap, $source, $destination,
            );
        }
    };
}

macro_rules! production_generic_template_facts {
    ($template:expr) => {
        proof! {
            generic_region_shared::generic_template_runtime_facts($template);
        }
    };
}

macro_rules! production_refinement_template_facts {
    ($template:expr) => {
        proof! {
            generic_region_shared::refinement_template_runtime_facts($template);
        }
    };
}

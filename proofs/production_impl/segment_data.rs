use vstd::prelude::*;

macro_rules! production_dimension_product {
    ($width:ident, $height:ident) => {
        proof! {
            segment_data_shared::dimension_product_runtime_fact($width, $height);
        }
    };
}

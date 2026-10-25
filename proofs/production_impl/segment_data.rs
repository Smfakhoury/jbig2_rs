use vstd::prelude::*;

macro_rules! production_dimension_product {
    ($width:ident, $height:ident) => {
        proof! {
            segment_data_shared::dimension_product_runtime_fact($width, $height);
        }
    };
}

macro_rules! production_profiles_fields {
    ($payload:ident, $profiles:ident, $count:ident) => {
        proof! {
            assert($count == crate::spec::segments::be_u32_at($payload@, 0));
            assert($profiles@.len() == $count);
            assert forall|index: int| 0 <= index < $profiles@.len() implies
                #[trigger] $profiles@[index]
                    == crate::spec::segments::be_u32_at($payload@, 4 + index * 4) by {
                assert($profiles@[0 + index]
                    == crate::spec::segments::be_u32_at($payload@, 4 + index * 4));
            }
        }
    };
}

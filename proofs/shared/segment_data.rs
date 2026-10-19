use vstd::prelude::*;

verus! {

pub(super) proof fn dimension_product_runtime_fact(width: u32, height: u32)
    ensures width as nat * height as nat <= u64::MAX as nat,
{
    super::segment_data_properties::dimension_product_exact(width, height);
}

}

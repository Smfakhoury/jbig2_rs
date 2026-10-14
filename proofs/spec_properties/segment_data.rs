use vstd::prelude::*;

verus! {

pub(super) proof fn dimension_product_exact(width: u32, height: u32)
    ensures width as nat * height as nat <= u64::MAX as nat,
{
    assert(width as nat <= u32::MAX as nat);
    assert(height as nat <= u32::MAX as nat);
    assert(u32::MAX as nat * u32::MAX as nat <= u64::MAX as nat) by (compute);
    assert(width as nat * height as nat
        <= u32::MAX as nat * u32::MAX as nat) by (nonlinear_arith);
}

}

//! Reviewed proof-only standard interface, excluded from the runtime package.
//! No capacity/length/reservation equality or lower bound is asserted.

#![cfg_attr(verus_keep_ghost, feature(allocator_api))]

#[cfg(verus_keep_ghost)]
use std::alloc::Allocator;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg(verus_keep_ghost)]
verus! {

pub assume_specification<T, A: Allocator>[ Vec::<T, A>::capacity ](value: &Vec<T, A>) -> usize;

pub fn observe_capacity_budget<T, A: Allocator>(
    value: &Vec<T, A>,
    element_bytes: u64,
    limit: u64,
) -> (out: Option<(usize, u64)>)
    ensures
        match out {
            Some((observed, bytes)) =>
                bytes as nat == observed as nat * element_bytes as nat
                && bytes <= limit,
            None => true,
        },
{
    let observed = value.capacity();
    let Ok(count) = u64::try_from(observed) else {
        return None;
    };
    let Some(bytes) = count.checked_mul(element_bytes) else {
        return None;
    };
    if bytes > limit {
        None
    } else {
        Some((observed, bytes))
    }
}

}

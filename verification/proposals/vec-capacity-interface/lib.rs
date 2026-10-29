//! Unapproved standard-interface proposal, excluded from the runtime package.
//! No capacity/length/reservation equality or lower bound is asserted.

#![cfg_attr(verus_keep_ghost, feature(allocator_api))]

#[cfg(verus_keep_ghost)]
use std::alloc::Allocator;
use vstd::prelude::*;

verus! {

pub assume_specification<T, A: Allocator>[ Vec::<T, A>::capacity ](value: &Vec<T, A>) -> usize;

}

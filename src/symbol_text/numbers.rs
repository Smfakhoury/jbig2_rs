//! Numbers implementation.

#[cfg(verus_keep_ghost)]
use super::*;

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out as int == value as int,
))]
pub(super) fn widen_u32_i64(value: u32) -> i64 {
    i64::from(value)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out as int == value as int,
))]
pub(super) fn widen_u8_i64(value: u8) -> i64 {
    i64::from(value)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out as int == value as int,
))]
pub(super) fn widen_u8_i32(value: u8) -> i32 {
    i32::from(value)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out == if value { 1i64 } else { 0i64 },
))]
// The pinned verifier needs the explicit Boolean relation instead of From<bool>.
#[allow(clippy::bool_to_int_with_if)]
pub(super) fn widen_bool_i64(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

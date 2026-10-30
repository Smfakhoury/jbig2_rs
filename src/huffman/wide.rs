//! Wide implementation.

#[cfg(not(verus_keep_ghost))]
use super::WideCode;
#[cfg(verus_keep_ghost)]
use super::*;

impl WideCode {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out.limbs@ == seq![0u64, 0u64, 0u64, 0u64],
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(super) const fn zero() -> Self {
        Self { limbs: [0; 4] }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == huffman_spec::checked_add_small(self, value),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    #[allow(clippy::cast_lossless, clippy::if_not_else)]
    pub(super) fn checked_add_small(self, value: u32) -> Option<Self> {
        let value64 = value as u64;
        let (low, carry) = if value64 <= u64::MAX - self.limbs[0] {
            (self.limbs[0] + value64, false)
        } else {
            (value64 - (u64::MAX - self.limbs[0]) - 1, true)
        };
        if !carry {
            return Some(Self {
                limbs: [low, self.limbs[1], self.limbs[2], self.limbs[3]],
            });
        }
        if self.limbs[1] != u64::MAX {
            return Some(Self {
                limbs: [low, self.limbs[1] + 1, self.limbs[2], self.limbs[3]],
            });
        }
        if self.limbs[2] != u64::MAX {
            return Some(Self {
                limbs: [low, 0, self.limbs[2] + 1, self.limbs[3]],
            });
        }
        if self.limbs[3] != u64::MAX {
            Some(Self {
                limbs: [low, 0, 0, self.limbs[3] + 1],
            })
        } else {
            None
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == huffman_spec::checked_shift_left_one(self),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(super) fn checked_shift_left_one(self) -> Option<Self> {
        if self.limbs[3] >> 63 != 0 {
            return None;
        }
        Some(Self {
            limbs: [
                self.limbs[0] << 1,
                (self.limbs[1] << 1) | (self.limbs[0] >> 63),
                (self.limbs[2] << 1) | (self.limbs[1] >> 63),
                (self.limbs[3] << 1) | (self.limbs[2] >> 63),
            ],
        })
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires bit <= 1,
        ensures out == huffman_spec::append_bit(self, bit),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(super) fn append_bit(self, bit: u32) -> Option<Self> {
        self.checked_shift_left_one()?.checked_add_small(bit)
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == huffman_spec::fits_in_bits(self, bit_count),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(super) fn fits_in_bits(self, bit_count: usize) -> bool {
        if bit_count >= 256 {
            return true;
        }
        let limb = bit_count / 64;
        let bit = bit_count % 64;
        if limb < 3 && self.limbs[3] != 0 {
            return false;
        }
        if limb < 2 && self.limbs[2] != 0 {
            return false;
        }
        if limb < 1 && self.limbs[1] != 0 {
            return false;
        }
        self.limbs[limb] >> bit == 0
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == huffman_spec::small_distance_from(self, lower, upper_bound),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    #[allow(clippy::cast_lossless, clippy::cast_possible_truncation)]
    pub(super) fn small_distance_from(self, lower: Self, upper_bound: u32) -> Option<u32> {
        let (difference0, borrow0) = subtract_limb(self.limbs[0], lower.limbs[0], false);
        let (difference1, borrow1) = subtract_limb(self.limbs[1], lower.limbs[1], borrow0);
        let (difference2, borrow2) = subtract_limb(self.limbs[2], lower.limbs[2], borrow1);
        let (difference3, borrow) = subtract_limb(self.limbs[3], lower.limbs[3], borrow2);
        let difference = [difference0, difference1, difference2, difference3];
        if borrow || difference[1] != 0 || difference[2] != 0 || difference[3] != 0 {
            return None;
        }
        if difference[0] > u32::MAX as u64 {
            return None;
        }
        let distance = difference[0] as u32;
        if distance < upper_bound {
            Some(distance)
        } else {
            None
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out == huffman_spec::subtract_limb(left, right, borrow),
))]
#[allow(clippy::bool_to_int_with_if)]
pub(super) fn subtract_limb(left: u64, right: u64, borrow: bool) -> (u64, bool) {
    let borrow_value = if borrow { 1 } else { 0 };
    let (partial, first_borrow) = if left >= right {
        (left - right, false)
    } else {
        (u64::MAX - (right - left) + 1, true)
    };
    let (value, second_borrow) = if partial >= borrow_value {
        (partial - borrow_value, false)
    } else {
        (u64::MAX, true)
    };
    (value, first_borrow || second_borrow)
}

//! Tables implementation.

#[cfg(verus_keep_ghost)]
use super::*;
#[cfg(not(verus_keep_ghost))]
use super::{HuffmanLine, LineKind};

macro_rules! normal_line {
    ($low:expr, $prefix:expr, $range:expr) => {
        HuffmanLine {
            prefix_len: $prefix,
            range_len: $range,
            range_low: $low,
            kind: LineKind::Normal,
        }
    };
}

macro_rules! lower_line {
    ($low:expr, $prefix:expr) => {
        HuffmanLine {
            prefix_len: $prefix,
            range_len: 32,
            range_low: $low,
            kind: LineKind::Lower,
        }
    };
}

macro_rules! upper_line {
    ($low:expr, $prefix:expr) => {
        HuffmanLine {
            prefix_len: $prefix,
            range_len: 32,
            range_low: $low,
            kind: LineKind::Upper,
        }
    };
}

macro_rules! oob_line {
    ($prefix:expr) => {
        HuffmanLine {
            prefix_len: $prefix,
            range_len: 0,
            range_low: 0,
            kind: LineKind::OutOfBand,
        }
    };
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(super) const TABLE_A: [HuffmanLine; 4] = [
    normal_line!(0, 1, 4),
    normal_line!(16, 2, 8),
    normal_line!(272, 3, 16),
    upper_line!(65_808, 3),
];

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(super) const TABLE_B: [HuffmanLine; 7] = [
    normal_line!(0, 1, 0),
    normal_line!(1, 2, 0),
    normal_line!(2, 3, 0),
    normal_line!(3, 4, 3),
    normal_line!(11, 5, 6),
    upper_line!(75, 6),
    oob_line!(6),
];

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(super) const TABLE_C: [HuffmanLine; 9] = [
    normal_line!(-256, 8, 8),
    normal_line!(0, 1, 0),
    normal_line!(1, 2, 0),
    normal_line!(2, 3, 0),
    normal_line!(3, 4, 3),
    normal_line!(11, 5, 6),
    lower_line!(-257, 8),
    upper_line!(75, 7),
    oob_line!(6),
];

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(super) const TABLE_D: [HuffmanLine; 6] = [
    normal_line!(1, 1, 0),
    normal_line!(2, 2, 0),
    normal_line!(3, 3, 0),
    normal_line!(4, 4, 3),
    normal_line!(12, 5, 6),
    upper_line!(76, 5),
];

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(super) const TABLE_E: [HuffmanLine; 8] = [
    normal_line!(-255, 7, 8),
    normal_line!(1, 1, 0),
    normal_line!(2, 2, 0),
    normal_line!(3, 3, 0),
    normal_line!(4, 4, 3),
    normal_line!(12, 5, 6),
    lower_line!(-256, 7),
    upper_line!(76, 6),
];

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(super) const TABLE_F: [HuffmanLine; 14] = [
    normal_line!(-2048, 5, 10),
    normal_line!(-1024, 4, 9),
    normal_line!(-512, 4, 8),
    normal_line!(-256, 4, 7),
    normal_line!(-128, 5, 6),
    normal_line!(-64, 5, 5),
    normal_line!(-32, 4, 5),
    normal_line!(0, 2, 7),
    normal_line!(128, 3, 7),
    normal_line!(256, 3, 8),
    normal_line!(512, 4, 9),
    normal_line!(1024, 4, 10),
    lower_line!(-2049, 6),
    upper_line!(2048, 6),
];

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(super) const TABLE_G: [HuffmanLine; 15] = [
    normal_line!(-1024, 4, 9),
    normal_line!(-512, 3, 8),
    normal_line!(-256, 4, 7),
    normal_line!(-128, 5, 6),
    normal_line!(-64, 5, 5),
    normal_line!(-32, 4, 5),
    normal_line!(0, 4, 5),
    normal_line!(32, 5, 5),
    normal_line!(64, 5, 6),
    normal_line!(128, 4, 7),
    normal_line!(256, 3, 8),
    normal_line!(512, 3, 9),
    normal_line!(1024, 3, 10),
    lower_line!(-1025, 5),
    upper_line!(2048, 5),
];

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(super) const TABLE_H: [HuffmanLine; 21] = [
    normal_line!(-15, 8, 3),
    normal_line!(-7, 9, 1),
    normal_line!(-5, 8, 1),
    normal_line!(-3, 9, 0),
    normal_line!(-2, 7, 0),
    normal_line!(-1, 4, 0),
    normal_line!(0, 2, 1),
    normal_line!(2, 5, 0),
    normal_line!(3, 6, 0),
    normal_line!(4, 3, 4),
    normal_line!(20, 6, 1),
    normal_line!(22, 4, 4),
    normal_line!(38, 4, 5),
    normal_line!(70, 5, 6),
    normal_line!(134, 5, 7),
    normal_line!(262, 6, 7),
    normal_line!(390, 7, 8),
    normal_line!(646, 6, 10),
    lower_line!(-16, 9),
    upper_line!(1670, 9),
    oob_line!(2),
];

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(super) const TABLE_I: [HuffmanLine; 22] = [
    normal_line!(-31, 8, 4),
    normal_line!(-15, 9, 2),
    normal_line!(-11, 8, 2),
    normal_line!(-7, 9, 1),
    normal_line!(-5, 7, 1),
    normal_line!(-3, 4, 1),
    normal_line!(-1, 3, 1),
    normal_line!(1, 3, 1),
    normal_line!(3, 5, 1),
    normal_line!(5, 6, 1),
    normal_line!(7, 3, 5),
    normal_line!(39, 6, 2),
    normal_line!(43, 4, 5),
    normal_line!(75, 4, 6),
    normal_line!(139, 5, 7),
    normal_line!(267, 5, 8),
    normal_line!(523, 6, 8),
    normal_line!(779, 7, 9),
    normal_line!(1291, 6, 11),
    lower_line!(-32, 9),
    upper_line!(3339, 9),
    oob_line!(2),
];

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(super) const TABLE_J: [HuffmanLine; 21] = [
    normal_line!(-21, 7, 4),
    normal_line!(-5, 8, 0),
    normal_line!(-4, 7, 0),
    normal_line!(-3, 5, 0),
    normal_line!(-2, 2, 2),
    normal_line!(2, 5, 0),
    normal_line!(3, 6, 0),
    normal_line!(4, 7, 0),
    normal_line!(5, 8, 0),
    normal_line!(6, 2, 6),
    normal_line!(70, 5, 5),
    normal_line!(102, 6, 5),
    normal_line!(134, 6, 6),
    normal_line!(198, 6, 7),
    normal_line!(326, 6, 8),
    normal_line!(582, 6, 9),
    normal_line!(1094, 6, 10),
    normal_line!(2118, 7, 11),
    lower_line!(-22, 8),
    upper_line!(4166, 8),
    oob_line!(2),
];

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(super) const TABLE_K: [HuffmanLine; 13] = [
    normal_line!(1, 1, 0),
    normal_line!(2, 2, 1),
    normal_line!(4, 4, 0),
    normal_line!(5, 4, 1),
    normal_line!(7, 5, 1),
    normal_line!(9, 5, 2),
    normal_line!(13, 6, 2),
    normal_line!(17, 7, 2),
    normal_line!(21, 7, 3),
    normal_line!(29, 7, 4),
    normal_line!(45, 7, 5),
    normal_line!(77, 7, 6),
    upper_line!(141, 7),
];

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(super) const TABLE_L: [HuffmanLine; 13] = [
    normal_line!(1, 1, 0),
    normal_line!(2, 2, 0),
    normal_line!(3, 3, 1),
    normal_line!(5, 5, 0),
    normal_line!(6, 5, 1),
    normal_line!(8, 6, 1),
    normal_line!(10, 7, 0),
    normal_line!(11, 7, 1),
    normal_line!(13, 7, 2),
    normal_line!(17, 7, 3),
    normal_line!(25, 7, 4),
    normal_line!(41, 8, 5),
    upper_line!(73, 8),
];

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(super) const TABLE_M: [HuffmanLine; 13] = [
    normal_line!(1, 1, 0),
    normal_line!(2, 3, 0),
    normal_line!(3, 4, 0),
    normal_line!(4, 5, 0),
    normal_line!(5, 4, 1),
    normal_line!(7, 3, 3),
    normal_line!(15, 6, 1),
    normal_line!(17, 6, 2),
    normal_line!(21, 6, 3),
    normal_line!(29, 6, 4),
    normal_line!(45, 6, 5),
    normal_line!(77, 7, 6),
    upper_line!(141, 7),
];

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(super) const TABLE_N: [HuffmanLine; 5] = [
    normal_line!(-2, 3, 0),
    normal_line!(-1, 3, 0),
    normal_line!(0, 1, 0),
    normal_line!(1, 3, 0),
    normal_line!(2, 3, 0),
];

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(super) const TABLE_O: [HuffmanLine; 13] = [
    normal_line!(-24, 7, 4),
    normal_line!(-8, 6, 2),
    normal_line!(-4, 5, 1),
    normal_line!(-2, 4, 0),
    normal_line!(-1, 3, 0),
    normal_line!(0, 1, 0),
    normal_line!(1, 3, 0),
    normal_line!(2, 4, 0),
    normal_line!(3, 5, 1),
    normal_line!(5, 6, 2),
    normal_line!(9, 7, 4),
    lower_line!(-25, 7),
    upper_line!(25, 7),
];

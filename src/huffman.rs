//! JBIG2 Huffman table construction and decoding (T.88, Annex B).

use core::mem::size_of;

use crate::reader::{bit_reader, BitReader, ByteReader};
use crate::{
    DecodeError, HuffmanErrorKind as HuffmanTableError, LimitExceeded, LimitKind, Limits,
    MalformedError,
};
#[cfg(verus_keep_ghost)]
use vstd::layout;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
use vstd::std_specs::convert::FromSpecImpl;

#[cfg(verus_keep_ghost)]
#[rustfmt::skip]
#[path = "../specs/huffman.rs"]
pub(crate) mod huffman_spec;

#[cfg(verus_keep_ghost)]
#[rustfmt::skip]
#[path = "../proofs/shared/huffman.rs"]
mod huffman_shared;

#[cfg(verus_keep_ghost)]
#[rustfmt::skip]
#[path = "../proofs/spec_properties/huffman.rs"]
mod huffman_properties;

#[cfg(verus_keep_ghost)]
#[macro_use]
#[rustfmt::skip]
#[path = "../proofs/production_impl/huffman.rs"]
mod huffman_proofs;

#[cfg_attr(verus_keep_ghost, verus_verify)]
const MAX_PREFIX_LEN: usize = 255;
#[cfg_attr(verus_keep_ghost, verus_verify)]
const PREFIX_LOOKUP_BITS: u8 = 8;
#[cfg_attr(verus_keep_ghost, verus_verify)]
const PREFIX_LOOKUP_SIZE: usize = 1 << PREFIX_LOOKUP_BITS;
#[cfg_attr(verus_keep_ghost, verus_verify)]
const MIN_FAST_PREFIX_LEN: usize = 8;

/// Error from Huffman table parsing, construction, or value decoding.
#[derive(Debug)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) enum HuffmanError {
    Decode(DecodeError),
    InvalidTable(HuffmanTableError),
}

impl From<DecodeError> for HuffmanError {
    fn from(error: DecodeError) -> Self {
        Self::Decode(error)
    }
}

impl From<MalformedError> for HuffmanError {
    fn from(error: MalformedError) -> Self {
        Self::Decode(error.into())
    }
}

impl From<LimitExceeded> for HuffmanError {
    fn from(error: LimitExceeded) -> Self {
        Self::Decode(error.into())
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl From<HuffmanError> for DecodeError {
    fn from(error: HuffmanError) -> Self {
        match error {
            HuffmanError::Decode(error) => error,
            HuffmanError::InvalidTable(kind) => {
                Self::Malformed(MalformedError::InvalidHuffman(kind))
            }
        }
    }
}

#[cfg(verus_keep_ghost)]
verus! {

// Project error policy, discharged by the executing From body through the pinned trait contract.
impl FromSpecImpl<HuffmanError> for DecodeError {
    open spec fn obeys_from_spec() -> bool {
        true
    }

    closed spec fn from_spec(error: HuffmanError) -> DecodeError {
        match error {
            HuffmanError::Decode(error) => error,
            HuffmanError::InvalidTable(kind) =>
                DecodeError::Malformed(MalformedError::InvalidHuffman(kind)),
        }
    }
}

}

/// The result of decoding one Huffman-coded value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) enum HuffmanValue {
    Integer(i32),
    OutOfBand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) enum LineKind {
    Normal,
    Lower,
    Upper,
    OutOfBand,
}

/// One line of a JBIG2 Huffman table before canonical codes are assigned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct HuffmanLine {
    pub(crate) prefix_len: u8,
    pub(crate) range_len: u8,
    pub(crate) range_low: i64,
    pub(crate) kind: LineKind,
}

impl HuffmanLine {
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub(crate) const fn normal(range_low: i32, prefix_len: u8, range_len: u8) -> Self {
        Self {
            prefix_len,
            range_len,
            range_low: range_low as i64,
            kind: LineKind::Normal,
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub(crate) const fn lower(range_low: i64, prefix_len: u8) -> Self {
        Self {
            prefix_len,
            range_len: 32,
            range_low,
            kind: LineKind::Lower,
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub(crate) const fn upper(range_low: i32, prefix_len: u8) -> Self {
        Self {
            prefix_len,
            range_len: 32,
            range_low: range_low as i64,
            kind: LineKind::Upper,
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub(crate) const fn out_of_band(prefix_len: u8) -> Self {
        Self {
            prefix_len,
            range_len: 0,
            range_low: 0,
            kind: LineKind::OutOfBand,
        }
    }
}

/// One of the fifteen standard Huffman tables in T.88 Annex B.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) enum StandardHuffmanTable {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
struct WideCode {
    // Little-endian limbs. Annex B permits prefix lengths through 255 bits.
    limbs: [u64; 4],
}

impl WideCode {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out.limbs@ == seq![0u64, 0u64, 0u64, 0u64],
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    const fn zero() -> Self {
        Self { limbs: [0; 4] }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == huffman_spec::checked_add_small(self, value),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    #[allow(clippy::cast_lossless, clippy::if_not_else)]
    fn checked_add_small(self, value: u32) -> Option<Self> {
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
    fn checked_shift_left_one(self) -> Option<Self> {
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
    fn append_bit(self, bit: u32) -> Option<Self> {
        self.checked_shift_left_one()?.checked_add_small(bit)
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == huffman_spec::fits_in_bits(self, bit_count),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    fn fits_in_bits(self, bit_count: usize) -> bool {
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
    fn small_distance_from(self, lower: Self, upper_bound: u32) -> Option<u32> {
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
fn subtract_limb(left: u64, right: u64, borrow: bool) -> (u64, bool) {
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

#[derive(Debug, Clone, Copy)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
enum HuffmanLines<'a> {
    Borrowed(&'a [HuffmanLine]),
    SymbolLengths(&'a [u8]),
    Standard(StandardLines),
}

impl HuffmanLines<'_> {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures huffman_spec::is_raw(self) ==> out == huffman_spec::raw_lines(self).len(),
    ))]
    fn len(self) -> usize {
        match self {
            Self::Borrowed(lines) => lines.len(),
            Self::SymbolLengths(lengths) => lengths.len(),
            Self::Standard(lines) => lines.len(),
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures huffman_spec::is_raw(self) ==> out ==
            if index < huffman_spec::raw_lines(self).len() {
                Some(huffman_spec::raw_lines(self)[index as int])
            } else { None },
    ))]
    fn line(self, index: usize) -> Option<HuffmanLine> {
        match self {
            Self::Borrowed(lines) => {
                if index < lines.len() {
                    Some(lines[index])
                } else {
                    None
                }
            }
            Self::SymbolLengths(lengths) => {
                if index < lengths.len() {
                    i32::try_from(index)
                        .ok()
                        .map(|value| HuffmanLine::normal(value, lengths[index], 0))
                } else {
                    None
                }
            }
            Self::Standard(lines) => lines.line(index),
        }
    }
}

/// A validated table with canonical prefix codes assigned as in T.88 B.3.
#[derive(Debug)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct HuffmanTable {
    lines: Vec<HuffmanLine>,
    ordered_lines: Vec<u32>,
    length_count: [u32; MAX_PREFIX_LEN + 1],
    first_code: [WideCode; MAX_PREFIX_LEN + 1],
    first_line: [u32; MAX_PREFIX_LEN + 1],
    prefix_lookup: Vec<u16>,
    prefer_prefix_lookup: bool,
    max_prefix_len: u8,
}

impl HuffmanTable {
    /// Validates table lines and assigns their canonical prefix codes.
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures match out {
            Ok(table) => huffman_spec::canonical_assignment(lines@, &table),
            Err(_) => true,
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn new(lines: &[HuffmanLine], limits: &Limits) -> Result<Self, HuffmanError> {
        Self::from_line_source(HuffmanLines::Borrowed(lines), limits)
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub(crate) fn new_symbol_ids(lengths: &[u8], limits: &Limits) -> Result<Self, HuffmanError> {
        if let Some(maximum) = lengths.len().checked_sub(1) {
            i32::try_from(maximum).map_err(|_| MalformedError::IntegerOverflow)?;
        }
        Self::from_line_source(HuffmanLines::SymbolLengths(lengths), limits)
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures match out {
            Ok(table) => huffman_spec::is_raw(lines) ==>
                huffman_spec::assignment_contents(huffman_spec::raw_lines(lines), &table)
                && huffman_spec::assignment_codes(huffman_spec::raw_lines(lines), &table)
                && huffman_spec::assignment_starts(huffman_spec::raw_lines(lines), &table)
                && huffman_spec::assignment_order(huffman_spec::raw_lines(lines), &table),
            Err(_) => true,
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    #[allow(clippy::redundant_closure, clippy::too_many_lines)]
    fn from_line_source(lines: HuffmanLines<'_>, limits: &Limits) -> Result<Self, HuffmanError> {
        let line_count = lines.len();
        validate_lines(lines)?;
        check_table_memory(line_count, false, limits)?;

        let mut owned_lines: Vec<HuffmanLine> = Vec::new();
        try_reserve::<HuffmanLine>(&mut owned_lines, line_count)?;
        let mut source_index = 0_usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                source_index <= line_count,
                owned_lines@.len() == source_index,
                huffman_spec::is_raw(lines) ==>
                    owned_lines@ == huffman_spec::raw_lines(lines).take(source_index as int),
            decreases line_count - source_index,
        ))]
        while source_index < line_count {
            let line = lines.line(source_index).ok_or(HuffmanError::InvalidTable(
                HuffmanTableError::InvalidLineOrder,
            ))?;
            owned_lines.push(line);
            source_index += 1;
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            if huffman_spec::is_raw(lines) {
                huffman_spec::full_prefix(huffman_spec::raw_lines(lines));
            }
        }

        let mut length_count = [0_u32; MAX_PREFIX_LEN + 1];
        let mut count_index = 0_usize;
        #[cfg(verus_keep_ghost)]
        proof! {
            if huffman_spec::is_raw(lines) {
                huffman_spec::histogram_empty(huffman_spec::raw_lines(lines), length_count@);
            }
        }
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                count_index <= line_count,
                huffman_spec::is_raw(lines) ==> line_count == huffman_spec::raw_lines(lines).len(),
                huffman_spec::is_raw(lines) ==>
                    huffman_spec::histogram(huffman_spec::raw_lines(lines), length_count@, count_index as nat),
            decreases line_count - count_index,
        ))]
        while count_index < line_count {
            #[cfg(verus_keep_ghost)]
            proof_decl! { let ghost old_counts = length_count@; }
            let line = lines.line(count_index).ok_or(HuffmanError::InvalidTable(
                HuffmanTableError::InvalidLineOrder,
            ))?;
            if line.prefix_len != 0 {
                let count = length_count
                    .get_mut(usize::from(line.prefix_len))
                    .ok_or(HuffmanTableError::Oversubscribed)
                    .map_err(|error| HuffmanError::InvalidTable(error))?;
                *count = count
                    .checked_add(1)
                    .ok_or(HuffmanTableError::Oversubscribed)
                    .map_err(|error| HuffmanError::InvalidTable(error))?;
            }
            #[cfg(verus_keep_ghost)]
            proof! {
                if huffman_spec::is_raw(lines) {
                    huffman_spec::histogram_step(
                        huffman_spec::raw_lines(lines), old_counts, length_count@, count_index as nat,
                    );
                }
            }
            count_index += 1;
        }

        let mut max_length = MAX_PREFIX_LEN;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                max_length <= MAX_PREFIX_LEN,
                forall|length: int| max_length < length < 256 ==> length_count@[length] == 0,
            decreases max_length,
        ))]
        while max_length > 0 && length_count[max_length] == 0 {
            max_length -= 1;
        }
        if max_length == 0 || length_count[max_length] == 0 {
            return Err(HuffmanError::InvalidTable(HuffmanTableError::NoCodes));
        }
        let mut min_length = 1_usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                1 <= min_length,
                min_length <= max_length,
                max_length <= MAX_PREFIX_LEN,
            decreases max_length - min_length,
        ))]
        while min_length < max_length && length_count[min_length] == 0 {
            min_length += 1;
        }
        let max_prefix_len = u8::try_from(max_length)
            .map_err(|_| HuffmanError::InvalidTable(HuffmanTableError::Oversubscribed))?;

        let mut first_code = [WideCode::zero(); MAX_PREFIX_LEN + 1];
        let mut first_line = [0_u32; MAX_PREFIX_LEN + 1];
        let mut ordered_lines = Vec::new();
        try_reserve::<u32>(&mut ordered_lines, line_count)?;

        let mut length = 1_usize;
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(huffman_spec::is_raw(lines) ==>
                huffman_spec::assignment_prefix(
                    huffman_spec::raw_lines(lines), first_code@, first_line@, ordered_lines@, 1,
                ));
        }
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                1 <= length,
                length <= max_prefix_len as usize + 1,
                huffman_spec::is_raw(lines) ==> line_count == huffman_spec::raw_lines(lines).len(),
                huffman_spec::is_raw(lines) ==>
                    huffman_spec::histogram(huffman_spec::raw_lines(lines), length_count@, line_count as nat),
                huffman_spec::is_raw(lines) ==>
                    huffman_spec::assignment_prefix(
                        huffman_spec::raw_lines(lines), first_code@, first_line@, ordered_lines@, length as nat,
                    ),
            decreases max_prefix_len as usize + 1 - length,
        ))]
        while length <= usize::from(max_prefix_len) {
            #[cfg(verus_keep_ghost)]
            proof_decl! {
                let ghost old_codes = first_code@;
                let ghost old_starts = first_line@;
                let ghost old_order = ordered_lines@;
            }
            let previous = canonical_first_code(first_code[length - 1], length_count[length - 1])
                .ok_or(HuffmanTableError::Oversubscribed)
                .map_err(|error| HuffmanError::InvalidTable(error))?;
            first_code[length] = previous;

            let count = length_count[length];
            if count != 0 {
                if !canonical_range_fits(previous, count, length) {
                    return Err(HuffmanError::InvalidTable(
                        HuffmanTableError::Oversubscribed,
                    ));
                }
                first_line[length] = u32::try_from(ordered_lines.len())
                    .map_err(|_| HuffmanError::InvalidTable(HuffmanTableError::Oversubscribed))?;
                let mut index = 0_usize;
                #[cfg_attr(verus_keep_ghost, verus_spec(
                    invariant
                        index <= line_count,
                        1 <= length <= max_prefix_len,
                        huffman_spec::is_raw(lines) ==> line_count == huffman_spec::raw_lines(lines).len(),
                        huffman_spec::is_raw(lines) ==> ordered_lines@ ==
                            old_order + huffman_spec::bucket(
                                huffman_spec::raw_lines(lines), length as nat, index as nat,
                            ),
                    decreases line_count - index,
                ))]
                while index < line_count {
                    #[cfg(verus_keep_ghost)]
                    proof_decl! { let ghost before_order = ordered_lines@; }
                    let line = lines.line(index).ok_or(HuffmanError::InvalidTable(
                        HuffmanTableError::InvalidLineOrder,
                    ))?;
                    if usize::from(line.prefix_len) == length {
                        let index = u32::try_from(index).map_err(|_| {
                            HuffmanError::InvalidTable(HuffmanTableError::Oversubscribed)
                        })?;
                        ordered_lines.push(index);
                    }
                    #[cfg(verus_keep_ghost)]
                    proof! {
                        if huffman_spec::is_raw(lines) {
                            huffman_spec::bucket_step(
                                huffman_spec::raw_lines(lines), length as nat, index as nat,
                                old_order, before_order, ordered_lines@,
                            );
                        }
                    }
                    index += 1;
                }
            }
            #[cfg(verus_keep_ghost)]
            proof! {
                if huffman_spec::is_raw(lines) {
                    if count == 0 {
                        huffman_spec::empty_bucket(
                            huffman_spec::raw_lines(lines), length as nat, line_count as nat, old_order,
                        );
                    }
                    huffman_spec::assignment_step(
                        huffman_spec::raw_lines(lines), length_count@,
                        old_codes, old_starts, old_order,
                        first_code@, first_line@, ordered_lines@, length as nat,
                    );
                }
            }
            length += 1;
        }
        // Short canonical codes already beat lookahead setup. Restrict the
        // fast path to tables whose shortest code consumes the full prefix;
        // longer codes still use the exact canonical fallback.
        let prefer_prefix_lookup = min_length >= MIN_FAST_PREFIX_LEN;
        let prefix_lookup = if prefer_prefix_lookup {
            check_table_memory(line_count, true, limits)?;
            build_prefix_lookup(
                &length_count,
                &first_code,
                &first_line,
                &ordered_lines,
                usize::from(max_prefix_len),
            )?
        } else {
            Vec::new()
        };

        #[cfg(verus_keep_ghost)]
        proof! {
            huffman_spec::canonical_assignment_definition();
            if huffman_spec::is_raw(lines) {
                assert(owned_lines@ == huffman_spec::raw_lines(lines));
                assert(huffman_spec::histogram(huffman_spec::raw_lines(lines), length_count@, line_count as nat));
                assert(length_count@[max_prefix_len as int] > 0);
                assert(forall|i: int| max_prefix_len < i < 256 ==> length_count@[i] == 0);
                assert(huffman_spec::assignment_prefix(
                    huffman_spec::raw_lines(lines), first_code@, first_line@, ordered_lines@,
                    max_prefix_len as nat + 1,
                ));
                let output = Self {
                    lines: owned_lines,
                    ordered_lines,
                    length_count,
                    first_code,
                    first_line,
                    prefix_lookup,
                    prefer_prefix_lookup,
                    max_prefix_len,
                };
                huffman_spec::assignment_complete(huffman_spec::raw_lines(lines), &output);
            }
        }
        Ok(Self {
            lines: owned_lines,
            ordered_lines,
            length_count,
            first_code,
            first_line,
            prefix_lookup,
            prefer_prefix_lookup,
            max_prefix_len,
        })
    }

    /// Decodes the coded table structure described by T.88 B.2.
    ///
    /// `bytes` is exactly one code table segment payload, including at most
    /// seven zero padding bits after the final table line.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    #[allow(clippy::cast_possible_wrap, clippy::too_many_lines)]
    pub(crate) fn from_encoded(bytes: &[u8], limits: &Limits) -> Result<Self, HuffmanError> {
        let byte_count = u64::try_from(bytes.len()).map_err(|_| MalformedError::IntegerOverflow)?;
        if byte_count > limits.max_stream_bytes {
            return Err(LimitExceeded {
                kind: LimitKind::StreamBytes,
                limit: limits.max_stream_bytes,
                actual: byte_count,
            }
            .into());
        }

        let mut header = ByteReader::new(bytes);
        let flags = header.read_u8()?;
        if flags & 0x80 != 0 {
            return Err(HuffmanError::InvalidTable(
                HuffmanTableError::ReservedFlags(flags),
            ));
        }
        let low = header.read_u32()? as i32;
        let high = header.read_u32()? as i32;
        if low >= high {
            return Err(HuffmanError::InvalidTable(
                HuffmanTableError::InvalidBounds { low, high },
            ));
        }

        let (has_oob, prefix_bits, range_bits) = encoded_table_fields(flags);
        if prefix_bits > 8 || range_bits > 8 {
            return Err(HuffmanError::InvalidTable(
                HuffmanTableError::InvalidRangePartition,
            ));
        }
        let encoded_bytes = header.remaining_bytes();
        if encoded_bytes.len() > usize::MAX / 8 {
            return Err(MalformedError::IntegerOverflow.into());
        }
        let mut encoded_lines = bit_reader(encoded_bytes);
        let mut lines = Vec::new();
        let mut current = i64::from(low);
        let high_i64 = i64::from(high);

        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                low as int <= current,
                current <= high_i64,
                high_i64 == high as int,
                high_i64 <= i32::MAX,
                i32::MIN <= current,
                current <= i32::MAX,
                prefix_bits <= 8,
                range_bits <= 8,
                huffman_spec::reader_valid(&encoded_lines),
            decreases high_i64 - current,
        ))]
        while current < high_i64 {
            let prefix_len = read_u8_bits(&mut encoded_lines, prefix_bits)?;
            let range_len = read_u8_bits(&mut encoded_lines, range_bits)?;
            if range_len > 32 {
                return Err(HuffmanError::InvalidTable(
                    HuffmanTableError::InvalidRangeLength(range_len),
                ));
            }
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(range_len <= 32);
            }
            let range_low = i32::try_from(current).map_err(|_| {
                HuffmanError::InvalidTable(HuffmanTableError::InvalidRangePartition)
            })?;
            push_parsed_line(
                &mut lines,
                HuffmanLine::normal(range_low, prefix_len, range_len),
                limits,
            )?;
            let width = 1_i64 << range_len;
            let remaining = high_i64
                .checked_sub(current)
                .ok_or(MalformedError::IntegerOverflow)?;
            if width > remaining {
                return Err(HuffmanError::InvalidTable(
                    HuffmanTableError::InvalidRangePartition,
                ));
            }
            let next = current
                .checked_add(width)
                .ok_or(MalformedError::IntegerOverflow)?;
            if next <= current || next > high_i64 {
                return Err(HuffmanError::InvalidTable(
                    HuffmanTableError::InvalidRangePartition,
                ));
            }
            current = next;
        }
        if current != high_i64 {
            return Err(HuffmanError::InvalidTable(
                HuffmanTableError::InvalidRangePartition,
            ));
        }

        let low_prefix = read_u8_bits(&mut encoded_lines, prefix_bits)?;
        push_parsed_line(
            &mut lines,
            HuffmanLine::lower(i64::from(low) - 1, low_prefix),
            limits,
        )?;
        let high_prefix = read_u8_bits(&mut encoded_lines, prefix_bits)?;
        push_parsed_line(&mut lines, HuffmanLine::upper(high, high_prefix), limits)?;
        if has_oob {
            let oob_prefix = read_u8_bits(&mut encoded_lines, prefix_bits)?;
            push_parsed_line(&mut lines, HuffmanLine::out_of_band(oob_prefix), limits)?;
        }

        let remaining_bits = padding_bit_count(header.remaining(), encoded_lines.bit_position)
            .ok_or(MalformedError::IntegerOverflow)?;
        if remaining_bits > 7 {
            return Err(HuffmanError::InvalidTable(HuffmanTableError::TrailingBits(
                remaining_bits as u64,
            )));
        }
        if remaining_bits != 0 {
            let padding =
                encoded_lines.read_bits(u8::try_from(remaining_bits).map_err(|_| {
                    HuffmanError::InvalidTable(HuffmanTableError::TrailingBits(
                        remaining_bits as u64,
                    ))
                })?)?;
            if padding != 0 {
                return Err(HuffmanError::InvalidTable(
                    HuffmanTableError::NonZeroPadding,
                ));
            }
        }

        Self::new(&lines, limits)
    }

    /// Constructs one of the standard tables in T.88 B.5.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub(crate) fn standard(
        table: StandardHuffmanTable,
        limits: &Limits,
    ) -> Result<Self, HuffmanError> {
        Self::from_line_source(HuffmanLines::Standard(standard_lines(table)), limits)
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == huffman_spec::table_has_oob(self),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn has_oob(&self) -> bool {
        let mut index = 0;
        let mut found = false;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                index <= self.lines@.len(),
                found == exists|line_index: int|
                    0 <= line_index < index
                    && self.lines@[line_index].kind == LineKind::OutOfBand,
            decreases self.lines@.len() - index,
        ))]
        while index < self.lines.len() {
            match self.lines[index].kind {
                LineKind::OutOfBand => found = true,
                LineKind::Normal | LineKind::Lower | LineKind::Upper => {}
            }
            index += 1;
        }
        #[cfg(verus_keep_ghost)]
        production_huffman_has_oob!(self);
        found
    }

    /// Decodes one integer or OOB value according to T.88 B.4.
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires huffman_spec::reader_valid(old(reader)),
        ensures huffman_spec::reader_valid(final(reader)),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn decode(&self, reader: &mut BitReader<'_>) -> Result<HuffmanValue, HuffmanError> {
        if self.prefer_prefix_lookup {
            if let Some(prefix) = peek_prefix(reader) {
                let entry = match self.prefix_lookup.get(usize::from(prefix)) {
                    Some(entry) => *entry,
                    None => 0,
                };
                if entry != 0 {
                    let line_index = u32::from(entry)
                        .checked_sub(1)
                        .ok_or(HuffmanError::InvalidTable(HuffmanTableError::InvalidCode))?;
                    let line = self
                        .lines
                        .get(line_index as usize)
                        .ok_or(HuffmanError::InvalidTable(HuffmanTableError::InvalidCode))?;
                    let length = line.prefix_len;
                    if length > PREFIX_LOOKUP_BITS {
                        return Err(HuffmanError::InvalidTable(HuffmanTableError::InvalidCode));
                    }
                    reader.bit_position = reader
                        .bit_position
                        .checked_add(usize::from(length))
                        .ok_or(MalformedError::IntegerOverflow)?;
                    return decode_line(*line, reader);
                }
            }
        }
        self.decode_fallback(reader)
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires huffman_spec::reader_valid(old(reader)),
        ensures huffman_spec::reader_valid(final(reader)),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    fn decode_fallback(&self, reader: &mut BitReader<'_>) -> Result<HuffmanValue, HuffmanError> {
        let mut code = WideCode { limbs: [0; 4] };
        let mut length = 1_usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                huffman_spec::reader_valid(reader),
                1 <= length,
                length <= self.max_prefix_len as usize + 1,
            decreases self.max_prefix_len as usize + 1 - length,
        ))]
        while length <= self.max_prefix_len as usize {
            #[cfg(verus_keep_ghost)]
            let start = reader.bit_position;
            let bit = match reader.read_bits(1) {
                Ok(bit) => bit,
                Err(error) => {
                    return Err(HuffmanError::Decode(DecodeError::Malformed(error)));
                }
            };
            #[cfg(verus_keep_ghost)]
            production_huffman_one_bit!(reader, start, bit);
            code = code
                .append_bit(bit)
                .ok_or(MalformedError::IntegerOverflow)?;
            let count = self.length_count[length];
            if let Some(distance) = code.small_distance_from(self.first_code[length], count) {
                let ordered_index = self.first_line[length]
                    .checked_add(distance)
                    .ok_or(MalformedError::IntegerOverflow)?
                    as usize;
                let line_index = *self
                    .ordered_lines
                    .get(ordered_index)
                    .ok_or(HuffmanError::InvalidTable(HuffmanTableError::InvalidCode))?;
                let line = self
                    .lines
                    .get(line_index as usize)
                    .ok_or(HuffmanError::InvalidTable(HuffmanTableError::InvalidCode))?;
                return decode_line(*line, reader);
            }
            length = next_prefix_length(length, usize::from(self.max_prefix_len));
        }
        Err(HuffmanError::InvalidTable(HuffmanTableError::InvalidCode))
    }

    #[cfg(test)]
    fn decode_canonical(&self, reader: &mut BitReader<'_>) -> Result<HuffmanValue, HuffmanError> {
        let mut code = WideCode { limbs: [0; 4] };
        for length in 1..=usize::from(self.max_prefix_len) {
            let bit = reader.read_bits(1)?;
            code = code
                .append_bit(bit)
                .ok_or(MalformedError::IntegerOverflow)?;
            let count = self.length_count[length];
            if let Some(distance) = code.small_distance_from(self.first_code[length], count) {
                let ordered_index = self.first_line[length]
                    .checked_add(distance)
                    .ok_or(MalformedError::IntegerOverflow)?
                    as usize;
                let line_index = *self
                    .ordered_lines
                    .get(ordered_index)
                    .ok_or(HuffmanError::InvalidTable(HuffmanTableError::InvalidCode))?;
                let line = self
                    .lines
                    .get(line_index as usize)
                    .ok_or(HuffmanError::InvalidTable(HuffmanTableError::InvalidCode))?;
                return decode_line(*line, reader);
            }
        }
        Err(HuffmanError::InvalidTable(HuffmanTableError::InvalidCode))
    }

    #[cfg(test)]
    fn code_for_line(&self, line_index: usize) -> Option<(WideCode, u8)> {
        let line = self.lines.get(line_index)?;
        let length = usize::from(line.prefix_len);
        if length == 0 {
            return None;
        }

        let first = usize::try_from(self.first_line[length]).ok()?;
        let count = usize::try_from(self.length_count[length]).ok()?;
        let within_length = self
            .ordered_lines
            .get(first..first.checked_add(count)?)?
            .iter()
            .position(|&candidate| usize::try_from(candidate).ok() == Some(line_index))?;
        let code = self.first_code[length].checked_add_small(u32::try_from(within_length).ok()?)?;
        Some((code, line.prefix_len))
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[allow(clippy::redundant_closure)]
fn build_prefix_lookup(
    length_count: &[u32; MAX_PREFIX_LEN + 1],
    first_code: &[WideCode; MAX_PREFIX_LEN + 1],
    first_line: &[u32; MAX_PREFIX_LEN + 1],
    ordered_lines: &[u32],
    maximum_length: usize,
) -> Result<Vec<u16>, HuffmanError> {
    let mut lookup = Vec::new();
    try_reserve::<u16>(&mut lookup, PREFIX_LOOKUP_SIZE)?;
    lookup.resize(PREFIX_LOOKUP_SIZE, 0);
    let lookup_length = maximum_length.min(usize::from(PREFIX_LOOKUP_BITS));
    let mut length = 1_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            lookup@.len() == PREFIX_LOOKUP_SIZE,
            lookup_length <= PREFIX_LOOKUP_BITS as usize,
            1 <= length,
            length <= lookup_length + 1,
        decreases lookup_length + 1 - length,
    ))]
    while length <= lookup_length {
        let count = length_count[length];
        let mut distance = 0_u32;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                lookup@.len() == PREFIX_LOOKUP_SIZE,
                lookup_length <= PREFIX_LOOKUP_BITS as usize,
                1 <= length,
                length <= lookup_length,
                distance <= count,
            decreases count - distance,
        ))]
        while distance < count {
            let code = first_code[length]
                .checked_add_small(distance)
                .ok_or(HuffmanTableError::Oversubscribed)
                .map_err(|error| HuffmanError::InvalidTable(error))?;
            if code.limbs[1] != 0
                || code.limbs[2] != 0
                || code.limbs[3] != 0
                || code.limbs[0] > u64::from(u8::MAX)
                || distance > u32::from(u8::MAX)
            {
                return Err(HuffmanError::InvalidTable(
                    HuffmanTableError::Oversubscribed,
                ));
            }
            let suffix_bits = usize::from(PREFIX_LOOKUP_BITS) - length;
            let repeats = 1_usize << suffix_bits;
            let code = usize::try_from(code.limbs[0])
                .map_err(|_| HuffmanError::InvalidTable(HuffmanTableError::Oversubscribed))?;
            let first_prefix = code << suffix_bits;
            let ordered_index = first_line[length]
                .checked_add(distance)
                .ok_or(MalformedError::IntegerOverflow)?;
            let line_index = *ordered_lines
                .get(ordered_index as usize)
                .ok_or(HuffmanError::InvalidTable(HuffmanTableError::InvalidCode))?;
            let Some(encoded_line) = line_index
                .checked_add(1)
                .and_then(|encoded| u16::try_from(encoded).ok())
            else {
                distance += 1;
                continue;
            };
            let mut suffix = 0_usize;
            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant
                    lookup@.len() == PREFIX_LOOKUP_SIZE,
                    lookup_length <= PREFIX_LOOKUP_BITS as usize,
                    1 <= length,
                    length <= lookup_length,
                    suffix <= repeats,
                decreases repeats - suffix,
            ))]
            while suffix < repeats {
                let index = first_prefix
                    .checked_add(suffix)
                    .ok_or(MalformedError::IntegerOverflow)?;
                let slot = lookup.get_mut(index).ok_or(HuffmanError::InvalidTable(
                    HuffmanTableError::Oversubscribed,
                ))?;
                if *slot != 0 {
                    return Err(HuffmanError::InvalidTable(
                        HuffmanTableError::Oversubscribed,
                    ));
                }
                *slot = encoded_line;
                suffix += 1;
            }
            distance += 1;
        }
        length += 1;
    }
    Ok(lookup)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires huffman_spec::reader_valid(reader),
    ensures match out {
        Some(_) =>
            reader.bit_position + PREFIX_LOOKUP_BITS as usize
                <= reader.bytes@.len() * 8,
        None => true,
    },
))]
#[allow(clippy::cast_possible_truncation)]
fn peek_prefix(reader: &BitReader<'_>) -> Option<u8> {
    let total_bits = reader.bytes.len().checked_mul(8)?;
    let remaining = total_bits.checked_sub(reader.bit_position)?;
    if remaining < usize::from(PREFIX_LOOKUP_BITS) {
        return None;
    }
    let byte_index = reader.bit_position / 8;
    let bit_offset = reader.bit_position % 8;
    let first = u16::from(*reader.bytes.get(byte_index)?);
    if bit_offset == 0 {
        return Some(first as u8);
    }
    let second_index = byte_index.checked_add(1)?;
    let second = u16::from(*reader.bytes.get(second_index)?);
    let combined = (first << 8) | second;
    let shift = 8_usize.checked_sub(bit_offset)?;
    Some(((combined >> shift) & 0xff) as u8)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires length <= _maximum, _maximum < usize::MAX,
    ensures
        out == length + 1,
        _maximum + 1 - out < _maximum + 1 - length,
))]
fn next_prefix_length(length: usize, _maximum: usize) -> usize {
    length + 1
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out == huffman_spec::canonical_first_code(previous, count),
))]
fn canonical_first_code(previous: WideCode, count: u32) -> Option<WideCode> {
    match previous.checked_add_small(count) {
        Some(sum) => sum.checked_shift_left_one(),
        None => None,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires count > 0,
    ensures out == huffman_spec::canonical_range_fits(first, count, length),
))]
fn canonical_range_fits(first: WideCode, count: u32, length: usize) -> bool {
    match first.checked_add_small(count - 1) {
        Some(last) => last.fits_in_bits(length),
        None => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
enum StandardLines {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
}

impl StandardLines {
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    const fn len(self) -> usize {
        match self {
            Self::A => 4,
            Self::B => 7,
            Self::C => 9,
            Self::D => 6,
            Self::E => 8,
            Self::F => 14,
            Self::G => 15,
            Self::H | Self::J => 21,
            Self::I => 22,
            Self::K | Self::L | Self::M | Self::O => 13,
            Self::N => 5,
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn line(self, index: usize) -> Option<HuffmanLine> {
        macro_rules! get {
            ($table:expr) => {
                if index < $table.len() {
                    Some($table[index])
                } else {
                    None
                }
            };
        }
        match self {
            Self::A => get!(TABLE_A),
            Self::B => get!(TABLE_B),
            Self::C => get!(TABLE_C),
            Self::D => get!(TABLE_D),
            Self::E => get!(TABLE_E),
            Self::F => get!(TABLE_F),
            Self::G => get!(TABLE_G),
            Self::H => get!(TABLE_H),
            Self::I => get!(TABLE_I),
            Self::J => get!(TABLE_J),
            Self::K => get!(TABLE_K),
            Self::L => get!(TABLE_L),
            Self::M => get!(TABLE_M),
            Self::N => get!(TABLE_N),
            Self::O => get!(TABLE_O),
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn standard_lines(table: StandardHuffmanTable) -> StandardLines {
    match table {
        StandardHuffmanTable::A => StandardLines::A,
        StandardHuffmanTable::B => StandardLines::B,
        StandardHuffmanTable::C => StandardLines::C,
        StandardHuffmanTable::D => StandardLines::D,
        StandardHuffmanTable::E => StandardLines::E,
        StandardHuffmanTable::F => StandardLines::F,
        StandardHuffmanTable::G => StandardLines::G,
        StandardHuffmanTable::H => StandardLines::H,
        StandardHuffmanTable::I => StandardLines::I,
        StandardHuffmanTable::J => StandardLines::J,
        StandardHuffmanTable::K => StandardLines::K,
        StandardHuffmanTable::L => StandardLines::L,
        StandardHuffmanTable::M => StandardLines::M,
        StandardHuffmanTable::N => StandardLines::N,
        StandardHuffmanTable::O => StandardLines::O,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires huffman_spec::reader_valid(old(reader)),
    ensures
        final(reader).bytes@ == old(reader).bytes@,
        huffman_spec::reader_valid(final(reader)),
        match out {
            Ok(HuffmanValue::OutOfBand) =>
                line.kind == LineKind::OutOfBand
                && final(reader).bit_position == old(reader).bit_position,
            Ok(HuffmanValue::Integer(value)) =>
                line.kind != LineKind::OutOfBand
                && final(reader).bit_position
                    == old(reader).bit_position + line.range_len as usize
                && huffman_spec::decoded_line_value(
                    line,
                    crate::spec::readers::bits_value(
                        old(reader).bytes@,
                        old(reader).bit_position as nat,
                        line.range_len as nat,
                    ),
                    value,
                ),
            Err(_) =>
                final(reader).bit_position == old(reader).bit_position
                || (
                    line.kind != LineKind::OutOfBand
                    && line.range_len <= 32
                    && final(reader).bit_position
                        == old(reader).bit_position + line.range_len as usize
                ),
        },
))]
#[allow(
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::manual_let_else
)]
fn decode_line(
    line: HuffmanLine,
    reader: &mut BitReader<'_>,
) -> Result<HuffmanValue, HuffmanError> {
    match line.kind {
        LineKind::OutOfBand => return Ok(HuffmanValue::OutOfBand),
        LineKind::Normal | LineKind::Lower | LineKind::Upper => {}
    }
    let offset = reader.read_bits(line.range_len)? as i64;
    let candidate = match line.kind {
        LineKind::Lower => line.range_low.checked_sub(offset),
        LineKind::Normal | LineKind::Upper => line.range_low.checked_add(offset),
        LineKind::OutOfBand => None,
    };
    let value = match candidate {
        Some(value) => value,
        None => {
            return Err(HuffmanError::Decode(DecodeError::Malformed(
                MalformedError::IntegerOverflow,
            )));
        }
    };
    if value < i32::MIN as i64 || value > i32::MAX as i64 {
        return Err(HuffmanError::Decode(DecodeError::Malformed(
            MalformedError::IntegerOverflow,
        )));
    }
    Ok(HuffmanValue::Integer(value as i32))
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        huffman_spec::reader_valid(old(reader)),
        count <= 8,
    ensures
        final(reader).bytes@ == old(reader).bytes@,
        huffman_spec::reader_valid(final(reader)),
        match out {
            Ok(value) =>
                final(reader).bit_position == old(reader).bit_position + count as usize
                && value as nat == crate::spec::readers::bits_value(
                    old(reader).bytes@,
                    old(reader).bit_position as nat,
                    count as nat,
                ),
            Err(_) => final(reader).bit_position == old(reader).bit_position,
        },
))]
#[allow(clippy::cast_lossless, clippy::cast_possible_truncation)]
fn read_u8_bits(reader: &mut BitReader<'_>, count: u8) -> Result<u8, HuffmanError> {
    #[cfg(verus_keep_ghost)]
    let start = reader.bit_position;
    let value = match reader.read_bits(count) {
        Ok(value) => value,
        Err(error) => {
            return Err(HuffmanError::Decode(DecodeError::Malformed(error)));
        }
    };
    #[cfg(verus_keep_ghost)]
    production_huffman_u8_bits!(reader, start, count, value);
    if value > u8::MAX as u32 {
        Err(HuffmanError::InvalidTable(
            HuffmanTableError::InvalidRangePartition,
        ))
    } else {
        Ok(value as u8)
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Some(remaining) =>
            payload_bytes <= usize::MAX / 8
            && consumed_bits <= payload_bytes * 8
            && remaining == payload_bytes * 8 - consumed_bits,
        None =>
            payload_bytes > usize::MAX / 8
            || consumed_bits > payload_bytes * 8,
    },
))]
fn padding_bit_count(payload_bytes: usize, consumed_bits: usize) -> Option<usize> {
    match payload_bytes.checked_mul(8) {
        Some(bits) => bits.checked_sub(consumed_bits),
        None => None,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures
        out.0 == (flags & 1 != 0),
        out.1 == ((((flags >> 1) & 7) as u16 + 1) as u8),
        out.2 == ((((flags >> 4) & 7) as u16 + 1) as u8),
))]
#[allow(clippy::cast_lossless, clippy::cast_possible_truncation)]
fn encoded_table_fields(flags: u8) -> (bool, u8, u8) {
    let prefix_bits = (((flags >> 1) & 7) as u16 + 1) as u8;
    let range_bits = (((flags >> 4) & 7) as u16 + 1) as u8;
    (flags & 1 != 0, prefix_bits, range_bits)
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn push_parsed_line(
    lines: &mut Vec<HuffmanLine>,
    line: HuffmanLine,
    limits: &Limits,
) -> Result<(), HuffmanError> {
    let new_len = lines
        .len()
        .checked_add(1)
        .ok_or(MalformedError::IntegerOverflow)?;
    check_table_memory(new_len, false, limits)?;
    let requested_bytes = allocation_size::<HuffmanLine>(new_len)?;
    if lines.try_reserve(1).is_err() {
        return Err(DecodeError::Allocation { requested_bytes }.into());
    }
    lines.push(line);
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures final(values)@ == old(values)@,
))]
fn try_reserve<T>(values: &mut Vec<T>, count: usize) -> Result<(), HuffmanError> {
    let requested_bytes = allocation_size::<T>(count)?;
    if values.try_reserve(count).is_err() {
        return Err(DecodeError::Allocation { requested_bytes }.into());
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(bytes) =>
            bytes as nat == count as nat * layout::size_of::<T>(),
        Err(_) =>
            count as nat * layout::size_of::<T>() > usize::MAX
            || count as nat * layout::size_of::<T>() > u64::MAX,
    },
))]
fn allocation_size<T>(count: usize) -> Result<u64, MalformedError> {
    let Some(bytes) = count.checked_mul(size_of::<T>()) else {
        return Err(MalformedError::IntegerOverflow);
    };
    let Ok(bytes) = u64::try_from(bytes) else {
        return Err(MalformedError::IntegerOverflow);
    };
    Ok(bytes)
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn table_memory(line_count: usize, prefix_lookup: bool) -> Result<u64, MalformedError> {
    let Ok(fixed) = u64::try_from(size_of::<HuffmanTable>()) else {
        return Err(MalformedError::IntegerOverflow);
    };
    let per_line = size_of::<HuffmanLine>()
        .checked_add(size_of::<u32>())
        .ok_or(MalformedError::IntegerOverflow)?;
    let variable_count = line_count
        .checked_mul(per_line)
        .ok_or(MalformedError::IntegerOverflow)?;
    let fixed = if prefix_lookup {
        fixed
            .checked_add(allocation_size::<u16>(PREFIX_LOOKUP_SIZE)?)
            .ok_or(MalformedError::IntegerOverflow)?
    } else {
        fixed
    };
    fixed
        .checked_add(allocation_size::<u8>(variable_count)?)
        .ok_or(MalformedError::IntegerOverflow)
}

#[allow(clippy::cast_lossless)]
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out is Ok ==> line_count <= u32::MAX,
))]
fn check_table_memory(
    line_count: usize,
    prefix_lookup: bool,
    limits: &Limits,
) -> Result<(), HuffmanError> {
    let line_count_u64 = line_count as u64;
    if line_count_exceeds_symbol_limit(line_count, limits.max_symbols) {
        return Err(LimitExceeded {
            kind: LimitKind::Symbols,
            limit: limits.max_symbols as u64,
            actual: line_count_u64,
        }
        .into());
    }
    let actual = table_memory(line_count, prefix_lookup)?;
    if actual > limits.max_decoder_managed_memory_bytes {
        return Err(LimitExceeded {
            kind: LimitKind::Memory,
            limit: limits.max_decoder_managed_memory_bytes,
            actual,
        }
        .into());
    }
    if !line_count_fits_u32(line_count) {
        return Err(HuffmanError::InvalidTable(
            HuffmanTableError::Oversubscribed,
        ));
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out == (line_count as nat > maximum as nat),
))]
#[allow(clippy::cast_lossless)]
fn line_count_exceeds_symbol_limit(line_count: usize, maximum: u32) -> bool {
    line_count as u64 > maximum as u64
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out == (line_count as nat <= u32::MAX as nat),
))]
#[allow(clippy::cast_lossless, clippy::checked_conversions)]
fn line_count_fits_u32(line_count: usize) -> bool {
    line_count as u64 <= u32::MAX as u64
}

#[allow(clippy::too_many_lines)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[allow(clippy::cast_lossless, clippy::manual_let_else)]
fn validate_lines(lines: HuffmanLines<'_>) -> Result<(), HuffmanError> {
    let line_count = lines.len();
    if line_count == 0 {
        return Err(HuffmanError::InvalidTable(HuffmanTableError::NoCodes));
    }

    let mut previous_normal_end: Option<i64> = None;
    let mut first_normal_start: Option<i64> = None;
    let mut lower: Option<i64> = None;
    let mut upper: Option<i64> = None;
    let mut oob_seen = false;
    let mut special_seen = false;
    let mut active_lower_seen = false;

    let mut index = 0;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index <= line_count,
        decreases line_count - index,
    ))]
    while index < line_count {
        let line = lines.line(index).ok_or(HuffmanError::InvalidTable(
            HuffmanTableError::InvalidLineOrder,
        ))?;
        if line.range_len > 32 {
            return Err(HuffmanError::InvalidTable(
                HuffmanTableError::InvalidRangeLength(line.range_len),
            ));
        }
        match line.kind {
            LineKind::Normal => {
                if special_seen {
                    return Err(HuffmanError::InvalidTable(
                        HuffmanTableError::InvalidLineOrder,
                    ));
                }
                let width_minus_one = if line.range_len == 32 {
                    u32::MAX as i64
                } else {
                    let width = 1_i64 << line.range_len;
                    match width.checked_sub(1) {
                        Some(value) => value,
                        None => {
                            return Err(HuffmanError::Decode(DecodeError::Malformed(
                                MalformedError::IntegerOverflow,
                            )));
                        }
                    }
                };
                let end = match line.range_low.checked_add(width_minus_one) {
                    Some(end) => end,
                    None => {
                        return Err(HuffmanError::Decode(DecodeError::Malformed(
                            MalformedError::IntegerOverflow,
                        )));
                    }
                };
                if line.range_low < i32::MIN as i64
                    || line.range_low > i32::MAX as i64
                    || end < i32::MIN as i64
                    || end > i32::MAX as i64
                {
                    return Err(HuffmanError::InvalidTable(
                        HuffmanTableError::InvalidRangePartition,
                    ));
                }
                if let Some(previous) = previous_normal_end {
                    if previous.checked_add(1) != Some(line.range_low) {
                        return Err(HuffmanError::InvalidTable(
                            HuffmanTableError::InvalidRangePartition,
                        ));
                    }
                } else {
                    first_normal_start = Some(line.range_low);
                }
                previous_normal_end = Some(end);
            }
            LineKind::Lower => {
                special_seen = true;
                if lower.is_some() {
                    return Err(HuffmanError::InvalidTable(
                        HuffmanTableError::DuplicateSpecialLine,
                    ));
                }
                lower = Some(line.range_low);
                if upper.is_some() || oob_seen || line.range_len != 32 {
                    return Err(HuffmanError::InvalidTable(
                        HuffmanTableError::InvalidLineOrder,
                    ));
                }
                active_lower_seen = line.prefix_len != 0;
            }
            LineKind::Upper => {
                special_seen = true;
                if upper.is_some() {
                    return Err(HuffmanError::InvalidTable(
                        HuffmanTableError::DuplicateSpecialLine,
                    ));
                }
                upper = Some(line.range_low);
                if oob_seen || line.range_len != 32 {
                    return Err(HuffmanError::InvalidTable(
                        HuffmanTableError::InvalidLineOrder,
                    ));
                }
            }
            LineKind::OutOfBand => {
                special_seen = true;
                if oob_seen {
                    return Err(HuffmanError::InvalidTable(
                        HuffmanTableError::DuplicateSpecialLine,
                    ));
                }
                if line.range_len != 0 {
                    return Err(HuffmanError::InvalidTable(
                        HuffmanTableError::InvalidSpecialLine,
                    ));
                }
                oob_seen = true;
            }
        }
        index += 1;
    }

    let first = match first_normal_start {
        Some(first) => first,
        None => {
            return Err(HuffmanError::InvalidTable(
                HuffmanTableError::InvalidRangePartition,
            ));
        }
    };
    let last = match previous_normal_end {
        Some(last) => last,
        None => {
            return Err(HuffmanError::InvalidTable(
                HuffmanTableError::InvalidRangePartition,
            ));
        }
    };
    if let Some(base) = lower {
        if first.checked_sub(1) != Some(base) || (base < i32::MIN as i64 && active_lower_seen) {
            return Err(HuffmanError::InvalidTable(
                HuffmanTableError::InvalidSpecialLine,
            ));
        }
    }
    if let Some(base) = upper {
        if last.checked_add(1) != Some(base) || base < i32::MIN as i64 || base > i32::MAX as i64 {
            return Err(HuffmanError::InvalidTable(
                HuffmanTableError::InvalidSpecialLine,
            ));
        }
    }
    Ok(())
}

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
const TABLE_A: [HuffmanLine; 4] = [
    normal_line!(0, 1, 4),
    normal_line!(16, 2, 8),
    normal_line!(272, 3, 16),
    upper_line!(65_808, 3),
];

#[cfg_attr(verus_keep_ghost, verus_verify)]
const TABLE_B: [HuffmanLine; 7] = [
    normal_line!(0, 1, 0),
    normal_line!(1, 2, 0),
    normal_line!(2, 3, 0),
    normal_line!(3, 4, 3),
    normal_line!(11, 5, 6),
    upper_line!(75, 6),
    oob_line!(6),
];

#[cfg_attr(verus_keep_ghost, verus_verify)]
const TABLE_C: [HuffmanLine; 9] = [
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
const TABLE_D: [HuffmanLine; 6] = [
    normal_line!(1, 1, 0),
    normal_line!(2, 2, 0),
    normal_line!(3, 3, 0),
    normal_line!(4, 4, 3),
    normal_line!(12, 5, 6),
    upper_line!(76, 5),
];

#[cfg_attr(verus_keep_ghost, verus_verify)]
const TABLE_E: [HuffmanLine; 8] = [
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
const TABLE_F: [HuffmanLine; 14] = [
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
const TABLE_G: [HuffmanLine; 15] = [
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
const TABLE_H: [HuffmanLine; 21] = [
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
const TABLE_I: [HuffmanLine; 22] = [
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
const TABLE_J: [HuffmanLine; 21] = [
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
const TABLE_K: [HuffmanLine; 13] = [
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
const TABLE_L: [HuffmanLine; 13] = [
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
const TABLE_M: [HuffmanLine; 13] = [
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
const TABLE_N: [HuffmanLine; 5] = [
    normal_line!(-2, 3, 0),
    normal_line!(-1, 3, 0),
    normal_line!(0, 1, 0),
    normal_line!(1, 3, 0),
    normal_line!(2, 3, 0),
];

#[cfg_attr(verus_keep_ghost, verus_verify)]
const TABLE_O: [HuffmanLine; 13] = [
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

#[cfg(test)]
mod tests;

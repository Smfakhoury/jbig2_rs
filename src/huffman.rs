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
        bit == 0 || self.limbs[limb] >> bit == 0
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
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn len(self) -> usize {
        match self {
            Self::Borrowed(lines) => lines.len(),
            Self::SymbolLengths(lengths) => lengths.len(),
            Self::Standard(lines) => lines.len(),
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
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
    #[cfg_attr(verus_keep_ghost, verus_verify)]
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

    #[cfg_attr(verus_keep_ghost, verus_verify)]
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
            decreases line_count - source_index,
        ))]
        while source_index < line_count {
            let line = lines.line(source_index).ok_or(HuffmanError::InvalidTable(
                HuffmanTableError::InvalidLineOrder,
            ))?;
            owned_lines.push(line);
            source_index += 1;
        }

        let mut length_count = [0_u32; MAX_PREFIX_LEN + 1];
        let mut count_index = 0_usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant count_index <= line_count,
            decreases line_count - count_index,
        ))]
        while count_index < line_count {
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
            count_index += 1;
        }

        let mut max_length = MAX_PREFIX_LEN;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant max_length <= MAX_PREFIX_LEN,
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

        let mut first_code = [WideCode::default(); MAX_PREFIX_LEN + 1];
        let mut first_line = [0_u32; MAX_PREFIX_LEN + 1];
        let mut ordered_lines = Vec::new();
        try_reserve::<u32>(&mut ordered_lines, line_count)?;

        let mut length = 1_usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                1 <= length,
                length <= max_prefix_len as usize + 1,
            decreases max_prefix_len as usize + 1 - length,
        ))]
        while length <= usize::from(max_prefix_len) {
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
                    invariant index <= line_count,
                    decreases line_count - index,
                ))]
                while index < line_count {
                    let line = lines.line(index).ok_or(HuffmanError::InvalidTable(
                        HuffmanTableError::InvalidLineOrder,
                    ))?;
                    if usize::from(line.prefix_len) == length {
                        let index = u32::try_from(index).map_err(|_| {
                            HuffmanError::InvalidTable(HuffmanTableError::Oversubscribed)
                        })?;
                        ordered_lines.push(index);
                    }
                    index += 1;
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
#[cfg_attr(verus_keep_ghost, verus_verify)]
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
mod tests {
    use super::*;

    const ALL_STANDARD_TABLES: [StandardHuffmanTable; 15] = [
        StandardHuffmanTable::A,
        StandardHuffmanTable::B,
        StandardHuffmanTable::C,
        StandardHuffmanTable::D,
        StandardHuffmanTable::E,
        StandardHuffmanTable::F,
        StandardHuffmanTable::G,
        StandardHuffmanTable::H,
        StandardHuffmanTable::I,
        StandardHuffmanTable::J,
        StandardHuffmanTable::K,
        StandardHuffmanTable::L,
        StandardHuffmanTable::M,
        StandardHuffmanTable::N,
        StandardHuffmanTable::O,
    ];

    fn bits(bit_string: &str) -> Vec<u8> {
        let mut bytes = vec![0_u8; bit_string.len().div_ceil(8)];
        for (position, bit) in bit_string.bytes().enumerate() {
            if bit == b'1' {
                bytes[position / 8] |= 1 << (7 - position % 8);
            }
        }
        bytes
    }

    fn decode_bits(table: &HuffmanTable, bit_string: &str) -> HuffmanValue {
        let bytes = bits(bit_string);
        let mut reader = bit_reader(&bytes);
        table.decode(&mut reader).expect("valid encoded value")
    }

    fn assert_decoders_equal(table: &HuffmanTable, bytes: &[u8], bit_position: usize) {
        let mut optimized = bit_reader(bytes);
        optimized.bit_position = bit_position;
        let mut canonical = optimized;
        let optimized_result = table.decode(&mut optimized);
        let canonical_result = table.decode_canonical(&mut canonical);
        assert_eq!(
            format!("{optimized_result:?}"),
            format!("{canonical_result:?}"),
            "bytes={bytes:02x?}, bit_position={bit_position}"
        );
        assert_eq!(
            optimized.bit_position, canonical.bit_position,
            "bytes={bytes:02x?}, bit_position={bit_position}"
        );
    }

    fn force_prefix_lookup(table: &mut HuffmanTable) {
        table.prefix_lookup = build_prefix_lookup(
            &table.length_count,
            &table.first_code,
            &table.first_line,
            &table.ordered_lines,
            usize::from(table.max_prefix_len),
        )
        .expect("prefix lookup");
        table.prefer_prefix_lookup = true;
    }

    fn append_code(output: &mut String, code: WideCode, length: u8) {
        for position in (0..usize::from(length)).rev() {
            let limb = position / 64;
            let bit = position % 64;
            output.push(if code.limbs[limb] >> bit & 1 == 0 {
                '0'
            } else {
                '1'
            });
        }
    }

    fn encoded_line(table: &HuffmanTable, index: usize, offset: u32) -> String {
        let (code, length) = table.code_for_line(index).expect("active line");
        let line = table.lines[index];
        let mut encoded = String::new();
        append_code(&mut encoded, code, length);
        for position in (0..line.range_len).rev() {
            encoded.push(if offset >> position & 1 == 0 {
                '0'
            } else {
                '1'
            });
        }
        encoded
    }

    fn code_string(table: &HuffmanTable, index: usize) -> String {
        let (code, length) = table.code_for_line(index).expect("active line");
        let mut result = String::new();
        append_code(&mut result, code, length);
        result
    }

    #[test]
    fn annex_b_table_a_codes_and_endpoints() {
        let table =
            HuffmanTable::standard(StandardHuffmanTable::A, &Limits::default()).expect("table A");
        assert_eq!(decode_bits(&table, "00000"), HuffmanValue::Integer(0));
        assert_eq!(decode_bits(&table, "01111"), HuffmanValue::Integer(15));
        assert_eq!(decode_bits(&table, "1000000000"), HuffmanValue::Integer(16));
        assert_eq!(
            decode_bits(&table, "1011111111"),
            HuffmanValue::Integer(271)
        );
        assert_eq!(
            decode_bits(&table, "1100000000000000000"),
            HuffmanValue::Integer(272)
        );
        assert_eq!(
            decode_bits(&table, "11100000000000000000000000000000000"),
            HuffmanValue::Integer(65_808)
        );
    }

    #[test]
    fn annex_b_table_b_oob_and_fixed_values() {
        let table =
            HuffmanTable::standard(StandardHuffmanTable::B, &Limits::default()).expect("table B");
        assert_eq!(decode_bits(&table, "0"), HuffmanValue::Integer(0));
        assert_eq!(decode_bits(&table, "10"), HuffmanValue::Integer(1));
        assert_eq!(decode_bits(&table, "110"), HuffmanValue::Integer(2));
        assert_eq!(decode_bits(&table, "1110000"), HuffmanValue::Integer(3));
        assert_eq!(decode_bits(&table, "111111"), HuffmanValue::OutOfBand);
    }

    #[test]
    fn every_line_of_every_standard_table_decodes() {
        for standard in ALL_STANDARD_TABLES {
            let mut table =
                HuffmanTable::standard(standard, &Limits::default()).expect("standard table");
            force_prefix_lookup(&mut table);
            for (index, line) in table.lines.iter().copied().enumerate() {
                if line.prefix_len == 0 {
                    continue;
                }
                if line.kind == LineKind::OutOfBand {
                    let encoded = encoded_line(&table, index, 0);
                    assert_eq!(decode_bits(&table, &encoded), HuffmanValue::OutOfBand);
                    continue;
                }

                let encoded = encoded_line(&table, index, 0);
                assert_eq!(
                    decode_bits(&table, &encoded),
                    HuffmanValue::Integer(
                        i32::try_from(line.range_low).expect("zero-offset value in range")
                    )
                );

                let maximum_offset =
                    match line.kind {
                        LineKind::Normal => {
                            if line.range_len == 32 {
                                u32::MAX
                            } else {
                                (1_u32 << line.range_len) - 1
                            }
                        }

                        LineKind::Lower => u32::try_from(line.range_low - i64::from(i32::MIN))
                            .expect("lower endpoint"),
                        LineKind::Upper => u32::try_from(i64::from(i32::MAX) - line.range_low)
                            .expect("upper endpoint"),
                        LineKind::OutOfBand => 0,
                    };
                let expected = if line.kind == LineKind::Lower {
                    line.range_low - i64::from(maximum_offset)
                } else {
                    line.range_low + i64::from(maximum_offset)
                };
                let encoded = encoded_line(&table, index, maximum_offset);
                assert_eq!(
                    decode_bits(&table, &encoded),
                    HuffmanValue::Integer(i32::try_from(expected).expect("endpoint in range"))
                );
            }
        }
    }

    #[test]
    fn prefix_lookup_matches_canonical_for_all_standard_prefixes() {
        for standard in ALL_STANDARD_TABLES {
            let table =
                HuffmanTable::standard(standard, &Limits::default()).expect("standard table");
            for prefix in 0_u32..=u32::from(u16::MAX) {
                let prefix = u16::try_from(prefix).expect("bounded prefix");
                let bytes = [
                    u8::try_from(prefix >> 8).expect("high byte"),
                    u8::try_from(prefix & 0xff).expect("low byte"),
                    0x5a,
                    0xa5,
                    0x3c,
                    0xc3,
                ];
                assert_decoders_equal(&table, &bytes, 0);
            }
            for byte_count in 0..=1 {
                let bytes = [0xa5];
                for bit_position in 0..=byte_count * 8 {
                    assert_decoders_equal(&table, &bytes[..byte_count], bit_position);
                }
            }
        }
    }

    #[test]
    fn randomized_custom_prefix_lookup_matches_canonical() {
        let mut random = 0x9e37_79b9_u32;
        for _ in 0..128 {
            random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let leaf_count = usize::try_from(random % 63 + 2).expect("small leaf count");
            let mut lengths = vec![1_u8, 1_u8];
            while lengths.len() < leaf_count {
                random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let index = usize::try_from(random).expect("u32 fits usize") % lengths.len();
                let child_length = lengths[index].checked_add(1).expect("bounded depth");
                lengths[index] = child_length;
                lengths.push(child_length);
            }

            let mut lines = Vec::with_capacity(lengths.len());
            let mut range_low = -10_000_i32;
            for length in lengths {
                random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let range_len = u8::try_from(random & 3).expect("two-bit range length");
                lines.push(HuffmanLine::normal(range_low, length, range_len));
                range_low = range_low
                    .checked_add(1_i32 << range_len)
                    .expect("small generated ranges");
            }
            let mut table = HuffmanTable::new(&lines, &Limits::default()).expect("custom table");
            force_prefix_lookup(&mut table);

            for _ in 0..512 {
                let mut bytes = [0_u8; 8];
                for byte in &mut bytes {
                    random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    *byte = u8::try_from(random >> 24).expect("high byte");
                }
                let bit_position = usize::try_from(random & 7).expect("three-bit position");
                assert_decoders_equal(&table, &bytes, bit_position);
            }
        }
    }

    #[test]
    #[ignore = "performance microbenchmark"]
    #[allow(clippy::cast_precision_loss)]
    fn benchmark_fixed_width_prefix_lookup_against_canonical() {
        use std::hint::black_box;
        use std::time::Instant;

        let lines = (0..=u8::MAX)
            .map(|value| HuffmanLine::normal(i32::from(value), 8, 0))
            .collect::<Vec<_>>();
        let table = HuffmanTable::new(&lines, &Limits::default()).expect("fixed-width table");
        let bytes = (0..=u8::MAX).cycle().take(65_536).collect::<Vec<_>>();
        let mut optimized_samples = Vec::new();
        let mut canonical_samples = Vec::new();
        for round in 0..9 {
            for canonical in [round % 2 != 0, round % 2 == 0] {
                let start = Instant::now();
                let mut reader = bit_reader(&bytes);
                for _ in 0..bytes.len() {
                    let value = if canonical {
                        table.decode_canonical(&mut reader)
                    } else {
                        table.decode(&mut reader)
                    }
                    .expect("complete fixed-width value");
                    black_box(value);
                }
                let elapsed = start.elapsed().as_nanos();
                if canonical {
                    canonical_samples.push(elapsed);
                } else {
                    optimized_samples.push(elapsed);
                }
            }
        }
        optimized_samples.sort_unstable();
        canonical_samples.sort_unstable();
        let optimized = optimized_samples[optimized_samples.len() / 2];
        let canonical = canonical_samples[canonical_samples.len() / 2];
        let gain = (canonical - optimized) as f64 * 100.0 / canonical as f64;
        eprintln!(
            "huffman-prefix: canonical={canonical}ns optimized={optimized}ns gain={gain:.2}%"
        );
    }

    #[test]
    #[ignore = "performance microbenchmark"]
    #[allow(clippy::cast_precision_loss)]
    fn benchmark_symbol_id_source_without_line_staging() {
        use std::hint::black_box;
        use std::time::Instant;

        let lengths = vec![12_u8; 4_096];
        let limits = Limits::default();
        let mut specialized_samples = Vec::new();
        let mut staged_samples = Vec::new();
        for round in 0..9 {
            for staged in [round % 2 != 0, round % 2 == 0] {
                let start = Instant::now();
                for _ in 0..32 {
                    if staged {
                        let lines = lengths
                            .iter()
                            .copied()
                            .enumerate()
                            .map(|(index, length)| {
                                HuffmanLine::normal(
                                    i32::try_from(index).expect("small index"),
                                    length,
                                    0,
                                )
                            })
                            .collect::<Vec<_>>();
                        black_box(HuffmanTable::new(&lines, &limits).expect("staged table"));
                    } else {
                        black_box(
                            HuffmanTable::new_symbol_ids(&lengths, &limits)
                                .expect("specialized table"),
                        );
                    }
                }
                let elapsed = start.elapsed().as_nanos();
                if staged {
                    staged_samples.push(elapsed);
                } else {
                    specialized_samples.push(elapsed);
                }
            }
        }
        specialized_samples.sort_unstable();
        staged_samples.sort_unstable();
        let specialized = specialized_samples[specialized_samples.len() / 2];
        let staged = staged_samples[staged_samples.len() / 2];
        let gain = (staged - specialized) as f64 * 100.0 / staged as f64;
        eprintln!("symbol-id-table: staged={staged}ns specialized={specialized}ns gain={gain:.2}%");
    }

    #[test]
    fn standard_prefixes_match_every_annex_b_table() {
        let expected: [&[&str]; 15] = [
            &["0", "10", "110", "111"],
            &["0", "10", "110", "1110", "11110", "111110", "111111"],
            &[
                "11111110", "0", "10", "110", "1110", "11110", "11111111", "1111110", "111110",
            ],
            &["0", "10", "110", "1110", "11110", "11111"],
            &[
                "1111110", "0", "10", "110", "1110", "11110", "1111111", "111110",
            ],
            &[
                "11100", "1000", "1001", "1010", "11101", "11110", "1011", "00", "010", "011",
                "1100", "1101", "111110", "111111",
            ],
            &[
                "1000", "000", "1001", "11010", "11011", "1010", "1011", "11100", "11101", "1100",
                "001", "010", "011", "11110", "11111",
            ],
            &[
                "11111100",
                "111111100",
                "11111101",
                "111111101",
                "1111100",
                "1010",
                "00",
                "11010",
                "111010",
                "100",
                "111011",
                "1011",
                "1100",
                "11011",
                "11100",
                "111100",
                "1111101",
                "111101",
                "111111110",
                "111111111",
                "01",
            ],
            &[
                "11111100",
                "111111100",
                "11111101",
                "111111101",
                "1111100",
                "1010",
                "010",
                "011",
                "11010",
                "111010",
                "100",
                "111011",
                "1011",
                "1100",
                "11011",
                "11100",
                "111100",
                "1111101",
                "111101",
                "111111110",
                "111111111",
                "00",
            ],
            &[
                "1111010", "11111100", "1111011", "11000", "00", "11001", "110110", "1111100",
                "11111101", "01", "11010", "110111", "111000", "111001", "111010", "111011",
                "111100", "1111101", "11111110", "11111111", "10",
            ],
            &[
                "0", "10", "1100", "1101", "11100", "11101", "111100", "1111010", "1111011",
                "1111100", "1111101", "1111110", "1111111",
            ],
            &[
                "0", "10", "110", "11100", "11101", "111100", "1111010", "1111011", "1111100",
                "1111101", "1111110", "11111110", "11111111",
            ],
            &[
                "0", "100", "1100", "11100", "1101", "101", "111010", "111011", "111100", "111101",
                "111110", "1111110", "1111111",
            ],
            &["100", "101", "0", "110", "111"],
            &[
                "1111100", "111100", "11100", "1100", "100", "0", "101", "1101", "11101", "111101",
                "1111101", "1111110", "1111111",
            ],
        ];

        for (table_number, standard) in ALL_STANDARD_TABLES.into_iter().enumerate() {
            let table =
                HuffmanTable::standard(standard, &Limits::default()).expect("standard table");
            let expected_codes = expected[table_number];
            assert_eq!(table.lines.len(), expected_codes.len());
            for (index, expected_code) in expected_codes.iter().enumerate() {
                assert_eq!(code_string(&table, index), *expected_code);
            }
        }
    }

    #[test]
    fn canonical_assignment_follows_table_order() {
        let lines = [
            HuffmanLine::normal(0, 2, 0),
            HuffmanLine::normal(1, 1, 0),
            HuffmanLine::normal(2, 3, 0),
            HuffmanLine::normal(3, 3, 0),
        ];
        let table = HuffmanTable::new(&lines, &Limits::default()).expect("valid table");
        assert_eq!(decode_bits(&table, "0"), HuffmanValue::Integer(1));
        assert_eq!(decode_bits(&table, "10"), HuffmanValue::Integer(0));
        assert_eq!(decode_bits(&table, "110"), HuffmanValue::Integer(2));
        assert_eq!(decode_bits(&table, "111"), HuffmanValue::Integer(3));
    }

    #[test]
    fn symbol_length_source_matches_materialized_line_oracle() {
        let lengths = [2_u8, 2, 3, 3, 3, 3];
        let lines = lengths
            .iter()
            .copied()
            .enumerate()
            .map(|(index, length)| {
                HuffmanLine::normal(i32::try_from(index).expect("small index"), length, 0)
            })
            .collect::<Vec<_>>();
        let specialized =
            HuffmanTable::new_symbol_ids(&lengths, &Limits::default()).expect("specialized table");
        let materialized =
            HuffmanTable::new(&lines, &Limits::default()).expect("materialized table");
        assert_eq!(specialized.lines.capacity(), materialized.lines.capacity());
        for prefix in 0_u16..=u16::MAX {
            let bytes = prefix.to_be_bytes();
            let mut specialized_reader = bit_reader(&bytes);
            let mut materialized_reader = bit_reader(&bytes);
            assert_eq!(
                format!("{:?}", specialized.decode(&mut specialized_reader)),
                format!("{:?}", materialized.decode(&mut materialized_reader))
            );
            assert_eq!(
                specialized_reader.bit_position,
                materialized_reader.bit_position
            );
        }
    }

    #[test]
    fn parses_annex_b_example_table() {
        let encoded = [
            0x42, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01, 0x10, 0x49, 0x23, 0x81, 0x80,
        ];
        let table = HuffmanTable::from_encoded(&encoded, &Limits::default())
            .expect("Annex B example table");
        assert_eq!(
            decode_bits(&table, "1100000000000000000"),
            HuffmanValue::Integer(272)
        );
        assert_eq!(
            decode_bits(&table, "11100000000000000000000000000000000"),
            HuffmanValue::Integer(65_808)
        );
    }

    #[test]
    fn parses_oob_custom_table() {
        // HTOOB=1, HTPS=2, HTRS=1, low=0, high=1.
        // Lines are (prefix=1, range=0), lower=0, upper=2, OOB=2.
        let encoded = [0x03, 0, 0, 0, 0, 0, 0, 0, 1, 0b0100_0101, 0];
        let table = HuffmanTable::from_encoded(&encoded, &Limits::default()).expect("custom table");
        assert_eq!(decode_bits(&table, "0"), HuffmanValue::Integer(0));
        assert_eq!(decode_bits(&table, "11"), HuffmanValue::OutOfBand);
    }

    #[test]
    fn rejects_reserved_flags_and_nonzero_padding() {
        let reserved = [0x80, 0, 0, 0, 0, 0, 0, 0, 1];
        assert!(matches!(
            HuffmanTable::from_encoded(&reserved, &Limits::default()),
            Err(HuffmanError::InvalidTable(
                HuffmanTableError::ReservedFlags(0x80)
            ))
        ));

        let padding = [0, 0, 0, 0, 0, 0, 0, 0, 1, 0b1011_1111];
        assert!(matches!(
            HuffmanTable::from_encoded(&padding, &Limits::default()),
            Err(HuffmanError::InvalidTable(
                HuffmanTableError::NonZeroPadding
            ))
        ));
    }

    #[test]
    fn rejects_invalid_bounds_partition_and_range_length() {
        let bounds = [0, 0, 0, 0, 1, 0, 0, 0, 1];
        assert!(matches!(
            HuffmanTable::from_encoded(&bounds, &Limits::default()),
            Err(HuffmanError::InvalidTable(
                HuffmanTableError::InvalidBounds { low: 1, high: 1 }
            ))
        ));

        // HTRS=6 and the first line has range length 33.
        let long_range = [0x50, 0, 0, 0, 0, 0, 0, 0, 1, 0b1100_0010];
        assert!(matches!(
            HuffmanTable::from_encoded(&long_range, &Limits::default()),
            Err(HuffmanError::InvalidTable(
                HuffmanTableError::InvalidRangeLength(33)
            ))
        ));

        // A two-value interval cannot be partitioned by a four-value line.
        let partition = [0x10, 0, 0, 0, 0, 0, 0, 0, 2, 0b1100_0000];
        assert!(matches!(
            HuffmanTable::from_encoded(&partition, &Limits::default()),
            Err(HuffmanError::InvalidTable(
                HuffmanTableError::InvalidRangePartition
            ))
        ));
    }

    #[test]
    fn rejects_oversubscribed_and_structurally_invalid_tables() {
        let oversubscribed = [
            HuffmanLine::normal(0, 1, 0),
            HuffmanLine::normal(1, 1, 0),
            HuffmanLine::normal(2, 1, 0),
        ];
        assert!(matches!(
            HuffmanTable::new(&oversubscribed, &Limits::default()),
            Err(HuffmanError::InvalidTable(
                HuffmanTableError::Oversubscribed
            ))
        ));

        let gap = [HuffmanLine::normal(0, 1, 0), HuffmanLine::normal(2, 1, 0)];
        assert!(matches!(
            HuffmanTable::new(&gap, &Limits::default()),
            Err(HuffmanError::InvalidTable(
                HuffmanTableError::InvalidRangePartition
            ))
        ));

        let no_codes = [HuffmanLine::normal(0, 0, 0)];
        assert!(matches!(
            HuffmanTable::new(&no_codes, &Limits::default()),
            Err(HuffmanError::InvalidTable(HuffmanTableError::NoCodes))
        ));
    }

    #[test]
    fn invalid_prefix_and_eof_are_oob_safe() {
        let lines = [HuffmanLine::normal(0, 1, 0), HuffmanLine::normal(1, 3, 0)];
        let table = HuffmanTable::new(&lines, &Limits::default()).expect("valid sparse table");

        let mut invalid_reader = bit_reader(&[0b1100_0000]);
        assert!(matches!(
            table.decode(&mut invalid_reader),
            Err(HuffmanError::InvalidTable(HuffmanTableError::InvalidCode))
        ));

        let mut eof_reader = bit_reader(&[0b0000_0001]);
        eof_reader.bit_position = 7;
        assert!(matches!(
            table.decode(&mut eof_reader),
            Err(HuffmanError::Decode(DecodeError::Malformed(
                MalformedError::UnexpectedEof {
                    offset: 8,
                    needed: 1,
                    remaining: 0
                }
            )))
        ));
        assert_eq!(eof_reader.bit_position, 8);
    }

    #[test]
    fn open_ranges_reject_values_outside_i32() {
        let table =
            HuffmanTable::standard(StandardHuffmanTable::A, &Limits::default()).expect("table A");
        let encoded = format!("111{:032b}", u32::MAX);
        let bytes = bits(&encoded);
        let mut reader = bit_reader(&bytes);
        assert!(matches!(
            table.decode(&mut reader),
            Err(HuffmanError::Decode(DecodeError::Malformed(
                MalformedError::IntegerOverflow
            )))
        ));
    }

    #[test]
    fn honors_memory_and_stream_limits() {
        let limits = Limits {
            max_decoder_managed_memory_bytes: 1,
            ..Limits::default()
        };
        assert!(matches!(
            HuffmanTable::standard(StandardHuffmanTable::A, &limits),
            Err(HuffmanError::Decode(DecodeError::LimitExceeded(
                LimitExceeded {
                    kind: LimitKind::Memory,
                    ..
                }
            )))
        ));

        let limits = Limits {
            max_stream_bytes: 8,
            ..Limits::default()
        };
        let encoded = [0_u8; 9];
        assert!(matches!(
            HuffmanTable::from_encoded(&encoded, &limits),
            Err(HuffmanError::Decode(DecodeError::LimitExceeded(
                LimitExceeded {
                    kind: LimitKind::StreamBytes,
                    limit: 8,
                    actual: 9
                }
            )))
        ));
    }

    #[test]
    fn supports_prefixes_longer_than_machine_words() {
        let mut lines = Vec::new();
        for value in 0..254_i32 {
            lines.push(HuffmanLine::normal(
                value,
                u8::try_from(value + 1).expect("length"),
                0,
            ));
        }
        lines.push(HuffmanLine::normal(254, 255, 0));
        lines.push(HuffmanLine::normal(255, 255, 0));
        let table = HuffmanTable::new(&lines, &Limits::default()).expect("255-bit table");
        let encoded = encoded_line(&table, 255, 0);
        assert_eq!(encoded.len(), 255);
        assert_eq!(decode_bits(&table, &encoded), HuffmanValue::Integer(255));
    }
}

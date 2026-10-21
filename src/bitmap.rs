use crate::{DecodeError, LimitExceeded, LimitKind, Limits, MalformedError};
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg(verus_keep_ghost)]
#[rustfmt::skip]
#[path = "../proofs/shared/bitmap.rs"]
pub(crate) mod bitmap_shared;

#[cfg(verus_keep_ghost)]
#[rustfmt::skip]
#[path = "../proofs/spec_properties/bitmap.rs"]
mod bitmap_properties;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) enum CombinationOperator {
    Or,
    And,
    Xor,
    Xnor,
    Replace,
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct RowBlit<'a> {
    pub(crate) source: &'a Bitmap,
    pub(crate) source_x: u32,
    pub(crate) destination_x: u32,
    pub(crate) width: u32,
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out == crate::spec::bitmap::bit_mask(x),
))]
fn pixel_mask(x: u32) -> u8 {
    match x % 8 {
        0 => 0x80,
        1 => 0x40,
        2 => 0x20,
        3 => 0x10,
        4 => 0x08,
        5 => 0x04,
        6 => 0x02,
        _ => 0x01,
    }
}

impl CombinationOperator {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == crate::spec::bitmap::combine(self, destination, source),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    #[allow(dead_code)]
    pub(crate) const fn apply(self, destination: bool, source: bool) -> bool {
        match self {
            Self::Or => destination || source,
            Self::And => destination && source,
            Self::Xor => destination != source,
            Self::Xnor => destination == source,
            Self::Replace => source,
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures forall|bit: u32| bit < 8 ==>
            crate::spec::bitmap::byte_operator_bit(out, bit)
                == crate::spec::bitmap::combine(
                    self, crate::spec::bitmap::byte_operator_bit(destination, bit),
                    crate::spec::bitmap::byte_operator_bit(source, bit),
                ),
    ))]
    const fn apply_byte(self, destination: u8, source: u8) -> u8 {
        let result = match self {
            Self::Or => destination | source,
            Self::And => destination & source,
            Self::Xor => destination ^ source,
            Self::Xnor => !(destination ^ source),
            Self::Replace => source,
        };
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal(crate::spec::bitmap::operator_byte);
            assert(result == crate::spec::bitmap::operator_byte(self, destination, source));
            assert forall|bit: u32| bit < 8 implies
                crate::spec::bitmap::byte_operator_bit(result, bit)
                    == crate::spec::bitmap::combine(
                        self, crate::spec::bitmap::byte_operator_bit(destination, bit),
                        crate::spec::bitmap::byte_operator_bit(source, bit),
                    ) by {
                bitmap_shared::byte_operator_semantics(self, destination, source, bit);
            }
        }
        result
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures forall|bit: u32| bit < 64 ==>
            crate::spec::bitmap::word_operator_bit(out, bit)
                == crate::spec::bitmap::combine(
                    self, crate::spec::bitmap::word_operator_bit(destination, bit),
                    crate::spec::bitmap::word_operator_bit(source, bit),
                ),
    ))]
    const fn apply_chunk(self, destination: u64, source: u64) -> u64 {
        let result = match self {
            Self::Or => destination | source,
            Self::And => destination & source,
            Self::Xor => destination ^ source,
            Self::Xnor => !(destination ^ source),
            Self::Replace => source,
        };
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal(crate::spec::bitmap::operator_word);
            assert(result == crate::spec::bitmap::operator_word(self, destination, source));
            assert forall|bit: u32| bit < 64 implies
                crate::spec::bitmap::word_operator_bit(result, bit)
                    == crate::spec::bitmap::combine(
                        self, crate::spec::bitmap::word_operator_bit(destination, bit),
                        crate::spec::bitmap::word_operator_bit(source, bit),
                    ) by {
                bitmap_shared::word_operator_semantics(self, destination, source, bit);
            }
        }
        result
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(
    requires old(destination)@.len() == source@.len(),
    ensures final(destination)@ == source@,
))]
#[inline]
fn copy_row_bytes(destination: &mut [u8], source: &[u8]) {
    let length = source.len();
    if length <= 2 {
        if length != 0 {
            destination[0] = source[0];
        }
        if length == 2 {
            destination[1] = source[1];
        }
    } else if length > 64 && length <= 128 {
        // Avoid the slow short-copy path with a fixed block and a small tail.
        destination[..64].copy_from_slice(&source[..64]);
        destination[64..].copy_from_slice(&source[64..]);
    } else {
        destination.copy_from_slice(source);
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures
        out.is_some() ==> start as nat + 8 <= bytes@.len(),
        out.is_some() == (start as nat + 8 <= bytes@.len()),
        out.is_some() ==> out == Some(crate::spec::bitmap::chunk_value(bytes@, start as int)),
))]
fn read_chunk(bytes: &[u8], start: usize) -> Option<u64> {
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(bytes@.len() == bytes.len());
        assert(bytes@.len() <= usize::MAX);
    }
    let end = start.checked_add(8)?;
    let chunk = bytes.get(start..end)?;
    let array = [
        *chunk.first()?,
        *chunk.get(1)?,
        *chunk.get(2)?,
        *chunk.get(3)?,
        *chunk.get(4)?,
        *chunk.get(5)?,
        *chunk.get(6)?,
        *chunk.get(7)?,
    ];
    Some(
        (u64::from(array[0]) << 56)
            | (u64::from(array[1]) << 48)
            | (u64::from(array[2]) << 40)
            | (u64::from(array[3]) << 32)
            | (u64::from(array[4]) << 24)
            | (u64::from(array[5]) << 16)
            | (u64::from(array[6]) << 8)
            | u64::from(array[7]),
    )
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures
        final(bytes)@.len() == old(bytes)@.len(),
        out ==> start as nat + 8 <= old(bytes)@.len(),
        out == (start as nat + 8 <= old(bytes)@.len()),
        out ==> forall|offset: int| 0 <= offset < 8 ==>
            final(bytes)@[start as int + offset]
                == crate::spec::bitmap::chunk_byte(value, offset),
        forall|index: int| 0 <= index < old(bytes)@.len()
            && (!out || index < start as int || start as int + 8 <= index)
            ==> final(bytes)@[index] == old(bytes)@[index],
))]
#[allow(clippy::cast_possible_truncation, clippy::ptr_arg)]
fn write_chunk(bytes: &mut Vec<u8>, start: usize, value: u64) -> bool {
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(bytes@.len() == bytes.len());
        assert(bytes@.len() <= usize::MAX);
    }
    let Some(end) = start.checked_add(8) else {
        return false;
    };
    let Some(chunk) = bytes.get_mut(start..end) else {
        return false;
    };
    let array = [
        (value >> 56) as u8,
        (value >> 48) as u8,
        (value >> 40) as u8,
        (value >> 32) as u8,
        (value >> 24) as u8,
        (value >> 16) as u8,
        (value >> 8) as u8,
        value as u8,
    ];
    chunk[0] = array[0];
    chunk[1] = array[1];
    chunk[2] = array[2];
    chunk[3] = array[3];
    chunk[4] = array[4];
    chunk[5] = array[5];
    chunk[6] = array[6];
    chunk[7] = array[7];
    true
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out.is_some() ==> (byte_index as nat) < bytes@.len(),
))]
fn read_shifted_byte(bytes: &[u8], byte_index: usize, shift: u32) -> Option<u8> {
    let first = *bytes.get(byte_index)?;
    if shift == 0 {
        return Some(first);
    }
    if shift >= 8 {
        return None;
    }
    let next_index = byte_index.checked_add(1)?;
    let second = *bytes.get(next_index)?;
    Some((first << shift) | (second >> (8 - shift)))
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out.is_some() ==> (byte_index as nat) < bytes@.len(),
))]
fn read_shifted_tail(bytes: &[u8], byte_index: usize, shift: u32, bit_count: u32) -> Option<u8> {
    let first = *bytes.get(byte_index)?;
    if shift == 0 {
        return Some(first);
    }
    if shift >= 8 || bit_count > 8 || bit_count == 0 {
        return None;
    }
    let shifted = first << shift;
    if bit_count <= 8 - shift {
        return Some(shifted);
    }
    let next_index = byte_index.checked_add(1)?;
    let second = *bytes.get(next_index)?;
    Some(shifted | (second >> (8 - shift)))
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out.is_some() ==> byte_index as nat + 8 <= bytes@.len(),
))]
fn read_shifted_chunk(bytes: &[u8], byte_index: usize, shift: u32) -> Option<u64> {
    let first = read_chunk(bytes, byte_index)?;
    if shift == 0 {
        return Some(first);
    }
    if shift >= 8 {
        return None;
    }
    let next_index = byte_index.checked_add(8)?;
    let next = u64::from(*bytes.get(next_index)?);
    Some((first << shift) | (next >> (8 - shift)))
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok((stride, length, length_u64)) =>
            crate::spec::bitmap::dimensions_checked(width, height)
            && crate::spec::bitmap::stride_checked(width)
            && crate::spec::bitmap::allocation_checked(width, height)
            && crate::spec::bitmap::limits_allow_bitmap(
                width,
                height,
                limits,
            )
            && stride as nat
                == crate::spec::bitmap::stride_for_width(width as nat)
            && length as nat
                == crate::spec::bitmap::stride_for_width(width as nat)
                    * height as nat
            && length_u64 as nat == length as nat,
        Err(_) => true,
    },
))]
fn checked_bitmap_layout(
    width: u32,
    height: u32,
    limits: &Limits,
) -> Result<(usize, usize, u64), DecodeError> {
    #[cfg(verus_keep_ghost)]
    production_bitmap_checked_dimensions!(width, height);
    enforce_limit(
        LimitKind::Width,
        u64::from(limits.max_width),
        u64::from(width),
    )?;
    enforce_limit(
        LimitKind::Height,
        u64::from(limits.max_height),
        u64::from(height),
    )?;

    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or(MalformedError::IntegerOverflow)?;
    enforce_limit(LimitKind::PagePixels, limits.max_pixels_per_page, pixels)?;

    let stride_u64 = u64::from(width)
        .checked_add(7)
        .ok_or(MalformedError::IntegerOverflow)?
        / 8;
    let length_u64 = stride_u64
        .checked_mul(u64::from(height))
        .ok_or(MalformedError::IntegerOverflow)?;
    enforce_limit(
        LimitKind::Memory,
        limits.max_decoder_managed_memory_bytes,
        length_u64,
    )?;

    let stride = usize::try_from(stride_u64).map_err(|_error| MalformedError::IntegerOverflow)?;
    let length = usize::try_from(length_u64).map_err(|_error| MalformedError::IntegerOverflow)?;
    Ok((stride, length, length_u64))
}

/// An owned row-major, one-bit-per-pixel bitmap.
///
/// Within each byte, the leftmost pixel is stored in the most significant bit.
#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(test, derive(Clone))]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub struct Bitmap {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) stride: usize,
    pub(crate) data: Vec<u8>,
}

impl Bitmap {
    /// Allocates a zero-filled bitmap under the supplied resource limits.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError::LimitExceeded`] if the dimensions or allocation
    /// exceed `limits`, [`DecodeError::Malformed`] if size arithmetic cannot be
    /// represented, or [`DecodeError::Allocation`] if storage cannot be
    /// reserved.
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures match out {
            Ok(bitmap) =>
                crate::views::bitmap_width(&bitmap) == width
                && crate::views::bitmap_height(&bitmap) == height
                && crate::spec::bitmap::storage_valid(&bitmap)
                && crate::spec::bitmap::zeroed_storage(&bitmap)
                && crate::spec::bitmap::limits_allow_bitmap(width, height, limits),
            Err(_) => true,
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub fn blank(width: u32, height: u32, limits: &Limits) -> Result<Self, DecodeError> {
        Self::filled(width, height, false, limits)
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures match out {
            Ok(bitmap) =>
                crate::views::bitmap_width(&bitmap) == width
                && crate::views::bitmap_height(&bitmap) == height
                && crate::spec::bitmap::storage_valid(&bitmap)
                && (!value ==> crate::spec::bitmap::zeroed_storage(&bitmap))
                && crate::spec::bitmap::limits_allow_bitmap(width, height, limits),
            Err(_) => true,
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn filled(
        width: u32,
        height: u32,
        value: bool,
        limits: &Limits,
    ) -> Result<Self, DecodeError> {
        let (stride, length, length_u64) = checked_bitmap_layout(width, height, limits)?;
        let mut data = Vec::new();
        if data.try_reserve(length).is_err() {
            return Err(DecodeError::Allocation {
                requested_bytes: length_u64,
            });
        }
        data.resize(length, if value { 0xFF } else { 0 });

        if value && width % 8 != 0 {
            let used_bits = (width % 8) as u8;
            let trailing_mask = 0xFF_u8 << (8 - used_bits);
            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant
                    row <= height,
                    data@.len() == length,
                    length as nat == stride as nat * height as nat,
                    stride > 0,
                decreases height - row,
            ))]
            for row in 0..height {
                let Ok(row) = usize::try_from(row) else {
                    return Err(MalformedError::IntegerOverflow.into());
                };
                let row_offset = row
                    .checked_mul(stride)
                    .ok_or(MalformedError::IntegerOverflow)?;
                let stride_minus_one = stride
                    .checked_sub(1)
                    .ok_or(MalformedError::IntegerOverflow)?;
                let final_byte = row_offset
                    .checked_add(stride_minus_one)
                    .ok_or(MalformedError::IntegerOverflow)?;
                #[cfg(verus_keep_ghost)]
                proof! {
                    assert(row < height as usize);
                    assert(row_offset as nat == row as nat * stride as nat);
                    assert(stride_minus_one as nat + 1 == stride as nat);
                    assert(final_byte as nat
                        == row_offset as nat + stride_minus_one as nat);
                    vstd::arithmetic::mul::lemma_mul_inequality(
                        row as int + 1,
                        height as int,
                        stride as int,
                    );
                    vstd::arithmetic::mul::lemma_mul_is_distributive_add_other_way(
                        stride as int,
                        row as int,
                        1,
                    );
                    vstd::arithmetic::mul::lemma_mul_is_commutative(
                        height as int,
                        stride as int,
                    );
                    assert((row as nat + 1) * stride as nat
                        <= height as nat * stride as nat);
                    assert(final_byte as nat + 1
                        == (row as nat + 1) * stride as nat);
                    assert(final_byte < data@.len());
                }
                data[final_byte] &= trailing_mask;
            }
        }

        let bitmap = Self {
            width,
            height,
            stride,
            data,
        };
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::views::bitmap_fields(&bitmap);
            reveal(crate::spec::bitmap::storage_valid);
        }
        Ok(bitmap)
    }

    /// Bitmap width in pixels.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == crate::views::bitmap_width(self),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub const fn width(&self) -> u32 {
        #[cfg(verus_keep_ghost)]
        production_bitmap_fields!(self);
        self.width
    }

    /// Bitmap height in pixels.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == crate::views::bitmap_height(self),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub const fn height(&self) -> u32 {
        #[cfg(verus_keep_ghost)]
        production_bitmap_fields!(self);
        self.height
    }

    /// Bytes between the start of adjacent rows.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == crate::views::bitmap_stride(self),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub const fn stride(&self) -> usize {
        #[cfg(verus_keep_ghost)]
        production_bitmap_fields!(self);
        self.stride
    }

    /// Packed bitmap bytes.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out@ == crate::views::bitmap_data(self),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub fn bytes(&self) -> &[u8] {
        #[cfg(verus_keep_ghost)]
        production_bitmap_fields!(self);
        &self.data
    }

    /// Returns a pixel, or `None` when the coordinates are outside the bitmap.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires crate::spec::bitmap::storage_valid(self),
        ensures out == crate::spec::bitmap::pixel_result(self, x, y),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub fn pixel(&self, x: u32, y: u32) -> Option<bool> {
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::views::bitmap_fields(self);
            reveal(crate::spec::bitmap::pixel_result);
            reveal(crate::spec::bitmap::coordinates_in_bounds);
        }
        if x >= self.width || y >= self.height {
            return None;
        }
        #[cfg(verus_keep_ghost)]
        production_bitmap_in_bounds_byte!(self, x, y);
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal(crate::spec::bitmap::byte_index);
            assert(y as nat <= u32::MAX as nat);
            assert(u32::MAX as nat <= usize::MAX) by (compute);
            assert(
                y as nat * self.stride as nat + x as nat / 8
                    < self.data@.len()
            );
            assert(self.data@.len() == self.data.len());
            assert(self.data@.len() <= usize::MAX);
            assert(y as nat * self.stride as nat <= usize::MAX);
        }
        let byte_index = usize::try_from(y)
            .ok()?
            .checked_mul(self.stride)?
            .checked_add(usize::try_from(x / 8).ok()?)?;
        let byte = self.data.get(byte_index)?;
        let value = *byte & pixel_mask(x) != 0;
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal(crate::spec::bitmap::stored_pixel);
            reveal(crate::spec::bitmap::byte_bit);
            reveal(crate::spec::bitmap::bit_mask);
            assert(byte_index as nat
                == crate::spec::bitmap::byte_index(self.stride, x, y));
        }
        Some(value)
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            row < old(self).height,
            start <= end <= old(self).width,
        ensures
            final(self).width == old(self).width,
            final(self).height == old(self).height,
            final(self).stride == old(self).stride,
            final(self).data@.len() == old(self).data@.len(),
            start == end ==> final(self).data@ == old(self).data@,
            start == end ==> out.is_ok(),
    ))]
    pub(crate) fn fill_black_span(
        &mut self,
        row: u32,
        start: u32,
        end: u32,
    ) -> Result<(), MalformedError> {
        if start > end || end > self.width || row >= self.height {
            return Err(MalformedError::IntegerOverflow);
        }
        if start == end {
            return Ok(());
        }
        let row_start = usize::try_from(row)
            .map_err(|_error| MalformedError::IntegerOverflow)?
            .checked_mul(self.stride)
            .ok_or(MalformedError::IntegerOverflow)?;
        let first_byte =
            usize::try_from(start / 8).map_err(|_error| MalformedError::IntegerOverflow)?;
        let first_index = row_start
            .checked_add(first_byte)
            .ok_or(MalformedError::IntegerOverflow)?;
        let start_shift = start % 8;
        let first_bits = (end - start).min(8 - start_shift);
        let first_mask = (0xFF_u8 >> start_shift) & (0xFF_u8 << (8 - start_shift - first_bits));
        let first = self
            .data
            .get_mut(first_index)
            .ok_or(MalformedError::IntegerOverflow)?;
        *first |= first_mask;

        let mut x = start + first_bits;
        let mut index = first_index
            .checked_add(1)
            .ok_or(MalformedError::IntegerOverflow)?;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                start < x <= end,
                end <= self.width,
                self.width == old(self).width,
                self.height == old(self).height,
                self.stride == old(self).stride,
                self.data@.len() == old(self).data@.len(),
            decreases end - x,
        ))]
        while end - x >= 64 {
            if !write_chunk(&mut self.data, index, u64::MAX) {
                return Err(MalformedError::IntegerOverflow);
            }
            x += 64;
            index = index
                .checked_add(8)
                .ok_or(MalformedError::IntegerOverflow)?;
        }
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                start < x <= end,
                end <= self.width,
                self.width == old(self).width,
                self.height == old(self).height,
                self.stride == old(self).stride,
                self.data@.len() == old(self).data@.len(),
            decreases end - x,
        ))]
        while end - x >= 8 {
            let byte = self
                .data
                .get_mut(index)
                .ok_or(MalformedError::IntegerOverflow)?;
            *byte = 0xFF;
            x += 8;
            index = index
                .checked_add(1)
                .ok_or(MalformedError::IntegerOverflow)?;
        }
        if x < end {
            let mask = 0xFF_u8 << (8 - (end - x));
            let byte = self
                .data
                .get_mut(index)
                .ok_or(MalformedError::IntegerOverflow)?;
            *byte |= mask;
        }
        Ok(())
    }

    #[cfg(verus_keep_ghost)]
    #[verus_spec(out =>
        requires
            crate::spec::bitmap::storage_valid(old(self)),
            crate::spec::bitmap::storage_valid(source),
        ensures
            out.is_ok(),
            crate::spec::bitmap::storage_valid(final(self)),
            final(self).width == old(self).width,
            final(self).height == old(self).height,
            final(self).stride == old(self).stride,
            final(self).data@.len() == old(self).data@.len(),
            forall|index: int|
                0 <= index < old(self).data@.len()
                && !crate::spec::bitmap::compose_byte_may_change(
                    index,
                    old(self).stride,
                    old(self).width,
                    old(self).height,
                    source.width,
                    source.height,
                    x,
                    y,
                )
                ==> final(self).data@[index] == old(self).data@[index],
    )]
    #[allow(clippy::too_many_lines)]
    fn compose_scalar_verified(
        &mut self,
        source: &Self,
        x: u32,
        y: u32,
        operator: CombinationOperator,
    ) -> Result<(), MalformedError> {
        proof! {
            crate::views::bitmap_fields(self);
            crate::views::bitmap_fields(source);
        }
        proof_decl! {
            let ghost original_data = self.data@;
        }
        let width = source.width.min(self.width.saturating_sub(x));
        let height = source.height.min(self.height.saturating_sub(y));
        production_bitmap_clipped_bounds!(self, source, x, y);
        proof! {
            reveal(crate::spec::bitmap::clipped_width);
            reveal(crate::spec::bitmap::clipped_height);
            assert(width as nat == crate::spec::bitmap::clipped_width(
                self.width,
                source.width,
                x,
            ));
            assert(height as nat == crate::spec::bitmap::clipped_height(
                self.height,
                source.height,
                y,
            ));
            assert(x as nat + width as nat <= if x <= self.width {
                self.width as nat
            } else {
                x as nat
            });
            assert(y as nat + height as nat <= if y <= self.height {
                self.height as nat
            } else {
                y as nat
            });
        }
        #[verus_spec(
            invariant
                crate::spec::bitmap::storage_valid(self),
                crate::spec::bitmap::storage_valid(source),
                self.width == old(self).width,
                self.height == old(self).height,
                self.stride == old(self).stride,
                self.data@.len() == original_data.len(),
                forall|index: int|
                    0 <= index < original_data.len()
                    && !crate::spec::bitmap::compose_byte_may_change(
                        index,
                        self.stride,
                        self.width,
                        self.height,
                        source.width,
                        source.height,
                        x,
                        y,
                    )
                    ==> self.data@[index] == original_data[index],
                source_y <= height,
                height <= source.height,
                width <= source.width,
                height > 0 ==> y < self.height,
                width > 0 ==> x < self.width,
                height > 0 ==> y as nat + height as nat <= self.height as nat,
                width > 0 ==> x as nat + width as nat <= self.width as nat,
            decreases height - source_y,
        )]
        for source_y in 0..height {
            let destination_y = y
                .checked_add(source_y)
                .ok_or(MalformedError::IntegerOverflow)?;
            #[verus_spec(
                invariant
                    crate::spec::bitmap::storage_valid(self),
                    crate::spec::bitmap::storage_valid(source),
                    self.width == old(self).width,
                    self.height == old(self).height,
                    self.stride == old(self).stride,
                    self.data@.len() == original_data.len(),
                    forall|index: int|
                        0 <= index < original_data.len()
                        && !crate::spec::bitmap::compose_byte_may_change(
                            index,
                            self.stride,
                            self.width,
                            self.height,
                            source.width,
                            source.height,
                            x,
                            y,
                        )
                        ==> self.data@[index] == original_data[index],
                    source_x <= width,
                    source_y < height,
                    height <= source.height,
                    width <= source.width,
                    height > 0 ==> y as nat + height as nat <= self.height as nat,
                    width > 0 ==> x as nat + width as nat <= self.width as nat,
                    destination_y as nat == y as nat + source_y as nat,
                decreases width - source_x,
            )]
            for source_x in 0..width {
                proof! {
                    crate::views::bitmap_fields(source);
                    crate::views::bitmap_fields(self);
                    assert(source_x < width);
                    assert(source_y < height);
                    assert(source_x < source.width);
                    assert(source_y < source.height);
                    assert((x as nat) + (source_x as nat) < self.width as nat);
                    assert((y as nat) + (source_y as nat) < self.height as nat);
                }
                let destination_x = x
                    .checked_add(source_x)
                    .ok_or(MalformedError::IntegerOverflow)?;
                let source_pixel = source.pixel(source_x, source_y);
                let destination_pixel = self.pixel(destination_x, destination_y);
                proof! {
                    reveal(crate::spec::bitmap::pixel_result);
                    reveal(crate::spec::bitmap::coordinates_in_bounds);
                    assert(source_pixel.is_some());
                    assert(destination_pixel.is_some());
                }
                let source_value = source_pixel.ok_or(MalformedError::IntegerOverflow)?;
                let destination_value = destination_pixel.ok_or(MalformedError::IntegerOverflow)?;
                proof_decl! {
                    let ghost before_data = self.data@;
                }
                self.set_pixel(
                    destination_x,
                    destination_y,
                    operator.apply(destination_value, source_value),
                )?;
                proof! {
                    crate::views::bitmap_fields(self);
                    crate::views::bitmap_fields(old(self));
                    assert forall|index: int|
                        0 <= index < original_data.len()
                        && !crate::spec::bitmap::compose_byte_may_change(
                            index,
                            self.stride,
                            self.width,
                            self.height,
                            source.width,
                            source.height,
                            x,
                            y,
                        )
                    implies self.data@[index] == original_data[index] by {
                        let changed = crate::spec::bitmap::byte_index(
                            self.stride,
                            destination_x,
                            destination_y,
                        ) as int;
                        if index == changed {
                            reveal(crate::spec::bitmap::compose_byte_may_change);
                            assert(
                                crate::spec::bitmap::compose_byte_may_change(
                                    index,
                                    self.stride,
                                    self.width,
                                    self.height,
                                    source.width,
                                    source.height,
                                    x,
                                    y,
                                )
                            ) by {
                                let sx = source_x as int;
                                let sy = source_y as int;
                                assert(destination_x as int == x as int + sx);
                                assert(destination_y as int == y as int + sy);
                                reveal(crate::spec::bitmap::clipped_width);
                                reveal(crate::spec::bitmap::clipped_height);
                                assert((source_x as nat)
                                    < crate::spec::bitmap::clipped_width(
                                        self.width,
                                        source.width,
                                        x,
                                    ));
                                assert((source_y as nat)
                                    < crate::spec::bitmap::clipped_height(
                                        self.height,
                                        source.height,
                                        y,
                                    ));
                                assert(exists|candidate_x: int, candidate_y: int|
                                    0 <= candidate_x
                                        < crate::spec::bitmap::clipped_width(
                                            self.width,
                                            source.width,
                                            x,
                                        )
                                    && 0 <= candidate_y
                                        < crate::spec::bitmap::clipped_height(
                                            self.height,
                                            source.height,
                                            y,
                                        )
                                    && index
                                        == #[trigger] crate::spec::bitmap::byte_index(
                                            self.stride,
                                            (x as int + candidate_x) as u32,
                                            (y as int + candidate_y) as u32,
                                        ) as int
                                ) by {
                                    assert(index
                                        == crate::spec::bitmap::byte_index(
                                            self.stride,
                                            (x as int + sx) as u32,
                                            (y as int + sy) as u32,
                                        ) as int);
                                }
                            }
                        } else {
                            assert(self.data@[index] == before_data[index]);
                            assert(before_data[index] == original_data[index]);
                        }
                    }
                }
            }
        }
        proof! {
            crate::views::bitmap_fields(self);
            crate::views::bitmap_fields(old(self));
        }
        Ok(())
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            crate::spec::bitmap::storage_valid(old(self)),
            source < old(self).height,
            destination < old(self).height,
        ensures
            crate::spec::bitmap::storage_valid(final(self)),
            final(self).width == old(self).width,
            final(self).height == old(self).height,
            final(self).stride == old(self).stride,
            final(self).data@.len() == old(self).data@.len(),
            crate::spec::bitmap::row_copy_result(old(self), final(self), source, destination),
            out.is_ok(),
    ))]
    pub(crate) fn copy_row(&mut self, source: u32, destination: u32) -> Result<(), MalformedError> {
        let data_length = self.data.len();
        #[cfg(verus_keep_ghost)]
        production_bitmap_row_copy_bounds!(self, source, destination);
        if source >= self.height || destination >= self.height {
            return Err(MalformedError::IntegerOverflow);
        }
        if source == destination || self.stride == 0 {
            return Ok(());
        }
        let source_start = usize::try_from(source)
            .map_err(|_error| MalformedError::IntegerOverflow)?
            .checked_mul(self.stride)
            .ok_or(MalformedError::IntegerOverflow)?;
        let destination_start = usize::try_from(destination)
            .map_err(|_error| MalformedError::IntegerOverflow)?
            .checked_mul(self.stride)
            .ok_or(MalformedError::IntegerOverflow)?;
        let source_end = source_start
            .checked_add(self.stride)
            .ok_or(MalformedError::IntegerOverflow)?;
        let destination_end = destination_start
            .checked_add(self.stride)
            .ok_or(MalformedError::IntegerOverflow)?;
        if source_end > data_length || destination_end > data_length {
            return Err(MalformedError::IntegerOverflow);
        }
        if source < destination {
            let (before, after) = self.data.split_at_mut(destination_start);
            copy_row_bytes(&mut after[..self.stride], &before[source_start..source_end]);
        } else {
            let (before, after) = self.data.split_at_mut(source_start);
            copy_row_bytes(
                &mut before[destination_start..destination_end],
                &after[..self.stride],
            );
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::views::bitmap_fields(self);
        }
        Ok(())
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            crate::spec::bitmap::storage_valid(old(self)),
            crate::spec::bitmap::storage_valid(source),
        ensures
            crate::spec::bitmap::storage_valid(final(self)),
            final(self).width == old(self).width,
            final(self).height == old(self).height,
            final(self).stride == old(self).stride,
            final(self).data@.len() == old(self).data@.len(),
            match out {
                Ok(()) => byte_count <= old(self).data@.len()
                    && byte_count <= source.data@.len()
                    && crate::spec::bitmap::prefix_copy_result(
                        old(self), final(self), source, byte_count,
                    ),
                Err(error) =>
                    error == MalformedError::IntegerOverflow
                    && (byte_count > old(self).data@.len() || byte_count > source.data@.len())
                    && final(self).data@ == old(self).data@,
            },
    ))]
    pub(crate) fn copy_prefix_from(
        &mut self,
        source: &Self,
        byte_count: usize,
    ) -> Result<(), MalformedError> {
        if byte_count > self.data.len() || byte_count > source.data.len() {
            return Err(MalformedError::IntegerOverflow);
        }
        // Keep fixed-word prefix copies inline without changing the row kernel.
        if byte_count == 8 {
            self.data.as_mut_slice()[..8].copy_from_slice(&source.data.as_slice()[..8]);
        } else {
            copy_row_bytes(
                &mut self.data.as_mut_slice()[..byte_count],
                &source.data.as_slice()[..byte_count],
            );
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::views::bitmap_fields(self);
            crate::views::bitmap_fields(old(self));
            crate::views::bitmap_fields(source);
            reveal(crate::spec::bitmap::storage_valid);
            reveal(crate::spec::bitmap::prefix_copy_result);
        }
        Ok(())
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            crate::spec::bitmap::storage_valid(old(self)),
            crate::spec::bitmap::storage_valid(source),
        ensures
            out.is_ok(),
            crate::spec::bitmap::storage_valid(final(self)),
            crate::views::bitmap_width(final(self))
                == crate::views::bitmap_width(old(self)),
            crate::views::bitmap_height(final(self))
                == crate::views::bitmap_height(old(self)),
            crate::views::bitmap_stride(final(self))
                == crate::views::bitmap_stride(old(self)),
            crate::views::bitmap_data(final(self)).len()
                == crate::views::bitmap_data(old(self)).len(),
            forall|index: int|
                0 <= index < crate::views::bitmap_data(old(self)).len()
                && !crate::spec::bitmap::compose_byte_may_change(
                    index,
                    crate::views::bitmap_stride(old(self)),
                    crate::views::bitmap_width(old(self)),
                    crate::views::bitmap_height(old(self)),
                    crate::views::bitmap_width(source),
                    crate::views::bitmap_height(source),
                    x,
                    y,
                )
                ==> crate::views::bitmap_data(final(self))[index]
                    == crate::views::bitmap_data(old(self))[index],
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn compose(
        &mut self,
        source: &Self,
        x: u32,
        y: u32,
        operator: CombinationOperator,
    ) -> Result<(), MalformedError> {
        #[cfg(verus_keep_ghost)]
        {
            let result = self.compose_scalar_verified(source, x, y, operator);
            proof! {
                crate::views::bitmap_fields(self);
                crate::views::bitmap_fields(old(self));
                crate::views::bitmap_fields(source);
            }
            return result;
        }
        #[cfg(not(verus_keep_ghost))]
        self.compose_chunked(source, x, y, operator)
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            crate::spec::bitmap::storage_valid(old(self)),
            crate::spec::bitmap::storage_valid(source),
        ensures
            final(self).width == old(self).width,
            final(self).height == old(self).height,
            final(self).stride == old(self).stride,
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    #[allow(clippy::too_many_lines)]
    fn compose_chunked(
        &mut self,
        source: &Self,
        x: u32,
        y: u32,
        operator: CombinationOperator,
    ) -> Result<(), MalformedError> {
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::views::bitmap_fields(self);
            crate::views::bitmap_fields(source);
        }
        let width = source.width.min(self.width.saturating_sub(x));
        let height = source.height.min(self.height.saturating_sub(y));
        #[cfg(verus_keep_ghost)]
        production_bitmap_clipped_bounds!(self, source, x, y);
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal(crate::spec::bitmap::clipped_width);
            reveal(crate::spec::bitmap::clipped_height);
            assert(width as nat == crate::spec::bitmap::clipped_width(
                self.width,
                source.width,
                x,
            ));
            assert(height as nat == crate::spec::bitmap::clipped_height(
                self.height,
                source.height,
                y,
            ));
            assert(x as nat + width as nat <= if x <= self.width {
                self.width as nat
            } else {
                x as nat
            });
            assert(y as nat + height as nat <= if y <= self.height {
                self.height as nat
            } else {
                y as nat
            });
        }
        if width == 0 || height == 0 {
            return Ok(());
        }
        let destination_column =
            usize::try_from(x / 8).map_err(|_error| MalformedError::IntegerOverflow)?;
        let destination_shift = x % 8;
        let width_usize =
            usize::try_from(width).map_err(|_error| MalformedError::IntegerOverflow)?;
        let full_bytes = width_usize / 8;
        let tail_bits =
            u32::try_from(width_usize % 8).map_err(|_error| MalformedError::IntegerOverflow)?;
        #[cfg(verus_keep_ghost)]
        proof! {
            vstd::arithmetic::div_mod::lemma_mod_bound(x as int, 8);
            vstd::arithmetic::div_mod::lemma_mod_bound(width_usize as int, 8);
            assert(destination_shift < 8);
            assert(tail_bits < 8);
        }
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                crate::spec::bitmap::storage_valid(source),
                self.width == old(self).width,
                self.height == old(self).height,
                self.stride == old(self).stride,
                source_y <= height,
                height <= source.height,
                width <= source.width,
                destination_shift < 8,
                tail_bits < 8,
                height > 0 ==> y < self.height,
                width > 0 ==> x < self.width,
                height > 0 ==> y as nat + height as nat <= self.height as nat,
                width > 0 ==> x as nat + width as nat <= self.width as nat,
            decreases height - source_y,
        ))]
        for source_y in 0..height {
            let source_row = usize::try_from(source_y)
                .map_err(|_error| MalformedError::IntegerOverflow)?
                .checked_mul(source.stride)
                .ok_or(MalformedError::IntegerOverflow)?;
            let destination_row = usize::try_from(
                y.checked_add(source_y)
                    .ok_or(MalformedError::IntegerOverflow)?,
            )
            .map_err(|_error| MalformedError::IntegerOverflow)?
            .checked_mul(self.stride)
            .and_then(|offset| offset.checked_add(destination_column))
            .ok_or(MalformedError::IntegerOverflow)?;

            if destination_shift == 0 {
                let mut byte = 0_usize;
                #[cfg_attr(verus_keep_ghost, verus_spec(
                    invariant byte <= full_bytes,
                    decreases full_bytes - byte,
                ))]
                while full_bytes - byte >= 8 {
                    let source_index = source_row
                        .checked_add(byte)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    let source_chunk = read_chunk(&source.data, source_index)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    let destination_index = destination_row
                        .checked_add(byte)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    let destination_chunk = read_chunk(&self.data, destination_index)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    if !write_chunk(
                        &mut self.data,
                        destination_index,
                        operator.apply_chunk(destination_chunk, source_chunk),
                    ) {
                        return Err(MalformedError::IntegerOverflow);
                    }
                    byte = byte.checked_add(8).ok_or(MalformedError::IntegerOverflow)?;
                }
                #[cfg_attr(verus_keep_ghost, verus_spec(
                    invariant byte <= full_bytes,
                    decreases full_bytes - byte,
                ))]
                while byte < full_bytes {
                    let source_index = source_row
                        .checked_add(byte)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    let source_value = *source
                        .data
                        .get(source_index)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    let destination_index = destination_row
                        .checked_add(byte)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    let destination_value = *self
                        .data
                        .get(destination_index)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    let destination_byte = self
                        .data
                        .get_mut(destination_index)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    *destination_byte = operator.apply_byte(destination_value, source_value);
                    byte = byte.checked_add(1).ok_or(MalformedError::IntegerOverflow)?;
                }
                if tail_bits != 0 {
                    let mask = 0xFF_u8 << (8 - tail_bits);
                    let source_index = source_row
                        .checked_add(full_bytes)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    let source_value = *source
                        .data
                        .get(source_index)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    let destination_index = destination_row
                        .checked_add(full_bytes)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    let destination_value = *self
                        .data
                        .get(destination_index)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    let combined = operator.apply_byte(destination_value, source_value);
                    let destination_byte = self
                        .data
                        .get_mut(destination_index)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    *destination_byte = (destination_value & !mask) | (combined & mask);
                }
            } else {
                let head_bits = width.min(8 - destination_shift);
                let head_mask = (0xFF_u8 >> destination_shift)
                    & (0xFF_u8 << (8 - destination_shift - head_bits));
                let destination_value = *self
                    .data
                    .get(destination_row)
                    .ok_or(MalformedError::IntegerOverflow)?;
                let source_value = *source
                    .data
                    .get(source_row)
                    .ok_or(MalformedError::IntegerOverflow)?
                    >> destination_shift;
                let combined = operator.apply_byte(destination_value, source_value);
                let destination_byte = self
                    .data
                    .get_mut(destination_row)
                    .ok_or(MalformedError::IntegerOverflow)?;
                *destination_byte = (destination_value & !head_mask) | (combined & head_mask);

                let mut source_bit = head_bits;
                let mut destination_index = destination_row
                    .checked_add(1)
                    .ok_or(MalformedError::IntegerOverflow)?;
                #[cfg_attr(verus_keep_ghost, verus_spec(
                    invariant head_bits <= source_bit <= width,
                    decreases width - source_bit,
                ))]
                while width - source_bit >= 64 {
                    let source_byte = source_row
                        .checked_add(
                            usize::try_from(source_bit / 8)
                                .map_err(|_error| MalformedError::IntegerOverflow)?,
                        )
                        .ok_or(MalformedError::IntegerOverflow)?;
                    let shift = source_bit % 8;
                    let source_chunk = read_shifted_chunk(&source.data, source_byte, shift)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    let destination_chunk = read_chunk(&self.data, destination_index)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    if !write_chunk(
                        &mut self.data,
                        destination_index,
                        operator.apply_chunk(destination_chunk, source_chunk),
                    ) {
                        return Err(MalformedError::IntegerOverflow);
                    }
                    source_bit = source_bit
                        .checked_add(64)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    destination_index = destination_index
                        .checked_add(8)
                        .ok_or(MalformedError::IntegerOverflow)?;
                }
                #[cfg_attr(verus_keep_ghost, verus_spec(
                    invariant head_bits <= source_bit <= width,
                    decreases width - source_bit,
                ))]
                while width - source_bit >= 8 {
                    let source_byte = source_row
                        .checked_add(
                            usize::try_from(source_bit / 8)
                                .map_err(|_error| MalformedError::IntegerOverflow)?,
                        )
                        .ok_or(MalformedError::IntegerOverflow)?;
                    let source_value = read_shifted_byte(&source.data, source_byte, source_bit % 8)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    let destination_value = *self
                        .data
                        .get(destination_index)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    let destination_byte = self
                        .data
                        .get_mut(destination_index)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    *destination_byte = operator.apply_byte(destination_value, source_value);
                    source_bit = source_bit
                        .checked_add(8)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    destination_index = destination_index
                        .checked_add(1)
                        .ok_or(MalformedError::IntegerOverflow)?;
                }
                let remaining = width - source_bit;
                if remaining != 0 {
                    let source_byte = source_row
                        .checked_add(
                            usize::try_from(source_bit / 8)
                                .map_err(|_error| MalformedError::IntegerOverflow)?,
                        )
                        .ok_or(MalformedError::IntegerOverflow)?;
                    let source_value =
                        read_shifted_tail(&source.data, source_byte, source_bit % 8, remaining)
                            .ok_or(MalformedError::IntegerOverflow)?;
                    let destination_value = *self
                        .data
                        .get(destination_index)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    let mask = 0xFF_u8 << (8 - remaining);
                    let combined = operator.apply_byte(destination_value, source_value);
                    let destination_byte = self
                        .data
                        .get_mut(destination_index)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    *destination_byte = (destination_value & !mask) | (combined & mask);
                }
            }
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::views::bitmap_fields(self);
            crate::views::bitmap_fields(old(self));
        }
        Ok(())
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            crate::spec::bitmap::storage_valid(old(self)),
            crate::spec::bitmap::storage_valid(source),
        ensures
            crate::spec::bitmap::storage_valid(final(self)),
            final(self).width == old(self).width,
            final(self).height == old(self).height,
            final(self).stride == old(self).stride,
            final(self).data@.len() == old(self).data@.len(),
            !crate::spec::bitmap::region_request_valid(
                old(self), source, source_x, source_y, destination_x, destination_y, width, height,
            ) ==> matches!(out, Err(MalformedError::IntegerOverflow)),
            (!crate::spec::bitmap::region_request_valid(
                old(self), source, source_x, source_y, destination_x, destination_y, width, height,
            ) || width == 0 || height == 0) ==> final(self).data@ == old(self).data@,
            (crate::spec::bitmap::region_request_valid(
                old(self), source, source_x, source_y, destination_x, destination_y, width, height,
            ) && (width == 0 || height == 0)) ==> out.is_ok(),
    ))]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn compose_region(
        &mut self,
        source: &Self,
        source_x: u32,
        source_y: u32,
        destination_x: u32,
        destination_y: u32,
        width: u32,
        height: u32,
        operator: CombinationOperator,
    ) -> Result<(), MalformedError> {
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::views::bitmap_fields(self);
            crate::views::bitmap_fields(source);
            crate::views::bitmap_fields(old(self));
        }
        let Some(source_end_x) = source_x.checked_add(width) else {
            return Err(MalformedError::IntegerOverflow);
        };
        let Some(source_end_y) = source_y.checked_add(height) else {
            return Err(MalformedError::IntegerOverflow);
        };
        let Some(destination_end_x) = destination_x.checked_add(width) else {
            return Err(MalformedError::IntegerOverflow);
        };
        let Some(destination_end_y) = destination_y.checked_add(height) else {
            return Err(MalformedError::IntegerOverflow);
        };
        if source_end_x > source.width
            || source_end_y > source.height
            || destination_end_x > self.width
            || destination_end_y > self.height
        {
            return Err(MalformedError::IntegerOverflow);
        }
        let result = self.compose_region_chunked(
            source,
            source_x,
            source_y,
            destination_x,
            destination_y,
            width,
            height,
            operator,
        );
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::views::bitmap_fields(self);
            crate::views::bitmap_fields(old(self));
        }
        result
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            crate::spec::bitmap::storage_valid(old(self)),
            crate::spec::bitmap::storage_valid(source),
            source_x as nat + width as nat <= source.width as nat,
            source_y as nat + height as nat <= source.height as nat,
            destination_x as nat + width as nat <= old(self).width as nat,
            destination_y as nat + height as nat <= old(self).height as nat,
        ensures
            final(self).width == old(self).width,
            final(self).height == old(self).height,
            final(self).stride == old(self).stride,
            final(self).data@.len() == old(self).data@.len(),
            (width == 0 || height == 0) ==> final(self).data@ == old(self).data@,
            (width == 0 || height == 0) ==> out.is_ok(),
    ))]
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn compose_region_chunked(
        &mut self,
        source: &Self,
        source_x: u32,
        source_y: u32,
        destination_x: u32,
        destination_y: u32,
        width: u32,
        height: u32,
        operator: CombinationOperator,
    ) -> Result<(), MalformedError> {
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::views::bitmap_fields(self);
            crate::views::bitmap_fields(source);
        }
        let Some(source_end_x) = source_x.checked_add(width) else {
            return Err(MalformedError::IntegerOverflow);
        };
        let Some(source_end_y) = source_y.checked_add(height) else {
            return Err(MalformedError::IntegerOverflow);
        };
        let Some(destination_end_x) = destination_x.checked_add(width) else {
            return Err(MalformedError::IntegerOverflow);
        };
        let Some(destination_end_y) = destination_y.checked_add(height) else {
            return Err(MalformedError::IntegerOverflow);
        };
        if source_end_x > source.width
            || source_end_y > source.height
            || destination_end_x > self.width
            || destination_end_y > self.height
        {
            return Err(MalformedError::IntegerOverflow);
        }
        if width == 0 || height == 0 {
            return Ok(());
        }
        let destination_column =
            usize::try_from(destination_x / 8).map_err(|_error| MalformedError::IntegerOverflow)?;
        let destination_shift = destination_x % 8;
        #[cfg(verus_keep_ghost)]
        proof! {
            vstd::arithmetic::div_mod::lemma_mod_bound(destination_x as int, 8);
            assert(destination_shift < 8);
        }
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                crate::spec::bitmap::storage_valid(source),
                self.width == old(self).width,
                self.height == old(self).height,
                self.stride == old(self).stride,
                self.data@.len() == old(self).data@.len(),
                row <= height,
                width > 0,
                height > 0,
                destination_shift < 8,
                source_x as nat + width as nat <= source.width as nat,
                source_y as nat + height as nat <= source.height as nat,
                destination_x as nat + width as nat <= self.width as nat,
                destination_y as nat + height as nat <= self.height as nat,
            decreases height - row,
        ))]
        for row in 0..height {
            let source_row = usize::try_from(
                source_y
                    .checked_add(row)
                    .ok_or(MalformedError::IntegerOverflow)?,
            )
            .map_err(|_error| MalformedError::IntegerOverflow)?
            .checked_mul(source.stride)
            .ok_or(MalformedError::IntegerOverflow)?;
            let destination_row = usize::try_from(
                destination_y
                    .checked_add(row)
                    .ok_or(MalformedError::IntegerOverflow)?,
            )
            .map_err(|_error| MalformedError::IntegerOverflow)?
            .checked_mul(self.stride)
            .and_then(|offset| offset.checked_add(destination_column))
            .ok_or(MalformedError::IntegerOverflow)?;

            let mut source_bit = source_x;
            let mut remaining = width;
            let mut destination_index = destination_row;
            if destination_shift != 0 {
                let head_bits = remaining.min(8 - destination_shift);
                let source_byte = source_row
                    .checked_add(
                        usize::try_from(source_bit / 8)
                            .map_err(|_error| MalformedError::IntegerOverflow)?,
                    )
                    .ok_or(MalformedError::IntegerOverflow)?;
                let source_value =
                    read_shifted_tail(&source.data, source_byte, source_bit % 8, head_bits)
                        .ok_or(MalformedError::IntegerOverflow)?
                        >> destination_shift;
                let destination_value = *self
                    .data
                    .get(destination_index)
                    .ok_or(MalformedError::IntegerOverflow)?;
                let mask = (0xFF_u8 >> destination_shift)
                    & (0xFF_u8 << (8 - destination_shift - head_bits));
                let combined = operator.apply_byte(destination_value, source_value);
                let destination_byte = self
                    .data
                    .get_mut(destination_index)
                    .ok_or(MalformedError::IntegerOverflow)?;
                *destination_byte = (destination_value & !mask) | (combined & mask);
                source_bit = source_bit
                    .checked_add(head_bits)
                    .ok_or(MalformedError::IntegerOverflow)?;
                remaining = remaining
                    .checked_sub(head_bits)
                    .ok_or(MalformedError::IntegerOverflow)?;
                destination_index = destination_index
                    .checked_add(1)
                    .ok_or(MalformedError::IntegerOverflow)?;
            }

            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant
                    remaining <= width,
                    self.data@.len() == old(self).data@.len(),
                    width > 0,
                    height > 0,
                decreases remaining,
            ))]
            while remaining >= 64 {
                let source_byte = source_row
                    .checked_add(
                        usize::try_from(source_bit / 8)
                            .map_err(|_error| MalformedError::IntegerOverflow)?,
                    )
                    .ok_or(MalformedError::IntegerOverflow)?;
                let source_chunk = read_shifted_chunk(&source.data, source_byte, source_bit % 8)
                    .ok_or(MalformedError::IntegerOverflow)?;
                let destination_chunk = read_chunk(&self.data, destination_index)
                    .ok_or(MalformedError::IntegerOverflow)?;
                if !write_chunk(
                    &mut self.data,
                    destination_index,
                    operator.apply_chunk(destination_chunk, source_chunk),
                ) {
                    return Err(MalformedError::IntegerOverflow);
                }
                source_bit = source_bit
                    .checked_add(64)
                    .ok_or(MalformedError::IntegerOverflow)?;
                remaining -= 64;
                destination_index = destination_index
                    .checked_add(8)
                    .ok_or(MalformedError::IntegerOverflow)?;
            }
            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant
                    remaining <= width,
                    self.data@.len() == old(self).data@.len(),
                    width > 0,
                    height > 0,
                decreases remaining,
            ))]
            while remaining >= 8 {
                let source_byte = source_row
                    .checked_add(
                        usize::try_from(source_bit / 8)
                            .map_err(|_error| MalformedError::IntegerOverflow)?,
                    )
                    .ok_or(MalformedError::IntegerOverflow)?;
                let source_value = read_shifted_byte(&source.data, source_byte, source_bit % 8)
                    .ok_or(MalformedError::IntegerOverflow)?;
                let destination_value = *self
                    .data
                    .get(destination_index)
                    .ok_or(MalformedError::IntegerOverflow)?;
                let destination_byte = self
                    .data
                    .get_mut(destination_index)
                    .ok_or(MalformedError::IntegerOverflow)?;
                *destination_byte = operator.apply_byte(destination_value, source_value);
                source_bit = source_bit
                    .checked_add(8)
                    .ok_or(MalformedError::IntegerOverflow)?;
                remaining -= 8;
                destination_index = destination_index
                    .checked_add(1)
                    .ok_or(MalformedError::IntegerOverflow)?;
            }
            if remaining != 0 {
                let source_byte = source_row
                    .checked_add(
                        usize::try_from(source_bit / 8)
                            .map_err(|_error| MalformedError::IntegerOverflow)?,
                    )
                    .ok_or(MalformedError::IntegerOverflow)?;
                let source_value =
                    read_shifted_tail(&source.data, source_byte, source_bit % 8, remaining)
                        .ok_or(MalformedError::IntegerOverflow)?;
                let destination_value = *self
                    .data
                    .get(destination_index)
                    .ok_or(MalformedError::IntegerOverflow)?;
                let mask = 0xFF_u8 << (8 - remaining);
                let combined = operator.apply_byte(destination_value, source_value);
                let destination_byte = self
                    .data
                    .get_mut(destination_index)
                    .ok_or(MalformedError::IntegerOverflow)?;
                *destination_byte = (destination_value & !mask) | (combined & mask);
            }
        }
        Ok(())
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            destination_shift < 8,
        ensures
            final(self).width == old(self).width,
            final(self).height == old(self).height,
            final(self).stride == old(self).stride,
            final(self).data@.len() == old(self).data@.len(),
    ))]
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn compose_row_blit_chunked(
        &mut self,
        source: &Self,
        source_row: usize,
        destination_row: usize,
        source_x: u32,
        destination_shift: u32,
        width: u32,
        operator: CombinationOperator,
    ) -> Result<(), MalformedError> {
        let mut source_bit = source_x;
        let mut remaining = width;
        let mut destination_index = destination_row;
        if destination_shift != 0 {
            let head_bits = remaining.min(8 - destination_shift);
            let source_byte = source_row
                .checked_add(
                    usize::try_from(source_bit / 8)
                        .map_err(|_error| MalformedError::IntegerOverflow)?,
                )
                .ok_or(MalformedError::IntegerOverflow)?;
            let source_value =
                read_shifted_tail(&source.data, source_byte, source_bit % 8, head_bits)
                    .ok_or(MalformedError::IntegerOverflow)?
                    >> destination_shift;
            let destination_value = *self
                .data
                .get(destination_index)
                .ok_or(MalformedError::IntegerOverflow)?;
            let mask =
                (0xFF_u8 >> destination_shift) & (0xFF_u8 << (8 - destination_shift - head_bits));
            let combined = operator.apply_byte(destination_value, source_value);
            let destination_byte = self
                .data
                .get_mut(destination_index)
                .ok_or(MalformedError::IntegerOverflow)?;
            *destination_byte = (destination_value & !mask) | (combined & mask);
            #[cfg(verus_keep_ghost)]
            proof! {
                crate::views::bitmap_fields(self);
                crate::views::bitmap_fields(old(self));
                reveal(crate::spec::bitmap::storage_valid);
            }
            source_bit = source_bit
                .checked_add(head_bits)
                .ok_or(MalformedError::IntegerOverflow)?;
            remaining = remaining
                .checked_sub(head_bits)
                .ok_or(MalformedError::IntegerOverflow)?;
            destination_index = destination_index
                .checked_add(1)
                .ok_or(MalformedError::IntegerOverflow)?;
        }
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                self.width == old(self).width,
                self.height == old(self).height,
                self.stride == old(self).stride,
                self.data@.len() == old(self).data@.len(),
                remaining <= width,
            decreases remaining,
        ))]
        while remaining >= 64 {
            let source_byte = source_row
                .checked_add(
                    usize::try_from(source_bit / 8)
                        .map_err(|_error| MalformedError::IntegerOverflow)?,
                )
                .ok_or(MalformedError::IntegerOverflow)?;
            let source_chunk = read_shifted_chunk(&source.data, source_byte, source_bit % 8)
                .ok_or(MalformedError::IntegerOverflow)?;
            let destination_chunk =
                read_chunk(&self.data, destination_index).ok_or(MalformedError::IntegerOverflow)?;
            if !write_chunk(
                &mut self.data,
                destination_index,
                operator.apply_chunk(destination_chunk, source_chunk),
            ) {
                #[cfg(verus_keep_ghost)]
                proof! {
                    crate::views::bitmap_fields(self);
                    crate::views::bitmap_fields(old(self));
                    reveal(crate::spec::bitmap::storage_valid);
                }
                return Err(MalformedError::IntegerOverflow);
            }
            #[cfg(verus_keep_ghost)]
            proof! {
                crate::views::bitmap_fields(self);
                crate::views::bitmap_fields(old(self));
                reveal(crate::spec::bitmap::storage_valid);
            }
            source_bit = source_bit
                .checked_add(64)
                .ok_or(MalformedError::IntegerOverflow)?;
            remaining -= 64;
            destination_index = destination_index
                .checked_add(8)
                .ok_or(MalformedError::IntegerOverflow)?;
        }
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                self.width == old(self).width,
                self.height == old(self).height,
                self.stride == old(self).stride,
                self.data@.len() == old(self).data@.len(),
                remaining <= width,
            decreases remaining,
        ))]
        while remaining >= 8 {
            let source_byte = source_row
                .checked_add(
                    usize::try_from(source_bit / 8)
                        .map_err(|_error| MalformedError::IntegerOverflow)?,
                )
                .ok_or(MalformedError::IntegerOverflow)?;
            let source_value = read_shifted_byte(&source.data, source_byte, source_bit % 8)
                .ok_or(MalformedError::IntegerOverflow)?;
            let destination_value = *self
                .data
                .get(destination_index)
                .ok_or(MalformedError::IntegerOverflow)?;
            let destination_byte = self
                .data
                .get_mut(destination_index)
                .ok_or(MalformedError::IntegerOverflow)?;
            *destination_byte = operator.apply_byte(destination_value, source_value);
            #[cfg(verus_keep_ghost)]
            proof! {
                crate::views::bitmap_fields(self);
                crate::views::bitmap_fields(old(self));
                reveal(crate::spec::bitmap::storage_valid);
            }
            source_bit = source_bit
                .checked_add(8)
                .ok_or(MalformedError::IntegerOverflow)?;
            remaining -= 8;
            destination_index = destination_index
                .checked_add(1)
                .ok_or(MalformedError::IntegerOverflow)?;
        }
        if remaining != 0 {
            let source_byte = source_row
                .checked_add(
                    usize::try_from(source_bit / 8)
                        .map_err(|_error| MalformedError::IntegerOverflow)?,
                )
                .ok_or(MalformedError::IntegerOverflow)?;
            let source_value =
                read_shifted_tail(&source.data, source_byte, source_bit % 8, remaining)
                    .ok_or(MalformedError::IntegerOverflow)?;
            let destination_value = *self
                .data
                .get(destination_index)
                .ok_or(MalformedError::IntegerOverflow)?;
            let mask = 0xFF_u8 << (8 - remaining);
            let combined = operator.apply_byte(destination_value, source_value);
            let destination_byte = self
                .data
                .get_mut(destination_index)
                .ok_or(MalformedError::IntegerOverflow)?;
            *destination_byte = (destination_value & !mask) | (combined & mask);
            #[cfg(verus_keep_ghost)]
            proof! {
                crate::views::bitmap_fields(self);
                reveal(crate::spec::bitmap::storage_valid);
            }
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::views::bitmap_fields(self);
            crate::views::bitmap_fields(old(self));
            reveal(crate::spec::bitmap::storage_valid);
        }
        Ok(())
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures
            final(self).width == old(self).width,
            final(self).height == old(self).height,
            final(self).stride == old(self).stride,
            final(self).data@.len() == old(self).data@.len(),
    ))]
    fn compose_aligned_row_chunked(
        &mut self,
        source: &Self,
        mut source_index: usize,
        mut destination_index: usize,
        mut byte_count: usize,
        operator: CombinationOperator,
    ) -> Result<(), MalformedError> {
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                self.width == old(self).width,
                self.height == old(self).height,
                self.stride == old(self).stride,
                self.data@.len() == old(self).data@.len(),
            decreases byte_count,
        ))]
        while byte_count >= 8 {
            let source_chunk =
                read_chunk(&source.data, source_index).ok_or(MalformedError::IntegerOverflow)?;
            let destination_chunk =
                read_chunk(&self.data, destination_index).ok_or(MalformedError::IntegerOverflow)?;
            if !write_chunk(
                &mut self.data,
                destination_index,
                operator.apply_chunk(destination_chunk, source_chunk),
            ) {
                #[cfg(verus_keep_ghost)]
                proof! {
                    crate::views::bitmap_fields(self);
                    crate::views::bitmap_fields(old(self));
                    reveal(crate::spec::bitmap::storage_valid);
                }
                return Err(MalformedError::IntegerOverflow);
            }
            #[cfg(verus_keep_ghost)]
            proof! {
                crate::views::bitmap_fields(self);
                crate::views::bitmap_fields(old(self));
                reveal(crate::spec::bitmap::storage_valid);
            }
            source_index = source_index
                .checked_add(8)
                .ok_or(MalformedError::IntegerOverflow)?;
            destination_index = destination_index
                .checked_add(8)
                .ok_or(MalformedError::IntegerOverflow)?;
            byte_count -= 8;
        }
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                self.width == old(self).width,
                self.height == old(self).height,
                self.stride == old(self).stride,
                self.data@.len() == old(self).data@.len(),
            decreases byte_count,
        ))]
        while byte_count != 0 {
            let source_value = *source
                .data
                .get(source_index)
                .ok_or(MalformedError::IntegerOverflow)?;
            let destination_value = *self
                .data
                .get(destination_index)
                .ok_or(MalformedError::IntegerOverflow)?;
            let destination_byte = self
                .data
                .get_mut(destination_index)
                .ok_or(MalformedError::IntegerOverflow)?;
            *destination_byte = operator.apply_byte(destination_value, source_value);
            #[cfg(verus_keep_ghost)]
            proof! {
                crate::views::bitmap_fields(self);
                crate::views::bitmap_fields(old(self));
                reveal(crate::spec::bitmap::storage_valid);
            }
            source_index = source_index
                .checked_add(1)
                .ok_or(MalformedError::IntegerOverflow)?;
            destination_index = destination_index
                .checked_add(1)
                .ok_or(MalformedError::IntegerOverflow)?;
            byte_count -= 1;
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::views::bitmap_fields(self);
            crate::views::bitmap_fields(old(self));
            reveal(crate::spec::bitmap::storage_valid);
        }
        Ok(())
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures
            final(self).width == old(self).width,
            final(self).height == old(self).height,
            final(self).stride == old(self).stride,
            final(self).data@.len() == old(self).data@.len(),
            height == 0 ==> final(self).data@ == old(self).data@,
            height == 0 ==> out.is_ok(),
    ))]
    pub(crate) fn compose_row_batch_chunked(
        &mut self,
        blits: &[RowBlit<'_>],
        source_y: u32,
        destination_y: u32,
        height: u32,
        operator: CombinationOperator,
    ) -> Result<(), MalformedError> {
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                row <= height,
                self.width == old(self).width,
                self.height == old(self).height,
                self.stride == old(self).stride,
                self.data@.len() == old(self).data@.len(),
                height == 0 ==> self.data@ == old(self).data@,
            decreases height - row,
        ))]
        for row in 0..height {
            let destination_base = usize::try_from(
                destination_y
                    .checked_add(row)
                    .ok_or(MalformedError::IntegerOverflow)?,
            )
            .map_err(|_error| MalformedError::IntegerOverflow)?
            .checked_mul(self.stride)
            .ok_or(MalformedError::IntegerOverflow)?;
            let mut blit_index = 0_usize;
            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant
                    blit_index <= blits@.len(),
                    self.width == old(self).width,
                    self.height == old(self).height,
                    self.stride == old(self).stride,
                    self.data@.len() == old(self).data@.len(),
                    height > 0,
                decreases blits@.len() - blit_index,
            ))]
            while blit_index < blits.len() {
                let blit = &blits[blit_index];
                let source_row = usize::try_from(
                    source_y
                        .checked_add(row)
                        .ok_or(MalformedError::IntegerOverflow)?,
                )
                .map_err(|_error| MalformedError::IntegerOverflow)?
                .checked_mul(blit.source.stride)
                .ok_or(MalformedError::IntegerOverflow)?;
                let destination_column = usize::try_from(blit.destination_x / 8)
                    .map_err(|_error| MalformedError::IntegerOverflow)?;
                let destination_row = destination_base
                    .checked_add(destination_column)
                    .ok_or(MalformedError::IntegerOverflow)?;
                if blit.source_x % 8 == 0 && blit.destination_x % 8 == 0 && blit.width % 8 == 0 {
                    let source_index = source_row
                        .checked_add(
                            usize::try_from(blit.source_x / 8)
                                .map_err(|_error| MalformedError::IntegerOverflow)?,
                        )
                        .ok_or(MalformedError::IntegerOverflow)?;
                    self.compose_aligned_row_chunked(
                        blit.source,
                        source_index,
                        destination_row,
                        usize::try_from(blit.width / 8)
                            .map_err(|_error| MalformedError::IntegerOverflow)?,
                        operator,
                    )?;
                } else {
                    self.compose_row_blit_chunked(
                        blit.source,
                        source_row,
                        destination_row,
                        blit.source_x,
                        blit.destination_x % 8,
                        blit.width,
                        operator,
                    )?;
                }
                blit_index += 1;
            }
        }
        Ok(())
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires crate::spec::bitmap::storage_valid(old(self)),
        ensures
            crate::views::bitmap_width(final(self))
                == crate::views::bitmap_width(old(self)),
            crate::views::bitmap_height(final(self))
                == crate::views::bitmap_height(old(self)),
            crate::views::bitmap_stride(final(self))
                == crate::views::bitmap_stride(old(self)),
            crate::spec::bitmap::storage_valid(final(self)),
            match out {
                Ok(()) =>
                    crate::spec::bitmap::coordinates_in_bounds(
                        crate::views::bitmap_width(old(self)),
                        crate::views::bitmap_height(old(self)),
                        x,
                        y,
                    )
                    && crate::views::bitmap_data(final(self)).len()
                        == crate::views::bitmap_data(old(self)).len()
                    && crate::spec::bitmap::stored_pixel(
                        final(self),
                        x,
                        y,
                    ) == value
                    && forall|index: int|
                        0 <= index
                            < crate::views::bitmap_data(old(self)).len()
                        ==> crate::views::bitmap_data(final(self))[index]
                            == if index
                                == crate::spec::bitmap::byte_index(
                                    crate::views::bitmap_stride(old(self)),
                                    x,
                                    y,
                                ) as int
                            {
                                crate::spec::bitmap::set_byte_bit(
                                    crate::views::bitmap_data(old(self))[index],
                                    x,
                                    value,
                                )
                            } else {
                                crate::views::bitmap_data(old(self))[index]
                            },
                Err(_) =>
                    !crate::spec::bitmap::coordinates_in_bounds(
                        crate::views::bitmap_width(old(self)),
                        crate::views::bitmap_height(old(self)),
                        x,
                        y,
                    )
                    && crate::views::bitmap_data(final(self))
                        == crate::views::bitmap_data(old(self)),
            },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn set_pixel(&mut self, x: u32, y: u32, value: bool) -> Result<(), MalformedError> {
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::views::bitmap_fields(self);
            reveal(crate::spec::bitmap::coordinates_in_bounds);
        }
        if x >= self.width || y >= self.height {
            return Err(MalformedError::IntegerOverflow);
        }
        #[cfg(verus_keep_ghost)]
        production_bitmap_in_bounds_byte!(self, x, y);
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal(crate::spec::bitmap::byte_index);
            assert(self.data@.len() == self.data.len());
            assert(self.data@.len() <= usize::MAX);
            assert(y as nat <= u32::MAX as nat);
            assert(u32::MAX as nat <= usize::MAX) by (compute);
            assert(y as nat * self.stride as nat <= usize::MAX);
        }
        let row = usize::try_from(y).map_err(|_error| MalformedError::IntegerOverflow)?;
        let row_offset = row
            .checked_mul(self.stride)
            .ok_or(MalformedError::IntegerOverflow)?;
        let column = usize::try_from(x / 8).map_err(|_error| MalformedError::IntegerOverflow)?;
        let byte_index = row_offset
            .checked_add(column)
            .ok_or(MalformedError::IntegerOverflow)?;
        if byte_index >= self.data.len() {
            return Err(MalformedError::IntegerOverflow);
        }
        let byte = &mut self.data[byte_index];
        let mask = pixel_mask(x);
        if value {
            *byte |= mask;
        } else {
            *byte &= !mask;
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::views::bitmap_fields(self);
            reveal(crate::spec::bitmap::set_byte_bit);
            reveal(crate::spec::bitmap::bit_mask);
            crate::bitmap::bitmap_shared::set_byte_reads_written_value(
                old(self).data@[
                    crate::spec::bitmap::byte_index(
                        old(self).stride,
                        x,
                        y,
                    ) as int
                ],
                x,
                value,
            );
            reveal(crate::spec::bitmap::stored_pixel);
            reveal(crate::spec::bitmap::byte_bit);
        }
        Ok(())
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) => actual <= limit,
        Err(error) =>
            actual > limit
            && error.kind == kind
            && error.limit == limit
            && error.actual == actual,
    },
))]
fn enforce_limit(kind: LimitKind, limit: u64, actual: u64) -> Result<(), LimitExceeded> {
    if actual > limit {
        return Err(LimitExceeded {
            kind,
            limit,
            actual,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::hint::black_box;
    use std::time::{Duration, Instant};

    fn scalar_compose(
        destination: &mut Bitmap,
        source: &Bitmap,
        x: u32,
        y: u32,
        operator: CombinationOperator,
    ) -> Result<(), MalformedError> {
        let width = source.width.min(destination.width.saturating_sub(x));
        let height = source.height.min(destination.height.saturating_sub(y));
        for source_y in 0..height {
            let destination_y = y
                .checked_add(source_y)
                .ok_or(MalformedError::IntegerOverflow)?;
            for source_x in 0..width {
                let destination_x = x
                    .checked_add(source_x)
                    .ok_or(MalformedError::IntegerOverflow)?;
                let source_value = source
                    .pixel(source_x, source_y)
                    .ok_or(MalformedError::IntegerOverflow)?;
                let destination_value = destination
                    .pixel(destination_x, destination_y)
                    .ok_or(MalformedError::IntegerOverflow)?;
                destination.set_pixel(
                    destination_x,
                    destination_y,
                    operator.apply(destination_value, source_value),
                )?;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn scalar_compose_region(
        destination: &mut Bitmap,
        source: &Bitmap,
        source_x: u32,
        source_y: u32,
        destination_x: u32,
        destination_y: u32,
        width: u32,
        height: u32,
        operator: CombinationOperator,
    ) -> Result<(), MalformedError> {
        if source_x
            .checked_add(width)
            .is_none_or(|end| end > source.width)
            || source_y
                .checked_add(height)
                .is_none_or(|end| end > source.height)
            || destination_x
                .checked_add(width)
                .is_none_or(|end| end > destination.width)
            || destination_y
                .checked_add(height)
                .is_none_or(|end| end > destination.height)
        {
            return Err(MalformedError::IntegerOverflow);
        }
        for row in 0..height {
            for column in 0..width {
                let sx = source_x + column;
                let sy = source_y + row;
                let dx = destination_x + column;
                let dy = destination_y + row;
                let incoming = source
                    .pixel(sx, sy)
                    .ok_or(MalformedError::IntegerOverflow)?;
                let current = destination
                    .pixel(dx, dy)
                    .ok_or(MalformedError::IntegerOverflow)?;
                destination.set_pixel(dx, dy, operator.apply(current, incoming))?;
            }
        }
        Ok(())
    }

    fn scalar_fill_black_span(
        bitmap: &mut Bitmap,
        row: u32,
        start: u32,
        end: u32,
    ) -> Result<(), MalformedError> {
        if start > end || end > bitmap.width || row >= bitmap.height {
            return Err(MalformedError::IntegerOverflow);
        }
        for x in start..end {
            bitmap.set_pixel(x, row, true)?;
        }
        Ok(())
    }

    fn scalar_copy_row(
        bitmap: &mut Bitmap,
        source: u32,
        destination: u32,
    ) -> Result<(), MalformedError> {
        if source >= bitmap.height || destination >= bitmap.height {
            return Err(MalformedError::IntegerOverflow);
        }
        if source == destination {
            return Ok(());
        }
        let source_start = usize::try_from(source)
            .map_err(|_error| MalformedError::IntegerOverflow)?
            * bitmap.stride;
        let destination_start = usize::try_from(destination)
            .map_err(|_error| MalformedError::IntegerOverflow)?
            * bitmap.stride;
        for index in 0..bitmap.stride {
            bitmap.data[destination_start + index] = bitmap.data[source_start + index];
        }
        Ok(())
    }

    fn patterned_bitmap(width: u32, height: u32, mut state: u64) -> Bitmap {
        let mut bitmap = Bitmap::blank(width, height, &Limits::default()).expect("bitmap fits");
        for y in 0..height {
            for x in 0..width {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                if state >> 63 != 0 {
                    bitmap.set_pixel(x, y, true).expect("pixel is in bounds");
                }
            }
        }
        bitmap
    }

    const OPERATOR_TRUTHS: [(CombinationOperator, [bool; 4]); 5] = [
        (CombinationOperator::Or, [false, true, true, true]),
        (CombinationOperator::And, [false, false, false, true]),
        (CombinationOperator::Xor, [false, true, true, false]),
        (CombinationOperator::Xnor, [true, false, false, true]),
        (CombinationOperator::Replace, [false, true, false, true]),
    ];

    #[test]
    fn byte_operators_match_all_independent_boolean_truth_tables() {
        for (operator, table) in OPERATOR_TRUTHS {
            for destination in 0..=u8::MAX {
                for source in 0..=u8::MAX {
                    let mut expected = 0;
                    for bit in 0..8 {
                        let index = 2 * usize::from((destination >> bit) & 1)
                            + usize::from((source >> bit) & 1);
                        if table[index] {
                            expected |= 1 << bit;
                        }
                    }
                    assert_eq!(operator.apply_byte(destination, source), expected);
                }
            }
        }
    }

    #[test]
    fn word_operators_match_independent_truth_tables_at_every_bit() {
        let mut values = vec![0, u64::MAX, 0x0123_4567_89ab_cdef, 0xaaaa_aaaa_aaaa_aaaa];
        for bit in 0..64 {
            values.push(1_u64 << bit);
            values.push(!(1_u64 << bit));
        }
        for (operator, table) in OPERATOR_TRUTHS {
            for &destination in &values {
                for &source in &values {
                    let mut expected = 0;
                    for bit in 0..64 {
                        let index = 2 * usize::try_from((destination >> bit) & 1).unwrap()
                            + usize::try_from((source >> bit) & 1).unwrap();
                        if table[index] {
                            expected |= 1_u64 << bit;
                        }
                    }
                    assert_eq!(operator.apply_chunk(destination, source), expected);
                }
            }
        }
    }

    #[test]
    fn blank_bitmap_has_expected_layout() {
        let bitmap = Bitmap::blank(9, 2, &Limits::default()).expect("bitmap should fit");
        assert_eq!(bitmap.width(), 9);
        assert_eq!(bitmap.height(), 2);
        assert_eq!(bitmap.stride(), 2);
        assert_eq!(bitmap.bytes(), &[0, 0, 0, 0]);
        assert_eq!(bitmap.pixel(8, 1), Some(false));
        assert_eq!(bitmap.pixel(9, 1), None);
    }

    #[test]
    fn bitmap_enforces_pixel_limit_before_allocation() {
        let limits = Limits {
            max_pixels_per_page: 8,
            ..Limits::default()
        };
        let error = Bitmap::blank(9, 1, &limits).expect_err("bitmap must exceed the limit");
        assert!(matches!(
            error,
            DecodeError::LimitExceeded(LimitExceeded {
                kind: LimitKind::PagePixels,
                limit: 8,
                actual: 9
            })
        ));
    }

    #[test]
    fn filled_bitmap_clears_padding_bits() {
        let bitmap = Bitmap::filled(9, 2, true, &Limits::default()).expect("bitmap should fit");
        assert_eq!(bitmap.bytes(), &[0xFF, 0x80, 0xFF, 0x80]);
        assert_eq!(bitmap.pixel(8, 1), Some(true));
    }

    #[test]
    fn composition_clips_and_applies_operators() {
        let limits = Limits::default();
        let source = Bitmap::filled(2, 2, true, &limits).expect("source should fit");
        let mut destination = Bitmap::blank(2, 2, &limits).expect("destination should fit");
        destination
            .compose(&source, 1, 1, CombinationOperator::Or)
            .expect("composition should succeed");
        assert_eq!(destination.pixel(0, 0), Some(false));
        assert_eq!(destination.pixel(1, 1), Some(true));

        destination
            .compose(&source, 0, 0, CombinationOperator::Xor)
            .expect("composition should succeed");
        assert_eq!(destination.pixel(0, 0), Some(true));
        assert_eq!(destination.pixel(1, 1), Some(false));
    }

    #[test]
    fn every_combination_operator_matches_boolean_definition() {
        let operators = [
            CombinationOperator::Or,
            CombinationOperator::And,
            CombinationOperator::Xor,
            CombinationOperator::Xnor,
            CombinationOperator::Replace,
        ];
        for operator in operators {
            for destination in [false, true] {
                for source in [false, true] {
                    let expected = match operator {
                        CombinationOperator::Or => destination || source,
                        CombinationOperator::And => destination && source,
                        CombinationOperator::Xor => destination != source,
                        CombinationOperator::Xnor => destination == source,
                        CombinationOperator::Replace => source,
                    };
                    assert_eq!(operator.apply(destination, source), expected);
                }
            }
        }
    }

    #[test]
    fn filled_layout_and_padding_are_deterministic() {
        let limits = Limits::default();
        for width in [0, 1, 7, 8, 9, 15, 16, 17] {
            for value in [false, true] {
                let bitmap = Bitmap::filled(width, 3, value, &limits).expect("bitmap fits");
                let expected_stride = usize::try_from(width.div_ceil(8)).expect("test width fits");
                assert_eq!(bitmap.stride(), expected_stride);
                assert_eq!(bitmap.bytes().len(), expected_stride * 3);
                for y in 0..bitmap.height() {
                    for x in 0..bitmap.width() {
                        assert_eq!(bitmap.pixel(x, y), Some(value));
                    }
                    if width % 8 != 0 {
                        let last = bitmap.bytes()[usize::try_from(y).expect("test row fits")
                            * expected_stride
                            + expected_stride
                            - 1];
                        let padding_mask = (1_u8 << (8 - width % 8)) - 1;
                        assert_eq!(last & padding_mask, 0);
                    }
                }
            }
        }
    }

    #[test]
    fn set_pixel_is_local_and_rejects_out_of_bounds() {
        let limits = Limits::default();
        let mut bitmap = Bitmap::blank(9, 2, &limits).expect("bitmap fits");
        bitmap.set_pixel(7, 0, true).expect("pixel is in bounds");
        assert_eq!(bitmap.bytes(), &[0x01, 0, 0, 0]);
        assert_eq!(bitmap.pixel(7, 0), Some(true));
        assert_eq!(bitmap.pixel(6, 0), Some(false));
        assert_eq!(bitmap.pixel(8, 0), Some(false));

        let before = bitmap.bytes().to_vec();
        assert!(matches!(
            bitmap.set_pixel(9, 0, true),
            Err(MalformedError::IntegerOverflow)
        ));
        assert_eq!(bitmap.bytes(), before);
    }

    #[test]
    fn compose_writes_only_the_clipped_destination() {
        let limits = Limits::default();
        let source = Bitmap::filled(3, 2, true, &limits).expect("source fits");
        let mut destination = Bitmap::blank(10, 3, &limits).expect("destination fits");
        destination
            .compose(&source, 8, 1, CombinationOperator::Replace)
            .expect("clipped composition succeeds");

        for y in 0..3 {
            for x in 0..10 {
                assert_eq!(
                    destination.pixel(x, y),
                    Some((y == 1 || y == 2) && (x == 8 || x == 9)),
                );
            }
        }
        for row in 0..3 {
            assert_eq!(destination.bytes()[row * 2 + 1] & 0x3f, 0);
        }

        let before = destination.bytes().to_vec();
        destination
            .compose(&source, 10, 0, CombinationOperator::Or)
            .expect("fully clipped composition succeeds");
        assert_eq!(destination.bytes(), before);
    }

    #[test]
    fn checked_layout_enforces_each_bitmap_limit() {
        let width_limits = Limits {
            max_width: 8,
            ..Limits::default()
        };
        assert!(matches!(
            checked_bitmap_layout(9, 1, &width_limits),
            Err(DecodeError::LimitExceeded(LimitExceeded {
                kind: LimitKind::Width,
                limit: 8,
                actual: 9,
            }))
        ));

        let memory_limits = Limits {
            max_decoder_managed_memory_bytes: 1,
            ..Limits::default()
        };
        assert!(matches!(
            checked_bitmap_layout(9, 1, &memory_limits),
            Err(DecodeError::LimitExceeded(LimitExceeded {
                kind: LimitKind::Memory,
                limit: 1,
                actual: 2,
            }))
        ));
    }

    #[test]
    fn chunk_composition_exhaustively_matches_scalar_oracle() {
        let operators = [
            CombinationOperator::Or,
            CombinationOperator::And,
            CombinationOperator::Xor,
            CombinationOperator::Xnor,
            CombinationOperator::Replace,
        ];
        for destination_width in 0_u32..=18 {
            for source_width in 0_u32..=18 {
                let destination_seed = u64::from(destination_width) << 32 | u64::from(source_width);
                let source = patterned_bitmap(source_width, 2, !destination_seed);
                for x in 0..=destination_width.saturating_add(1) {
                    for operator in operators {
                        let mut expected = patterned_bitmap(destination_width, 3, destination_seed);
                        let mut actual = expected.clone();
                        scalar_compose(&mut expected, &source, x, 1, operator)
                            .expect("scalar composition succeeds");
                        actual
                            .compose(&source, x, 1, operator)
                            .expect("chunk composition succeeds");
                        assert_eq!(
                            actual, expected,
                            "destination width {destination_width}, source width {source_width}, \
                             x {x}, operator {operator:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn chunk_composition_randomized_matches_scalar_oracle() {
        let operators = [
            CombinationOperator::Or,
            CombinationOperator::And,
            CombinationOperator::Xor,
            CombinationOperator::Xnor,
            CombinationOperator::Replace,
        ];
        let mut state = 0x4d59_5df4_d0f3_3173_u64;
        for case in 0..2_000_u64 {
            state = state
                .wrapping_mul(2_862_933_555_777_941_757)
                .wrapping_add(3_037_000_493);
            let destination_width =
                u32::try_from(state & u64::from(u32::MAX)).expect("masked value fits") % 521;
            let destination_height =
                u32::try_from((state >> 16) & u64::from(u32::MAX)).expect("masked value fits") % 9;
            state = state.rotate_left(29) ^ case;
            let source_width =
                u32::try_from(state & u64::from(u32::MAX)).expect("masked value fits") % 521;
            let source_height =
                u32::try_from((state >> 20) & u64::from(u32::MAX)).expect("masked value fits") % 9;
            let x = ((state >> 32) as u32) % destination_width.saturating_add(3).max(1);
            let y = ((state >> 44) as u32) % destination_height.saturating_add(3).max(1);
            let operator = operators
                [usize::try_from(state % operators.len() as u64).expect("operator index fits")];
            let source = patterned_bitmap(source_width, source_height, state ^ 0xa5a5_a5a5);
            let mut expected =
                patterned_bitmap(destination_width, destination_height, state ^ 0x5a5a_5a5a);
            let mut actual = expected.clone();
            let expected_result = scalar_compose(&mut expected, &source, x, y, operator);
            let actual_result = actual.compose(&source, x, y, operator);
            assert_eq!(actual_result, expected_result, "case {case}");
            assert_eq!(actual, expected, "case {case}");
        }
    }

    #[test]
    fn zero_area_region_requests_succeed_without_changing_storage() {
        let source = patterned_bitmap(17, 3, 17);
        let original = patterned_bitmap(25, 4, 29);
        for operator in [
            CombinationOperator::Or,
            CombinationOperator::And,
            CombinationOperator::Xor,
            CombinationOperator::Xnor,
            CombinationOperator::Replace,
        ] {
            for (sx, sy, dx, dy, width, height) in [
                (17, 0, 25, 0, 0, 3),
                (0, 3, 0, 4, 17, 0),
                (17, 3, 25, 4, 0, 0),
                (0, 0, 0, 0, 0, 0),
            ] {
                let mut destination = original.clone();
                destination
                    .compose_region(&source, sx, sy, dx, dy, width, height, operator)
                    .expect("valid zero-area request");
                assert_eq!(destination, original);
            }
        }
    }

    #[test]
    fn invalid_region_requests_preserve_exact_error_and_all_storage() {
        let source = patterned_bitmap(17, 3, 17);
        let original = patterned_bitmap(25, 4, 29);
        for operator in [
            CombinationOperator::Or,
            CombinationOperator::And,
            CombinationOperator::Xor,
            CombinationOperator::Xnor,
            CombinationOperator::Replace,
        ] {
            for (sx, sy, dx, dy, width, height) in [
                (u32::MAX, 0, 0, 0, 1, 1),
                (0, u32::MAX, 0, 0, 1, 1),
                (0, 0, u32::MAX, 0, 1, 1),
                (0, 0, 0, u32::MAX, 1, 1),
                (18, 0, 0, 0, 0, 1),
                (0, 4, 0, 0, 1, 0),
                (0, 0, 26, 0, 0, 1),
                (0, 0, 0, 5, 1, 0),
                (17, 0, 0, 0, 1, 1),
                (0, 3, 0, 0, 1, 1),
                (0, 0, 25, 0, 1, 1),
                (0, 0, 0, 4, 1, 1),
            ] {
                let mut destination = original.clone();
                assert!(matches!(
                    destination.compose_region(&source, sx, sy, dx, dy, width, height, operator),
                    Err(MalformedError::IntegerOverflow)
                ));
                assert_eq!(destination, original);
            }
        }
    }

    #[test]
    fn source_offset_region_composition_exhaustively_matches_scalar_oracle() {
        let operators = [
            CombinationOperator::Or,
            CombinationOperator::And,
            CombinationOperator::Xor,
            CombinationOperator::Xnor,
            CombinationOperator::Replace,
        ];
        for source_width in 0_u32..=18 {
            for destination_width in 0_u32..=18 {
                let source = patterned_bitmap(source_width, 3, u64::from(source_width));
                let original =
                    patterned_bitmap(destination_width, 4, !u64::from(destination_width));
                for source_x in 0..=source_width {
                    for destination_x in 0..=destination_width {
                        let max_width =
                            (source_width - source_x).min(destination_width - destination_x);
                        for width in 0..=max_width {
                            for operator in operators {
                                let mut expected = original.clone();
                                let mut actual = original.clone();
                                scalar_compose_region(
                                    &mut expected,
                                    &source,
                                    source_x,
                                    1,
                                    destination_x,
                                    1,
                                    width,
                                    2,
                                    operator,
                                )
                                .expect("scalar region composition succeeds");
                                actual
                                    .compose_region(
                                        &source,
                                        source_x,
                                        1,
                                        destination_x,
                                        1,
                                        width,
                                        2,
                                        operator,
                                    )
                                    .expect("chunk region composition succeeds");
                                assert_eq!(
                                    actual, expected,
                                    "source width {source_width}, destination width \
                                     {destination_width}, source x {source_x}, destination x \
                                     {destination_x}, width {width}, operator {operator:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn source_offset_region_composition_randomized_matches_scalar_oracle() {
        let operators = [
            CombinationOperator::Or,
            CombinationOperator::And,
            CombinationOperator::Xor,
            CombinationOperator::Xnor,
            CombinationOperator::Replace,
        ];
        let mut state = 0xd1b5_4a32_d192_ed03_u64;
        for case in 0..2_000_u64 {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let source_width =
                u32::try_from(state & u64::from(u16::MAX)).expect("masked value fits") % 521;
            let destination_width = u32::try_from((state >> 16) & u64::from(u16::MAX))
                .expect("masked value fits")
                % 521;
            let source_height = u32::try_from((state >> 32) & 15).expect("masked value fits");
            let destination_height = u32::try_from((state >> 40) & 15).expect("masked value fits");
            let source_x = if source_width == 0 {
                0
            } else {
                u32::try_from((state >> 8) & u64::from(u32::MAX)).expect("masked value fits")
                    % (source_width + 1)
            };
            let destination_x = if destination_width == 0 {
                0
            } else {
                u32::try_from((state >> 24) & u64::from(u32::MAX)).expect("masked value fits")
                    % (destination_width + 1)
            };
            let source_y = if source_height == 0 {
                0
            } else {
                u32::try_from((state >> 36) & u64::from(u32::MAX)).expect("masked value fits")
                    % (source_height + 1)
            };
            let destination_y = if destination_height == 0 {
                0
            } else {
                u32::try_from((state >> 44) & u64::from(u32::MAX)).expect("masked value fits")
                    % (destination_height + 1)
            };
            let width = u32::try_from((state >> 12) & u64::from(u32::MAX))
                .expect("masked value fits")
                % ((source_width - source_x).min(destination_width - destination_x) + 1);
            let height = u32::try_from((state >> 20) & u64::from(u32::MAX))
                .expect("masked value fits")
                % ((source_height - source_y).min(destination_height - destination_y) + 1);
            let operator = operators
                [usize::try_from(case % operators.len() as u64).expect("operator index fits")];
            let source = patterned_bitmap(source_width, source_height, state);
            let mut expected = patterned_bitmap(destination_width, destination_height, !state);
            let mut actual = expected.clone();
            scalar_compose_region(
                &mut expected,
                &source,
                source_x,
                source_y,
                destination_x,
                destination_y,
                width,
                height,
                operator,
            )
            .expect("scalar region composition succeeds");
            actual
                .compose_region(
                    &source,
                    source_x,
                    source_y,
                    destination_x,
                    destination_y,
                    width,
                    height,
                    operator,
                )
                .expect("chunk region composition succeeds");
            assert_eq!(actual, expected, "case {case}");
        }
    }

    #[test]
    fn prefix_copy_preserves_suffix_source_and_error_state() {
        for (source_height, destination_height) in [
            (0, 0),
            (0, 9),
            (9, 0),
            (3, 7),
            (7, 3),
            (66, 129),
            (129, 66),
            (257, 1024),
        ] {
            let source = patterned_bitmap(8, source_height, 17);
            let original = patterned_bitmap(8, destination_height, 29);
            let immutable = source.clone();
            let limit = source.data.len().min(original.data.len());
            for count in 0..=limit {
                let mut expected = original.clone();
                expected.data[..count].copy_from_slice(&source.data[..count]);
                let mut actual = original.clone();
                actual.copy_prefix_from(&source, count).unwrap();
                assert_eq!(
                    actual, expected,
                    "source {source_height}, destination {destination_height}, count {count}"
                );
                assert_eq!(source, immutable);
            }
            for count in [
                limit + 1,
                source.data.len().max(original.data.len()) + 1,
                usize::MAX,
            ] {
                let mut actual = original.clone();
                assert!(matches!(
                    actual.copy_prefix_from(&source, count),
                    Err(MalformedError::IntegerOverflow)
                ));
                assert_eq!(actual, original);
                assert_eq!(source, immutable);
            }
        }
    }

    #[test]
    fn blank_storage_is_zero_for_empty_and_partial_rows() {
        for width in [0, 1, 7, 8, 9, 63, 64, 65] {
            for height in [0, 1, 3] {
                let bitmap = Bitmap::blank(width, height, &Limits::default()).unwrap();
                assert_eq!((bitmap.width(), bitmap.height()), (width, height));
                assert!(bitmap.bytes().iter().all(|byte| *byte == 0));
                for y in 0..height {
                    for x in 0..width {
                        assert_eq!(bitmap.pixel(x, y), Some(false));
                    }
                }
            }
        }
    }

    #[test]
    fn chunk_values_match_independent_big_endian_oracles() {
        let mut values = vec![0, u64::MAX, 0x0123_4567_89ab_cdef];
        for offset in 0..8 {
            for byte in 0..=u8::MAX {
                values.push(u64::from(byte) << (offset * 8));
            }
        }
        for value in values {
            let encoded = value.to_be_bytes();
            for start in 0..=9 {
                let mut bytes = vec![0xa5; 25];
                let mut expected = bytes.clone();
                expected[start..start + 8].copy_from_slice(&encoded);
                assert!(write_chunk(&mut bytes, start, value));
                assert_eq!(bytes, expected);
                assert_eq!(read_chunk(&bytes, start), Some(value));
                assert_eq!(
                    read_chunk(&expected, start),
                    Some(u64::from_be_bytes(encoded)),
                );
            }
        }
    }

    #[test]
    fn chunk_access_accepts_exactly_complete_ranges_and_preserves_failed_writes() {
        for length in 0..=25 {
            let original: Vec<u8> = (0..length)
                .map(|index| u8::try_from((index * 37 + 19) % 256).expect("byte"))
                .collect();
            for start in (0..=length).chain([usize::MAX - 7, usize::MAX]) {
                let range = start
                    .checked_add(8)
                    .filter(|&end| end <= length)
                    .map(|end| start..end);
                let expected_read = range.as_ref().map(|range| {
                    u64::from_be_bytes(
                        original[range.clone()]
                            .try_into()
                            .expect("exactly eight bytes"),
                    )
                });
                assert_eq!(read_chunk(&original, start), expected_read);
                for value in [0, u64::MAX, 0x0123_4567_89ab_cdef] {
                    let mut bytes = original.clone();
                    let mut expected = original.clone();
                    if let Some(range) = &range {
                        expected[range.clone()].copy_from_slice(&value.to_be_bytes());
                    }
                    assert_eq!(write_chunk(&mut bytes, start, value), range.is_some());
                    assert_eq!(bytes, expected);
                }
            }
        }
    }

    #[test]
    fn row_copy_bytes_and_chunk_access_have_exact_boundaries() {
        for length in [0, 1, 2, 8, 64, 65, 66, 127, 128, 129, 257] {
            let source: Vec<u8> = (0..length)
                .map(|index| u8::try_from(index % 251).unwrap())
                .collect();
            let mut destination = vec![0xa5; length];
            copy_row_bytes(&mut destination, &source);
            assert_eq!(destination, source);
        }
        let mut bytes = vec![0x81; 8];
        let original = bytes.clone();
        for start in [1, usize::MAX - 7, usize::MAX] {
            assert_eq!(read_chunk(&bytes, start), None);
            assert!(!write_chunk(&mut bytes, start, u64::MAX));
            assert_eq!(bytes, original);
        }
        assert_eq!(read_shifted_byte(&bytes, 0, 8), None);
        assert_eq!(read_shifted_tail(&bytes, 0, 1, 0), None);
        assert_eq!(read_shifted_tail(&bytes, 0, 1, 9), None);
        assert_eq!(read_shifted_chunk(&bytes, 0, 8), None);
    }

    #[test]
    fn row_copy_rejects_invalid_indices_and_storage_without_mutation() {
        for (source, destination) in [(2, 0), (0, 2), (2, 2)] {
            let mut bitmap = Bitmap::blank(8, 2, &Limits::default()).unwrap();
            let original = bitmap.clone();
            assert!(matches!(
                bitmap.copy_row(source, destination),
                Err(MalformedError::IntegerOverflow)
            ));
            assert_eq!(bitmap, original);
        }
        for (stride, source, destination) in [
            (usize::MAX, 2, 0),
            (usize::MAX, 0, 2),
            (usize::MAX, 1, 0),
            (usize::MAX, 0, 1),
            (1, 1, 0),
            (1, 0, 1),
        ] {
            let mut bitmap = Bitmap {
                width: 8,
                height: 3,
                stride,
                data: vec![0xa5],
            };
            let original = bitmap.clone();
            assert!(matches!(
                bitmap.copy_row(source, destination),
                Err(MalformedError::IntegerOverflow)
            ));
            assert_eq!(bitmap, original);
        }
    }

    #[test]
    fn chunk_span_fill_and_row_copy_match_scalar_oracles() {
        for width in 0_u32..=130 {
            let original = patterned_bitmap(width, 4, u64::from(width));
            for row in 0_u32..4 {
                for start in 0..=width {
                    for end in start..=width {
                        let mut expected = original.clone();
                        let mut actual = original.clone();
                        scalar_fill_black_span(&mut expected, row, start, end)
                            .expect("scalar fill succeeds");
                        actual
                            .fill_black_span(row, start, end)
                            .expect("chunk fill succeeds");
                        assert_eq!(actual, expected, "width {width}, row {row}, {start}..{end}");
                    }
                }
                for destination in 0_u32..4 {
                    let mut expected = original.clone();
                    let mut actual = original.clone();
                    scalar_copy_row(&mut expected, row, destination).expect("scalar copy succeeds");
                    actual
                        .copy_row(row, destination)
                        .expect("chunk copy succeeds");
                    assert_eq!(
                        actual, expected,
                        "width {width}, source {row}, destination {destination}"
                    );
                }
                let source = patterned_bitmap(width, 4, !u64::from(width));
                for byte_count in 0..=original.data.len() {
                    let mut expected = original.clone();
                    let mut actual = original.clone();
                    expected.data[..byte_count].copy_from_slice(&source.data[..byte_count]);
                    actual
                        .copy_prefix_from(&source, byte_count)
                        .expect("chunk prefix copy succeeds");
                    assert_eq!(actual, expected, "width {width}, byte count {byte_count}");
                }
            }
        }
    }

    fn median_duration(mut values: Vec<Duration>) -> Duration {
        values.sort_unstable();
        values[values.len() / 2]
    }

    #[test]
    #[ignore = "microbenchmark; run explicitly with --release --ignored --nocapture"]
    fn benchmark_chunk_composition_against_fa117f0_scalar_oracle() {
        let operators = [
            CombinationOperator::Or,
            CombinationOperator::And,
            CombinationOperator::Xor,
            CombinationOperator::Xnor,
            CombinationOperator::Replace,
        ];
        let source = patterned_bitmap(1024, 512, 0x1234_5678_9abc_def0);
        let original = patterned_bitmap(1032, 512, 0xfedc_ba98_7654_3210);
        let mut chunk_times = Vec::new();
        let mut scalar_times = Vec::new();
        for sample in 0..11 {
            let mut chunk = original.clone();
            let mut scalar = original.clone();
            if sample % 2 == 0 {
                let start = Instant::now();
                for x in [0, 3] {
                    for operator in operators {
                        chunk
                            .compose(&source, x, 0, operator)
                            .expect("chunk composition succeeds");
                    }
                }
                chunk_times.push(start.elapsed());
                black_box(chunk.bytes());

                let start = Instant::now();
                for x in [0, 3] {
                    for operator in operators {
                        scalar_compose(&mut scalar, &source, x, 0, operator)
                            .expect("scalar composition succeeds");
                    }
                }
                scalar_times.push(start.elapsed());
                black_box(scalar.bytes());
            } else {
                let start = Instant::now();
                for x in [0, 3] {
                    for operator in operators {
                        scalar_compose(&mut scalar, &source, x, 0, operator)
                            .expect("scalar composition succeeds");
                    }
                }
                scalar_times.push(start.elapsed());
                black_box(scalar.bytes());

                let start = Instant::now();
                for x in [0, 3] {
                    for operator in operators {
                        chunk
                            .compose(&source, x, 0, operator)
                            .expect("chunk composition succeeds");
                    }
                }
                chunk_times.push(start.elapsed());
                black_box(chunk.bytes());
            }
            assert_eq!(chunk, scalar);
        }
        let chunk = median_duration(chunk_times);
        let scalar = median_duration(scalar_times);
        println!(
            "composition scalar_fa117f0_median_ns={} chunk_median_ns={} speedup={:.3}",
            scalar.as_nanos(),
            chunk.as_nanos(),
            scalar.as_secs_f64() / chunk.as_secs_f64(),
        );
    }

    #[test]
    #[ignore = "microbenchmark; run explicitly with --release --ignored --nocapture"]
    fn benchmark_source_offset_composition_against_6333205_scalar_oracle() {
        let operators = [
            CombinationOperator::Or,
            CombinationOperator::And,
            CombinationOperator::Xor,
            CombinationOperator::Xnor,
            CombinationOperator::Replace,
        ];
        let source = patterned_bitmap(1032, 512, 0x0ddc_0ffe_e15e_beef);
        let original = patterned_bitmap(1040, 512, 0x5ca1_ab1e_0123_4567);
        let mut chunk_times = Vec::new();
        let mut scalar_times = Vec::new();
        for sample in 0..11 {
            let mut chunk = original.clone();
            let mut scalar = original.clone();
            if sample % 2 == 0 {
                let start = Instant::now();
                for operator in operators {
                    chunk
                        .compose_region(&source, 3, 0, 5, 0, 1024, 512, operator)
                        .expect("chunk region composition succeeds");
                }
                chunk_times.push(start.elapsed());
                black_box(chunk.bytes());

                let start = Instant::now();
                for operator in operators {
                    scalar_compose_region(&mut scalar, &source, 3, 0, 5, 0, 1024, 512, operator)
                        .expect("scalar region composition succeeds");
                }
                scalar_times.push(start.elapsed());
                black_box(scalar.bytes());
            } else {
                let start = Instant::now();
                for operator in operators {
                    scalar_compose_region(&mut scalar, &source, 3, 0, 5, 0, 1024, 512, operator)
                        .expect("scalar region composition succeeds");
                }
                scalar_times.push(start.elapsed());
                black_box(scalar.bytes());

                let start = Instant::now();
                for operator in operators {
                    chunk
                        .compose_region(&source, 3, 0, 5, 0, 1024, 512, operator)
                        .expect("chunk region composition succeeds");
                }
                chunk_times.push(start.elapsed());
                black_box(chunk.bytes());
            }
            assert_eq!(chunk, scalar);
        }
        let chunk = median_duration(chunk_times);
        let scalar = median_duration(scalar_times);
        println!(
            "source-offset-composition scalar_6333205_median_ns={} chunk_median_ns={} \
             speedup={:.3}",
            scalar.as_nanos(),
            chunk.as_nanos(),
            scalar.as_secs_f64() / chunk.as_secs_f64(),
        );
    }
}

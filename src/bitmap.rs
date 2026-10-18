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
        ensures true,
    ))]
    const fn apply_byte(self, destination: u8, source: u8) -> u8 {
        match self {
            Self::Or => destination | source,
            Self::And => destination & source,
            Self::Xor => destination ^ source,
            Self::Xnor => !(destination ^ source),
            Self::Replace => source,
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures true,
    ))]
    const fn apply_chunk(self, destination: u64, source: u64) -> u64 {
        match self {
            Self::Or => destination | source,
            Self::And => destination & source,
            Self::Xor => destination ^ source,
            Self::Xnor => !(destination ^ source),
            Self::Replace => source,
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out.is_some() ==> start as nat + 8 <= bytes@.len(),
))]
fn read_chunk(bytes: &[u8], start: usize) -> Option<u64> {
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
))]
#[allow(clippy::cast_possible_truncation, clippy::ptr_arg)]
fn write_chunk(bytes: &mut Vec<u8>, start: usize, value: u64) -> bool {
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
            source < old(self).height,
            destination < old(self).height,
        ensures
            final(self).width == old(self).width,
            final(self).height == old(self).height,
            final(self).stride == old(self).stride,
    ))]
    pub(crate) fn copy_row(&mut self, source: u32, destination: u32) -> Result<(), MalformedError> {
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
        let mut index = 0_usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                self.width == old(self).width,
                self.height == old(self).height,
                self.stride == old(self).stride,
                index <= self.stride,
            decreases self.stride - index,
        ))]
        while self.stride - index >= 8 {
            let source_index = source_start
                .checked_add(index)
                .ok_or(MalformedError::IntegerOverflow)?;
            let destination_index = destination_start
                .checked_add(index)
                .ok_or(MalformedError::IntegerOverflow)?;
            let value =
                read_chunk(&self.data, source_index).ok_or(MalformedError::IntegerOverflow)?;
            if !write_chunk(&mut self.data, destination_index, value) {
                return Err(MalformedError::IntegerOverflow);
            }
            index = index
                .checked_add(8)
                .ok_or(MalformedError::IntegerOverflow)?;
        }
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                self.width == old(self).width,
                self.height == old(self).height,
                self.stride == old(self).stride,
                index <= self.stride,
            decreases self.stride - index,
        ))]
        while index < self.stride {
            let source_index = source_start
                .checked_add(index)
                .ok_or(MalformedError::IntegerOverflow)?;
            let destination_index = destination_start
                .checked_add(index)
                .ok_or(MalformedError::IntegerOverflow)?;
            let value = *self
                .data
                .get(source_index)
                .ok_or(MalformedError::IntegerOverflow)?;
            let destination_byte = self
                .data
                .get_mut(destination_index)
                .ok_or(MalformedError::IntegerOverflow)?;
            *destination_byte = value;
            index = index
                .checked_add(1)
                .ok_or(MalformedError::IntegerOverflow)?;
        }
        Ok(())
    }

    #[cfg(verus_keep_ghost)]
    #[verus_spec(out =>
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
            out.is_ok(),
    )]
    pub(crate) fn copy_row_scalar_verified(
        &mut self,
        source: u32,
        destination: u32,
    ) -> Result<(), MalformedError> {
        if source >= self.height || destination >= self.height {
            return Err(MalformedError::IntegerOverflow);
        }
        if source == destination {
            return Ok(());
        }
        let width = self.width;
        let height = self.height;
        let stride = self.stride;
        let data_len = self.data.len();
        proof! {
            crate::views::bitmap_fields(self);
            reveal(crate::spec::bitmap::storage_valid);
            assert(data_len == stride as nat * height as nat);
            vstd::arithmetic::mul::lemma_mul_is_commutative(
                stride as int,
                height as int,
            );
            assert(data_len == height as nat * stride as nat);
            assert(source as nat + 1 <= height as nat);
            assert(destination as nat + 1 <= height as nat);
            vstd::arithmetic::mul::lemma_mul_inequality(
                source as int + 1,
                height as int,
                stride as int,
            );
            vstd::arithmetic::mul::lemma_mul_inequality(
                destination as int + 1,
                height as int,
                stride as int,
            );
            vstd::arithmetic::mul::lemma_mul_is_distributive_add_other_way(
                stride as int,
                source as int,
                1,
            );
            vstd::arithmetic::mul::lemma_mul_is_distributive_add_other_way(
                stride as int,
                destination as int,
                1,
            );
            assert(source as nat * stride as nat + stride as nat <= data_len);
            assert(destination as nat * stride as nat + stride as nat <= data_len);
            assert(source as nat * stride as nat <= usize::MAX);
            assert(destination as nat * stride as nat <= usize::MAX);
        }
        let source_start = (source as usize) * stride;
        let destination_start = (destination as usize) * stride;
        let mut index = 0_usize;
        #[verus_spec(
            invariant
                self.width == width,
                self.height == height,
                self.stride == stride,
                self.data@.len() == data_len,
                index <= stride,
                source_start + stride <= data_len,
                destination_start + stride <= data_len,
            decreases stride - index,
        )]
        while index < stride {
            proof! {
                assert(source_start + index < data_len);
                assert(destination_start + index < data_len);
            }
            let value = self.data[source_start + index];
            self.data[destination_start + index] = value;
            index += 1;
        }
        proof! {
            crate::views::bitmap_fields(self);
            reveal(crate::spec::bitmap::storage_valid);
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
}

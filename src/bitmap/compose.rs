#[cfg(verus_keep_ghost)]
use super::bitmap_shared;
use super::chunks::{
    read_chunk, read_shifted_byte, read_shifted_chunk, read_shifted_tail, write_chunk,
};
use super::{Bitmap, CombinationOperator};
use crate::MalformedError;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

impl Bitmap {
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
        let result = self.compose_chunked(source, x, y, operator);
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::views::bitmap_fields(self);
            crate::views::bitmap_fields(old(self));
            crate::views::bitmap_fields(source);
            reveal(crate::spec::bitmap::compose_frame);
            reveal(crate::spec::bitmap::storage_valid);
        }
        result
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            crate::spec::bitmap::storage_valid(old(self)),
            crate::spec::bitmap::storage_valid(source),
        ensures
            final(self).width == old(self).width,
            final(self).height == old(self).height,
            final(self).stride == old(self).stride,
            final(self).data@.len() == old(self).data@.len(),
            out.is_ok(),
            crate::spec::bitmap::compose_frame(
                old(self).data@, final(self).data@,
                old(self).stride, old(self).width, old(self).height,
                source.width, source.height, x, y,
            ),
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
            crate::views::bitmap_fields(old(self));
            crate::views::bitmap_fields(source);
            assert(self.data@.len() == self.data.len());
            assert(source.data@.len() == source.data.len());
            assert(self.data@.len() <= usize::MAX);
            assert(source.data@.len() <= usize::MAX);
            assert(u32::MAX as nat <= usize::MAX) by (compute);
            reveal(crate::spec::bitmap::compose_frame);
        }
        #[cfg(verus_keep_ghost)]
        proof_decl! { let ghost original_data = self.data@; }
        let width = source.width.min(self.width.saturating_sub(x));
        let height = source.height.min(self.height.saturating_sub(y));
        #[cfg(verus_keep_ghost)]
        production_bitmap_clipped_bounds!(self, source, x, y);
        #[cfg(verus_keep_ghost)]
        production_bitmap_clipped_dimensions!(self, source, x, y, width, height);
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
                self.data@.len() == old(self).data@.len(),
                crate::spec::bitmap::storage_valid(self),
                source_y <= height,
                width > 0,
                width as nat == crate::spec::bitmap::clipped_width(self.width, source.width, x),
                height as nat == crate::spec::bitmap::clipped_height(self.height, source.height, y),
                crate::spec::bitmap::compose_frame(
                    original_data, self.data@, self.stride, self.width, self.height,
                    source.width, source.height, x, y,
                ),
                self.data@.len() <= usize::MAX,
                source.data@.len() <= usize::MAX,
                destination_column as int == x as int / 8,
                destination_shift == x % 8,
                width_usize == width,
                full_bytes as int == width as int / 8,
                tail_bits == width % 8,
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
            #[cfg(verus_keep_ghost)]
            production_bitmap_compose_row_bounds!(self, source, source_y, x, y, width);
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
            .ok_or(MalformedError::IntegerOverflow)?;
            let destination_row = destination_row
                .checked_add(destination_column)
                .ok_or(MalformedError::IntegerOverflow)?;

            #[cfg(verus_keep_ghost)]
            proof! {
                assert(source_row as nat + source.stride as nat <= source.data@.len());
                assert(destination_row as int
                    == (y as int + source_y as int) * self.stride as int + x as int / 8);
                assert((y as int + source_y as int) * self.stride as int
                    + self.stride as int <= self.data@.len());
                assert(source_row < source.data@.len());
                assert(destination_row < self.data@.len());
            }
            if destination_shift == 0 {
                let mut byte = 0_usize;
                #[cfg_attr(verus_keep_ghost, verus_spec(
                    invariant
                        byte <= full_bytes,
                        self.width == old(self).width,
                        self.height == old(self).height,
                        self.stride == old(self).stride,
                        self.data@.len() == old(self).data@.len(),
                        self.data@.len() <= usize::MAX,
                        source.data@.len() <= usize::MAX,
                        full_bytes as int == width as int / 8,
                        tail_bits == width % 8,
                        source_row as nat + full_bytes as nat
                            + if tail_bits > 0 { 1nat } else { 0nat } <= source.data@.len(),
                        destination_row as nat + full_bytes as nat
                            + if tail_bits > 0 { 1nat } else { 0nat } <= self.data@.len(),
                        x % 8 == 0,
                        destination_row as int == (y as int + source_y as int)
                            * self.stride as int + x as int / 8,
                        width as nat == crate::spec::bitmap::clipped_width(self.width, source.width, x),
                        source_y < crate::spec::bitmap::clipped_height(self.height, source.height, y),
                        crate::spec::bitmap::compose_frame(
                            original_data, self.data@, self.stride, self.width, self.height,
                            source.width, source.height, x, y,
                        ),
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
                    #[cfg(verus_keep_ghost)]
                    proof_decl! { let ghost before_write_data = self.data@; }
                    if !write_chunk(
                        &mut self.data,
                        destination_index,
                        operator.apply_chunk(destination_chunk, source_chunk),
                    ) {
                        return Err(MalformedError::IntegerOverflow);
                    }
                    #[cfg(verus_keep_ghost)]
                    production_bitmap_compose_frame!(
                        self,
                        source,
                        x,
                        y,
                        source_y,
                        destination_index,
                        8,
                        before_write_data,
                        original_data
                    );
                    byte = byte.checked_add(8).ok_or(MalformedError::IntegerOverflow)?;
                }
                #[cfg_attr(verus_keep_ghost, verus_spec(
                    invariant
                        byte <= full_bytes,
                        self.width == old(self).width,
                        self.height == old(self).height,
                        self.stride == old(self).stride,
                        self.data@.len() == old(self).data@.len(),
                        self.data@.len() <= usize::MAX,
                        source.data@.len() <= usize::MAX,
                        full_bytes as int == width as int / 8,
                        tail_bits == width % 8,
                        x % 8 == 0,
                        destination_row as int == (y as int + source_y as int)
                            * self.stride as int + x as int / 8,
                        width as nat == crate::spec::bitmap::clipped_width(self.width, source.width, x),
                        source_y < crate::spec::bitmap::clipped_height(self.height, source.height, y),
                        crate::spec::bitmap::compose_frame(
                            original_data, self.data@, self.stride, self.width, self.height,
                            source.width, source.height, x, y,
                        ),
                        source_row as nat + full_bytes as nat
                            + if tail_bits > 0 { 1nat } else { 0nat } <= source.data@.len(),
                        destination_row as nat + full_bytes as nat
                            + if tail_bits > 0 { 1nat } else { 0nat } <= self.data@.len(),
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
                    #[cfg(verus_keep_ghost)]
                    proof_decl! { let ghost before_write_data = self.data@; }
                    let destination_byte = self
                        .data
                        .get_mut(destination_index)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    *destination_byte = operator.apply_byte(destination_value, source_value);
                    #[cfg(verus_keep_ghost)]
                    production_bitmap_compose_frame!(
                        self,
                        source,
                        x,
                        y,
                        source_y,
                        destination_index,
                        1,
                        before_write_data,
                        original_data
                    );
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
                    #[cfg(verus_keep_ghost)]
                    proof_decl! { let ghost before_write_data = self.data@; }
                    let destination_byte = self
                        .data
                        .get_mut(destination_index)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    *destination_byte = (destination_value & !mask) | (combined & mask);
                    #[cfg(verus_keep_ghost)]
                    production_bitmap_compose_frame!(
                        self,
                        source,
                        x,
                        y,
                        source_y,
                        destination_index,
                        1,
                        before_write_data,
                        original_data
                    );
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
                #[cfg(verus_keep_ghost)]
                proof_decl! { let ghost before_write_data = self.data@; }
                let destination_byte = self
                    .data
                    .get_mut(destination_row)
                    .ok_or(MalformedError::IntegerOverflow)?;
                *destination_byte = (destination_value & !head_mask) | (combined & head_mask);
                #[cfg(verus_keep_ghost)]
                production_bitmap_compose_frame!(
                    self,
                    source,
                    x,
                    y,
                    source_y,
                    destination_row,
                    1,
                    before_write_data,
                    original_data
                );

                let mut source_bit = head_bits;
                let mut destination_index = destination_row
                    .checked_add(1)
                    .ok_or(MalformedError::IntegerOverflow)?;
                #[cfg_attr(verus_keep_ghost, verus_spec(
                    invariant
                        head_bits <= source_bit <= width,
                        (source_bit as int - head_bits as int) % 8 == 0,
                        destination_index as int == destination_row as int + 1
                            + (source_bit as int - head_bits as int) / 8,
                        destination_index as int <= (y as int + source_y as int)
                            * self.stride as int + self.stride as int,
                        self.width == old(self).width,
                        self.height == old(self).height,
                        self.stride == old(self).stride,
                        self.data@.len() == old(self).data@.len(),
                        self.data@.len() <= usize::MAX,
                        source.data@.len() <= usize::MAX,
                        self.stride as int == (self.width as int + 7) / 8,
                        source.stride as int == (source.width as int + 7) / 8,
                        width as nat == crate::spec::bitmap::clipped_width(self.width, source.width, x),
                        source_y < crate::spec::bitmap::clipped_height(self.height, source.height, y),
                        crate::spec::bitmap::compose_frame(
                            original_data, self.data@, self.stride, self.width, self.height,
                            source.width, source.height, x, y,
                        ),
                        source_row as nat + source.stride as nat <= source.data@.len(),
                        destination_row as int == (y as int + source_y as int)
                            * self.stride as int + x as int / 8,
                        (y as int + source_y as int) * self.stride as int
                            + self.stride as int <= self.data@.len(),
                        width <= source.width,
                        x as nat + width as nat <= self.width as nat,
                        0 < destination_shift < 8,
                        destination_shift == x % 8,
                        head_bits as int == if width <= 8 - destination_shift {
                            width as int
                        } else { 8 - destination_shift },
                    decreases width - source_bit,
                ))]
                while width - source_bit >= 64 {
                    #[cfg(verus_keep_ghost)]
                    proof! {
                        bitmap_shared::aligned_byte_position(x, head_bits, width, source_bit);
                        bitmap_shared::packed_span_bounds(source.width, source.stride, source_bit, 64);
                        let destination_bit = (x as int + source_bit as int) as u32;
                        bitmap_shared::packed_span_bounds(self.width, self.stride, destination_bit, 64);
                    }
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
                    #[cfg(verus_keep_ghost)]
                    proof_decl! { let ghost before_write_data = self.data@; }
                    if !write_chunk(
                        &mut self.data,
                        destination_index,
                        operator.apply_chunk(destination_chunk, source_chunk),
                    ) {
                        return Err(MalformedError::IntegerOverflow);
                    }
                    #[cfg(verus_keep_ghost)]
                    production_bitmap_compose_frame!(
                        self,
                        source,
                        x,
                        y,
                        source_y,
                        destination_index,
                        8,
                        before_write_data,
                        original_data
                    );
                    source_bit = source_bit
                        .checked_add(64)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    destination_index = destination_index
                        .checked_add(8)
                        .ok_or(MalformedError::IntegerOverflow)?;
                }
                #[cfg_attr(verus_keep_ghost, verus_spec(
                    invariant
                        head_bits <= source_bit <= width,
                        (source_bit as int - head_bits as int) % 8 == 0,
                        destination_index as int == destination_row as int + 1
                            + (source_bit as int - head_bits as int) / 8,
                        destination_index as int <= (y as int + source_y as int)
                            * self.stride as int + self.stride as int,
                        self.width == old(self).width,
                        self.height == old(self).height,
                        self.stride == old(self).stride,
                        self.data@.len() == old(self).data@.len(),
                        self.data@.len() <= usize::MAX,
                        source.data@.len() <= usize::MAX,
                        self.stride as int == (self.width as int + 7) / 8,
                        source.stride as int == (source.width as int + 7) / 8,
                        width as nat == crate::spec::bitmap::clipped_width(self.width, source.width, x),
                        source_y < crate::spec::bitmap::clipped_height(self.height, source.height, y),
                        crate::spec::bitmap::compose_frame(
                            original_data, self.data@, self.stride, self.width, self.height,
                            source.width, source.height, x, y,
                        ),
                        source_row as nat + source.stride as nat <= source.data@.len(),
                        destination_row as int == (y as int + source_y as int)
                            * self.stride as int + x as int / 8,
                        (y as int + source_y as int) * self.stride as int
                            + self.stride as int <= self.data@.len(),
                        width <= source.width,
                        x as nat + width as nat <= self.width as nat,
                        0 < destination_shift < 8,
                        destination_shift == x % 8,
                        head_bits as int == if width <= 8 - destination_shift {
                            width as int
                        } else { 8 - destination_shift },
                    decreases width - source_bit,
                ))]
                while width - source_bit >= 8 {
                    #[cfg(verus_keep_ghost)]
                    proof! {
                        bitmap_shared::aligned_byte_position(x, head_bits, width, source_bit);
                        bitmap_shared::packed_span_bounds(source.width, source.stride, source_bit, 8);
                        let destination_bit = (x as int + source_bit as int) as u32;
                        bitmap_shared::packed_span_bounds(self.width, self.stride, destination_bit, 8);
                    }
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
                    #[cfg(verus_keep_ghost)]
                    proof_decl! { let ghost before_write_data = self.data@; }
                    let destination_byte = self
                        .data
                        .get_mut(destination_index)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    *destination_byte = operator.apply_byte(destination_value, source_value);
                    #[cfg(verus_keep_ghost)]
                    production_bitmap_compose_frame!(
                        self,
                        source,
                        x,
                        y,
                        source_y,
                        destination_index,
                        1,
                        before_write_data,
                        original_data
                    );
                    source_bit = source_bit
                        .checked_add(8)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    destination_index = destination_index
                        .checked_add(1)
                        .ok_or(MalformedError::IntegerOverflow)?;
                }
                let remaining = width - source_bit;
                if remaining != 0 {
                    #[cfg(verus_keep_ghost)]
                    proof! {
                        bitmap_shared::aligned_byte_position(x, head_bits, width, source_bit);
                        bitmap_shared::packed_span_bounds(source.width, source.stride, source_bit, remaining);
                        let destination_bit = (x as int + source_bit as int) as u32;
                        bitmap_shared::packed_span_bounds(self.width, self.stride, destination_bit, remaining);
                    }
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
                    #[cfg(verus_keep_ghost)]
                    proof_decl! { let ghost before_write_data = self.data@; }
                    let destination_byte = self
                        .data
                        .get_mut(destination_index)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    *destination_byte = (destination_value & !mask) | (combined & mask);
                    #[cfg(verus_keep_ghost)]
                    production_bitmap_compose_frame!(
                        self,
                        source,
                        x,
                        y,
                        source_y,
                        destination_index,
                        1,
                        before_write_data,
                        original_data
                    );
                }
            }
            #[cfg(verus_keep_ghost)]
            proof! {
                crate::views::bitmap_fields(self);
                crate::views::bitmap_fields(old(self));
                reveal(crate::spec::bitmap::storage_valid);
            }
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::views::bitmap_fields(self);
            crate::views::bitmap_fields(old(self));
        }
        Ok(())
    }
}

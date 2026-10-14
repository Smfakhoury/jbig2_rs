use crate::{DecodeError, LimitExceeded, LimitKind, Limits, MalformedError};
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg(verus_keep_ghost)]
#[rustfmt::skip]
#[path = "../proofs/shared/bitmap.rs"]
mod bitmap_shared;

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
    pub fn blank(width: u32, height: u32, limits: &Limits) -> Result<Self, DecodeError> {
        Self::filled(width, height, false, limits)
    }

    pub(crate) fn filled(
        width: u32,
        height: u32,
        value: bool,
        limits: &Limits,
    ) -> Result<Self, DecodeError> {
        let (stride, length, length_u64) = checked_bitmap_layout(width, height, limits)?;
        let mut data = Vec::new();
        data.try_reserve_exact(length)
            .map_err(|_| DecodeError::Allocation {
                requested_bytes: length_u64,
            })?;
        data.resize(length, if value { 0xFF } else { 0 });

        if value && width % 8 != 0 {
            let used_bits = u8::try_from(width % 8).map_err(|_| MalformedError::IntegerOverflow)?;
            let trailing_mask = 0xFF_u8 << (8 - used_bits);
            for row in 0..height {
                let row = usize::try_from(row).map_err(|_| MalformedError::IntegerOverflow)?;
                let final_byte = row
                    .checked_mul(stride)
                    .and_then(|offset| offset.checked_add(stride.checked_sub(1)?))
                    .ok_or(MalformedError::IntegerOverflow)?;
                let byte = data
                    .get_mut(final_byte)
                    .ok_or(MalformedError::IntegerOverflow)?;
                *byte &= trailing_mask;
            }
        }

        Ok(Self {
            width,
            height,
            stride,
            data,
        })
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
    #[allow(clippy::too_many_lines)]
    pub(crate) fn compose(
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
        #[cfg(verus_keep_ghost)]
        proof_decl! {
            let ghost original_data = self.data@;
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
        #[cfg_attr(verus_keep_ghost, verus_spec(
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
        ))]
        for source_y in 0..height {
            let destination_y = y
                .checked_add(source_y)
                .ok_or(MalformedError::IntegerOverflow)?;
            #[cfg_attr(verus_keep_ghost, verus_spec(
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
            ))]
            for source_x in 0..width {
                #[cfg(verus_keep_ghost)]
                proof! {
                    crate::views::bitmap_fields(source);
                    crate::views::bitmap_fields(self);
                    assert(source_x < width);
                    assert(source_y < height);
                    assert(source_x < source.width);
                    assert(source_y < source.height);
                    assert(
                        (x as nat) + (source_x as nat) < self.width as nat
                    );
                    assert(
                        (y as nat) + (source_y as nat) < self.height as nat
                    );
                }
                let destination_x = x
                    .checked_add(source_x)
                    .ok_or(MalformedError::IntegerOverflow)?;
                let source_pixel = source.pixel(source_x, source_y);
                let destination_pixel = self.pixel(destination_x, destination_y);
                #[cfg(verus_keep_ghost)]
                proof! {
                    reveal(crate::spec::bitmap::pixel_result);
                    reveal(crate::spec::bitmap::coordinates_in_bounds);
                    assert(source_pixel.is_some());
                    assert(destination_pixel.is_some());
                }
                let source_value = source_pixel.ok_or(MalformedError::IntegerOverflow)?;
                let destination_value = destination_pixel.ok_or(MalformedError::IntegerOverflow)?;
                #[cfg(verus_keep_ghost)]
                proof_decl! {
                    let ghost before_data = self.data@;
                }
                self.set_pixel(
                    destination_x,
                    destination_y,
                    operator.apply(destination_value, source_value),
                )?;
                #[cfg(verus_keep_ghost)]
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
                            reveal(
                                crate::spec::bitmap::compose_byte_may_change
                            );
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
                                assert(destination_x as int
                                    == x as int + sx);
                                assert(destination_y as int
                                    == y as int + sy);
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
                                assert(exists|candidate_x: int,
                                    candidate_y: int|
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
                                    assert(
                                        index
                                            == crate::spec::bitmap::byte_index(
                                                self.stride,
                                                (x as int + sx) as u32,
                                                (y as int + sy) as u32,
                                            ) as int
                                    );
                                }
                            }
                        } else {
                            assert(self.data@[index] == before_data[index]);
                            assert(before_data[index]
                                == original_data[index]);
                        }
                    }
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
}

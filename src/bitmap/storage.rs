#[cfg(verus_keep_ghost)]
use super::bitmap_shared;
use super::Bitmap;
use crate::{DecodeError, LimitExceeded, LimitKind, Limits, MalformedError};
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

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
pub(super) fn checked_bitmap_layout(
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
                && crate::spec::bitmap::filled_storage(&bitmap, value)
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
            #[cfg(verus_keep_ghost)]
            proof! { bitmap_shared::initial_black_fill(data@, width, stride, height); }
            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant
                    row <= height,
                    value,
                    width % 8 != 0,
                    trailing_mask == (0xFFu8 << (8u8 - (width % 8) as u8)),
                    data@.len() == length,
                    length as nat == stride as nat * height as nat,
                    stride > 0,
                    stride as nat == crate::spec::bitmap::stride_for_width(width as nat),
                    bitmap_shared::black_fill_prefix(data@, width, stride, height, row as nat),
                ensures bitmap_shared::black_fill_prefix(
                    data@, width, stride, height, height as nat,
                ),
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
                production_filled_tail_bounds!(
                    data,
                    height,
                    stride,
                    row,
                    row_offset,
                    stride_minus_one,
                    final_byte
                );
                #[cfg(verus_keep_ghost)]
                proof_decl! { let ghost before_fill = data@; }
                data[final_byte] &= trailing_mask;
                #[cfg(verus_keep_ghost)]
                production_filled_storage_step!(
                    before_fill,
                    data,
                    width,
                    stride,
                    height,
                    row,
                    trailing_mask
                );
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
        #[cfg(verus_keep_ghost)]
        production_filled_storage_result!(bitmap, value);
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

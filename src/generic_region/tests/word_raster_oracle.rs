#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

use super::*;

/// Test-only full-raster oracle for the rolling nominal template-0 decoder.
#[cfg(test)]
#[derive(Debug)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(super) struct GenericWordRaster {
    width: u32,
    height: u32,
    stride_words: usize,
    words: Vec<u32>,
}

#[cfg(test)]
impl GenericWordRaster {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out.is_some() ==> {
            let raster = out.unwrap();
            &&& raster.width == width
            &&& raster.height == height
            &&& raster.words@.len() == raster.stride_words as nat * height as nat
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(super) fn try_new(width: u32, height: u32) -> Option<Self> {
        let stride_u32 = width.checked_add(31)? / 32;
        let stride_words = usize::try_from(stride_u32).ok()?;
        let height_usize = usize::try_from(height).ok()?;
        let word_count = stride_words.checked_mul(height_usize)?;
        let mut words = Vec::new();
        if words.try_reserve(word_count).is_err() {
            return None;
        }
        words.resize(word_count, 0);
        Some(Self {
            width,
            height,
            stride_words,
            words,
        })
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn row_start(&self, row: i64) -> Option<usize> {
        if row < 0 || row > i64::from(u32::MAX) {
            return None;
        }
        let row = u32::try_from(row).ok()?;
        if row >= self.height {
            return None;
        }
        usize::try_from(row).ok()?.checked_mul(self.stride_words)
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn pixel(&self, x: i64, y: i64) -> bool {
        if x < 0 || x >= i64::from(self.width) {
            return false;
        }
        let Some(row_start) = self.row_start(y) else {
            return false;
        };
        let Ok(x) = u32::try_from(x) else {
            return false;
        };
        let Ok(word_column) = usize::try_from(x / 32) else {
            return false;
        };
        let Some(index) = row_start.checked_add(word_column) else {
            return false;
        };
        let Some(word) = self.words.get(index) else {
            return false;
        };
        *word & (0x8000_0000_u32 >> (x % 32)) != 0
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(
        requires count <= 7,
    ))]
    fn row_bits(&self, row: i64, start_x: i64, count: u8) -> usize {
        let mut bits = 0_usize;
        let mut index = 0_u8;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                index <= count,
                count <= 7,
            decreases count - index,
        ))]
        while index < count {
            let Some(sample_x) = start_x.checked_add(i64::from(index)) else {
                return bits;
            };
            bits = (bits << 1) | usize::from(self.pixel(sample_x, row));
            index += 1;
        }
        bits
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn row_word(&self, row_start: Option<usize>, x: u64) -> u32 {
        let Some(row_start) = row_start else {
            return 0;
        };
        let word_column_u64 = x / 32;
        let Ok(word_column) = usize::try_from(word_column_u64) else {
            return 0;
        };
        if word_column >= self.stride_words {
            return 0;
        }
        let Some(first_index) = row_start.checked_add(word_column) else {
            return 0;
        };
        let Some(first) = self.words.get(first_index) else {
            return 0;
        };
        let first = *first;
        let shift = (x % 32) as u32;
        if shift == 0 {
            return first;
        }
        let second = match word_column.checked_add(1) {
            Some(column) if column < self.stride_words => match row_start.checked_add(column) {
                Some(index) if index < self.words.len() => self.words[index],
                _ => 0,
            },
            _ => 0,
        };
        (first << shift) | (second >> (32 - shift))
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures
            final(self).width == old(self).width,
            final(self).height == old(self).height,
            final(self).stride_words == old(self).stride_words,
    ))]
    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn set_black_in_row(&mut self, row_start: usize, x: u32) -> Result<(), GenericRegionError> {
        let column = usize::try_from(x / 32).map_err(|_error| integer_overflow())?;
        let index = row_start.checked_add(column).ok_or_else(integer_overflow)?;
        let Some(word) = self.words.get_mut(index) else {
            return Err(integer_overflow());
        };
        *word |= 0x8000_0000_u32 >> (x % 32);
        Ok(())
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures
            final(self).width == old(self).width,
            final(self).height == old(self).height,
            final(self).stride_words == old(self).stride_words,
    ))]
    fn copy_row(&mut self, source: u32, destination: u32) -> Result<(), GenericRegionError> {
        if source >= self.height || destination >= self.height {
            return Err(integer_overflow());
        }
        let source = usize::try_from(source).map_err(|_error| integer_overflow())?;
        let destination = usize::try_from(destination).map_err(|_error| integer_overflow())?;
        let source_start = source
            .checked_mul(self.stride_words)
            .ok_or_else(integer_overflow)?;
        let destination_start = destination
            .checked_mul(self.stride_words)
            .ok_or_else(integer_overflow)?;
        let source_end = source_start
            .checked_add(self.stride_words)
            .ok_or_else(integer_overflow)?;
        let destination_end = destination_start
            .checked_add(self.stride_words)
            .ok_or_else(integer_overflow)?;
        if source_end > self.words.len() || destination_end > self.words.len() {
            return Err(integer_overflow());
        }
        #[cfg(not(verus_keep_ghost))]
        {
            self.words
                .copy_within(source_start..source_end, destination_start);
        }
        #[cfg(verus_keep_ghost)]
        let mut column = 0_usize;
        #[cfg(verus_keep_ghost)]
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant column <= self.stride_words,
            decreases self.stride_words - column,
        ))]
        #[cfg(verus_keep_ghost)]
        while column < self.stride_words {
            let source_index = source_start
                .checked_add(column)
                .ok_or_else(integer_overflow)?;
            let destination_index = destination_start
                .checked_add(column)
                .ok_or_else(integer_overflow)?;
            let Some(value) = self.words.get(source_index) else {
                return Err(integer_overflow());
            };
            let value = *value;
            let Some(word) = self.words.get_mut(destination_index) else {
                return Err(integer_overflow());
            };
            *word = value;
            column += 1;
        }
        Ok(())
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            crate::spec::bitmap::storage_valid(old(bitmap)),
            old(bitmap).width == self.width,
            old(bitmap).height == self.height,
        ensures match out {
            Ok(()) =>
                crate::spec::bitmap::storage_valid(final(bitmap))
                && final(bitmap).width == old(bitmap).width
                && final(bitmap).height == old(bitmap).height
                && final(bitmap).stride == old(bitmap).stride,
            Err(_) => true,
        },
    ))]
    #[allow(clippy::cast_possible_truncation, clippy::too_many_lines)]
    pub(super) fn materialize(&self, bitmap: &mut Bitmap) -> Result<(), GenericRegionError> {
        if bitmap.width != self.width || bitmap.height != self.height {
            return Err(integer_overflow());
        }
        #[cfg(verus_keep_ghost)]
        let bitmap_width = bitmap.width;
        #[cfg(verus_keep_ghost)]
        let bitmap_height = bitmap.height;
        #[cfg(verus_keep_ghost)]
        let bitmap_stride = bitmap.stride;
        #[cfg(verus_keep_ghost)]
        let bitmap_length = bitmap.data.len();
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::views::bitmap_fields(bitmap);
            reveal(crate::spec::bitmap::storage_valid);
            assert(bitmap_length as nat
                == bitmap_stride as nat * bitmap_height as nat);
        }
        let mut y = 0_u32;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                y <= self.height,
                bitmap.width == self.width,
                bitmap.height == self.height,
                bitmap.width == bitmap_width,
                bitmap.height == bitmap_height,
                bitmap.stride == bitmap_stride,
                bitmap.data@.len() == bitmap_length,
                bitmap_length as nat
                    == bitmap_stride as nat * bitmap_height as nat,
            decreases self.height - y,
        ))]
        while y < self.height {
            let row = usize::try_from(y).map_err(|_error| integer_overflow())?;
            let word_start = row
                .checked_mul(self.stride_words)
                .ok_or_else(integer_overflow)?;
            let byte_start = row
                .checked_mul(bitmap.stride)
                .ok_or_else(integer_overflow)?;
            let full_words = bitmap.stride / 4;
            let mut word_column = 0_usize;
            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant
                    word_column <= full_words,
                    bitmap.width == self.width,
                    bitmap.height == self.height,
                    bitmap.width == bitmap_width,
                    bitmap.height == bitmap_height,
                    bitmap.stride == bitmap_stride,
                    bitmap.data@.len() == bitmap_length,
                    bitmap_length as nat
                        == bitmap_stride as nat * bitmap_height as nat,
                decreases full_words - word_column,
            ))]
            while word_column < full_words {
                let word_index = word_start
                    .checked_add(word_column)
                    .ok_or_else(integer_overflow)?;
                let Some(word) = self.words.get(word_index) else {
                    return Err(integer_overflow());
                };
                let word = *word;
                let byte_index = byte_start
                    .checked_add(word_column.checked_mul(4).ok_or_else(integer_overflow)?)
                    .ok_or_else(integer_overflow)?;
                let Some(byte_end) = byte_index.checked_add(4) else {
                    return Err(integer_overflow());
                };
                #[cfg(not(verus_keep_ghost))]
                {
                    let Some(bytes) = bitmap.data.get_mut(byte_index..byte_end) else {
                        return Err(integer_overflow());
                    };
                    bytes.copy_from_slice(&[
                        (word >> 24) as u8,
                        (word >> 16) as u8,
                        (word >> 8) as u8,
                        word as u8,
                    ]);
                }
                #[cfg(verus_keep_ghost)]
                {
                    let mut offset = 0_usize;
                    #[cfg_attr(verus_keep_ghost, verus_spec(
                        invariant
                            offset <= 4,
                            bitmap.width == bitmap_width,
                            bitmap.height == bitmap_height,
                            bitmap.stride == bitmap_stride,
                            bitmap.data@.len() == bitmap_length,
                            bitmap_length as nat
                                == bitmap_stride as nat * bitmap_height as nat,
                        decreases 4 - offset,
                    ))]
                    while offset < 4 {
                        let Some(offset_index) = byte_index.checked_add(offset) else {
                            return Err(integer_overflow());
                        };
                        let Some(byte) = bitmap.data.get_mut(offset_index) else {
                            return Err(integer_overflow());
                        };
                        *byte = word_byte(word, offset);
                        offset += 1;
                    }
                }
                word_column += 1;
            }
            let tail_start = full_words.checked_mul(4).ok_or_else(integer_overflow)?;
            let tail_length = bitmap.stride - tail_start;
            let mut tail_offset = 0_usize;
            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant
                    tail_offset <= tail_length,
                    tail_length <= 3,
                    bitmap.width == self.width,
                    bitmap.height == self.height,
                    bitmap.width == bitmap_width,
                    bitmap.height == bitmap_height,
                    bitmap.stride == bitmap_stride,
                    bitmap.data@.len() == bitmap_length,
                    bitmap_length as nat
                        == bitmap_stride as nat * bitmap_height as nat,
                decreases tail_length - tail_offset,
            ))]
            while tail_offset < tail_length {
                let word_index = word_start
                    .checked_add(full_words)
                    .ok_or_else(integer_overflow)?;
                let Some(word) = self.words.get(word_index) else {
                    return Err(integer_overflow());
                };
                let word = *word;
                let Some(byte_column) = tail_start.checked_add(tail_offset) else {
                    return Err(integer_overflow());
                };
                let byte_index = byte_start
                    .checked_add(byte_column)
                    .ok_or_else(integer_overflow)?;
                let Some(byte) = bitmap.data.get_mut(byte_index) else {
                    return Err(integer_overflow());
                };
                *byte = word_byte(word, tail_offset);
                tail_offset += 1;
            }
            y += 1;
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::views::bitmap_fields(bitmap);
            reveal(crate::spec::bitmap::storage_valid);
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::ptr_arg)]
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        rows <= old(raster).height,
        crate::arithmetic::arithmetic_spec::decoder_valid(old(decoder)),
        generic_region_spec::contexts_valid(old(contexts)@),
    ensures
        out.is_ok() ==> final(raster).width == old(raster).width,
        out.is_ok() ==> final(raster).height == old(raster).height,
        out.is_ok() ==> final(raster).stride_words == old(raster).stride_words,
        generic_region_spec::contexts_valid(final(contexts)@),
        out.is_ok() ==> crate::arithmetic::arithmetic_spec::decoder_valid(final(decoder)),
))]
#[allow(clippy::too_many_lines)]
pub(super) fn decode_nominal_template0_words(
    raster: &mut GenericWordRaster,
    rows: u32,
    typical_prediction: bool,
    decoder: &mut ArithmeticDecoder<'_>,
    contexts: &mut Vec<ArithmeticContext>,
) -> Result<(), GenericRegionError> {
    if rows > raster.height {
        return Err(GenericRegionError::RowCountMismatch {
            expected: raster.height,
            actual: rows,
        });
    }
    #[cfg(verus_keep_ghost)]
    let raster_width = raster.width;
    #[cfg(verus_keep_ghost)]
    let raster_height = raster.height;
    #[cfg(verus_keep_ghost)]
    let raster_stride = raster.stride_words;
    let mut ltp = false;
    let mut y = 0_u32;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            y <= rows,
            rows <= raster.height,
            raster.width == raster_width,
            raster.height == raster_height,
            raster.stride_words == raster_stride,
            crate::arithmetic::arithmetic_spec::decoder_valid(decoder),
            generic_region_spec::contexts_valid(contexts@),
        decreases rows - y,
    ))]
    while y < rows {
        if typical_prediction {
            let context = GenericTemplate::Template0.sltp_context();
            let sltp = decode_probability_signal(decoder, contexts, context);
            if sltp > 1 {
                return Err(probability_signal_error(
                    decoder,
                    context,
                    contexts.len(),
                    sltp,
                ));
            }
            ltp ^= sltp != 0;
        }
        if ltp {
            if y != 0 {
                raster.copy_row(y - 1, y)?;
            }
            y += 1;
            continue;
        }

        let mut history = 0_usize;
        let y_i64 = i64::from(y);
        let top_row = raster.row_start(y_i64 - 2);
        let previous_row = raster.row_start(y_i64 - 1);
        let mut top = raster.row_bits(y_i64 - 2, -2, 5);
        let mut previous = raster.row_bits(y_i64 - 1, -3, 7);
        let row = usize::try_from(y).map_err(|_error| integer_overflow())?;
        let row_start = row
            .checked_mul(raster.stride_words)
            .ok_or_else(integer_overflow)?;
        let mut x = 0_u32;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                x <= raster.width,
                y < rows,
                rows <= raster.height,
                raster.width == raster_width,
                raster.height == raster_height,
                raster.stride_words == raster_stride,
                crate::arithmetic::arithmetic_spec::decoder_valid(decoder),
                generic_region_spec::contexts_valid(contexts@),
            decreases raster.width - x,
        ))]
        while x < raster.width {
            #[cfg(verus_keep_ghost)]
            let chunk_start = x;
            let mut top_queue = raster.row_word(top_row, u64::from(x) + 3);
            let mut previous_queue = raster.row_word(previous_row, u64::from(x) + 4);
            let remaining = raster.width - x;
            let chunk_width = remaining.min(32);
            let Some(chunk_end) = x.checked_add(chunk_width) else {
                return Err(integer_overflow());
            };
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(chunk_width > 0);
                assert(chunk_end <= raster.width);
            }
            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant
                    x <= chunk_end,
                    chunk_end <= raster.width,
                    y < rows,
                    rows <= raster.height,
                    raster.width == raster_width,
                    raster.height == raster_height,
                    raster.stride_words == raster_stride,
                    crate::arithmetic::arithmetic_spec::decoder_valid(decoder),
                    generic_region_spec::contexts_valid(contexts@),
                decreases chunk_end - x,
            ))]
            while x < chunk_end {
                let context = (top << 11) | (previous << 4) | history & 0x0f;
                top = ((top << 1) | usize::from(top_queue & 0x8000_0000 != 0)) & 0x1f;
                previous =
                    ((previous << 1) | usize::from(previous_queue & 0x8000_0000 != 0)) & 0x7f;
                top_queue <<= 1;
                previous_queue <<= 1;

                let signal = decode_probability_signal(decoder, contexts, context);
                if signal > 1 {
                    return Err(probability_signal_error(
                        decoder,
                        context,
                        contexts.len(),
                        signal,
                    ));
                }
                history = ((history << 1) | usize::from(signal)) & 0x0f;
                if signal != 0 {
                    raster.set_black_in_row(row_start, x)?;
                }
                x += 1;
            }
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(x == chunk_end);
                assert(x > chunk_start);
            }
        }
        y += 1;
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn word_raster_storage_bytes(width: u32, height: u32) -> Option<u64> {
    let stride_words = u64::from(width).checked_add(31)? / 32;
    stride_words.checked_mul(u64::from(height))?.checked_mul(4)
}

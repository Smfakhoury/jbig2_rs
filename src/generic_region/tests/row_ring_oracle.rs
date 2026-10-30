#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

use super::*;

/// Minimal MSB-first rolling rows for nominal template-0 arithmetic decoding.
///
/// At most the current row and its two predecessors are retained. Completed
/// rows are written immediately into the public byte-packed bitmap.
#[cfg(test)]
#[derive(Debug)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(super) struct GenericRowRing {
    width: u32,
    stride_words: usize,
    row_slots: usize,
    words: Vec<u32>,
    pub(super) charged_bytes: u64,
}

#[cfg(test)]
impl GenericRowRing {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out.is_some() ==> {
            let ring = out.unwrap();
            &&& ring.width == width
            &&& ring.words@.len() == ring.stride_words as nat * ring.row_slots as nat
            &&& ring.row_slots == (if rows < 3 { rows } else { 3 }) as usize
            &&& ring.charged_bytes <= available_bytes
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(super) fn try_new(width: u32, rows: u32, available_bytes: u64) -> Option<Self> {
        let stride_u32 = width.checked_add(31)? / 32;
        let stride_words = usize::try_from(stride_u32).ok()?;
        let row_slots = usize::try_from(rows.min(3)).ok()?;
        let word_count = stride_words.checked_mul(row_slots)?;
        let requested_bytes = u64::try_from(word_count).ok()?.checked_mul(4)?;
        if requested_bytes > available_bytes {
            return None;
        }
        let mut words = Vec::new();
        if words.try_reserve(word_count).is_err() {
            return None;
        }
        #[cfg(not(verus_keep_ghost))]
        let charged_bytes = u64::try_from(words.capacity()).ok()?.checked_mul(4)?;
        #[cfg(verus_keep_ghost)]
        let charged_bytes = requested_bytes;
        if charged_bytes > available_bytes {
            return None;
        }
        words.resize(word_count, 0);
        Some(Self {
            width,
            stride_words,
            row_slots,
            words,
            charged_bytes,
        })
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn slot_for_row(&self, row: u32) -> Option<usize> {
        if self.row_slots == 0 {
            return None;
        }
        usize::try_from(row).ok().map(|row| row % self.row_slots)
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn row_start(&self, slot: Option<usize>) -> Option<usize> {
        let slot = slot?;
        if slot >= self.row_slots {
            return None;
        }
        slot.checked_mul(self.stride_words)
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn pixel(&self, slot: Option<usize>, x: i64) -> bool {
        if x < 0 || x >= i64::from(self.width) {
            return false;
        }
        let Some(row_start) = self.row_start(slot) else {
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
    fn row_bits(&self, slot: Option<usize>, start_x: i64, count: u8) -> usize {
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
            bits = (bits << 1) | usize::from(self.pixel(slot, sample_x));
            index += 1;
        }
        bits
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    fn row_word(&self, row_start: Option<usize>, x: u64) -> u32 {
        let Some(row_start) = row_start else {
            return 0;
        };
        let Ok(word_column) = usize::try_from(x / 32) else {
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
            final(self).stride_words == old(self).stride_words,
            final(self).row_slots == old(self).row_slots,
            final(self).charged_bytes == old(self).charged_bytes,
            out.is_ok() ==>
                final(self).words@.len() == old(self).words@.len(),
    ))]
    fn clear_row(&mut self, slot: usize) -> Result<(), GenericRegionError> {
        let start = self.row_start(Some(slot)).ok_or_else(integer_overflow)?;
        let end = start
            .checked_add(self.stride_words)
            .ok_or_else(integer_overflow)?;
        let Some(row) = self.words.get_mut(start..end) else {
            return Err(integer_overflow());
        };
        let mut column = 0_usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant column <= row@.len(),
            decreases row@.len() - column,
        ))]
        while column < row.len() {
            row[column] = 0;
            column += 1;
        }
        Ok(())
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures
            final(self).width == old(self).width,
            final(self).stride_words == old(self).stride_words,
            final(self).row_slots == old(self).row_slots,
            final(self).charged_bytes == old(self).charged_bytes,
            out.is_ok() ==>
                final(self).words@.len() == old(self).words@.len(),
    ))]
    fn copy_row(&mut self, source: usize, destination: usize) -> Result<(), GenericRegionError> {
        let source_start = self.row_start(Some(source)).ok_or_else(integer_overflow)?;
        let destination_start = self
            .row_start(Some(destination))
            .ok_or_else(integer_overflow)?;
        let source_end = source_start
            .checked_add(self.stride_words)
            .ok_or_else(integer_overflow)?;
        if source_end > self.words.len() {
            return Err(integer_overflow());
        }
        #[cfg(verus_keep_ghost)]
        let word_length = self.words.len();
        #[cfg(not(verus_keep_ghost))]
        self.words
            .copy_within(source_start..source_end, destination_start);
        #[cfg(verus_keep_ghost)]
        let mut column = 0_usize;
        #[cfg(verus_keep_ghost)]
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                column <= self.stride_words,
                self.words@.len() == word_length,
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

    #[allow(clippy::inline_always)]
    #[inline(always)]
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures
            final(self).width == old(self).width,
            final(self).stride_words == old(self).stride_words,
            final(self).row_slots == old(self).row_slots,
            final(self).charged_bytes == old(self).charged_bytes,
            out.is_ok() ==>
                final(self).words@.len() == old(self).words@.len(),
    ))]
    fn set_black_in_row(&mut self, row_start: usize, x: u32) -> Result<(), GenericRegionError> {
        let column = usize::try_from(x / 32).map_err(|_error| integer_overflow())?;
        let index = row_start.checked_add(column).ok_or_else(integer_overflow)?;
        let Some(word) = self.words.get_mut(index) else {
            return Err(integer_overflow());
        };
        *word |= 0x8000_0000_u32 >> (x % 32);
        Ok(())
    }

    #[allow(clippy::cast_possible_truncation, clippy::too_many_lines)]
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            crate::spec::bitmap::storage_valid(old(bitmap)),
            old(bitmap).width == self.width,
        ensures match out {
            Ok(()) =>
                crate::spec::bitmap::storage_valid(final(bitmap))
                && final(bitmap).width == old(bitmap).width
                && final(bitmap).height == old(bitmap).height
                && final(bitmap).stride == old(bitmap).stride,
            Err(_) => true,
        },
    ))]
    fn flush_row(
        &self,
        slot: usize,
        bitmap: &mut Bitmap,
        y: u32,
    ) -> Result<(), GenericRegionError> {
        if bitmap.width != self.width || y >= bitmap.height {
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
        let word_start = self.row_start(Some(slot)).ok_or_else(integer_overflow)?;
        let row = usize::try_from(y).map_err(|_error| integer_overflow())?;
        let byte_start = row
            .checked_mul(bitmap.stride)
            .ok_or_else(integer_overflow)?;
        let full_words = bitmap.stride / 4;
        let mut word_column = 0_usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                word_column <= full_words,
                bitmap.width == bitmap_width,
                bitmap.height == bitmap_height,
                bitmap.stride == bitmap_stride,
                bitmap.data@.len() == bitmap_length,
                bitmap_length as nat
                    == bitmap_stride as nat * bitmap_height as nat,
            decreases full_words - word_column,
        ))]
        while word_column < full_words {
            let word = *self
                .words
                .get(
                    word_start
                        .checked_add(word_column)
                        .ok_or_else(integer_overflow)?,
                )
                .ok_or_else(integer_overflow)?;
            let byte_column = word_column.checked_mul(4).ok_or_else(integer_overflow)?;
            let byte_index = byte_start
                .checked_add(byte_column)
                .ok_or_else(integer_overflow)?;
            let byte_end = byte_index.checked_add(4).ok_or_else(integer_overflow)?;
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
        let mut tail = 0_usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                tail <= tail_length,
                tail_length <= 3,
                bitmap.width == bitmap_width,
                bitmap.height == bitmap_height,
                bitmap.stride == bitmap_stride,
                bitmap.data@.len() == bitmap_length,
                bitmap_length as nat
                    == bitmap_stride as nat * bitmap_height as nat,
            decreases tail_length - tail,
        ))]
        while tail < tail_length {
            let word = *self
                .words
                .get(
                    word_start
                        .checked_add(full_words)
                        .ok_or_else(integer_overflow)?,
                )
                .ok_or_else(integer_overflow)?;
            let byte_column = tail_start.checked_add(tail).ok_or_else(integer_overflow)?;
            let byte_index = byte_start
                .checked_add(byte_column)
                .ok_or_else(integer_overflow)?;
            let Some(byte) = bitmap.data.get_mut(byte_index) else {
                return Err(integer_overflow());
            };
            *byte = word_byte(word, tail);
            tail += 1;
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::views::bitmap_fields(bitmap);
            reveal(crate::spec::bitmap::storage_valid);
        }
        Ok(())
    }
}

#[allow(clippy::ptr_arg)]
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        crate::spec::bitmap::storage_valid(old(bitmap)),
        rows <= old(bitmap).height,
        old(ring).width == old(bitmap).width,
        crate::arithmetic::arithmetic_spec::decoder_valid(old(decoder)),
        generic_region_spec::contexts_valid(old(contexts)@),
    ensures
        out.is_ok() ==> crate::spec::bitmap::storage_valid(final(bitmap)),
        generic_region_spec::contexts_valid(final(contexts)@),
        out.is_ok() ==> crate::arithmetic::arithmetic_spec::decoder_valid(final(decoder)),
))]
#[allow(clippy::too_many_lines)]
#[cfg(test)]
pub(super) fn decode_nominal_template0_row_ring(
    ring: &mut GenericRowRing,
    bitmap: &mut Bitmap,
    rows: u32,
    typical_prediction: bool,
    decoder: &mut ArithmeticDecoder<'_>,
    contexts: &mut Vec<ArithmeticContext>,
) -> Result<(), GenericRegionError> {
    if rows > bitmap.height || ring.width != bitmap.width {
        return Err(GenericRegionError::RowCountMismatch {
            expected: bitmap.height,
            actual: rows,
        });
    }
    let mut ltp = false;
    let mut y = 0_u32;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            y <= rows,
            rows <= bitmap.height,
            ring.width == bitmap.width,
            crate::spec::bitmap::storage_valid(bitmap),
            crate::arithmetic::arithmetic_spec::decoder_valid(decoder),
            generic_region_spec::contexts_valid(contexts@),
        decreases rows - y,
    ))]
    while y < rows {
        let current_slot = ring.slot_for_row(y).ok_or_else(integer_overflow)?;
        ring.clear_row(current_slot)?;
        let current_row = ring
            .row_start(Some(current_slot))
            .ok_or_else(integer_overflow)?;
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
                let previous_slot = ring.slot_for_row(y - 1).ok_or_else(integer_overflow)?;
                ring.copy_row(previous_slot, current_slot)?;
            }
            ring.flush_row(current_slot, bitmap, y)?;
            y += 1;
            continue;
        }

        let top_slot = y.checked_sub(2).and_then(|row| ring.slot_for_row(row));
        let previous_slot = y.checked_sub(1).and_then(|row| ring.slot_for_row(row));
        let top_row = ring.row_start(top_slot);
        let previous_row = ring.row_start(previous_slot);
        let mut history = 0_usize;
        let mut top = ring.row_bits(top_slot, -2, 5);
        let mut previous = ring.row_bits(previous_slot, -3, 7);
        let mut x = 0_u32;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                x <= ring.width,
                y < rows,
                rows <= bitmap.height,
                ring.width == bitmap.width,
                crate::spec::bitmap::storage_valid(bitmap),
                crate::arithmetic::arithmetic_spec::decoder_valid(decoder),
                generic_region_spec::contexts_valid(contexts@),
            decreases ring.width - x,
        ))]
        while x < ring.width {
            let mut top_queue = ring.row_word(top_row, u64::from(x) + 3);
            let mut previous_queue = ring.row_word(previous_row, u64::from(x) + 4);
            let remaining = ring.width - x;
            let chunk_width = remaining.min(32);
            let chunk_end = x.checked_add(chunk_width).ok_or_else(integer_overflow)?;
            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant
                    x <= chunk_end,
                    chunk_end <= ring.width,
                    y < rows,
                    rows <= bitmap.height,
                    ring.width == bitmap.width,
                    crate::spec::bitmap::storage_valid(bitmap),
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
                    ring.set_black_in_row(current_row, x)?;
                }
                x += 1;
            }
        }
        ring.flush_row(current_slot, bitmap, y)?;
        y += 1;
    }
    Ok(())
}

#[cfg(test)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(super) fn row_ring_storage_bytes(width: u32, rows: u32) -> Option<u64> {
    let stride_words = u64::from(width).checked_add(31)? / 32;
    stride_words
        .checked_mul(u64::from(rows.min(3)))?
        .checked_mul(4)
}

#[cfg(test)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(super) fn row_ring_fits_managed_memory(
    width: u32,
    rows: u32,
    context_bytes: u64,
    bitmap_bytes: u64,
    limits: &Limits,
) -> Result<bool, GenericRegionError> {
    let Some(row_bytes) = row_ring_storage_bytes(width, rows) else {
        return Err(integer_overflow());
    };
    let Some(managed) = context_bytes
        .checked_add(bitmap_bytes)
        .and_then(|bytes| bytes.checked_add(row_bytes))
    else {
        return Err(integer_overflow());
    };
    Ok(managed <= limits.max_decoder_managed_memory_bytes)
}

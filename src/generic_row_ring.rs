use super::{
    decode_probability_signal, enforce_managed_memory, integer_overflow, probability_signal_error,
    ArithmeticContext, ArithmeticDecoder, Bitmap, GenericRegionError, GenericTemplate, Limits,
};

#[derive(Debug)]
struct GenericRowRing {
    width: u32,
    stride_words: usize,
    row_slots: usize,
    words: Vec<u32>,
    charged_bytes: u64,
}

impl GenericRowRing {
    #[cold]
    #[inline(never)]
    fn try_new(width: u32, rows: u32, available_bytes: u64) -> Option<Self> {
        let stride_words = usize::try_from(width.checked_add(31)? / 32).ok()?;
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
        let charged_bytes = u64::try_from(words.capacity()).ok()?.checked_mul(4)?;
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

    #[inline]
    fn slot_for_row(&self, row: u32) -> Option<usize> {
        (self.row_slots != 0)
            .then(|| usize::try_from(row).ok())
            .flatten()
            .map(|row| row % self.row_slots)
    }

    #[inline]
    fn row_start(&self, slot: Option<usize>) -> Option<usize> {
        let slot = slot?;
        (slot < self.row_slots)
            .then(|| slot.checked_mul(self.stride_words))
            .flatten()
    }

    fn row_bits(&self, slot: Option<usize>, start_x: i64, count: u8) -> usize {
        let Some(row_start) = self.row_start(slot) else {
            return 0;
        };
        let mut bits = 0_usize;
        for offset in 0..count {
            let x = start_x + i64::from(offset);
            let bit = if x < 0 || x >= i64::from(self.width) {
                false
            } else {
                let x = u32::try_from(x).expect("bounded nonnegative x");
                let index = row_start + usize::try_from(x / 32).expect("u32 fits usize");
                self.words[index] & (0x8000_0000_u32 >> (x % 32)) != 0
            };
            bits = (bits << 1) | usize::from(bit);
        }
        bits
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn row_word(&self, row_start: Option<usize>, x: u64) -> u32 {
        let Some(row_start) = row_start else {
            return 0;
        };
        let Ok(column) = usize::try_from(x / 32) else {
            return 0;
        };
        if column >= self.stride_words {
            return 0;
        }
        let first = self.words[row_start + column];
        let shift = (x % 32) as u32;
        if shift == 0 {
            return first;
        }
        let second = if column + 1 < self.stride_words {
            self.words[row_start + column + 1]
        } else {
            0
        };
        (first << shift) | (second >> (32 - shift))
    }

    fn clear_row(&mut self, slot: usize) -> Result<usize, GenericRegionError> {
        let start = self.row_start(Some(slot)).ok_or_else(integer_overflow)?;
        let end = start
            .checked_add(self.stride_words)
            .ok_or_else(integer_overflow)?;
        self.words
            .get_mut(start..end)
            .ok_or_else(integer_overflow)?
            .fill(0);
        Ok(start)
    }

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
        self.words
            .copy_within(source_start..source_end, destination_start);
        Ok(())
    }

    #[allow(clippy::cast_possible_truncation, clippy::inline_always)]
    #[inline(always)]
    fn set_black(&mut self, row_start: usize, x: u32) {
        let column = (x / 32) as usize;
        self.words[row_start + column] |= 0x8000_0000_u32 >> (x % 32);
    }

    fn flush_row(
        &self,
        slot: usize,
        bitmap: &mut Bitmap,
        y: u32,
    ) -> Result<(), GenericRegionError> {
        let word_start = self.row_start(Some(slot)).ok_or_else(integer_overflow)?;
        let row = usize::try_from(y).map_err(|_error| integer_overflow())?;
        let byte_start = row
            .checked_mul(bitmap.stride)
            .ok_or_else(integer_overflow)?;
        let full_words = bitmap.stride / 4;
        let mut column = 0_usize;
        while column < full_words {
            let word = *self
                .words
                .get(word_start + column)
                .ok_or_else(integer_overflow)?;
            let packed = word.to_be_bytes();
            let byte_index = byte_start
                .checked_add(column.checked_mul(4).ok_or_else(integer_overflow)?)
                .ok_or_else(integer_overflow)?;
            let byte_end = byte_index.checked_add(4).ok_or_else(integer_overflow)?;
            bitmap
                .data
                .get_mut(byte_index..byte_end)
                .ok_or_else(integer_overflow)?
                .copy_from_slice(&packed);
            column += 1;
        }
        let tail_start = full_words.checked_mul(4).ok_or_else(integer_overflow)?;
        let tail_length = bitmap.stride - tail_start;
        let mut tail = 0_usize;
        while tail < tail_length {
            let word = *self
                .words
                .get(word_start + full_words)
                .ok_or_else(integer_overflow)?;
            let byte_index = byte_start
                .checked_add(tail_start.checked_add(tail).ok_or_else(integer_overflow)?)
                .ok_or_else(integer_overflow)?;
            *bitmap
                .data
                .get_mut(byte_index)
                .ok_or_else(integer_overflow)? = super::word_byte(word, tail);
            tail += 1;
        }
        Ok(())
    }
}

#[cold]
#[inline(never)]
fn storage_bytes(width: u32, rows: u32) -> Option<u64> {
    let stride_words = u64::from(width).checked_add(31)? / 32;
    stride_words
        .checked_mul(u64::from(rows.min(3)))?
        .checked_mul(4)
}

#[cold]
#[inline(never)]
fn fits_managed_memory(
    width: u32,
    rows: u32,
    context_bytes: u64,
    bitmap_bytes: u64,
    limits: &Limits,
) -> Result<bool, GenericRegionError> {
    let row_bytes = storage_bytes(width, rows).ok_or_else(integer_overflow)?;
    let managed = context_bytes
        .checked_add(bitmap_bytes)
        .and_then(|bytes| bytes.checked_add(row_bytes))
        .ok_or_else(integer_overflow)?;
    Ok(managed <= limits.max_decoder_managed_memory_bytes)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn try_decode(
    bitmap: &mut Bitmap,
    width: u32,
    rows: u32,
    context_bytes: u64,
    bitmap_bytes: u64,
    limits: &Limits,
    typical_prediction: bool,
    decoder: &mut ArithmeticDecoder<'_>,
    contexts: &mut Vec<ArithmeticContext>,
) -> Result<bool, GenericRegionError> {
    if !fits_managed_memory(width, rows, context_bytes, bitmap_bytes, limits)? {
        return Ok(false);
    }
    let base_managed = context_bytes
        .checked_add(bitmap_bytes)
        .ok_or_else(integer_overflow)?;
    let available_bytes = limits
        .max_decoder_managed_memory_bytes
        .checked_sub(base_managed)
        .ok_or_else(integer_overflow)?;
    let Some(mut ring) = GenericRowRing::try_new(width, rows, available_bytes) else {
        return Ok(false);
    };
    let charged_context_bytes = context_bytes
        .checked_add(ring.charged_bytes)
        .ok_or_else(integer_overflow)?;
    enforce_managed_memory(charged_context_bytes, bitmap_bytes, limits)?;
    decode(
        &mut ring,
        bitmap,
        rows,
        typical_prediction,
        decoder,
        contexts,
    )?;
    drop(ring);
    Ok(true)
}

fn decode(
    ring: &mut GenericRowRing,
    bitmap: &mut Bitmap,
    rows: u32,
    typical_prediction: bool,
    decoder: &mut ArithmeticDecoder<'_>,
    contexts: &mut Vec<ArithmeticContext>,
) -> Result<(), GenericRegionError> {
    let mut ltp = false;
    for y in 0..rows {
        let current_slot = ring.slot_for_row(y).ok_or_else(integer_overflow)?;
        let current_row = ring.clear_row(current_slot)?;
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
                let previous = ring.slot_for_row(y - 1).ok_or_else(integer_overflow)?;
                ring.copy_row(previous, current_slot)?;
            }
            ring.flush_row(current_slot, bitmap, y)?;
            continue;
        }

        let top_slot = y.checked_sub(2).and_then(|row| ring.slot_for_row(row));
        let previous_slot = y.checked_sub(1).and_then(|row| ring.slot_for_row(row));
        let top_row = ring.row_start(top_slot);
        let previous_row = ring.row_start(previous_slot);
        let mut top = ring.row_bits(top_slot, -2, 5);
        let mut previous = ring.row_bits(previous_slot, -3, 7);
        let mut history = 0_usize;
        let mut x = 0_u32;
        while x < ring.width {
            let mut top_queue = ring.row_word(top_row, u64::from(x) + 3);
            let mut previous_queue = ring.row_word(previous_row, u64::from(x) + 4);
            let chunk_end = x
                .checked_add((ring.width - x).min(32))
                .ok_or_else(integer_overflow)?;
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
                    ring.set_black(current_row, x);
                }
                x += 1;
            }
        }
        ring.flush_row(current_slot, bitmap, y)?;
    }
    Ok(())
}

//! Spans implementation.

#[cfg(verus_keep_ghost)]
use super::*;
#[cfg(not(verus_keep_ghost))]
use super::{Bitmap, MmrError};

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires mmr_spec::bitmap_storage_valid(old(bitmap)),
    ensures
        final(bitmap).width == old(bitmap).width,
        final(bitmap).height == old(bitmap).height,
        final(bitmap).stride == old(bitmap).stride,
        final(bitmap).data@.len() == old(bitmap).data@.len(),
        mmr_spec::bitmap_storage_valid(final(bitmap)),
        (start > end || end > old(bitmap).width || row >= old(bitmap).height)
            ==> matches!(out, Err(MmrError::InvalidTransition)),
        (start <= end <= old(bitmap).width && row < old(bitmap).height
            && (!black || start == end)) ==> out.is_ok(),
        (start > end || end > old(bitmap).width || row >= old(bitmap).height
            || !black || start == end)
            ==> final(bitmap).data@ == old(bitmap).data@,
        (crate::spec::bitmap::storage_valid(old(bitmap))
            && start <= end <= old(bitmap).width && row < old(bitmap).height)
            ==> out.is_ok(),
        (out.is_ok() && crate::spec::bitmap::storage_valid(old(bitmap))
            && start <= end <= old(bitmap).width && row < old(bitmap).height)
            ==> mmr_spec::span_result(old(bitmap), final(bitmap), row, start, end, black),
))]
pub(super) fn fill_span(
    bitmap: &mut Bitmap,
    row: u32,
    start: u32,
    end: u32,
    black: bool,
) -> Result<(), MmrError> {
    if start > end || end > bitmap.width || row >= bitmap.height {
        return Err(MmrError::InvalidTransition);
    }
    if !black {
        return Ok(());
    }
    bitmap
        .fill_black_span(row, start, end)
        .map_err(|_error| MmrError::InvalidTransition)?;
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(
    requires
        mmr_spec::transition_prefix_valid(old(transitions)@, start, _width),
        start <= end <= _width,
    ensures
        mmr_spec::transition_prefix_valid(final(transitions)@, end, _width),
        final(transitions)@.len() <= old(transitions)@.len() + 1,
        start == end ==> final(transitions)@.len() == old(transitions)@.len(),
        mmr_spec::transition_span_result(
            old(transitions)@, final(transitions)@,
            *old(emitted_black), *final(emitted_black), start, end, black,
        ),
))]
pub(super) fn record_transition_span(
    transitions: &mut Vec<u32>,
    emitted_black: &mut bool,
    start: u32,
    end: u32,
    _width: u32,
    black: bool,
) {
    if start == end {
        return;
    }
    let changed = *emitted_black != black;
    #[cfg(verus_keep_ghost)]
    proof_decl! {
        let ghost before = transitions@;
    }
    if changed {
        transitions.push(start);
    }
    *emitted_black = black;
    #[cfg(verus_keep_ghost)]
    production_mmr_transition_span!(before, start, end, _width, changed);
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        mmr_spec::bitmap_storage_valid(bitmap),
        row < bitmap.height,
        end <= bitmap.width,
        mmr_spec::transition_prefix_valid(old(transitions)@, start, width),
        start <= end <= width,
    ensures
    match out {
        Ok(()) =>
            final(bitmap).width == old(bitmap).width
            && final(bitmap).height == old(bitmap).height
            && final(bitmap).stride == old(bitmap).stride
            && final(bitmap).data@.len() == old(bitmap).data@.len()
            && mmr_spec::bitmap_storage_valid(final(bitmap))
            && mmr_spec::transition_prefix_valid(final(transitions)@, end, width)
            && final(transitions)@.len() <= old(transitions)@.len() + 1
            && (start == end ==>
                final(transitions)@.len() == old(transitions)@.len())
            && mmr_spec::transition_span_result(
                old(transitions)@, final(transitions)@,
                *old(emitted_black), *final(emitted_black), start, end, black,
            )
            && (crate::spec::bitmap::storage_valid(old(bitmap)) ==>
                mmr_spec::span_result(old(bitmap), final(bitmap), row, start, end, black)
                && mmr_spec::span_pixels(old(bitmap), final(bitmap), row, start, end, black)),
        Err(_) => final(transitions)@ == old(transitions)@
            && *final(emitted_black) == *old(emitted_black),
    },
    crate::spec::bitmap::storage_valid(old(bitmap)) ==> out.is_ok(),
    mmr_spec::row_cache_prefix(
        old(bitmap), row, old(transitions)@, *old(emitted_black), start,
    ) ==> mmr_spec::row_cache_prefix(
        final(bitmap), row, final(transitions)@, *final(emitted_black), end,
    ),
    mmr_spec::future_rows_white(old(bitmap), row as nat + 1) ==>
        mmr_spec::future_rows_white(final(bitmap), row as nat + 1),
    crate::spec::bitmap::storage_valid(old(bitmap)) ==>
        forall|other: u32, cache: Seq<u32>|
            other != row && mmr_spec::row_cache_matches(old(bitmap), other, cache) ==>
                mmr_spec::row_cache_matches(final(bitmap), other, cache),
))]
#[allow(clippy::too_many_arguments)]
pub(super) fn fill_recorded_span(
    bitmap: &mut Bitmap,
    row: u32,
    start: u32,
    end: u32,
    black: bool,
    width: u32,
    transitions: &mut Vec<u32>,
    emitted_black: &mut bool,
) -> Result<(), MmrError> {
    #[cfg(verus_keep_ghost)]
    proof_decl! {
        let ghost old_cache = transitions@;
        let ghost old_emitted = *emitted_black;
    }
    fill_span(bitmap, row, start, end, black)?;
    record_transition_span(transitions, emitted_black, start, end, width, black);
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::views::bitmap_fields(old(bitmap));
        crate::views::bitmap_fields(bitmap);
        reveal(crate::spec::bitmap::storage_valid);
        if crate::spec::bitmap::storage_valid(old(bitmap)) {
            mmr_properties::span_pixel_frame(old(bitmap), bitmap, row, start, end, black);
            mmr_properties::span_row_cache_frame(old(bitmap), bitmap, row, start, end, black);
            if mmr_spec::future_rows_white(old(bitmap), row as nat + 1) {
                mmr_properties::span_keeps_future_white(
                    old(bitmap), bitmap, row, start, end, black, row as nat + 1,
                );
            }
        }
        if mmr_spec::row_cache_prefix(old(bitmap), row, old_cache, old_emitted, start) {
            mmr_properties::recorded_span_cache_pixels(
                old(bitmap), bitmap, row, start, end, black, old_cache, transitions@,
                old_emitted, *emitted_black, width,
            );
        }
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(
    requires mmr_spec::transition_prefix_valid(old(transitions)@, width, width),
    ensures
        mmr_spec::reference_line_valid(final(transitions)@, width),
        final(transitions)@.len() == old(transitions)@.len() + 2,
        final(transitions)@ == old(transitions)@.push(width).push(width),
))]
pub(super) fn finish_transition_row(transitions: &mut Vec<u32>, width: u32) {
    #[cfg(verus_keep_ghost)]
    proof_decl! {
        let ghost before = transitions@;
    }
    transitions.push(width);
    transitions.push(width);
    #[cfg(verus_keep_ghost)]
    production_mmr_transition_finish!(before, width);
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        mmr_spec::bitmap_storage_valid(bitmap),
        row < bitmap.height,
        width <= bitmap.width,
    ensures match out {
        Ok(()) =>
            mmr_spec::reference_line_valid(final(transitions)@, width)
            && final(transitions)@.len() <= width as nat + 2,
        Err(_) => true,
    },
))]
#[cfg(test)]
pub(super) fn collect_transitions(
    bitmap: &Bitmap,
    row: u32,
    width: u32,
    transitions: &mut Vec<u32>,
) -> Result<(), MmrError> {
    transitions.clear();
    #[cfg(verus_keep_ghost)]
    production_mmr_transition_empty!(width);
    let mut previous_black = false;
    let row_index = usize::try_from(row).map_err(|_error| MmrError::IntegerOverflow)?;
    let row_base = row_index
        .checked_mul(bitmap.stride)
        .ok_or(MmrError::IntegerOverflow)?;
    let mut x = 0_u32;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            mmr_spec::bitmap_storage_valid(bitmap),
            row < bitmap.height,
            width <= bitmap.width,
            x <= width,
            mmr_spec::transition_prefix_valid(transitions@, x, width),
            transitions@.len() <= x as nat,
        decreases width - x,
    ))]
    while x < width {
        #[cfg(verus_keep_ghost)]
        production_mmr_span_byte!(bitmap, row, x);
        let byte_in_row = usize::try_from(x / 8).map_err(|_error| MmrError::IntegerOverflow)?;
        let byte_index = row_base
            .checked_add(byte_in_row)
            .ok_or(MmrError::IntegerOverflow)?;
        if byte_index >= bitmap.data.len() {
            return Err(MmrError::InvalidTransition);
        }
        let bit = 7_u32.checked_sub(x % 8).ok_or(MmrError::IntegerOverflow)?;
        let black = bitmap.data[byte_index] & (1_u8 << bit) != 0;
        let changed = black != previous_black;
        #[cfg(verus_keep_ghost)]
        proof_decl! {
            let ghost before = transitions@;
        }
        if changed {
            transitions.push(x);
            previous_black = black;
        }
        #[cfg(verus_keep_ghost)]
        production_mmr_transition_advance!(before, x, width, changed);
        x = x.checked_add(1).ok_or(MmrError::IntegerOverflow)?;
    }
    // One real imaginary end transition and one b2 sentinel.
    #[cfg(verus_keep_ghost)]
    proof_decl! {
        let ghost before = transitions@;
    }
    transitions.push(width);
    transitions.push(width);
    #[cfg(verus_keep_ghost)]
    production_mmr_transition_finish!(before, width);
    Ok(())
}

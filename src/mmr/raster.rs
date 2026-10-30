//! Raster implementation.

#[cfg(verus_keep_ghost)]
use super::*;
#[cfg(not(verus_keep_ghost))]
use super::{
    bit_reader, decode_horizontal_runs, decode_mode, enforce_input_limit, enforce_work,
    fill_recorded_span, finish_transition_row, prepare_transition_storage, reference_elements_from,
    BitReader, Bitmap, Limits, MmrError, MmrScratch, Mode,
};

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok((bitmap, reader)) =>
            crate::spec::bitmap::storage_valid(&bitmap)
            && mmr_spec::reader_valid(&reader)
            && reader.bytes@ == data@
            && mmr_spec::bitmap_decodes(
                data@, width, height, &bitmap, reader.bit_position as nat,
            ),
        Err(_) => true,
    },
))]
#[allow(clippy::too_many_lines)]
pub(super) fn decode_bitmap<'a>(
    data: &'a [u8],
    width: u32,
    height: u32,
    limits: &Limits,
    scratch: &mut MmrScratch,
) -> Result<(Bitmap, BitReader<'a>), MmrError> {
    enforce_input_limit(data, limits)?;
    if data.len() > usize::MAX / 8 {
        return Err(MmrError::IntegerOverflow);
    }
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or(MmrError::IntegerOverflow)?;
    let mut work = pixels.checked_mul(2).ok_or(MmrError::IntegerOverflow)?;
    enforce_work(work, limits)?;
    let mut bitmap = match Bitmap::blank(width, height, limits) {
        Ok(bitmap) => bitmap,
        Err(error) => return Err(MmrError::Bitmap(error)),
    };
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::views::bitmap_fields(&bitmap);
        reveal(crate::spec::bitmap::storage_valid);
        reveal(mmr_spec::bitmap_storage_valid);
        crate::bitmap::bitmap_shared::stride_covers_width(width);
        mmr_properties::zeroed_rows_are_white(&bitmap);
    }
    prepare_transition_storage(width, &bitmap, limits, scratch)?;
    let mut transitions = &mut scratch.transitions;
    let mut row_transitions = &mut scratch.row_transitions;
    transitions.push(width);
    transitions.push(width);
    #[cfg(verus_keep_ghost)]
    proof! {
        reveal(mmr_spec::reference_line_valid);
        reveal(mmr_spec::transitions_ordered);
        mmr_properties::initial_reference_cache(width);
        assert(transitions@ == Seq::empty().push(width).push(width));
    }

    let mut reader = bit_reader(data);
    #[cfg(verus_keep_ghost)]
    proof! {
        reveal(mmr_spec::reader_valid);
    }
    let max_modes = u64::from(width)
        .checked_mul(2)
        .and_then(|value| value.checked_add(2))
        .ok_or(MmrError::IntegerOverflow)?;
    #[cfg(verus_keep_ghost)]
    production_mmr_trace_begin!(traced_rows);

    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            row <= height,
            bitmap.width == width,
            bitmap.height == height,
            crate::spec::bitmap::storage_valid(&bitmap),
            mmr_spec::bitmap_storage_valid(&bitmap),
            mmr_spec::future_rows_white(&bitmap, row as nat),
            row == 0 ==> mmr_spec::white_reference_cache(transitions@, width),
            row > 0 ==> mmr_spec::row_cache_matches(
                &bitmap, (row - 1) as u32, transitions@,
            ),
            mmr_spec::reference_line_valid(transitions@, width),
            transitions@.len() <= width as nat + 2,
            mmr_spec::reader_valid(&reader),
            reader.bytes@ == data@,
            mmr_spec::raster_outer(
                data@, width, row, &bitmap, traced_rows, transitions@,
                reader.bit_position as nat,
            ),
        decreases height - row,
    ))]
    for row in 0..height {
        let mut x = 0_u32;
        let mut black = false;
        let mut emitted_black = false;
        let mut initial = true;
        let mut mode_count = 0_u64;
        let mut reference_index = 0_usize;
        row_transitions.clear();
        #[cfg(verus_keep_ghost)]
        production_mmr_transition_empty!(width);
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::views::bitmap_fields(&bitmap);
            reveal(crate::spec::bitmap::storage_valid);
            mmr_properties::empty_row_cache(&bitmap, row);
            mmr_properties::white_future_advance(&bitmap, row as nat, row as nat + 1);
        }
        #[cfg(verus_keep_ghost)]
        production_mmr_trace_row_start!(traced_start, traced_steps, reader);

        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                row < height,
                x <= width,
                mode_count <= max_modes,
                bitmap.width == width,
                bitmap.height == height,
                bitmap.stride as nat
                    == crate::spec::bitmap::stride_for_width(bitmap.width as nat),
                mmr_spec::bitmap_storage_valid(&bitmap),
                crate::spec::bitmap::storage_valid(&bitmap),
                mmr_spec::future_rows_white(&bitmap, row as nat + 1),
                mmr_spec::row_cache_prefix(
                    &bitmap, row, row_transitions@, emitted_black, x,
                ),
                row > 0 ==> mmr_spec::row_cache_matches(
                    &bitmap, (row - 1) as u32, transitions@,
                ),
                mmr_spec::reference_line_valid(transitions@, width),
                transitions@.len() <= width as nat + 2,
                reference_index < transitions@.len(),
                mmr_properties::reference_prefix_skipped(
                    transitions@, x, initial, reference_index as int,
                ),
                mmr_spec::transition_prefix_valid(row_transitions@, x, width),
                row_transitions@.len() <= x as nat,
                mmr_spec::reader_valid(&reader),
                reader.bytes@ == data@,
                mmr_spec::raster_inner(
                    data@, width, row, &bitmap, traced_rows, transitions@,
                    traced_start, traced_steps, initial,
                    mmr_spec::raster_state(
                        reader.bit_position as nat, x, black,
                        row_transitions@, emitted_black,
                    ),
                ),
            decreases max_modes + 1 - mode_count,
        ))]
        while x < width {
            mode_count = mode_count.checked_add(1).ok_or(MmrError::IntegerOverflow)?;
            work = work.checked_add(1).ok_or(MmrError::IntegerOverflow)?;
            enforce_work(work, limits)?;
            if mode_count > max_modes {
                return Err(MmrError::TooManyModes);
            }
            #[cfg(verus_keep_ghost)]
            production_mmr_mode_budget!(mode_count, max_modes);

            #[cfg(verus_keep_ghost)]
            proof_decl! {
                let ghost previous_reference_index = reference_index;
                let ghost reference_x = x;
            }
            let (b1, b2) = reference_elements_from(
                transitions,
                x,
                black,
                width,
                initial,
                &mut reference_index,
            )?;
            #[cfg(verus_keep_ghost)]
            proof! {
                assert forall|previous: int| 0 <= previous < reference_index implies
                    !mmr_spec::reference_position_eligible(
                        #[trigger] transitions@[previous], x, initial,
                    ) by {
                    if previous < previous_reference_index {
                        assert(!mmr_spec::reference_position_eligible(
                            transitions@[previous], x, initial,
                        ));
                    }
                }
                assert(mmr_spec::reference_pair_matches(
                    transitions@, x, black, width, initial, 0, b1, b2,
                ));
                assert forall|previous: int| 0 <= previous < reference_index implies
                    #[trigger] transitions@[previous] <= reference_x by {}
            }
            #[cfg(verus_keep_ghost)]
            production_mmr_trace_before!(
                traced_before,
                reader,
                x,
                black,
                row_transitions,
                emitted_black
            );
            let mode_offset = reader.bit_position;
            match decode_mode(&mut reader)? {
                Mode::Extension => {
                    return Err(MmrError::ExtensionMode {
                        bit_offset: mode_offset,
                    });
                }
                Mode::Pass => {
                    #[cfg(verus_keep_ghost)]
                    production_mmr_trace_mode_end!(traced_mode_end, reader);
                    if b2 <= x || b2 > width {
                        return Err(MmrError::NonProgress);
                    }
                    #[cfg(verus_keep_ghost)]
                    production_mmr_pass_progress!(x, b2, width);
                    fill_recorded_span(
                        &mut bitmap,
                        row,
                        x,
                        b2,
                        black,
                        width,
                        row_transitions,
                        &mut emitted_black,
                    )?;
                    x = b2;
                    #[cfg(verus_keep_ghost)]
                    production_mmr_trace_step!(
                        data,
                        transitions,
                        width,
                        traced_start,
                        traced_steps,
                        traced_before,
                        Mode::Pass,
                        traced_mode_end,
                        b1,
                        b2,
                        x,
                        reader,
                        x,
                        black,
                        row_transitions,
                        emitted_black
                    );
                }
                Mode::Horizontal => {
                    #[cfg(verus_keep_ghost)]
                    production_mmr_trace_mode_end!(traced_mode_end, reader);
                    let (after_first, end) = decode_horizontal_runs(&mut reader, black, x, width)?;
                    fill_recorded_span(
                        &mut bitmap,
                        row,
                        x,
                        after_first,
                        black,
                        width,
                        row_transitions,
                        &mut emitted_black,
                    )?;
                    fill_recorded_span(
                        &mut bitmap,
                        row,
                        after_first,
                        end,
                        !black,
                        width,
                        row_transitions,
                        &mut emitted_black,
                    )?;
                    x = end;
                    #[cfg(verus_keep_ghost)]
                    production_mmr_trace_step!(
                        data,
                        transitions,
                        width,
                        traced_start,
                        traced_steps,
                        traced_before,
                        Mode::Horizontal,
                        traced_mode_end,
                        b1,
                        b2,
                        after_first,
                        reader,
                        x,
                        black,
                        row_transitions,
                        emitted_black
                    );
                }
                Mode::Vertical(displacement) => {
                    #[cfg(verus_keep_ghost)]
                    production_mmr_trace_mode_end!(traced_mode_end, reader);
                    let target = match displacement {
                        -3 => b1.checked_sub(3),
                        -2 => b1.checked_sub(2),
                        -1 => b1.checked_sub(1),
                        0 => Some(b1),
                        1 => b1.checked_add(1),
                        2 => b1.checked_add(2),
                        3 => b1.checked_add(3),
                        _ => None,
                    }
                    .ok_or(MmrError::InvalidTransition)?;
                    if target < x || target > width {
                        return Err(MmrError::InvalidTransition);
                    }
                    #[cfg(verus_keep_ghost)]
                    production_mmr_vertical_progress!(x, target, width, black);
                    fill_recorded_span(
                        &mut bitmap,
                        row,
                        x,
                        target,
                        black,
                        width,
                        row_transitions,
                        &mut emitted_black,
                    )?;
                    x = target;
                    black = !black;
                    #[cfg(verus_keep_ghost)]
                    production_mmr_trace_step!(
                        data,
                        transitions,
                        width,
                        traced_start,
                        traced_steps,
                        traced_before,
                        Mode::Vertical(displacement),
                        traced_mode_end,
                        b1,
                        b2,
                        x,
                        reader,
                        x,
                        black,
                        row_transitions,
                        emitted_black
                    );
                }
            }
            #[cfg(verus_keep_ghost)]
            proof! {
                crate::views::bitmap_fields(&bitmap);
                reveal(crate::spec::bitmap::storage_valid);
                assert(reference_x <= x);
            }
            initial = false;
            #[cfg(verus_keep_ghost)]
            proof! {
                assert forall|previous: int| 0 <= previous < reference_index implies
                    !mmr_spec::reference_position_eligible(
                        #[trigger] transitions@[previous], x, initial,
                    ) by {
                    assert(transitions@[previous] <= reference_x);
                }
            }
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::views::bitmap_fields(&bitmap);
            reveal(crate::spec::bitmap::storage_valid);
            reveal(mmr_spec::bitmap_storage_valid);
            mmr_properties::complete_row_cache(&bitmap, row, row_transitions@, emitted_black);
        }
        #[cfg(verus_keep_ghost)]
        production_mmr_trace_row_complete!(
            traced_row,
            traced_start,
            traced_steps,
            reader,
            x,
            black,
            row_transitions,
            emitted_black
        );

        #[cfg(verus_keep_ghost)]
        proof_decl! { let ghost completed_cache = row_transitions@; }
        finish_transition_row(row_transitions, width);
        #[cfg(verus_keep_ghost)]
        proof! { mmr_properties::finished_row_cache(&bitmap, row, completed_cache, width); }
        core::mem::swap(&mut transitions, &mut row_transitions);
        #[cfg(verus_keep_ghost)]
        proof! { assert(mmr_spec::row_cache_matches(&bitmap, row, transitions@)); }
        #[cfg(verus_keep_ghost)]
        production_mmr_trace_row_push!(data, width, bitmap, traced_rows, traced_row);
        #[cfg(verus_keep_ghost)]
        production_mmr_row_termination!(row, height);
    }

    #[cfg(verus_keep_ghost)]
    production_mmr_trace_output!(data, width, height, bitmap, reader, traced_rows);
    Ok((bitmap, reader))
}

//! Reference implementation.

#[cfg(not(verus_keep_ghost))]
use super::MmrError;
#[cfg(verus_keep_ghost)]
use super::*;

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        mmr_spec::reference_line_valid(transitions@, width),
        x <= width,
    ensures
    out.is_ok(),
    match out {
        Ok((b1, b2)) => x <= b1 <= b2 <= width
            && mmr_spec::reference_pair_matches(
                transitions@, x, black, width, initial, 0, b1, b2,
            ),
        Err(_) => true,
    },
))]
pub(super) fn reference_elements(
    transitions: &[u32],
    x: u32,
    black: bool,
    width: u32,
    initial: bool,
) -> Result<(u32, u32), MmrError> {
    if transitions.len() < 2 {
        return Err(MmrError::InvalidTransition);
    }
    let search_end = transitions
        .len()
        .checked_sub(1)
        .ok_or(MmrError::IntegerOverflow)?;
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            mmr_spec::reference_line_valid(transitions@, width),
            x <= width,
            search_end == transitions@.len() - 1,
            index <= search_end,
            forall|previous: int| 0 <= previous < index ==>
                !mmr_spec::reference_position_eligible(
                    #[trigger] transitions@[previous], x, initial,
                ) || (previous % 2 == 0) == black,
        decreases search_end - index,
    ))]
    while index < search_end {
        let position = *transitions.get(index).ok_or(MmrError::InvalidTransition)?;
        if position > x || (initial && position == x) {
            // Reference transitions alternate from imaginary white. Therefore
            // the colour immediately after even-indexed transitions is black.
            let post_transition_black = index % 2 == 0;
            if post_transition_black != black {
                let next_index = index.checked_add(1).ok_or(MmrError::IntegerOverflow)?;
                let b2 = *transitions
                    .get(next_index)
                    .ok_or(MmrError::InvalidTransition)?;
                #[cfg(verus_keep_ghost)]
                production_mmr_reference_bounds!(
                    transitions,
                    index,
                    next_index,
                    position,
                    b2,
                    width
                );
                #[cfg(verus_keep_ghost)]
                proof! {
                    assert(mmr_spec::first_reference_candidate(
                        transitions@, x, black, initial, 0, index as int,
                    ));
                    assert(mmr_spec::reference_pair_matches(
                        transitions@, x, black, width, initial, 0, position, b2,
                    ));
                }
                return Ok((position, b2));
            }
        }
        index = index.checked_add(1).ok_or(MmrError::IntegerOverflow)?;
    }
    // T.6 2.2.5.2 places either missing reference element at the
    // imaginary changing element immediately after the line.
    Ok((width, width))
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        mmr_spec::reference_line_valid(transitions@, width),
        x <= width,
        *old(search_start) < transitions@.len(),
    ensures
        *final(search_start) < transitions@.len(),
        out.is_ok(),
        match out {
            Ok((b1, b2)) => x <= b1 <= b2 <= width
                && mmr_spec::reference_cursor_matches(
                    transitions@, x, initial,
                    *old(search_start) as int, *final(search_start) as int,
                )
                && mmr_spec::reference_pair_matches(
                    transitions@, x, black, width, initial,
                    *old(search_start) as int, b1, b2,
                )
                && (mmr_properties::reference_prefix_skipped(
                    transitions@, x, initial, *old(search_start) as int,
                ) ==> mmr_spec::reference_pair_matches(
                        transitions@, x, black, width, initial, 0, b1, b2,
                    )),
            Err(_) => true,
        },
))]
#[allow(clippy::too_many_lines)]
pub(super) fn reference_elements_from(
    transitions: &[u32],
    x: u32,
    black: bool,
    width: u32,
    initial: bool,
    search_start: &mut usize,
) -> Result<(u32, u32), MmrError> {
    let search_end = transitions
        .len()
        .checked_sub(1)
        .ok_or(MmrError::InvalidTransition)?;
    if *search_start > search_end {
        return Err(MmrError::InvalidTransition);
    }
    let mut index = *search_start;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            mmr_spec::reference_line_valid(transitions@, width),
            x <= width,
            search_end == transitions@.len() - 1,
            *old(search_start) <= index <= search_end,
            forall|previous: int| *old(search_start) <= previous < index ==>
                !mmr_spec::reference_position_eligible(
                    #[trigger] transitions@[previous], x, initial,
                ),
        ensures
            mmr_spec::reference_cursor_matches(
                transitions@, x, initial, *old(search_start) as int, index as int,
            ),
        decreases search_end - index,
    ))]
    while index < search_end {
        let position = *transitions.get(index).ok_or(MmrError::InvalidTransition)?;
        if position > x || (initial && position == x) {
            break;
        }
        index = index.checked_add(1).ok_or(MmrError::IntegerOverflow)?;
    }
    *search_start = index;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            mmr_spec::reference_line_valid(transitions@, width),
            x <= width,
            search_end == transitions@.len() - 1,
            *search_start <= search_end,
            *search_start <= index <= search_end,
            mmr_spec::reference_cursor_matches(
                transitions@, x, initial, *old(search_start) as int, *search_start as int,
            ),
            forall|previous: int| *old(search_start) <= previous < index ==>
                !mmr_spec::reference_position_eligible(
                    #[trigger] transitions@[previous], x, initial,
                ) || (previous % 2 == 0) == black,
        decreases search_end - index,
    ))]
    while index < search_end {
        let position = *transitions.get(index).ok_or(MmrError::InvalidTransition)?;
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal(mmr_spec::reference_line_valid);
            reveal(mmr_spec::transitions_ordered);
            assert(transitions@[*search_start as int] >= x);
            if *search_start < index {
                assert(transitions@[*search_start as int] <= transitions@[index as int]);
            }
        }
        if position < x {
            return Err(MmrError::InvalidTransition);
        }
        let post_transition_black = index % 2 == 0;
        if post_transition_black != black {
            let next_index = index.checked_add(1).ok_or(MmrError::IntegerOverflow)?;
            let b2 = *transitions
                .get(next_index)
                .ok_or(MmrError::InvalidTransition)?;
            #[cfg(verus_keep_ghost)]
            production_mmr_reference_bounds!(transitions, index, next_index, position, b2, width);
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(mmr_spec::first_reference_candidate(
                    transitions@, x, black, initial, *old(search_start) as int, index as int,
                ));
                assert(mmr_spec::reference_pair_matches(
                    transitions@, x, black, width, initial,
                    *old(search_start) as int, position, b2,
                ));
                if mmr_properties::reference_prefix_skipped(
                    transitions@, x, initial, *old(search_start) as int,
                ) {
                    mmr_properties::reference_suffix_is_global(
                        transitions@, x, black, width, initial,
                        *old(search_start) as int, position, b2,
                    );
                }
            }
            return Ok((position, b2));
        }
        index = index.checked_add(1).ok_or(MmrError::IntegerOverflow)?;
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(mmr_spec::reference_pair_matches(
            transitions@, x, black, width, initial,
            *old(search_start) as int, width, width,
        ));
        if mmr_properties::reference_prefix_skipped(
            transitions@, x, initial, *old(search_start) as int,
        ) {
            mmr_properties::reference_suffix_is_global(
                transitions@, x, black, width, initial,
                *old(search_start) as int, width, width,
            );
        }
    }
    Ok((width, width))
}

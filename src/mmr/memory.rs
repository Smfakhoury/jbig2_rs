//! Memory implementation.

#[cfg(verus_keep_ghost)]
use super::*;
#[cfg(not(verus_keep_ghost))]
use super::{Bitmap, DecodeError, LimitExceeded, LimitKind, Limits, MmrError, MmrScratch};

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) => data@.len() <= limits.max_stream_bytes,
        Err(_) => true,
    },
))]
pub(super) fn enforce_input_limit(data: &[u8], limits: &Limits) -> Result<(), MmrError> {
    let actual = u64::try_from(data.len()).map_err(|_error| MmrError::IntegerOverflow)?;
    if actual > limits.max_stream_bytes {
        return Err(MmrError::Bitmap(DecodeError::LimitExceeded(
            LimitExceeded {
                kind: LimitKind::StreamBytes,
                limit: limits.max_stream_bytes,
                actual,
            },
        )));
    }

    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) => actual <= limits.max_work_units,
        Err(_) => true,
    },
))]
pub(super) fn enforce_work(actual: u64, limits: &Limits) -> Result<(), MmrError> {
    if actual > limits.max_work_units {
        return Err(MmrError::Bitmap(DecodeError::LimitExceeded(
            LimitExceeded {
                kind: LimitKind::Work,
                limit: limits.max_work_units,
                actual,
            },
        )));
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) =>
            final(scratch).transitions@.len() == 0
            && final(scratch).row_transitions@.len() == 0
            && final(scratch).retained_bytes <= limits.max_decoder_managed_memory_bytes,
        Err(_) => true,
    },
))]
pub(super) fn prepare_transition_storage(
    width: u32,
    bitmap: &Bitmap,
    limits: &Limits,
    scratch: &mut MmrScratch,
) -> Result<(), MmrError> {
    let capacity_u64 = u64::from(width)
        .checked_add(2)
        .ok_or(MmrError::IntegerOverflow)?;
    let transition_bytes = capacity_u64
        .checked_mul(4)
        .ok_or(MmrError::IntegerOverflow)?;
    let all_transition_bytes = transition_bytes
        .checked_mul(2)
        .ok_or(MmrError::IntegerOverflow)?;
    let bitmap_bytes = bitmap.allocated_bytes();
    let total = bitmap_bytes
        .checked_add(all_transition_bytes)
        .ok_or(MmrError::IntegerOverflow)?;
    if total > limits.max_decoder_managed_memory_bytes {
        return Err(MmrError::Bitmap(DecodeError::LimitExceeded(
            LimitExceeded {
                kind: LimitKind::Memory,
                limit: limits.max_decoder_managed_memory_bytes,
                actual: total,
            },
        )));
    }
    {
        let retained_total = bitmap_bytes
            .checked_add(scratch.retained_bytes())
            .ok_or(MmrError::IntegerOverflow)?;
        if retained_total > limits.max_decoder_managed_memory_bytes {
            scratch.release();
        }
    }
    #[cfg(verus_keep_ghost)]
    production_mmr_memory_bound!(bitmap_bytes, transition_bytes, all_transition_bytes, limits);

    let Ok(capacity) = usize::try_from(capacity_u64) else {
        return Err(MmrError::IntegerOverflow);
    };
    scratch.clear();
    #[cfg(test)]
    {
        if scratch.transitions.capacity() < capacity {
            scratch.reserve_events += 1;
        }
        if scratch.row_transitions.capacity() < capacity {
            scratch.reserve_events += 1;
        }
    }
    if scratch.transitions.try_reserve(capacity).is_err() {
        return Err(MmrError::Allocation {
            requested_bytes: transition_bytes,
        });
    }
    if scratch.row_transitions.try_reserve(capacity).is_err() {
        scratch.retained_bytes = observed_transition_bytes(scratch);
        return Err(MmrError::Allocation {
            requested_bytes: transition_bytes,
        });
    }
    {
        scratch.retained_bytes = observed_transition_bytes(scratch);
        let actual_total = bitmap_bytes
            .checked_add(scratch.retained_bytes())
            .ok_or(MmrError::IntegerOverflow)?;
        if actual_total > limits.max_decoder_managed_memory_bytes {
            scratch.release();
            return Err(MmrError::Bitmap(DecodeError::LimitExceeded(
                LimitExceeded {
                    kind: LimitKind::Memory,
                    limit: limits.max_decoder_managed_memory_bytes,
                    actual: actual_total,
                },
            )));
        }
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(super) fn observed_transition_bytes(scratch: &MmrScratch) -> u64 {
    let first = scratch.transitions.capacity() as u64;
    let second = scratch.row_transitions.capacity() as u64;
    first.saturating_add(second).saturating_mul(4)
}

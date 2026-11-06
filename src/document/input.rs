use super::{DecodeError, LimitExceeded, LimitKind, Limits, MalformedError, ParsedSegment, Read};

#[cfg(verus_keep_ghost)]
use super::document_spec;
#[cfg(verus_keep_ghost)]
use vstd::{layout, prelude::*};

pub(crate) fn read_bounded_input<R: Read>(
    reader: &mut R,
    limits: &Limits,
) -> Result<Vec<u8>, DecodeError> {
    let mut output = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let count = reader.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        let new_length = output
            .len()
            .checked_add(count)
            .ok_or(MalformedError::IntegerOverflow)?;
        let new_length_u64 =
            u64::try_from(new_length).map_err(|_| MalformedError::IntegerOverflow)?;
        if new_length_u64 > limits.max_stream_bytes {
            return Err(LimitExceeded {
                kind: LimitKind::StreamBytes,
                limit: limits.max_stream_bytes,
                actual: new_length_u64,
            }
            .into());
        }
        output
            .try_reserve(count)
            .map_err(|_| DecodeError::Allocation {
                requested_bytes: new_length_u64,
            })?;
        output.extend_from_slice(&chunk[..count]);
    }
    Ok(output)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) => length as nat <= limits.max_stream_bytes,
        Err(_) => true,
    },
    out == document_spec::quota_result(
        length as nat, limits.max_stream_bytes, LimitKind::StreamBytes,
    ),
))]
pub(super) fn enforce_stream_limit(length: usize, limits: &Limits) -> Result<(), DecodeError> {
    let Ok(actual) = u64::try_from(length) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    if actual > limits.max_stream_bytes {
        return Err(LimitExceeded {
            kind: LimitKind::StreamBytes,
            limit: limits.max_stream_bytes,
            actual,
        }
        .into());
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) => count as nat + 1 <= limits.max_segments,
        Err(_) => true,
    },
    out == document_spec::next_count_quota_result(
        count, limits.max_segments, LimitKind::Segments,
    ),
))]
pub(super) fn enforce_segment_limit(count: usize, limits: &Limits) -> Result<(), DecodeError> {
    let Some(next_count) = count.checked_add(1) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    let Ok(actual) = u64::try_from(next_count) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    if actual > u64::from(limits.max_segments) {
        return Err(LimitExceeded {
            kind: LimitKind::Segments,
            limit: u64::from(limits.max_segments),
            actual,
        }
        .into());
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) => count as nat + 1 <= limits.max_pages,
        Err(_) => true,
    },
    out == document_spec::next_count_quota_result(
        count, limits.max_pages, LimitKind::Pages,
    ),
))]
pub(super) fn enforce_page_limit(count: usize, limits: &Limits) -> Result<(), DecodeError> {
    let Some(next_count) = count.checked_add(1) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    let Ok(actual) = u64::try_from(next_count) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    if actual > u64::from(limits.max_pages) {
        return Err(LimitExceeded {
            kind: LimitKind::Pages,
            limit: u64::from(limits.max_pages),
            actual,
        }
        .into());
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out == document_spec::quota_result(
        input_length as nat + segments@.len()
            + document_spec::reference_total(segments@, segments@.len()),
        limits.max_work_units,
        LimitKind::Work,
    ),
))]
pub(super) fn enforce_work_limit(
    input_length: usize,
    segments: &[ParsedSegment<'_>],
    limits: &Limits,
) -> Result<(), DecodeError> {
    let Ok(input_work) = u64::try_from(input_length) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    let Ok(segment_work) = u64::try_from(segments.len()) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    let mut reference_work = 0_u64;
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index <= segments@.len(),
            input_work as nat == input_length as nat,
            segment_work as nat == segments@.len(),
            reference_work as nat == document_spec::reference_total(segments@, index as nat),
        decreases segments@.len() - index,
    ))]
    while index < segments.len() {
        #[cfg(verus_keep_ghost)]
        proof! {
            document_spec::reference_total_monotone(segments@, index as nat + 1, segments@.len());
            reveal_with_fuel(document_spec::reference_total, 1);
        }
        let Ok(count) = u64::try_from(segments[index].header.referred_segments.len()) else {
            return Err(MalformedError::IntegerOverflow.into());
        };
        let Some(next) = reference_work.checked_add(count) else {
            return Err(MalformedError::IntegerOverflow.into());
        };
        reference_work = next;
        index += 1;
    }
    let Some(partial) = input_work.checked_add(segment_work) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    let Some(actual) = partial.checked_add(reference_work) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    if actual > limits.max_work_units {
        return Err(LimitExceeded {
            kind: LimitKind::Work,
            limit: limits.max_work_units,
            actual,
        }
        .into());
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) => input_length as nat <= limits.max_work_units,
        Err(_) => true,
    },
    out == document_spec::quota_result(
        input_length as nat, limits.max_work_units, LimitKind::Work,
    ),
))]
pub(super) fn enforce_input_work(input_length: usize, limits: &Limits) -> Result<(), DecodeError> {
    let Ok(actual) = u64::try_from(input_length) else {
        return Err(MalformedError::IntegerOverflow.into());
    };
    if actual > limits.max_work_units {
        return Err(LimitExceeded {
            kind: LimitKind::Work,
            limit: limits.max_work_units,
            actual,
        }
        .into());
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(bytes) =>
            bytes as nat == count as nat * layout::size_of::<T>(),
        Err(_) =>
            count as nat * layout::size_of::<T>() > usize::MAX
            || count as nat * layout::size_of::<T>() > u64::MAX,
    },
))]
pub(super) fn allocation_size<T>(count: usize) -> Result<u64, MalformedError> {
    let Some(bytes) = count.checked_mul(std::mem::size_of::<T>()) else {
        return Err(MalformedError::IntegerOverflow);
    };
    let Ok(bytes) = u64::try_from(bytes) else {
        return Err(MalformedError::IntegerOverflow);
    };
    Ok(bytes)
}

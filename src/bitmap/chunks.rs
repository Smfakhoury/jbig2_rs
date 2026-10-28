#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures
        out.is_some() ==> start as nat + 8 <= bytes@.len(),
        out.is_some() == (start as nat + 8 <= bytes@.len()),
        out.is_some() ==> out == Some(crate::spec::bitmap::chunk_value(bytes@, start as int)),
))]
pub(super) fn read_chunk(bytes: &[u8], start: usize) -> Option<u64> {
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(bytes@.len() == bytes.len());
        assert(bytes@.len() <= usize::MAX);
    }
    let end = start.checked_add(8)?;
    let chunk = bytes.get(start..end)?;
    let array = [
        *chunk.first()?,
        *chunk.get(1)?,
        *chunk.get(2)?,
        *chunk.get(3)?,
        *chunk.get(4)?,
        *chunk.get(5)?,
        *chunk.get(6)?,
        *chunk.get(7)?,
    ];
    Some(
        (u64::from(array[0]) << 56)
            | (u64::from(array[1]) << 48)
            | (u64::from(array[2]) << 40)
            | (u64::from(array[3]) << 32)
            | (u64::from(array[4]) << 24)
            | (u64::from(array[5]) << 16)
            | (u64::from(array[6]) << 8)
            | u64::from(array[7]),
    )
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures
        final(bytes)@.len() == old(bytes)@.len(),
        out ==> start as nat + 8 <= old(bytes)@.len(),
        out == (start as nat + 8 <= old(bytes)@.len()),
        out ==> forall|offset: int| 0 <= offset < 8 ==>
            final(bytes)@[start as int + offset]
                == crate::spec::bitmap::chunk_byte(value, offset),
        forall|index: int| 0 <= index < old(bytes)@.len()
            && (!out || index < start as int || start as int + 8 <= index)
            ==> final(bytes)@[index] == old(bytes)@[index],
))]
#[allow(clippy::cast_possible_truncation, clippy::ptr_arg)]
pub(super) fn write_chunk(bytes: &mut Vec<u8>, start: usize, value: u64) -> bool {
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(bytes@.len() == bytes.len());
        assert(bytes@.len() <= usize::MAX);
    }
    let Some(end) = start.checked_add(8) else {
        return false;
    };
    let Some(chunk) = bytes.get_mut(start..end) else {
        return false;
    };
    let array = [
        (value >> 56) as u8,
        (value >> 48) as u8,
        (value >> 40) as u8,
        (value >> 32) as u8,
        (value >> 24) as u8,
        (value >> 16) as u8,
        (value >> 8) as u8,
        value as u8,
    ];
    chunk[0] = array[0];
    chunk[1] = array[1];
    chunk[2] = array[2];
    chunk[3] = array[3];
    chunk[4] = array[4];
    chunk[5] = array[5];
    chunk[6] = array[6];
    chunk[7] = array[7];
    true
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures
        out.is_some() ==> (byte_index as nat) < bytes@.len(),
        (shift < 8 && byte_index as nat + if shift == 0 { 1nat } else { 2nat }
            <= bytes@.len()) ==> out.is_some(),
))]
pub(super) fn read_shifted_byte(bytes: &[u8], byte_index: usize, shift: u32) -> Option<u8> {
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(bytes@.len() == bytes.len());
        assert(bytes@.len() <= usize::MAX);
    }
    let first = *bytes.get(byte_index)?;
    if shift == 0 {
        return Some(first);
    }
    if shift >= 8 {
        return None;
    }
    let next_index = byte_index.checked_add(1)?;
    let second = *bytes.get(next_index)?;
    Some((first << shift) | (second >> (8 - shift)))
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures
        out.is_some() ==> (byte_index as nat) < bytes@.len(),
        (shift < 8 && 0 < bit_count <= 8
            && byte_index as nat + if shift == 0 || bit_count <= 8 - shift {
                1nat
            } else { 2nat } <= bytes@.len()) ==> out.is_some(),
))]
pub(super) fn read_shifted_tail(
    bytes: &[u8],
    byte_index: usize,
    shift: u32,
    bit_count: u32,
) -> Option<u8> {
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(bytes@.len() == bytes.len());
        assert(bytes@.len() <= usize::MAX);
    }
    let first = *bytes.get(byte_index)?;
    if shift == 0 {
        return Some(first);
    }
    if shift >= 8 || bit_count > 8 || bit_count == 0 {
        return None;
    }
    let shifted = first << shift;
    if bit_count <= 8 - shift {
        return Some(shifted);
    }
    let next_index = byte_index.checked_add(1)?;
    let second = *bytes.get(next_index)?;
    Some(shifted | (second >> (8 - shift)))
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures
        out.is_some() ==> byte_index as nat + 8 <= bytes@.len(),
        (shift < 8 && byte_index as nat + if shift == 0 { 8nat } else { 9nat }
            <= bytes@.len()) ==> out.is_some(),
))]
pub(super) fn read_shifted_chunk(bytes: &[u8], byte_index: usize, shift: u32) -> Option<u64> {
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(bytes@.len() == bytes.len());
        assert(bytes@.len() <= usize::MAX);
    }
    let first = read_chunk(bytes, byte_index)?;
    if shift == 0 {
        return Some(first);
    }
    if shift >= 8 {
        return None;
    }
    let next_index = byte_index.checked_add(8)?;
    let next = u64::from(*bytes.get(next_index)?);
    Some((first << shift) | (next >> (8 - shift)))
}

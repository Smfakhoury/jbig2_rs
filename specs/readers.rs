use vstd::prelude::*;

verus! {

pub open spec fn bit_at(bytes: Seq<u8>, position: nat) -> nat
    recommends
        position < bytes.len() * 8,
{
    ((bytes[(position / 8) as int] >> ((7 - position % 8) as u8)) & 1) as nat
}

pub open spec fn bits_value(bytes: Seq<u8>, start: nat, count: nat) -> nat
    decreases count,
{
    if count == 0 {
        0
    } else {
        bits_value(bytes, start, (count - 1) as nat) * 2
            + bit_at(bytes, (start + count - 1) as nat)
    }
}

pub open spec fn eof_error(offset: usize, needed: usize, remaining: usize) -> crate::MalformedError {
    crate::MalformedError::UnexpectedEof {
        offset: offset as u64, needed: needed as u64, remaining: remaining as u64,
    }
}

pub open spec fn bit_read_error(
    bytes: Seq<u8>, position: usize, count: u8,
) -> Option<crate::MalformedError>
    recommends position <= bytes.len() * 8, bytes.len() <= usize::MAX / 8,
{
    if count > 32 {
        Some(crate::MalformedError::InvalidBitCount(count))
    } else if count as nat > bytes.len() * 8 - position {
        Some(eof_error(position, count as usize, (bytes.len() * 8 - position) as usize))
    } else {
        None
    }
}

}

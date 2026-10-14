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

}

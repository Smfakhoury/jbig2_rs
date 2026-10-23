use vstd::prelude::*;

verus! {

pub(crate) open spec fn bits_limit(count: nat) -> nat
    decreases count,
{
    if count == 0 { 1 } else { 2 * bits_limit((count - 1) as nat) }
}

pub(crate) proof fn bits_limit_monotone(first: nat, last: nat)
    requires first <= last,
    ensures bits_limit(first) <= bits_limit(last),
    decreases last,
{
    if first < last {
        bits_limit_monotone(first, (last - 1) as nat);
    }
}

pub(crate) proof fn bits_value_bound(bytes: Seq<u8>, start: nat, count: nat)
    requires start + count <= bytes.len() * 8,
    ensures crate::spec::readers::bits_value(bytes, start, count) < bits_limit(count),
    decreases count,
{
    if count > 0 {
        bits_value_bound(bytes, start, (count - 1) as nat);
        let position = start + count - 1;
        let byte = bytes[(position / 8) as int];
        let shift = (7 - position % 8) as u8;
        assert(((byte >> shift) & 1) <= 1) by (bit_vector);
    }
}

pub(crate) proof fn bits_value_append_one(bytes: Seq<u8>, start: nat, count: nat)
    requires start + count < bytes.len() * 8,
    ensures
        crate::spec::readers::bits_value(bytes, start, count + 1)
            == crate::spec::readers::bits_value(bytes, start, count) * 2
                + crate::spec::readers::bits_value(bytes, start + count, 1),
{
    reveal_with_fuel(crate::spec::readers::bits_value, 2);
}

}

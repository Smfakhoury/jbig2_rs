use vstd::prelude::*;

verus! {

pub(super) proof fn pow2_positive(exponent: nat)
    ensures super::huffman_spec::pow2(exponent) > 0,
    decreases exponent,
{
    reveal(super::huffman_spec::pow2);
    if exponent != 0 {
        pow2_positive((exponent - 1) as nat);
    }
}

pub(super) proof fn bits_value_is_bounded(
    bytes: Seq<u8>,
    start: nat,
    count: nat,
)
    requires start + count <= bytes.len() * 8,
    ensures
        crate::spec::readers::bits_value(bytes, start, count)
            < super::huffman_spec::pow2(count),
    decreases count,
{
    reveal(crate::spec::readers::bits_value);
    reveal(crate::spec::readers::bit_at);
    reveal(super::huffman_spec::pow2);
    if count != 0 {
        bits_value_is_bounded(bytes, start, (count - 1) as nat);
        let position = start + count - 1;
        let byte = bytes[(position / 8) as int];
        let shift = (7 - position % 8) as u8;
        assert(((byte >> shift) & 1) <= 1) by (bit_vector);
    }
}

pub(super) proof fn pow2_is_byte_bounded(count: u8)
    requires count <= 8,
    ensures super::huffman_spec::pow2(count as nat) <= 256,
{
    reveal_with_fuel(super::huffman_spec::pow2, 9);
    if count == 0 {
    } else if count == 1 {
    } else if count == 2 {
    } else if count == 3 {
    } else if count == 4 {
    } else if count == 5 {
    } else if count == 6 {
    } else if count == 7 {
    } else {
        assert(count == 8);
    }
}

}

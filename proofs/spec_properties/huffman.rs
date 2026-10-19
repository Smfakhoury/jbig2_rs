use vstd::prelude::*;

verus! {

pub(super) proof fn decoded_line_value_is_deterministic(
    line: super::HuffmanLine,
    offset: nat,
    left: i32,
    right: i32,
)
    requires
        super::huffman_spec::decoded_line_value(line, offset, left),
        super::huffman_spec::decoded_line_value(line, offset, right),
    ensures left == right,
{
    reveal(super::huffman_spec::decoded_line_value);
}

}

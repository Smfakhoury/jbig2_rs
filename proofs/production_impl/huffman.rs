use vstd::prelude::*;

macro_rules! production_huffman_has_oob {
    ($table:ident) => {
        proof! {
            crate::huffman::huffman_spec::table_has_oob_definition($table);
        }
    };
}

macro_rules! production_huffman_one_bit {
    ($reader:ident, $start:ident, $bit:ident) => {
        proof! {
            crate::huffman::huffman_shared::bits_value_is_bounded(
                $reader.bytes@,
                $start as nat,
                1,
            );
            reveal_with_fuel(crate::huffman::huffman_spec::pow2, 2);
            assert($bit as nat == crate::spec::readers::bits_value(
                $reader.bytes@,
                $start as nat,
                1,
            ));
            assert($bit <= 1);
        }
    };
}

macro_rules! production_huffman_u8_bits {
    ($reader:ident, $start:ident, $count:ident, $value:ident) => {
        proof! {
            crate::huffman::huffman_shared::bits_value_is_bounded(
                $reader.bytes@,
                $start as nat,
                $count as nat,
            );
            crate::huffman::huffman_shared::pow2_is_byte_bounded($count);
            assert($value <= u8::MAX);
        }
    };
}

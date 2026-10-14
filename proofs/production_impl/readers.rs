use vstd::prelude::*;

macro_rules! production_read_bytes_addition {
    ($reader:ident, $length:ident) => {
        proof! {
            assert($length <= $reader.bytes@.len() - $reader.position);
            assert($reader.bytes@.len() == $reader.bytes.len());
            assert($length <= $reader.bytes.len() - $reader.position);
            assert($length <= usize::MAX - $reader.position);
            assert($reader.position + $length <= $reader.bytes@.len());
        }
    };
}

macro_rules! production_read_bits_before {
    ($previous_value:ident, $previous_position:ident, $value:ident, $position:ident) => {
        proof_decl! {
            let ghost $previous_value = $value;
            let ghost $previous_position = $position;
        }
    };
}

macro_rules! production_read_bits_after {
    (
        $previous_value:ident,
        $previous_position:ident,
        $value:ident,
        $position:ident,
        $byte:ident,
        $bit:ident,
        $shift:ident,
        $reader:ident,
        $start:ident
    ) => {
        proof! {
            reveal(crate::spec::readers::bits_value);
            reveal(crate::spec::readers::bit_at);
            assert($previous_position / 8 < $reader.bytes@.len());
            assert($byte == $reader.bytes@[
                ($previous_position / 8) as int
            ]);
            assert($shift == 7 - ($previous_position % 8));
            assert($bit == (($byte >> $shift) & 1) as u32);
            assert($bit as nat == (($byte >> $shift) & 1) as nat);
            assert($position == $previous_position + 1);
            assert(
                ($position - $start) as int
                    == ($previous_position - $start) as int + 1
            );
            assert(
                crate::spec::readers::bit_at(
                    $reader.bytes@,
                    $previous_position as nat,
                ) == $bit as nat
            );
            assert(
                crate::spec::readers::bits_value(
                    $reader.bytes@,
                    $start as nat,
                    ($position - $start) as nat,
                )
                    == $previous_value as nat * 2 + $bit as nat
            );
        }
    };
}

use vstd::prelude::*;

macro_rules! production_qe_transition_valid {
    ($index:expr, $state:ident) => {
        proof! {
            crate::arithmetic::arithmetic_shared::qe_transition_valid($index as int);
            assert($state.next_mps < 47);
            assert($state.next_lps < 47);
            assert($state.qe > 0);
            assert($state.qe <= 0x5601);
        }
    };
}

macro_rules! production_decode_interval_subtraction {
    ($decoder:ident, $qe:ident) => {
        proof! {
            assert($decoder.interval >= crate::arithmetic::MIN_INTERVAL);
            assert($qe <= 0x5601u32);
            assert($decoder.interval > $qe);
        }
    };
}

macro_rules! production_reveal_decoder_valid {
    () => {
        proof! {
            reveal(crate::arithmetic::arithmetic_spec::decoder_valid);
            reveal(crate::arithmetic::arithmetic_spec::decoder_state_valid);
            reveal(crate::arithmetic::arithmetic_spec::decoder_position_valid);
        }
    };
}

macro_rules! production_integer_history_index {
    ($previous:expr) => {
        proof! {
            crate::arithmetic::arithmetic_properties::binary_history_index_is_valid(
                $previous,
            );
        }
    };
}

macro_rules! production_integer_magnitude {
    ($magnitude_bits:ident, $offset:ident) => {
        proof! {
            crate::arithmetic::arithmetic_properties::integer_magnitude_fits_i64(
                $magnitude_bits,
                $offset,
            );
        }
    };
}

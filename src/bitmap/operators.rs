use super::CombinationOperator;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

impl CombinationOperator {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out == crate::spec::bitmap::combine(self, destination, source),
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    #[allow(dead_code)]
    pub(crate) const fn apply(self, destination: bool, source: bool) -> bool {
        match self {
            Self::Or => destination || source,
            Self::And => destination && source,
            Self::Xor => destination != source,
            Self::Xnor => destination == source,
            Self::Replace => source,
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures forall|bit: u32| bit < 8 ==>
            crate::spec::bitmap::byte_operator_bit(out, bit)
                == crate::spec::bitmap::combine(
                    self, crate::spec::bitmap::byte_operator_bit(destination, bit),
                    crate::spec::bitmap::byte_operator_bit(source, bit),
                ),
    ))]
    pub(super) const fn apply_byte(self, destination: u8, source: u8) -> u8 {
        let result = match self {
            Self::Or => destination | source,
            Self::And => destination & source,
            Self::Xor => destination ^ source,
            Self::Xnor => !(destination ^ source),
            Self::Replace => source,
        };
        #[cfg(verus_keep_ghost)]
        production_bitmap_byte_operator!(self, destination, source, result);
        result
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures forall|bit: u32| bit < 64 ==>
            crate::spec::bitmap::word_operator_bit(out, bit)
                == crate::spec::bitmap::combine(
                    self, crate::spec::bitmap::word_operator_bit(destination, bit),
                    crate::spec::bitmap::word_operator_bit(source, bit),
                ),
    ))]
    pub(super) const fn apply_chunk(self, destination: u64, source: u64) -> u64 {
        let result = match self {
            Self::Or => destination | source,
            Self::And => destination & source,
            Self::Xor => destination ^ source,
            Self::Xnor => !(destination ^ source),
            Self::Replace => source,
        };
        #[cfg(verus_keep_ghost)]
        production_bitmap_word_operator!(self, destination, source, result);
        result
    }
}

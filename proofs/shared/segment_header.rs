use vstd::prelude::*;

verus! {

pub(super) proof fn reference_width_runtime_fact(segment_number: u32)
    ensures crate::spec::segments::reference_width(segment_number) == if segment_number <= 256 {
        1u8
    } else if segment_number <= 65_536 {
        2u8
    } else {
        4u8
    },
{
    super::segment_header_properties::reference_width_cases(segment_number);
}

pub(super) proof fn count_code_runtime_fact(code: u8)
    requires code <= 7,
    ensures code <= 4 || code == 5 || code == 6 || code == 7,
{
    super::segment_header_properties::count_code_partition(code);
}

}

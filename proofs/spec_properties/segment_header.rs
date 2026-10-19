use vstd::prelude::*;

verus! {

pub(super) proof fn reference_width_cases(segment_number: u32)
    ensures
        crate::spec::segments::reference_width(segment_number) == 1
            <==> segment_number <= 256,
        crate::spec::segments::reference_width(segment_number) == 2
            <==> 257 <= segment_number <= 65_536,
        crate::spec::segments::reference_width(segment_number) == 4
            <==> 65_537 <= segment_number,
{
}

pub(super) proof fn count_code_partition(code: u8)
    requires code <= 7,
    ensures code <= 4 || code == 5 || code == 6 || code == 7,
{
}

}

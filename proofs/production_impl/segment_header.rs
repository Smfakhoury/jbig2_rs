use vstd::prelude::*;

macro_rules! production_reference_width {
    ($segment_number:ident) => {
        proof! {
            segment_header_shared::reference_width_runtime_fact($segment_number);
        }
    };
}

macro_rules! production_count_code_partition {
    ($count_code:ident) => {
        proof! {
            segment_header_shared::count_code_runtime_fact($count_code);
        }
    };
}

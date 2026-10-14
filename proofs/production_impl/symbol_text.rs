use vstd::prelude::*;

macro_rules! production_symbol_tracker_fields {
    ($tracker:expr) => {
        proof! {
            crate::symbol_text::symbol_text_shared::tracker_fields($tracker);
        }
    };
}

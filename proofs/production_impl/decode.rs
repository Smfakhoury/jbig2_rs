use vstd::prelude::*;

macro_rules! production_decoded_page_fields {
    ($page:ident) => {
        proof! {
            crate::views::decoded_page_fields($page);
        }
    };
}

macro_rules! production_decode_page_fields {
    ($page:expr) => {
        proof! {
            decode_spec::page_fields($page);
        }
    };
}

macro_rules! production_decode_document_fields {
    ($document:expr) => {
        proof! {
            decode_spec::document_fields($document);
        }
    };
}

macro_rules! production_collector_fields {
    ($collector:expr) => {
        proof! {
            decode_shared::collector_fields($collector);
        }
    };
}

macro_rules! production_retained_fields {
    ($budget:expr) => {
        proof! {
            decode_shared::retained_fields($budget);
        }
    };
}

macro_rules! production_work_fields {
    ($work:expr) => {
        proof! {
            decode_shared::work_fields($work);
        }
    };
}

macro_rules! production_symbol_resource_lookup {
    ($symbols:ident, $number:ident, $index:ident) => {
        proof! {
            assert(crate::halftone::halftone_spec::bitmaps_valid(
                $symbols@[$index as int].1.exported@,
            ));
            assert forall|other: int| decode_spec::first_symbol_index($symbols@, $number, other)
                implies other == $index as int by {
                if other < $index as int {
                    assert($symbols@[other].0 != $number);
                } else if other > $index as int {
                    assert($symbols@[$index as int].0 != $number);
                }
            }
        }
    };
}

macro_rules! production_symbol_resource_insert {
    ($symbols:ident, $resource:ident) => {
        proof! {
            assert forall|index: int| 0 <= index < $symbols@.len() implies
                crate::halftone::halftone_spec::bitmaps_valid(
                    #[trigger] $symbols@[index].1.exported@,
                ) by {
                if index < old($symbols)@.len() {
                    assert($symbols@[index] == old($symbols)@[index]);
                    assert(crate::halftone::halftone_spec::bitmaps_valid(
                        old($symbols)@[index].1.exported@,
                    ));
                } else {
                    assert($symbols@[index].1 == $resource);
                    assert(crate::halftone::halftone_spec::bitmaps_valid($resource.exported@));
                }
            }
        }
    };
}

macro_rules! production_pattern_store_removed {
    ($patterns:ident, $pattern_index:ident, $original_patterns:ident) => {
        proof! {
            reveal(decode_spec::pattern_store_valid);
            assert forall|entry: int| 0 <= entry < $patterns@.len()
                implies crate::halftone::halftone_spec::bitmaps_valid(
                    #[trigger] $patterns@[entry].1@
                ) by {
                if entry < $pattern_index {
                    assert($patterns@[entry] == $original_patterns[entry]);
                } else {
                    assert($patterns@[entry] == $original_patterns[entry + 1]);
                }
            }
        }
    };
}

macro_rules! production_symbol_store_removed {
    ($symbols:ident, $symbol_index:ident, $original_symbols:ident) => {
        proof! {
            reveal(decode_spec::symbol_store_valid);
            assert forall|entry: int| 0 <= entry < $symbols@.len()
                implies crate::halftone::halftone_spec::bitmaps_valid(
                    #[trigger] $symbols@[entry].1.exported@
                ) by {
                if entry < $symbol_index {
                    assert($symbols@[entry] == $original_symbols[entry]);
                    assert(crate::halftone::halftone_spec::bitmaps_valid(
                        $original_symbols[entry].1.exported@,
                    ));
                } else {
                    assert($symbols@[entry] == $original_symbols[entry + 1]);
                    assert(crate::halftone::halftone_spec::bitmaps_valid(
                        $original_symbols[entry + 1].1.exported@,
                    ));
                }
            }
        }
    };
}

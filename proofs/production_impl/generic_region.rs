use vstd::prelude::*;

macro_rules! production_generic_template_facts {
    ($template:expr) => {
        proof! {
            generic_region_shared::generic_template_runtime_facts($template);
        }
    };
}

macro_rules! production_refinement_template_facts {
    ($template:expr) => {
        proof! {
            generic_region_shared::refinement_template_runtime_facts($template);
        }
    };
}

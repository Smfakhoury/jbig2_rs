use vstd::prelude::*;

verus! {

pub(super) proof fn generic_template_runtime_facts(
    template: super::GenericTemplate,
)
    ensures
        10 <= super::generic_region_spec::generic_context_bits(template),
        super::generic_region_spec::generic_context_bits(template) <= 16,
        super::generic_region_spec::generic_sltp_context(template)
            < super::generic_region_spec::pow2(
                super::generic_region_spec::generic_context_bits(template),
            ),
{
    super::generic_region_properties::all_generic_templates_have_bounded_sltp(
        template,
    );
    match template {
        super::GenericTemplate::Template0 => {},
        super::GenericTemplate::Template1 => {},
        super::GenericTemplate::Template2 => {},
        super::GenericTemplate::Template3 => {},
    }
}

pub(super) proof fn refinement_template_runtime_facts(
    template: super::RefinementTemplate,
)
    ensures
        10 <= super::generic_region_spec::refinement_context_bits(template),
        super::generic_region_spec::refinement_context_bits(template) <= 13,
        super::generic_region_spec::refinement_sltp_context(template)
            < super::generic_region_spec::pow2(
                super::generic_region_spec::refinement_context_bits(template),
            ),
{
    super::generic_region_properties::all_refinement_templates_have_bounded_sltp(
        template,
    );
    match template {
        super::RefinementTemplate::Template0 => {},
        super::RefinementTemplate::Template1 => {},
    }
}

}

// Included in halftone_spec to preserve closed-model and field visibility.
verus! {

pub(super) proof fn work_fields(work: &super::WorkBudget)
    ensures
        work_limit(work) == work.limit,
        work_used(work) == work.used,
{}

}

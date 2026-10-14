use vstd::prelude::*;

verus! {

pub(super) proof fn collector_fields(collector: &crate::decode::PageCollector)
    ensures
        super::decode_spec::collector_pages(collector) == collector.pages@,
        super::decode_spec::collector_bitmap_bytes(collector) == collector.bitmap_bytes,
        super::decode_spec::collector_memory_limit(collector) == collector.memory_limit,
        super::decode_spec::collector_decoded_limit(collector) == collector.decoded_limit,
{
}

pub(super) proof fn retained_fields(budget: &crate::decode::RetainedBudget)
    ensures super::decode_spec::retained_bytes(budget) == budget.bytes,
{
}

pub(super) proof fn work_fields(work: &crate::decode::DecodeWork)
    ensures super::decode_spec::work_used(work) == work.used,
{
}

pub(super) proof fn runtime_page_fields(page: &crate::decode::RuntimePage)
    ensures
        super::decode_spec::runtime_last_stripe_end(page) == page.last_stripe_end,
        super::decode_spec::runtime_open_stripe_max_row(page) == page.open_stripe_max_row,
{
}

}

use vstd::prelude::*;

verus! {

pub(super) proof fn span_byte_is_in_bounds(
    bitmap: &crate::bitmap::Bitmap,
    row: u32,
    x: u32,
)
    requires
        super::mmr_spec::bitmap_storage_valid(bitmap),
        row < bitmap.height,
        x < bitmap.width,
    ensures
        super::mmr_spec::span_byte(bitmap, row, x) < bitmap.data@.len(),
{
    reveal(super::mmr_spec::bitmap_storage_valid);
    vstd::arithmetic::div_mod::lemma_fundamental_div_mod(x as int, 8);
    vstd::arithmetic::div_mod::lemma_mod_bound(x as int, 8);
    assert((x as int) == 8 * ((x as int) / 8) + (x as int) % 8);
    assert(0 <= (x as int) % 8 < 8);
    assert((x as int) < (bitmap.stride as int) * 8);
    assert((x as int) / 8 < bitmap.stride as int);
    assert((x as nat) / 8 < bitmap.stride as nat);
    assert(bitmap.stride > 0);
    let s = bitmap.stride as int;
    let q = (x as int) / 8;
    let r = row as int;
    let h = bitmap.height as int;
    assert(q < s);
    assert(r < h);
    vstd::arithmetic::mul::lemma_mul_is_distributive_add_other_way(s, r, 1);
    assert(r * s + q < (r + 1) * s);
    vstd::arithmetic::mul::lemma_mul_inequality(r + 1, h, s);
    assert((r + 1) * s <= h * s);
    assert(
        row as nat * bitmap.stride as nat + x as nat / 8
            < bitmap.height as nat * bitmap.stride as nat
    );
    reveal(super::mmr_spec::span_byte);
    assert(
        super::mmr_spec::span_byte(bitmap, row, x)
            == row as nat * bitmap.stride as nat + x as nat / 8
    );
    assert(
        bitmap.data@.len()
            == bitmap.stride as nat * bitmap.height as nat
    );
    vstd::arithmetic::mul::lemma_mul_is_commutative(
        bitmap.stride as int,
        bitmap.height as int,
    );
    assert(
        bitmap.data@.len()
            == bitmap.height as nat * bitmap.stride as nat
    );
}

pub(super) proof fn strict_progress_is_bounded(
    old_x: u32,
    new_x: u32,
    width: u32,
)
    requires old_x < new_x, new_x <= width,
    ensures width - new_x < width - old_x,
{}

pub(super) proof fn makeup_counter_terminates(
    count: u32,
    maximum: u32,
)
    requires count <= maximum,
    ensures
        maximum - count <= maximum,
        count < maximum ==> maximum - (count + 1) < maximum - count,
{}

pub(super) proof fn row_counter_terminates(row: u32, height: u32)
    requires row < height,
    ensures height - (row + 1) < height - row,
{}

pub(super) proof fn work_increment_preserves_limit(
    old_work: u64,
    new_work: u64,
    limit: u64,
)
    requires new_work == old_work + 1, new_work <= limit,
    ensures old_work <= new_work, new_work <= limit,
{}

pub(super) proof fn transition_prefix_empty(width: u32)
    ensures super::mmr_spec::transition_prefix_valid(Seq::<u32>::empty(), 0, width),
{
    reveal(super::mmr_spec::transition_prefix_valid);
}

pub(super) proof fn transition_prefix_advance(
    transitions: Seq<u32>,
    scanned: u32,
    width: u32,
)
    requires
        super::mmr_spec::transition_prefix_valid(transitions, scanned, width),
        scanned < width,
    ensures
        super::mmr_spec::transition_prefix_valid(
            transitions,
            (scanned as int + 1) as u32,
            width,
        ),
{
    reveal(super::mmr_spec::transition_prefix_valid);
}

pub(super) proof fn transition_prefix_push(
    transitions: Seq<u32>,
    scanned: u32,
    width: u32,
)
    requires
        super::mmr_spec::transition_prefix_valid(transitions, scanned, width),
        scanned < width,
    ensures
        super::mmr_spec::transition_prefix_valid(
            transitions.push(scanned),
            (scanned as int + 1) as u32,
            width,
        ),
{
    reveal(super::mmr_spec::transition_prefix_valid);
    assert forall|index: int|
        0 <= index < transitions.push(scanned).len()
    implies #[trigger] transitions.push(scanned)[index] < scanned + 1 by {
        if index < transitions.len() {
            assert(transitions.push(scanned)[index] == transitions[index]);
        } else {
            assert(index == transitions.len());
            assert(transitions.push(scanned)[index] == scanned);
        }
    }
    assert forall|left: int, right: int|
        0 <= left < right < transitions.push(scanned).len()
    implies
        #[trigger] transitions.push(scanned)[left]
            <= #[trigger] transitions.push(scanned)[right] by {
        if right < transitions.len() {
            assert(transitions.push(scanned)[left] == transitions[left]);
            assert(transitions.push(scanned)[right] == transitions[right]);
        } else {
            assert(right == transitions.len());
            assert(transitions.push(scanned)[right] == scanned);
            assert(transitions.push(scanned)[left] == transitions[left]);
            assert(transitions[left] < scanned);
        }
    }
}

pub(super) proof fn transition_prefix_finish(
    transitions: Seq<u32>,
    width: u32,
)
    requires super::mmr_spec::transition_prefix_valid(transitions, width, width),
    ensures super::mmr_spec::reference_line_valid(
        transitions.push(width).push(width),
        width,
    ),
{
    reveal(super::mmr_spec::transition_prefix_valid);
    reveal(super::mmr_spec::reference_line_valid);
    reveal(super::mmr_spec::transitions_ordered);
    assert forall|index: int|
        0 <= index < transitions.push(width).push(width).len()
    implies #[trigger] transitions.push(width).push(width)[index] <= width by {
        if index < transitions.len() {
            assert(transitions.push(width).push(width)[index] == transitions[index]);
        }
    }
    assert forall|left: int, right: int|
        0 <= left < right < transitions.push(width).push(width).len()
    implies
        #[trigger] transitions.push(width).push(width)[left]
            <= #[trigger] transitions.push(width).push(width)[right] by {
        if right < transitions.len() {
            assert(transitions.push(width).push(width)[left] == transitions[left]);
            assert(transitions.push(width).push(width)[right] == transitions[right]);
        } else if left < transitions.len() {
            assert(transitions.push(width).push(width)[left] == transitions[left]);
        }
    }
}

}

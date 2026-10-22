// Implementation accumulator invariant, included in mmr_spec without a new module.
// Its unchanged definition remains part of the private helper contracts' audit surface.
verus! {

pub(super) open spec fn transition_prefix_valid(
    transitions: Seq<u32>,
    scanned: u32,
    width: u32,
) -> bool {
    scanned <= width
        && forall|index: int| 0 <= index < transitions.len() ==>
            #[trigger] transitions[index] < scanned
        && forall|left: int, right: int|
            0 <= left < right < transitions.len() ==>
                #[trigger] transitions[left] <= #[trigger] transitions[right]
}

}

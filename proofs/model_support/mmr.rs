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

pub(super) open spec fn transition_span_result(
    before: Seq<u32>,
    after: Seq<u32>,
    before_black: bool,
    after_black: bool,
    start: u32,
    end: u32,
    black: bool,
) -> bool {
    after == (if start < end && before_black != black { before.push(start) } else { before })
        && after_black == (if start < end { black } else { before_black })
}

pub(super) open spec fn reference_cursor_matches(
    transitions: Seq<u32>, x: u32, initial: bool, start: int, cursor: int,
) -> bool {
    0 <= start <= cursor < transitions.len()
        && (cursor == transitions.len() - 1
            || reference_position_eligible(transitions[cursor], x, initial))
        && forall|previous: int| start <= previous < cursor ==>
            !reference_position_eligible(#[trigger] transitions[previous], x, initial)
}

}

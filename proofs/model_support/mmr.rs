// Source-only author navigation; named purposes are not normative approval.
// Guide, official acquisition and source identity: ../../specs/README.md
// All requirement links and inherited locators: ../../validation/traceability/requirements.json
// REQUIREMENT CONTEXT is not a direct normative equation, including for proof/policy helpers.

// Implementation accumulator invariant, included in mmr_spec without a new module.
// Its unchanged definition remains part of the private helper contracts' audit surface.
verus! {

// Model navigation: spec::transition_prefix_valid [proof-support]
// Purpose: Transition/cache prefixes and white unvisited-row frames for raster loops.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
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

// Model navigation: spec::transition_span_result [proof-support]
// Purpose: Tuple-equality bridge to the shared transition-span update.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub(super) open spec fn transition_span_result(
    before: Seq<u32>,
    after: Seq<u32>,
    before_black: bool,
    after_black: bool,
    start: u32,
    end: u32,
    black: bool,
) -> bool {
    (after, after_black) == transition_span_state(before, before_black, start, end, black)
}

// Model navigation: spec::row_cache_prefix [proof-support]
// Purpose: Transition/cache prefixes and white unvisited-row frames for raster loops.
// Audit: mmr-row-cache-prefix
// REQUIREMENT CONTEXT: T6-REFERENCE-NEXT-LINE | ITU-T T.6 (1988) | 2.2.1
// Fidelity: unresolved; no independent normative approval.
pub(super) open spec fn row_cache_prefix(
    bitmap: &crate::Bitmap, row: u32, transitions: Seq<u32>,
    emitted_black: bool, scanned: u32,
) -> bool {
    crate::spec::bitmap::storage_valid(bitmap)
        && row < bitmap.height && scanned <= bitmap.width
        && emitted_black == (transitions.len() % 2 == 1)
        && forall|x: u32| x < bitmap.width ==>
            #[trigger] crate::spec::bitmap::stored_pixel(bitmap, x, row)
                == (x < scanned && transition_black_at(transitions, x))
}

// Model navigation: spec::future_rows_white [proof-support]
// Purpose: Transition/cache prefixes and white unvisited-row frames for raster loops.
// Audit: mmr-future-rows-white
// REQUIREMENT CONTEXT: T6-REFERENCE-FIRST-WHITE | ITU-T T.6 (1988) | 2.2.1
// Fidelity: unresolved; no independent normative approval.
pub(super) open spec fn future_rows_white(bitmap: &crate::Bitmap, first: nat) -> bool {
    crate::spec::bitmap::storage_valid(bitmap)
        && forall|x: u32, y: u32| x < bitmap.width && first <= y < bitmap.height ==>
            !#[trigger] crate::spec::bitmap::stored_pixel(bitmap, x, y)
}

// Model navigation: spec::raster_outer [proof-support]
// Purpose: Transition/cache prefixes and white unvisited-row frames for raster loops.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub(super) open spec fn raster_outer(
    bytes: Seq<u8>, width: u32, row: u32, bitmap: &crate::Bitmap,
    rows: Seq<RasterRow>, reference: Seq<u32>, position: nat,
) -> bool {
    rows.len() == row
        && raster_rows(bytes, width, rows)
        && rows_cache_pixels(bitmap, rows)
        && reference == rows_reference(rows, width)
        && position == rows_position(rows)
}

// Model navigation: spec::raster_inner [proof-support]
// Purpose: Transition/cache prefixes and white unvisited-row frames for raster loops.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub(super) open spec fn raster_inner(
    bytes: Seq<u8>, width: u32, row: u32, bitmap: &crate::Bitmap,
    rows: Seq<RasterRow>, reference: Seq<u32>, start: nat,
    steps: Seq<RasterStep>, initial: bool, state: RasterState,
) -> bool {
    rows.len() == row
        && raster_rows(bytes, width, rows)
        && rows_cache_pixels(bitmap, rows)
        && reference == rows_reference(rows, width)
        && start == rows_position(rows)
        && initial == (steps.len() == 0)
        && raster_steps(bytes, reference, width, start, steps, state)
}

pub(super) proof fn bounded_bitmap_witness(
    bytes: Seq<u8>, width: u32, height: u32, bitmap: &crate::Bitmap,
    policy: super::EofbPolicy, end: nat,
)
    requires
        bitmap_decodes(bytes, width, height, bitmap, end),
        match policy {
            super::EofbPolicy::Optional => true,
            super::EofbPolicy::Required => required_eofb_prefix(bytes, end, bytes.len() * 8),
        },
    ensures bounded_bitmap_decodes(bytes, width, height, bitmap, policy),
{
    reveal(bounded_bitmap_decodes);
}

pub(super) proof fn prefix_bitmap_witness(
    bytes: Seq<u8>, width: u32, height: u32, bitmap: &crate::Bitmap,
    end: nat, consumed: usize,
)
    requires
        bitmap_decodes(bytes, width, height, bitmap, end),
        required_eofb_prefix(bytes, end, consumed as nat * 8),
    ensures prefix_bitmap_decodes(bytes, width, height, bitmap, consumed),
{
    reveal(prefix_bitmap_decodes);
}

// Model navigation: spec::reference_cursor_matches [proof-support]
// Purpose: Cursor eligibility and exclusion of skipped reference positions.
// Audit: mmr-reference-cursor
// REQUIREMENT CONTEXT: T6-REFERENCE-CHANGING-ELEMENTS | ITU-T T.6 (1988) | 2.2.2; 2.2.5.2
// Fidelity: unresolved; no independent normative approval.
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

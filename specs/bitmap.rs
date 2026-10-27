// Source-only author navigation; named purposes are not normative approval.
// Guide, official acquisition and source identity: README.md
// All requirement links and inherited locators: ../validation/traceability/requirements.json
// REQUIREMENT CONTEXT is not a direct normative equation, including for proof/policy helpers.

use vstd::prelude::*;

verus! {

// Model navigation: spec::combine [project-result-model]
// Purpose: Exact packed operator, span, fill, copy and unchanged-frame results.
// Audit: bitmap-combine
// REQUIREMENT CONTEXT: T88-COMBINATION-BOOLEAN-RESULT | ITU-T T.88 (08/2018) | 4.3
// Fidelity: unresolved; no independent normative approval.
pub(crate) open spec fn combine(
    operator: crate::bitmap::CombinationOperator,
    destination: bool,
    source: bool,
) -> bool {
    match operator {
        crate::bitmap::CombinationOperator::Or => destination || source,
        crate::bitmap::CombinationOperator::And => destination && source,
        crate::bitmap::CombinationOperator::Xor => destination != source,
        crate::bitmap::CombinationOperator::Xnor => destination == source,
        crate::bitmap::CombinationOperator::Replace => source,
    }
}

// Model navigation: spec::operator_byte [project-result-model]
// Purpose: Exact packed operator, span, fill, copy and unchanged-frame results.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub(crate) open spec fn operator_byte(
    operator: crate::bitmap::CombinationOperator, destination: u8, source: u8,
) -> u8 {
    match operator {
        crate::bitmap::CombinationOperator::Or => destination | source,
        crate::bitmap::CombinationOperator::And => destination & source,
        crate::bitmap::CombinationOperator::Xor => destination ^ source,
        crate::bitmap::CombinationOperator::Xnor => !(destination ^ source),
        crate::bitmap::CombinationOperator::Replace => source,
    }
}

// Model navigation: spec::operator_word [project-result-model]
// Purpose: Exact packed operator, span, fill, copy and unchanged-frame results.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub(crate) open spec fn operator_word(
    operator: crate::bitmap::CombinationOperator, destination: u64, source: u64,
) -> u64 {
    match operator {
        crate::bitmap::CombinationOperator::Or => destination | source,
        crate::bitmap::CombinationOperator::And => destination & source,
        crate::bitmap::CombinationOperator::Xor => destination ^ source,
        crate::bitmap::CombinationOperator::Xnor => !(destination ^ source),
        crate::bitmap::CombinationOperator::Replace => source,
    }
}

// Model navigation: spec::byte_operator_bit [proof-support]
// Purpose: Bit projections, packed addresses, clipped extents and touched-byte vocabulary.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn byte_operator_bit(value: u8, bit: u32) -> bool
    recommends bit < 8,
{
    ((value >> (bit as u8)) & 1u8) != 0
}

// Model navigation: spec::word_operator_bit [proof-support]
// Purpose: Bit projections, packed addresses, clipped extents and touched-byte vocabulary.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn word_operator_bit(value: u64, bit: u32) -> bool
    recommends bit < 64,
{
    ((value >> (bit as u64)) & 1u64) != 0
}

// Model navigation: spec::stride_for_width [proof-support]
// Purpose: Bit projections, packed addresses, clipped extents and touched-byte vocabulary.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn stride_for_width(width: nat) -> nat {
    (width + 7) / 8
}

// Model navigation: spec::dimensions_checked [project-policy]
// Purpose: Checked arithmetic and caller Limits admission policy.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn dimensions_checked(width: u32, height: u32) -> bool {
    (width as nat) * (height as nat) <= u64::MAX
}

// Model navigation: spec::stride_checked [project-policy]
// Purpose: Checked arithmetic and caller Limits admission policy.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn stride_checked(width: u32) -> bool {
    (width as nat) + 7 <= u64::MAX
        && stride_for_width(width as nat) <= usize::MAX
}

// Model navigation: spec::allocation_checked [project-policy]
// Purpose: Checked arithmetic and caller Limits admission policy.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn allocation_checked(width: u32, height: u32) -> bool {
    stride_checked(width)
        && stride_for_width(width as nat) * (height as nat) <= usize::MAX
        && stride_for_width(width as nat) * (height as nat) <= u64::MAX
}

// Model navigation: spec::coordinates_in_bounds [representation-invariant]
// Purpose: Packed dimensions, storage length and unused-padding invariants.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn coordinates_in_bounds(
    width: u32,
    height: u32,
    x: u32,
    y: u32,
) -> bool {
    x < width && y < height
}

// Model navigation: spec::byte_index [proof-support]
// Purpose: Bit projections, packed addresses, clipped extents and touched-byte vocabulary.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn byte_index(stride: usize, x: u32, y: u32) -> nat {
    (y as nat) * (stride as nat) + (x as nat) / 8
}

// Model navigation: spec::bit_mask [proof-support]
// Purpose: Bit projections, packed addresses, clipped extents and touched-byte vocabulary.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn bit_mask(x: u32) -> u8 {
    match x % 8 {
        0 => 0x80,
        1 => 0x40,
        2 => 0x20,
        3 => 0x10,
        4 => 0x08,
        5 => 0x04,
        6 => 0x02,
        _ => 0x01,
    }
}

// Model navigation: spec::byte_bit [proof-support]
// Purpose: Bit projections, packed addresses, clipped extents and touched-byte vocabulary.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn byte_bit(byte: u8, x: u32) -> bool {
    byte & bit_mask(x) != 0
}

// Model navigation: spec::set_byte_bit [proof-support]
// Purpose: Bit projections, packed addresses, clipped extents and touched-byte vocabulary.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn set_byte_bit(byte: u8, x: u32, value: bool) -> u8 {
    if value {
        byte | bit_mask(x)
    } else {
        byte & !bit_mask(x)
    }
}

// Model navigation: spec::chunk_byte [proof-support]
// Purpose: Bit projections, packed addresses, clipped extents and touched-byte vocabulary.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn chunk_byte(value: u64, offset: int) -> u8
    recommends 0 <= offset < 8,
{
    if offset == 0 { (value >> 56) as u8 }
    else if offset == 1 { (value >> 48) as u8 }
    else if offset == 2 { (value >> 40) as u8 }
    else if offset == 3 { (value >> 32) as u8 }
    else if offset == 4 { (value >> 24) as u8 }
    else if offset == 5 { (value >> 16) as u8 }
    else if offset == 6 { (value >> 8) as u8 }
    else { value as u8 }
}

// Model navigation: spec::chunk_value [proof-support]
// Purpose: Bit projections, packed addresses, clipped extents and touched-byte vocabulary.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn chunk_value(bytes: Seq<u8>, start: int) -> u64
    recommends 0 <= start && start + 8 <= bytes.len(),
{
    ((bytes[start] as u64) << 56)
        | ((bytes[start + 1] as u64) << 48)
        | ((bytes[start + 2] as u64) << 40)
        | ((bytes[start + 3] as u64) << 32)
        | ((bytes[start + 4] as u64) << 24)
        | ((bytes[start + 5] as u64) << 16)
        | ((bytes[start + 6] as u64) << 8)
        | bytes[start + 7] as u64
}

// Model navigation: spec::stored_pixel [proof-support]
// Purpose: Bit projections, packed addresses, clipped extents and touched-byte vocabulary.
// Audit: stored-pixel
// REQUIREMENT CONTEXT: T88-GENERIC-TYPICAL-ROW | ITU-T T.88 (08/2018) | 6.2.5.5; 6.2.5.7 step 3c
// Fidelity: unresolved; no independent normative approval.
pub open spec fn stored_pixel(
    bitmap: &crate::bitmap::Bitmap,
    x: u32,
    y: u32,
) -> bool
    recommends coordinates_in_bounds(
                   crate::views::bitmap_width(bitmap),
                   crate::views::bitmap_height(bitmap),
                   x,
                   y,
               ),
               byte_index(crate::views::bitmap_stride(bitmap), x, y)
                   < crate::views::bitmap_data(bitmap).len(),
{
    byte_bit(
        crate::views::bitmap_data(bitmap)[
            byte_index(crate::views::bitmap_stride(bitmap), x, y) as int
        ],
        x,
    )
}

// Model navigation: spec::padding_is_zero [representation-invariant]
// Purpose: Packed dimensions, storage length and unused-padding invariants.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn padding_is_zero(bitmap: &crate::bitmap::Bitmap) -> bool {
    forall|y: int, x: int|
        0 <= y < crate::views::bitmap_height(bitmap) as int
        && crate::views::bitmap_width(bitmap) as int
            <= x < (crate::views::bitmap_stride(bitmap) as int) * 8
        ==> !#[trigger] byte_bit(
            crate::views::bitmap_data(bitmap)[
                y * crate::views::bitmap_stride(bitmap) as int + x / 8
            ],
            x as u32,
        )
}

// Model navigation: spec::storage_valid [representation-invariant]
// Purpose: Packed dimensions, storage length and unused-padding invariants.
// Audit: bitmap-storage-valid
// REQUIREMENT CONTEXT: T6-REFERENCE-FIRST-WHITE | ITU-T T.6 (1988) | 2.2.1
// Fidelity: unresolved; no independent normative approval.
pub open spec fn storage_valid(bitmap: &crate::bitmap::Bitmap) -> bool {
    crate::views::bitmap_stride(bitmap) as nat
        == stride_for_width(crate::views::bitmap_width(bitmap) as nat)
        && crate::views::bitmap_data(bitmap).len()
            == (crate::views::bitmap_stride(bitmap) as nat)
                * (crate::views::bitmap_height(bitmap) as nat)
}

// Model navigation: spec::black_span_bits [project-result-model]
// Purpose: Exact packed operator, span, fill, copy and unchanged-frame results.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn black_span_bits(
    before: Seq<u8>,
    after: Seq<u8>,
    row_start: int,
    start: u32,
    end: u32,
) -> bool {
    &&& after.len() == before.len()
    &&& forall|index: int, bit: u32| 0 <= index < before.len() && bit < 8 ==>
        #[trigger] byte_bit(after[index], bit)
            == (row_start <= index
                && start as int <= (index - row_start) * 8 + bit as int
                && (index - row_start) * 8 + (bit as int) < end as int
                || byte_bit(before[index], bit))
}

// Model navigation: spec::black_span_result [project-result-model]
// Purpose: Exact packed operator, span, fill, copy and unchanged-frame results.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn black_span_result(
    before: &crate::bitmap::Bitmap,
    after: &crate::bitmap::Bitmap,
    row: u32,
    start: u32,
    end: u32,
) -> bool {
    &&& black_span_bits(
        crate::views::bitmap_data(before), crate::views::bitmap_data(after),
        row as int * crate::views::bitmap_stride(before) as int, start, end,
    )
    &&& forall|index: int| 0 <= index < crate::views::bitmap_data(before).len()
        && (start == end
            || index < row as int * crate::views::bitmap_stride(before) as int + start as int / 8
            || row as int * crate::views::bitmap_stride(before) as int + (end as int + 7) / 8 <= index)
        ==> crate::views::bitmap_data(after)[index] == crate::views::bitmap_data(before)[index]
    &&& forall|x: u32, y: u32|
        coordinates_in_bounds(crate::views::bitmap_width(before), crate::views::bitmap_height(before), x, y)
        ==> stored_pixel(after, x, y)
            == (y == row && start <= x < end || stored_pixel(before, x, y))
}

// Model navigation: spec::region_request_valid [representation-invariant]
// Purpose: Packed dimensions, storage length and unused-padding invariants.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn region_request_valid(
    destination: &crate::bitmap::Bitmap,
    source: &crate::bitmap::Bitmap,
    source_x: u32,
    source_y: u32,
    destination_x: u32,
    destination_y: u32,
    width: u32,
    height: u32,
) -> bool {
    &&& source_x as nat + width as nat <= crate::views::bitmap_width(source) as nat
    &&& source_y as nat + height as nat <= crate::views::bitmap_height(source) as nat
    &&& destination_x as nat + width as nat <= crate::views::bitmap_width(destination) as nat
    &&& destination_y as nat + height as nat <= crate::views::bitmap_height(destination) as nat
}

// Model navigation: spec::zeroed_storage [project-result-model]
// Purpose: Exact packed operator, span, fill, copy and unchanged-frame results.
// Audit: zeroed-storage
// REQUIREMENT CONTEXT: T88-GENERIC-TYPICAL-ROW | ITU-T T.88 (08/2018) | 6.2.5.5; 6.2.5.7 step 3c
// Fidelity: unresolved; no independent normative approval.
pub open spec fn zeroed_storage(bitmap: &crate::bitmap::Bitmap) -> bool {
    forall|index: int| 0 <= index < crate::views::bitmap_data(bitmap).len() ==>
        crate::views::bitmap_data(bitmap)[index] == 0
}

// Model navigation: spec::filled_byte [project-result-model]
// Purpose: Exact packed operator, span, fill, copy and unchanged-frame results.
// Audit: filled-storage-byte
// REQUIREMENT CONTEXT: T88-PAGE-DEFAULT-PIXEL | ITU-T T.88 (08/2018) | 7.4.8.5 bit 2; 8.2 step 3
// Fidelity: unresolved; no independent normative approval.
pub(crate) open spec fn filled_byte(width: u32, column: nat, value: bool) -> u8 {
    if !value {
        0
    } else if width % 8 != 0 && column + 1 == stride_for_width(width as nat) {
        0xFFu8 << (8u8 - (width % 8) as u8)
    } else {
        0xFF
    }
}

// Model navigation: spec::filled_storage [project-result-model]
// Purpose: Exact packed operator, span, fill, copy and unchanged-frame results.
// Audit: filled-storage-result
// REQUIREMENT CONTEXT: T88-PAGE-DEFAULT-PIXEL | ITU-T T.88 (08/2018) | 7.4.8.5 bit 2; 8.2 step 3
// Fidelity: unresolved; no independent normative approval.
pub(crate) open spec fn filled_storage(
    bitmap: &crate::bitmap::Bitmap, value: bool,
) -> bool {
    &&& storage_valid(bitmap)
    &&& forall|row: int, column: int|
        0 <= row < crate::views::bitmap_height(bitmap)
        && 0 <= column < crate::views::bitmap_stride(bitmap) ==>
            #[trigger] crate::views::bitmap_data(bitmap)[
                row * crate::views::bitmap_stride(bitmap) as int + column
            ] == filled_byte(crate::views::bitmap_width(bitmap), column as nat, value)
}

// Model navigation: spec::prefix_copy_result [project-result-model]
// Purpose: Exact packed operator, span, fill, copy and unchanged-frame results.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn prefix_copy_result(
    before: &crate::bitmap::Bitmap,
    after: &crate::bitmap::Bitmap,
    source: &crate::bitmap::Bitmap,
    count: usize,
) -> bool {
    &&& crate::views::bitmap_width(after) == crate::views::bitmap_width(before)
    &&& crate::views::bitmap_height(after) == crate::views::bitmap_height(before)
    &&& crate::views::bitmap_stride(after) == crate::views::bitmap_stride(before)
    &&& crate::views::bitmap_data(after).len() == crate::views::bitmap_data(before).len()
    &&& forall|index: int| 0 <= index < crate::views::bitmap_data(before).len() ==>
        crate::views::bitmap_data(after)[index] == if index < count {
            crate::views::bitmap_data(source)[index]
        } else {
            crate::views::bitmap_data(before)[index]
        }
}

// Model navigation: spec::row_copy_result [project-result-model]
// Purpose: Exact packed operator, span, fill, copy and unchanged-frame results.
// Audit: row-copy-result
// REQUIREMENT CONTEXT: T88-GENERIC-TYPICAL-ROW | ITU-T T.88 (08/2018) | 6.2.5.5; 6.2.5.7 step 3c
// Fidelity: unresolved; no independent normative approval.
pub open spec fn row_copy_result(
    before: &crate::bitmap::Bitmap,
    after: &crate::bitmap::Bitmap,
    source: u32,
    destination: u32,
) -> bool {
    let stride = crate::views::bitmap_stride(before) as int;
    let source_start = source as int * stride;
    let destination_start = destination as int * stride;
    &&& crate::views::bitmap_width(after) == crate::views::bitmap_width(before)
    &&& crate::views::bitmap_height(after) == crate::views::bitmap_height(before)
    &&& crate::views::bitmap_stride(after) == crate::views::bitmap_stride(before)
    &&& crate::views::bitmap_data(after).len() == crate::views::bitmap_data(before).len()
    &&& forall|index: int| 0 <= index < crate::views::bitmap_data(before).len() ==>
        crate::views::bitmap_data(after)[index]
            == if destination_start <= index < destination_start + stride {
                crate::views::bitmap_data(before)[source_start + index - destination_start]
            } else {
                crate::views::bitmap_data(before)[index]
            }
}

// Model navigation: spec::invariant [representation-invariant]
// Purpose: Packed dimensions, storage length and unused-padding invariants.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn invariant(bitmap: &crate::bitmap::Bitmap) -> bool {
    storage_valid(bitmap) && padding_is_zero(bitmap)
}

// Model navigation: spec::pixel_result [project-result-model]
// Purpose: Exact packed operator, span, fill, copy and unchanged-frame results.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn pixel_result(
    bitmap: &crate::bitmap::Bitmap,
    x: u32,
    y: u32,
) -> Option<bool> {
    if coordinates_in_bounds(
        crate::views::bitmap_width(bitmap),
        crate::views::bitmap_height(bitmap),
        x,
        y,
    ) {
        Some(stored_pixel(bitmap, x, y))
    } else {
        None
    }
}

// Model navigation: spec::limits_allow_bitmap [project-policy]
// Purpose: Checked arithmetic and caller Limits admission policy.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn limits_allow_bitmap(
    width: u32,
    height: u32,
    limits: &crate::Limits,
) -> bool {
    width <= limits.max_width
        && height <= limits.max_height
        && (width as nat) * (height as nat)
            <= limits.max_pixels_per_page
        && stride_for_width(width as nat) * (height as nat)
            <= limits.max_decoder_managed_memory_bytes
}

// Model navigation: spec::filled_pixels [project-result-model]
// Purpose: Exact packed operator, span, fill, copy and unchanged-frame results.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn filled_pixels(
    bitmap: &crate::bitmap::Bitmap,
    value: bool,
) -> bool {
    forall|x: u32, y: u32|
        coordinates_in_bounds(
            crate::views::bitmap_width(bitmap),
            crate::views::bitmap_height(bitmap),
            x,
            y,
        )
        ==> stored_pixel(bitmap, x, y) == value
}

// Model navigation: spec::clipped_width [proof-support]
// Purpose: Bit projections, packed addresses, clipped extents and touched-byte vocabulary.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn clipped_width(
    destination_width: u32,
    source_width: u32,
    x: u32,
) -> nat {
    if x >= destination_width {
        0
    } else if source_width <= destination_width - x {
        source_width as nat
    } else {
        (destination_width - x) as nat
    }
}

// Model navigation: spec::clipped_height [proof-support]
// Purpose: Bit projections, packed addresses, clipped extents and touched-byte vocabulary.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn clipped_height(
    destination_height: u32,
    source_height: u32,
    y: u32,
) -> nat {
    if y >= destination_height {
        0
    } else if source_height <= destination_height - y {
        source_height as nat
    } else {
        (destination_height - y) as nat
    }
}

// Model navigation: spec::compose_byte_may_change [proof-support]
// Purpose: Bit projections, packed addresses, clipped extents and touched-byte vocabulary.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn compose_byte_may_change(
    index: int,
    stride: usize,
    destination_width: u32,
    destination_height: u32,
    source_width: u32,
    source_height: u32,
    x: u32,
    y: u32,
) -> bool {
    exists|source_x: int, source_y: int|
        0 <= source_x
            < clipped_width(destination_width, source_width, x)
        && 0 <= source_y
            < clipped_height(destination_height, source_height, y)
        && index == #[trigger] byte_index(
            stride,
            (x as int + source_x) as u32,
            (y as int + source_y) as u32,
        ) as int
}

// Model navigation: spec::compose_frame [project-result-model]
// Purpose: Exact packed operator, span, fill, copy and unchanged-frame results.
// Mapping: no registered requirement/locator mapping.
// Fidelity: unresolved; no independent normative approval.
pub open spec fn compose_frame(
    before: Seq<u8>,
    after: Seq<u8>,
    stride: usize,
    destination_width: u32,
    destination_height: u32,
    source_width: u32,
    source_height: u32,
    x: u32,
    y: u32,
) -> bool {
    before.len() == after.len()
        && forall|index: int| 0 <= index < before.len()
            && !compose_byte_may_change(
                index, stride, destination_width, destination_height,
                source_width, source_height, x, y,
            ) ==> after[index] == before[index]
}

}

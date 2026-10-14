use vstd::prelude::*;

verus! {

pub(super) proof fn all_combination_operators_are_boolean(
    operator: crate::bitmap::CombinationOperator,
    destination: bool,
    source: bool,
)
    ensures
        crate::spec::bitmap::combine(operator, destination, source)
            == match operator {
                crate::bitmap::CombinationOperator::Or =>
                    destination || source,
                crate::bitmap::CombinationOperator::And =>
                    destination && source,
                crate::bitmap::CombinationOperator::Xor =>
                    destination != source,
                crate::bitmap::CombinationOperator::Xnor =>
                    destination == source,
                crate::bitmap::CombinationOperator::Replace => source,
            },
{
}

pub(super) proof fn clipping_never_exceeds_destination(
    destination_width: u32,
    destination_height: u32,
    source_width: u32,
    source_height: u32,
    x: u32,
    y: u32,
)
    ensures
        x as nat + crate::spec::bitmap::clipped_width(
            destination_width,
            source_width,
            x,
        ) <= if x <= destination_width {
            destination_width as nat
        } else {
            x as nat
        },
        y as nat + crate::spec::bitmap::clipped_height(
            destination_height,
            source_height,
            y,
        ) <= if y <= destination_height {
            destination_height as nat
        } else {
            y as nat
        },
{
    reveal(crate::spec::bitmap::clipped_width);
    reveal(crate::spec::bitmap::clipped_height);
}

pub(super) proof fn invariant_has_exact_layout(
    bitmap: &crate::bitmap::Bitmap,
)
    requires crate::spec::bitmap::invariant(bitmap),
    ensures
        crate::views::bitmap_stride(bitmap) as nat
            == crate::spec::bitmap::stride_for_width(
                crate::views::bitmap_width(bitmap) as nat,
            ),
        crate::views::bitmap_data(bitmap).len()
            == (crate::views::bitmap_stride(bitmap) as nat)
                * (crate::views::bitmap_height(bitmap) as nat),
{
    reveal(crate::spec::bitmap::invariant);
    reveal(crate::spec::bitmap::storage_valid);
}

pub(super) proof fn invariant_padding_is_deterministic(
    bitmap: &crate::bitmap::Bitmap,
    x: int,
    y: int,
)
    requires
        crate::spec::bitmap::invariant(bitmap),
        0 <= y < crate::views::bitmap_height(bitmap) as int,
        crate::views::bitmap_width(bitmap) as int
            <= x < (crate::views::bitmap_stride(bitmap) as int) * 8,
    ensures
        !crate::spec::bitmap::byte_bit(
            crate::views::bitmap_data(bitmap)[
                y * crate::views::bitmap_stride(bitmap) as int + x / 8
            ],
            x as u32,
        ),
{
    reveal(crate::spec::bitmap::invariant);
    reveal(crate::spec::bitmap::padding_is_zero);
}

}

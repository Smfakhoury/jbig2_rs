#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

use super::*;

#[cfg(test)]
#[allow(clippy::too_many_arguments, clippy::ptr_arg)]
pub(super) fn decode_generic_with_oracle(
    bitmap: &mut Bitmap,
    rows: u32,
    template: GenericTemplate,
    extended: bool,
    adaptive: &[TemplateOffset; 12],
    typical_prediction: bool,
    skip: Option<&Bitmap>,
    decoder: &mut ArithmeticDecoder<'_>,
    contexts: &mut Vec<ArithmeticContext>,
) -> Result<(), GenericRegionError> {
    let mut ltp = false;
    let bitmap_width = bitmap.width;
    let bitmap_height = bitmap.height;
    if rows > bitmap_height {
        return Err(GenericRegionError::RowCountMismatch {
            expected: bitmap_height,
            actual: rows,
        });
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::views::bitmap_fields(bitmap);
    }
    let mut y = 0_u32;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            y <= rows,
            rows <= bitmap_height,
            crate::views::bitmap_width(bitmap) == bitmap_width,
            crate::views::bitmap_height(bitmap) == bitmap_height,
            crate::spec::bitmap::storage_valid(bitmap),
            crate::arithmetic::arithmetic_spec::decoder_valid(decoder),
            generic_region_spec::contexts_valid(contexts@),
            match skip {
                Some(mask) => crate::spec::bitmap::storage_valid(mask),
                None => true,
            },
        decreases rows - y,
    ))]
    while y < rows {
        if typical_prediction {
            let sltp = decode_probability(decoder, contexts, template.sltp_context())?;
            ltp ^= sltp != 0;
        }

        if ltp {
            if y != 0 {
                #[cfg(verus_keep_ghost)]
                proof! {
                    crate::views::bitmap_fields(bitmap);
                    assert(bitmap.height == bitmap_height);
                    assert(y - 1 < bitmap_height);
                    assert(y < bitmap_height);
                }
                copy_bitmap_row(bitmap, y - 1, y)?;
                #[cfg(verus_keep_ghost)]
                proof! {
                    crate::views::bitmap_fields(bitmap);
                    assert(bitmap.width == bitmap_width);
                    assert(bitmap.height == bitmap_height);
                }
            }
            y += 1;
            continue;
        }

        let mut x = 0_u32;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                x <= bitmap_width,
                y < rows,
                rows <= bitmap_height,
                crate::views::bitmap_width(bitmap) == bitmap_width,
                crate::views::bitmap_height(bitmap) == bitmap_height,
                crate::spec::bitmap::storage_valid(bitmap),
                crate::arithmetic::arithmetic_spec::decoder_valid(decoder),
                generic_region_spec::contexts_valid(contexts@),
                match skip {
                    Some(mask) => crate::spec::bitmap::storage_valid(mask),
                    None => true,
                },
            decreases bitmap_width - x,
        ))]
        while x < bitmap_width {
            let skipped = match skip {
                Some(mask) => mask.pixel(x, y).unwrap_or(false),
                None => false,
            };
            if skipped {
                bitmap.set_pixel(x, y, false)?;
                #[cfg(verus_keep_ghost)]
                proof! {
                    crate::views::bitmap_fields(bitmap);
                    assert(x < bitmap_width);
                }
                x += 1;
                continue;
            }
            let context = generic_context(bitmap, x, y, template, extended, adaptive);
            let value = decode_probability(decoder, contexts, context)? != 0;
            bitmap.set_pixel(x, y, value)?;
            #[cfg(verus_keep_ghost)]
            proof! {
                crate::views::bitmap_fields(bitmap);
                assert(x < bitmap_width);
            }
            x += 1;
        }
        y += 1;
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(super) fn decode_generic_with_callback<F>(
    bitmap: &mut Bitmap,
    rows: u32,
    template: GenericTemplate,
    extended: bool,
    adaptive: &[TemplateOffset; 12],
    typical_prediction: bool,
    skip: Option<&Bitmap>,
    mut decode: F,
) -> Result<(), GenericRegionError>
where
    F: FnMut(usize) -> Result<u8, GenericRegionError>,
{
    let mut ltp = false;
    for y in 0..rows {
        if typical_prediction {
            let sltp = decode(template.sltp_context())?;
            ltp ^= sltp != 0;
        }
        if ltp {
            if y != 0 {
                copy_bitmap_row(bitmap, y - 1, y)?;
            }
            continue;
        }
        for x in 0..bitmap.width() {
            let skipped = skip.and_then(|mask| mask.pixel(x, y)).unwrap_or(false);
            if skipped {
                bitmap.set_pixel(x, y, false)?;
                continue;
            }
            let context = generic_context(bitmap, x, y, template, extended, adaptive);
            let value = decode(context)? != 0;
            bitmap.set_pixel(x, y, value)?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::ptr_arg, clippy::too_many_arguments)]
pub(super) fn decode_refinement_with_oracle(
    bitmap: &mut Bitmap,
    reference: &Bitmap,
    template: RefinementTemplate,
    adaptive: [TemplateOffset; 2],
    typical_prediction: bool,
    parameters: RefinementParameters,
    decoder: &mut ArithmeticDecoder<'_>,
    contexts: &mut Vec<ArithmeticContext>,
) -> Result<(), GenericRegionError> {
    let mut ltp = false;
    let bitmap_width = bitmap.width;
    let bitmap_height = bitmap.height;
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::views::bitmap_fields(bitmap);
    }
    let mut y = 0_u32;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            y <= bitmap_height,
            crate::views::bitmap_width(bitmap) == bitmap_width,
            crate::views::bitmap_height(bitmap) == bitmap_height,
            crate::spec::bitmap::storage_valid(bitmap),
            crate::spec::bitmap::storage_valid(reference),
            crate::arithmetic::arithmetic_spec::decoder_valid(decoder),
            generic_region_spec::contexts_valid(contexts@),
        decreases bitmap_height - y,
    ))]
    while y < bitmap_height {
        if typical_prediction {
            let sltp = decode_probability(decoder, contexts, template.sltp_context())?;
            ltp ^= sltp != 0;
        }
        let mut x = 0_u32;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                x <= bitmap_width,
                y < bitmap_height,
                crate::views::bitmap_width(bitmap) == bitmap_width,
                crate::views::bitmap_height(bitmap) == bitmap_height,
                crate::spec::bitmap::storage_valid(bitmap),
                crate::spec::bitmap::storage_valid(reference),
                crate::arithmetic::arithmetic_spec::decoder_valid(decoder),
                generic_region_spec::contexts_valid(contexts@),
            decreases bitmap_width - x,
        ))]
        while x < bitmap_width {
            if ltp {
                if let Some(prediction) = refinement_prediction(reference, x, y, parameters) {
                    bitmap.set_pixel(x, y, prediction)?;
                    #[cfg(verus_keep_ghost)]
                    proof! {
                        crate::views::bitmap_fields(bitmap);
                    }
                    x += 1;
                    continue;
                }
            }
            let context =
                refinement_context(bitmap, reference, x, y, template, adaptive, parameters);
            let value = decode_probability(decoder, contexts, context)? != 0;
            bitmap.set_pixel(x, y, value)?;
            #[cfg(verus_keep_ghost)]
            proof! {
                crate::views::bitmap_fields(bitmap);
            }
            x += 1;
        }
        y += 1;
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn decode_refinement_with_callback<F>(
    bitmap: &mut Bitmap,
    reference: &Bitmap,
    template: RefinementTemplate,
    adaptive: [TemplateOffset; 2],
    typical_prediction: bool,
    parameters: RefinementParameters,
    mut decode: F,
) -> Result<(), GenericRegionError>
where
    F: FnMut(usize) -> Result<u8, GenericRegionError>,
{
    let mut ltp = false;
    for y in 0..bitmap.height() {
        if typical_prediction {
            let sltp = decode(template.sltp_context())?;
            ltp ^= sltp != 0;
        }
        for x in 0..bitmap.width() {
            if ltp {
                if let Some(prediction) = refinement_prediction(reference, x, y, parameters) {
                    bitmap.set_pixel(x, y, prediction)?;
                    continue;
                }
            }
            let context =
                refinement_context(bitmap, reference, x, y, template, adaptive, parameters);
            let value = decode(context)? != 0;
            bitmap.set_pixel(x, y, value)?;
        }
    }
    Ok(())
}

//! Refinement implementation.

#[cfg(verus_keep_ghost)]
use super::*;
#[cfg(not(verus_keep_ghost))]
use super::{
    check_dimensions, checked_unsigned_delta, required_integer, widen_bool_i64, Bitmap,
    IntegerField, Limits, MalformedError, RefinementGeometry, RefinementRequest, ResourceTracker,
    SymbolCodec, SymbolTextError,
};

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(geometry) =>
            geometry.width as int == base_width as int + delta_width as int
            && geometry.height as int == base_height as int + delta_height as int
            && geometry.width <= limits.max_width
            && geometry.height <= limits.max_height
            && geometry.reference_dx as int
                == symbol_text_spec::floor_half(delta_width as int) + delta_x as int
            && geometry.reference_dy as int
                == symbol_text_spec::floor_half(delta_height as int) + delta_y as int,
        Err(_) => true,
    },
))]
pub(crate) fn refinement_geometry(
    base_width: u32,
    base_height: u32,
    delta_width: i64,
    delta_height: i64,
    delta_x: i64,
    delta_y: i64,
    limits: &Limits,
) -> Result<RefinementGeometry, SymbolTextError> {
    let width = checked_unsigned_delta(base_width, delta_width, limits.max_width)?;
    let height = checked_unsigned_delta(base_height, delta_height, limits.max_height)?;
    check_dimensions(width, height, limits)?;
    let reference_x_offset = floor_divide_by_two(delta_width)
        .checked_add(delta_x)
        .ok_or(MalformedError::IntegerOverflow)?;
    let reference_y_offset = floor_divide_by_two(delta_height)
        .checked_add(delta_y)
        .ok_or(MalformedError::IntegerOverflow)?;
    Ok(RefinementGeometry {
        width,
        height,
        reference_dx: i32::try_from(reference_x_offset)
            .map_err(|_error| MalformedError::IntegerOverflow)?,
        reference_dy: i32::try_from(reference_y_offset)
            .map_err(|_error| MalformedError::IntegerOverflow)?,
    })
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out as int == symbol_text_spec::floor_half(value as int),
))]
pub(in crate::symbol_text) fn floor_divide_by_two(value: i64) -> i64 {
    value / 2 - widen_bool_i64(value < 0 && value % 2 != 0)
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        symbol_text_spec::codec_valid(old(codec)),
        crate::spec::bitmap::storage_valid(reference),
    ensures match out {
        Ok(bitmap) =>
            symbol_text_spec::codec_valid(final(codec))
                && crate::spec::bitmap::storage_valid(&bitmap)
                && symbol_text_spec::work_units(final(resources))
                    == symbol_text_spec::work_units(old(resources)),
        Err(_) => true,
    },
))]
pub(in crate::symbol_text) fn decode_refined_bitmap(
    codec: &mut SymbolCodec<'_>,
    reference: &Bitmap,
    template: u8,
    adaptive_pixels: [(i8, i8); 2],
    huffman: bool,
    limits: &Limits,
    resources: &mut ResourceTracker,
) -> Result<Bitmap, SymbolTextError> {
    let delta_width = required_integer(codec, IntegerField::RefinementWidth)?;
    let delta_height = required_integer(codec, IntegerField::RefinementHeight)?;
    let delta_x = required_integer(codec, IntegerField::RefinementX)?;
    let delta_y = required_integer(codec, IntegerField::RefinementY)?;
    let declared_data_size = if huffman {
        let size = required_integer(codec, IntegerField::RefinementSize)?;
        Some(
            u32::try_from(size)
                .map_err(|_| SymbolTextError::InvalidEntropyValue("refinement size"))?,
        )
    } else {
        None
    };
    let geometry = refinement_geometry(
        reference.width(),
        reference.height(),
        delta_width,
        delta_height,
        delta_x,
        delta_y,
        limits,
    )?;
    resources.charge_bitmap(geometry.width, geometry.height, limits)?;
    let request = RefinementRequest {
        reference,
        geometry,
        template,
        adaptive_pixels,
        declared_data_size,
    };
    let bitmap = codec.decode_refinement(request, limits)?;
    if bitmap.width() != geometry.width || bitmap.height() != geometry.height {
        return Err(SymbolTextError::InvalidDimensions);
    }
    Ok(bitmap)
}

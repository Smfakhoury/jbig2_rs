#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

mod benchmarks;
mod composition;
mod layout_and_access;
mod region_composition;

use super::access::copy_row_bytes;
use super::chunks::{
    read_chunk, read_shifted_byte, read_shifted_chunk, read_shifted_tail, write_chunk,
};
use super::storage::checked_bitmap_layout;
use super::*;
use crate::{DecodeError, LimitExceeded, LimitKind, Limits, MalformedError};
use std::hint::black_box;
use std::time::{Duration, Instant};

fn scalar_compose(
    destination: &mut Bitmap,
    source: &Bitmap,
    x: u32,
    y: u32,
    operator: CombinationOperator,
) -> Result<(), MalformedError> {
    let width = source.width.min(destination.width.saturating_sub(x));
    let height = source.height.min(destination.height.saturating_sub(y));
    for source_y in 0..height {
        let destination_y = y
            .checked_add(source_y)
            .ok_or(MalformedError::IntegerOverflow)?;
        for source_x in 0..width {
            let destination_x = x
                .checked_add(source_x)
                .ok_or(MalformedError::IntegerOverflow)?;
            let source_value = source
                .pixel(source_x, source_y)
                .ok_or(MalformedError::IntegerOverflow)?;
            let destination_value = destination
                .pixel(destination_x, destination_y)
                .ok_or(MalformedError::IntegerOverflow)?;
            destination.set_pixel(
                destination_x,
                destination_y,
                operator.apply(destination_value, source_value),
            )?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn scalar_compose_region(
    destination: &mut Bitmap,
    source: &Bitmap,
    source_x: u32,
    source_y: u32,
    destination_x: u32,
    destination_y: u32,
    width: u32,
    height: u32,
    operator: CombinationOperator,
) -> Result<(), MalformedError> {
    if source_x
        .checked_add(width)
        .is_none_or(|end| end > source.width)
        || source_y
            .checked_add(height)
            .is_none_or(|end| end > source.height)
        || destination_x
            .checked_add(width)
            .is_none_or(|end| end > destination.width)
        || destination_y
            .checked_add(height)
            .is_none_or(|end| end > destination.height)
    {
        return Err(MalformedError::IntegerOverflow);
    }
    for row in 0..height {
        for column in 0..width {
            let sx = source_x + column;
            let sy = source_y + row;
            let dx = destination_x + column;
            let dy = destination_y + row;
            let incoming = source
                .pixel(sx, sy)
                .ok_or(MalformedError::IntegerOverflow)?;
            let current = destination
                .pixel(dx, dy)
                .ok_or(MalformedError::IntegerOverflow)?;
            destination.set_pixel(dx, dy, operator.apply(current, incoming))?;
        }
    }
    Ok(())
}

fn scalar_fill_black_span(
    bitmap: &mut Bitmap,
    row: u32,
    start: u32,
    end: u32,
) -> Result<(), MalformedError> {
    if start > end || end > bitmap.width || row >= bitmap.height {
        return Err(MalformedError::IntegerOverflow);
    }
    for x in start..end {
        bitmap.set_pixel(x, row, true)?;
    }
    Ok(())
}

fn scalar_copy_row(
    bitmap: &mut Bitmap,
    source: u32,
    destination: u32,
) -> Result<(), MalformedError> {
    if source >= bitmap.height || destination >= bitmap.height {
        return Err(MalformedError::IntegerOverflow);
    }
    if source == destination {
        return Ok(());
    }
    let source_start =
        usize::try_from(source).map_err(|_error| MalformedError::IntegerOverflow)? * bitmap.stride;
    let destination_start = usize::try_from(destination)
        .map_err(|_error| MalformedError::IntegerOverflow)?
        * bitmap.stride;
    for index in 0..bitmap.stride {
        bitmap.data[destination_start + index] = bitmap.data[source_start + index];
    }
    Ok(())
}

fn patterned_bitmap(width: u32, height: u32, mut state: u64) -> Bitmap {
    let mut bitmap = Bitmap::blank(width, height, &Limits::default()).expect("bitmap fits");
    for y in 0..height {
        for x in 0..width {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            if state >> 63 != 0 {
                bitmap.set_pixel(x, y, true).expect("pixel is in bounds");
            }
        }
    }
    bitmap
}

const OPERATOR_TRUTHS: [(CombinationOperator, [bool; 4]); 5] = [
    (CombinationOperator::Or, [false, true, true, true]),
    (CombinationOperator::And, [false, false, false, true]),
    (CombinationOperator::Xor, [false, true, true, false]),
    (CombinationOperator::Xnor, [true, false, false, true]),
    (CombinationOperator::Replace, [false, true, false, true]),
];

fn median_duration(mut values: Vec<Duration>) -> Duration {
    values.sort_unstable();
    values[values.len() / 2]
}

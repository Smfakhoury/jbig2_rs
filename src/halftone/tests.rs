#![allow(clippy::duplicated_attributes)]
#![cfg(test)]

mod benchmarks;
mod codec_state;
mod error_adapters;
mod geometry_and_render;
mod headers_and_patterns;

use super::headers::*;
use super::planes::*;
use super::render::*;
use super::resources::*;
use super::*;
use crate::reader::ByteReader;
use std::collections::VecDeque;
use std::hint::black_box;
use std::time::{Duration, Instant};

fn assert_decode_error_exact(actual: DecodeError, expected: DecodeError) {
    match (actual, expected) {
        (DecodeError::Malformed(actual), DecodeError::Malformed(expected)) => {
            assert_eq!(actual, expected);
        }
        (DecodeError::LimitExceeded(actual), DecodeError::LimitExceeded(expected)) => {
            assert_eq!(actual, expected);
        }
        (
            DecodeError::Allocation {
                requested_bytes: actual,
            },
            DecodeError::Allocation {
                requested_bytes: expected,
            },
        ) => assert_eq!(actual, expected),
        (DecodeError::Io(actual), DecodeError::Io(expected)) => assert_eq!(actual, expected),
        (DecodeError::Unsupported(actual), DecodeError::Unsupported(expected)) => {
            assert_eq!(actual, expected);
        }
        (actual, expected) => panic!("wrong classification: {actual:?}, expected {expected:?}"),
    }
}

fn empty_request(coding: GenericRegionCoding) -> GenericRegionRequest<'static> {
    GenericRegionRequest {
        width: 0,
        height: 0,
        coding,
        typical_prediction: false,
        extended_template: false,
        skip: None,
    }
}

fn generous_limits() -> Limits {
    Limits {
        max_width: u32::MAX,
        max_height: u32::MAX,
        max_pixels_per_page: u64::MAX,
        max_decoder_managed_memory_bytes: u64::MAX,
        max_symbols: u32::MAX,
        max_work_units: u64::MAX,
        ..Limits::default()
    }
}

fn bitmap(rows: &[&str]) -> Bitmap {
    let height = u32::try_from(rows.len()).expect("test height fits");
    let width = u32::try_from(rows.first().map_or(0, |row| row.len())).expect("test width fits");
    let mut output =
        Bitmap::blank(width, height, &generous_limits()).expect("test bitmap allocates");
    for (y, row) in rows.iter().enumerate() {
        assert_eq!(row.len(), width as usize);
        for (x, value) in row.bytes().enumerate() {
            output
                .set_pixel(
                    u32::try_from(x).expect("test x fits"),
                    u32::try_from(y).expect("test y fits"),
                    value == b'1',
                )
                .expect("test pixel is in bounds");
        }
    }
    output
}

fn next_random(state: &mut u64) -> u32 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    (*state >> 32) as u32
}

fn random_bitmap(width: u32, height: u32, state: &mut u64) -> Bitmap {
    let mut output =
        Bitmap::blank(width, height, &generous_limits()).expect("random bitmap allocates");
    for byte in &mut output.data {
        *byte = u8::try_from(next_random(state) & 0xff).expect("bounded byte");
    }
    if width % 8 != 0 {
        let mask = 0xff_u8 << (8 - width % 8);
        for row in 0..height {
            let row_start = usize::try_from(row).expect("row fits") * output.stride;
            output.data[row_start + output.stride - 1] &= mask;
        }
    }
    output
}

fn scalar_xor_gray_plane_oracle(
    current: &mut Bitmap,
    previous: &Bitmap,
) -> Result<(), HalftoneError> {
    for y in 0..current.height {
        for x in 0..current.width {
            let coded = current.pixel(x, y).ok_or(MalformedError::IntegerOverflow)?;
            let previous = previous
                .pixel(x, y)
                .ok_or(MalformedError::IntegerOverflow)?;
            current.set_pixel(x, y, gray_to_binary_bit(coded, Some(previous)))?;
        }
    }
    Ok(())
}

fn scalar_accumulate_gray_byte_oracle(
    values: &mut [u32],
    offset: usize,
    byte: u8,
    plane_index: u8,
) {
    for bit in 0..8 {
        if byte & (0x80 >> bit) != 0 {
            values[offset + bit] |= 1_u32 << plane_index;
        }
    }
}

fn scalar_render_clipped_pattern_oracle(
    destination: &mut Bitmap,
    pattern: &Bitmap,
    x: i64,
    y: i64,
    operator: CombinationOperator,
) -> Result<(), HalftoneError> {
    let right = x
        .checked_add(i64::from(pattern.width()))
        .ok_or(MalformedError::IntegerOverflow)?;
    let bottom = y
        .checked_add(i64::from(pattern.height()))
        .ok_or(MalformedError::IntegerOverflow)?;
    let (x_start, x_end) = clipped_axis(x, right, destination.width());
    let (y_start, y_end) = clipped_axis(y, bottom, destination.height());
    for destination_y in y_start..y_end {
        for destination_x in x_start..x_end {
            let source_x =
                u32::try_from(destination_x - x).map_err(|_| MalformedError::IntegerOverflow)?;
            let source_y =
                u32::try_from(destination_y - y).map_err(|_| MalformedError::IntegerOverflow)?;
            let destination_x =
                u32::try_from(destination_x).map_err(|_| MalformedError::IntegerOverflow)?;
            let destination_y =
                u32::try_from(destination_y).map_err(|_| MalformedError::IntegerOverflow)?;
            let source = pattern
                .pixel(source_x, source_y)
                .ok_or(MalformedError::IntegerOverflow)?;
            let current = destination
                .pixel(destination_x, destination_y)
                .ok_or(MalformedError::IntegerOverflow)?;
            destination.set_pixel(
                destination_x,
                destination_y,
                operator.apply(current, source),
            )?;
        }
    }
    Ok(())
}

fn median_duration(mut values: Vec<Duration>) -> Duration {
    values.sort_unstable();
    values[values.len() / 2]
}

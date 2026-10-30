#![allow(clippy::duplicated_attributes)]
#![cfg(test)]

mod benchmarks;
mod diagnostics;
mod headers_and_prediction;
mod preflight_and_templates;
mod raster_parity;
mod row_ring_oracle;
mod scalar_oracles;
mod segment_framing;
mod sessions_and_snapshots;
mod word_raster_oracle;

use row_ring_oracle::*;
use scalar_oracles::*;
use word_raster_oracle::*;

use super::contexts::*;
use super::generic::*;
use super::generic_context::*;
use super::headers::*;
use super::refinement::*;
use super::segments::*;
use super::support::*;
use super::*;
use crate::bitmap::CombinationOperator;
use crate::mmr::MmrScratch;
use crate::{LimitExceeded, LimitKind};
use std::collections::VecDeque;
use std::hint::black_box;
use std::time::{Duration, Instant};

fn control_pixel(bitmap: &Bitmap, x: i128, row: i128) -> bool {
    if x < 0 || x >= i128::from(bitmap.width) || row < 0 || row >= i128::from(bitmap.height) {
        return false;
    }
    let x = usize::try_from(x).unwrap();
    let row = usize::try_from(row).unwrap();
    bitmap.data[row * bitmap.stride + x / 8] & (0x80 >> (x % 8)) != 0
}

fn control_window(bitmap: &Bitmap, center: i64, row: i64, left: i32, count: u8) -> usize {
    let start = i128::from(center) + i128::from(left);
    let end = start + i128::from(count);
    if !(1..=8).contains(&count)
        || row < 0
        || row >= i64::from(bitmap.height)
        || start < i128::from(i64::MIN)
        || start > i128::from(i64::MAX)
        || end < i128::from(i64::MIN)
        || end > i128::from(i64::MAX)
    {
        return 0;
    }
    (0..count).fold(0, |value, offset| {
        value * 2
            + usize::from(control_pixel(
                bitmap,
                start + i128::from(offset),
                i128::from(row),
            ))
    })
}

fn control_malformed(error: GenericRegionError) -> MalformedError {
    match DecodeError::from(error) {
        DecodeError::Malformed(error) => error,
        other => panic!("expected a typed malformed projection: {other:?}"),
    }
}

fn region_prefix(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&0_u32.to_be_bytes());
    bytes.extend_from_slice(&0_u32.to_be_bytes());
    bytes.push(4);
    bytes
}

fn region_info(width: u32, height: u32) -> RegionInfo {
    RegionInfo {
        width,
        height,
        x: 0,
        y: 0,
        operator: CombinationOperator::Replace,
    }
}

fn generic_header(
    width: u32,
    height: u32,
    template: GenericTemplate,
    extended: bool,
    typical: bool,
) -> GenericRegionHeader {
    let mut adaptive = [TemplateOffset::ZERO; 12];
    match (template, extended) {
        (GenericTemplate::Template0, false) => {
            adaptive[..4].copy_from_slice(&[
                TemplateOffset::new(3, -1),
                TemplateOffset::new(-3, -1),
                TemplateOffset::new(2, -2),
                TemplateOffset::new(-2, -2),
            ]);
        }
        (GenericTemplate::Template0, true) => {
            adaptive.copy_from_slice(&[
                TemplateOffset::new(-2, 0),
                TemplateOffset::new(0, -2),
                TemplateOffset::new(-2, -1),
                TemplateOffset::new(-1, -2),
                TemplateOffset::new(1, -2),
                TemplateOffset::new(2, -1),
                TemplateOffset::new(-3, 0),
                TemplateOffset::new(-4, 0),
                TemplateOffset::new(2, -2),
                TemplateOffset::new(3, -1),
                TemplateOffset::new(-2, -2),
                TemplateOffset::new(-3, -1),
            ]);
        }
        (GenericTemplate::Template1, _) => {
            adaptive[0] = TemplateOffset::new(3, -1);
        }
        (GenericTemplate::Template2 | GenericTemplate::Template3, _) => {
            adaptive[0] = TemplateOffset::new(2, -1);
        }
    }
    GenericRegionHeader {
        region: region_info(width, height),
        mmr: false,
        template,
        typical_prediction: typical,
        extended_template: extended,
        adaptive,
    }
}

fn decode_bits(
    bitmap: &mut Bitmap,
    rows: u32,
    header: &GenericRegionHeader,
    skip: Option<&Bitmap>,
    bits: &[u8],
) -> Result<Vec<usize>, GenericRegionError> {
    let mut bits = VecDeque::from(bits.to_vec());
    let mut contexts = Vec::new();
    decode_generic_with_callback(
        bitmap,
        rows,
        header.template,
        header.extended_template,
        &header.adaptive,
        header.typical_prediction,
        skip,
        |context| {
            contexts.push(context);
            bits.pop_front()
                .ok_or(GenericRegionError::RowCountMismatch {
                    expected: 1,
                    actual: 0,
                })
        },
    )?;
    Ok(contexts)
}

fn next_random(state: &mut u64) -> u32 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    (*state >> 32) as u32
}

fn random_bitmap(width: u32, height: u32, state: &mut u64) -> Bitmap {
    let limits = Limits::default();
    let mut bitmap = Bitmap::blank(width, height, &limits).expect("random bitmap");
    for y in 0..height {
        for x in 0..width {
            if next_random(state).is_multiple_of(4) {
                bitmap.set_pixel(x, y, true).expect("in bounds");
            }
        }
    }
    bitmap
}

fn arithmetic_bytes(state: &mut u64) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(1_026);
    for _ in 0..1_024 {
        bytes.push((next_random(state) & 0x7f) as u8);
    }
    bytes.extend_from_slice(&ARITHMETIC_TERMINATOR);
    bytes
}

fn arithmetic_segment_payload(width: u32, height: u32, typical: bool, coded: &[u8]) -> Vec<u8> {
    let mut payload = region_prefix(width, height);
    payload.push(if typical { 0x08 } else { 0 });
    for offset in [
        TemplateOffset::new(3, -1),
        TemplateOffset::new(-3, -1),
        TemplateOffset::new(2, -2),
        TemplateOffset::new(-2, -2),
    ] {
        payload.push(offset.x.to_be_bytes()[3]);
        payload.push(offset.y.to_be_bytes()[3]);
    }
    payload.extend_from_slice(coded);
    payload
}

fn assert_decoder_state_eq(left: &ArithmeticDecoder<'_>, right: &ArithmeticDecoder<'_>) {
    assert_eq!(left.byte_position, right.byte_position);
    assert_eq!(left.current_byte, right.current_byte);
    assert_eq!(left.code, right.code);
    assert_eq!(left.interval, right.interval);
    assert_eq!(left.bit_count, right.bit_count);
    assert_eq!(left.marker, right.marker);
}

fn assert_word_template0_parity(
    width: u32,
    height: u32,
    rows: u32,
    extended: bool,
    typical: bool,
    coded: &[u8],
) {
    let limits = Limits::default();
    let header = generic_header(width, height, GenericTemplate::Template0, extended, typical);
    let mut word_decoder = ArithmeticDecoder::new(coded).expect("word decoder");
    let mut ring_decoder = ArithmeticDecoder::new(coded).expect("ring decoder");
    let mut byte_decoder = ArithmeticDecoder::new(coded).expect("byte decoder");
    let mut word_contexts =
        vec![ArithmeticContext::new(); generic_context_count(GenericTemplate::Template0)];
    let mut ring_contexts = word_contexts.clone();
    let mut byte_contexts = word_contexts.clone();
    let mut raster = GenericWordRaster::try_new(width, height).expect("word raster");
    let mut word_bitmap = Bitmap::blank(width, height, &limits).expect("word bitmap");
    let mut ring_bitmap = Bitmap::blank(width, height, &limits).expect("ring bitmap");
    let mut byte_bitmap = Bitmap::blank(width, height, &limits).expect("byte bitmap");
    let mut ring = GenericRowRing::try_new(width, rows, u64::MAX).expect("row ring");

    let word_result = decode_nominal_template0_words(
        &mut raster,
        rows,
        typical,
        &mut word_decoder,
        &mut word_contexts,
    );
    let ring_result = decode_nominal_template0_row_ring(
        &mut ring,
        &mut ring_bitmap,
        rows,
        typical,
        &mut ring_decoder,
        &mut ring_contexts,
    );
    let byte_result = decode_generic_with_oracle(
        &mut byte_bitmap,
        rows,
        GenericTemplate::Template0,
        extended,
        &header.adaptive,
        typical,
        None,
        &mut byte_decoder,
        &mut byte_contexts,
    );
    assert_eq!(
        word_result.as_ref().err().map(ToString::to_string),
        byte_result.as_ref().err().map(ToString::to_string)
    );
    assert_eq!(
        ring_result.as_ref().err().map(ToString::to_string),
        byte_result.as_ref().err().map(ToString::to_string)
    );
    if word_result.is_ok() {
        raster
            .materialize(&mut word_bitmap)
            .expect("materialize word raster");
        assert_eq!(word_bitmap, byte_bitmap);
        assert_eq!(ring_bitmap, byte_bitmap);
    }
    assert_eq!(word_contexts, byte_contexts);
    assert_eq!(ring_contexts, byte_contexts);
    assert_decoder_state_eq(&word_decoder, &byte_decoder);
    assert_decoder_state_eq(&ring_decoder, &byte_decoder);
}

fn median_duration(mut values: Vec<Duration>) -> Duration {
    values.sort_unstable();
    values[values.len() / 2]
}

fn assert_raw_guard_result(
    actual: Result<(), GenericRegionError>,
    expected: Result<(), Option<LimitExceeded>>,
) {
    match actual {
        Ok(()) => assert_eq!(expected, Ok(())),
        Err(GenericRegionError::Decode(DecodeError::Malformed(
            MalformedError::IntegerOverflow,
        ))) => assert_eq!(expected, Err(None)),
        Err(GenericRegionError::Decode(DecodeError::LimitExceeded(error))) => {
            assert_eq!(expected, Err(Some(error)));
        }
        other => panic!("unexpected raw guard result: {other:?}"),
    }
}

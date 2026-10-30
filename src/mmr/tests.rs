#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

mod horizontal_runs;
mod normative_decode;
mod prefix_codes;
mod scratch_and_diagnostics;
mod transition_cache;

use super::*;

fn previous_horizontal_runs(
    reader: &mut BitReader<'_>,
    black: bool,
    x: u32,
    width: u32,
) -> Result<(u32, u32), MmrError> {
    let remaining = width.checked_sub(x).ok_or(MmrError::InvalidTransition)?;
    let first = decode_run(reader, black, remaining)?;
    let after_first = x.checked_add(first).ok_or(MmrError::IntegerOverflow)?;
    let second_remaining = width
        .checked_sub(after_first)
        .ok_or(MmrError::RunExceedsRow)?;
    let second = decode_run(reader, !black, second_remaining)?;
    let end = after_first
        .checked_add(second)
        .ok_or(MmrError::IntegerOverflow)?;
    if end > width {
        return Err(MmrError::RunExceedsRow);
    }
    if end == x {
        return Err(MmrError::NonProgress);
    }
    Ok((after_first, end))
}

fn horizontal_error(error: MmrError) -> MalformedError {
    let DecodeError::Malformed(error) = DecodeError::from(error) else {
        panic!("horizontal runs produce typed malformed errors");
    };
    error
}

fn run_sequence_bytes(codes: &[RunCode], start: usize) -> ([u8; 8], usize) {
    let mut packed = 0_u64;
    let mut count = 0;
    for code in codes {
        packed = (packed << code.length) | u64::from(code.bits);
        count += usize::from(code.length);
    }
    assert!(start + count < 64);
    let leading = if start == 0 {
        0
    } else {
        (0xa5_u64 << 56) & (u64::MAX << (64 - start))
    };
    (
        (leading | (packed << (64 - start - count)) | 1).to_be_bytes(),
        count,
    )
}

fn prefix_window_bytes(window: u16, width: u8, start: usize) -> [u8; 8] {
    let value = u64::from(window) << (64 - start - usize::from(width));
    let leading = if start == 0 {
        0
    } else {
        (0xa5_u64 << 56) & (u64::MAX << (64 - start))
    };
    (value | leading | 0x5a).to_be_bytes()
}

fn span_test_bitmap(width: u32) -> Bitmap {
    let mut bitmap = Bitmap::blank(width, 3, &Limits::default()).expect("bounded bitmap");
    for (index, byte) in bitmap.data.iter_mut().enumerate() {
        *byte = u8::try_from(index % 256)
            .expect("bounded pattern")
            .wrapping_mul(37)
            .wrapping_add(11);
    }
    bitmap
}

fn assert_incremental_transitions_match_scan(bitmap: &Bitmap) {
    for row in 0..bitmap.height() {
        let mut scanned = Vec::new();
        collect_transitions(bitmap, row, bitmap.width(), &mut scanned)
            .expect("oracle scan must cover a valid bitmap row");

        let mut incremental = Vec::new();
        incremental
            .try_reserve(usize::try_from(bitmap.width()).expect("test width fits usize") + 2)
            .expect("test transition storage must allocate");
        let mut emitted_black = false;
        for x in 0..bitmap.width() {
            let black = bitmap.pixel(x, row).expect("test pixel is in bounds");
            record_transition_span(
                &mut incremental,
                &mut emitted_black,
                x,
                x + 1,
                bitmap.width(),
                black,
            );
        }
        finish_transition_row(&mut incremental, bitmap.width());
        assert_eq!(incremental, scanned, "transition mismatch on row {row}");
    }
}

fn decode(data: &[u8], width: u32, height: u32) -> Bitmap {
    decode_mmr(
        data,
        width,
        height,
        &Limits::default(),
        EofbPolicy::Optional,
    )
    .expect("independently derived MMR vector must decode")
}

fn assert_row(bitmap: &Bitmap, row: u32, expected: &[bool]) {
    assert_eq!(
        usize::try_from(bitmap.width()).expect("test width fits usize"),
        expected.len()
    );
    for (x, value) in expected.iter().copied().enumerate() {
        assert_eq!(
            bitmap.pixel(u32::try_from(x).expect("test x fits u32"), row,),
            Some(value),
            "pixel mismatch at ({x}, {row})"
        );
    }
}

fn capacity_charge(scratch: &MmrScratch) -> u64 {
    u64::try_from(
        (scratch.transitions.capacity() + scratch.row_transitions.capacity())
            * core::mem::size_of::<u32>(),
    )
    .unwrap()
}

fn fresh_capacity_charge(width: u32) -> u64 {
    let mut first = Vec::<u32>::new();
    let mut second = Vec::<u32>::new();
    let count = usize::try_from(width).unwrap() + 2;
    first.try_reserve(count).unwrap();
    second.try_reserve(count).unwrap();
    u64::try_from((first.capacity() + second.capacity()) * core::mem::size_of::<u32>()).unwrap()
}

fn bitmap_capacity_charge(width: u32) -> u64 {
    let bitmap = Bitmap::blank(width, 1, &Limits::default()).unwrap();
    u64::try_from(bitmap.data.capacity()).unwrap()
}

fn assert_limit(error: MmrError, kind: LimitKind, limit: u64, actual: u64) {
    let DecodeError::LimitExceeded(found) = DecodeError::from(error) else {
        panic!("expected an unchanged resource-limit error");
    };
    assert_eq!(
        found,
        LimitExceeded {
            kind,
            limit,
            actual
        }
    );
}

fn assert_kind(error: MmrError, expected: MmrErrorKind) {
    let DecodeError::Malformed(found) = DecodeError::from(error) else {
        panic!("expected a typed malformed MMR error");
    };
    assert_eq!(found, MalformedError::InvalidMmr(expected));
}

fn assert_cleared(scratch: &MmrScratch, charge: u64) {
    assert!(scratch.transitions.is_empty());
    assert!(scratch.row_transitions.is_empty());
    assert_eq!(capacity_charge(scratch), charge);
    assert_eq!(scratch.retained_bytes(), charge);
}

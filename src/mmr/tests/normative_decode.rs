#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

use super::*;

#[test]
fn all_white_uses_vertical_zero() {
    let bitmap = decode(&[0x80], 8, 1);
    assert_row(&bitmap, 0, &[false; 8]);
}

#[test]
fn all_black_uses_zero_white_horizontal_run() {
    let bitmap = decode(&[0x26, 0xA2, 0x80], 8, 1);
    assert_row(&bitmap, 0, &[true; 8]);
}

#[test]
fn horizontal_white_makeup_run_decodes() {
    let bitmap = decode(&[0x3B, 0x35, 0x0D, 0xC0], 64, 1);
    assert_row(&bitmap, 0, &[false; 64]);
}

#[test]
fn horizontal_black_makeup_run_decodes() {
    let bitmap = decode(&[0x26, 0xA0, 0x78, 0x6E], 64, 1);
    assert_row(&bitmap, 0, &[true; 64]);
}

#[test]
fn identical_black_rows_update_the_reference_line() {
    let bitmap = decode(&[0x26, 0xA2, 0xE0], 8, 2);
    assert_row(&bitmap, 0, &[true; 8]);
    assert_row(&bitmap, 1, &[true; 8]);
}

#[test]
fn identical_islands_use_three_vertical_zero_codes() {
    let bitmap = decode(&[0x2F, 0xF8], 8, 2);
    let island = [false, false, true, true, false, false, false, false];
    assert_row(&bitmap, 0, &island);
    assert_row(&bitmap, 1, &island);
}

#[test]
fn island_shift_right_uses_vertical_right_one() {
    let bitmap = decode(&[0x2F, 0xDB, 0x80], 8, 2);
    assert_row(
        &bitmap,
        0,
        &[false, false, true, true, false, false, false, false],
    );
    assert_row(
        &bitmap,
        1,
        &[false, false, false, true, true, false, false, false],
    );
}

#[test]
fn island_shift_left_uses_vertical_left_one() {
    let bitmap = decode(&[0x2F, 0xD2, 0x80], 8, 2);
    assert_row(
        &bitmap,
        0,
        &[false, false, true, true, false, false, false, false],
    );
    assert_row(
        &bitmap,
        1,
        &[false, true, true, false, false, false, false, false],
    );
}

#[test]
fn pass_skips_reference_island() {
    let bitmap = decode(&[0x2F, 0xC6], 8, 2);
    assert_row(
        &bitmap,
        0,
        &[false, false, true, true, false, false, false, false],
    );
    assert_row(&bitmap, 1, &[false; 8]);
}

#[test]
fn required_eofb_can_start_off_byte_boundary() {
    let bitmap = decode_mmr(
        &[0x80, 0x08, 0x00, 0x80],
        8,
        1,
        &Limits::default(),
        EofbPolicy::Required,
    )
    .expect("EOFB vector must decode");
    assert_row(&bitmap, 0, &[false; 8]);
}

#[test]
fn required_eofb_rejects_omission() {
    let error = decode_mmr(&[0x80], 8, 1, &Limits::default(), EofbPolicy::Required)
        .expect_err("required EOFB must not be omitted");
    assert!(matches!(error, MmrError::MissingEofb));
}

#[test]
fn prefix_decoder_consumes_two_concatenated_eofb_planes() {
    let stream = [
        0x80, 0x08, 0x00, 0x80, // all-white plane and EOFB
        0x26, 0xA2, 0x80, 0x08, 0x00, 0x80, // all-black plane and EOFB
    ];

    let (white, white_bytes) =
        decode_mmr_prefix(&stream, 8, 1, &Limits::default()).expect("first prefix must decode");
    assert_eq!(white_bytes, 4);
    assert_row(&white, 0, &[false; 8]);

    let remainder = stream
        .get(white_bytes..)
        .expect("consumed byte count is within the test stream");
    let (black, black_bytes) =
        decode_mmr_prefix(remainder, 8, 1, &Limits::default()).expect("second prefix must decode");
    assert_eq!(black_bytes, 6);
    assert_eq!(white_bytes + black_bytes, stream.len());
    assert_row(&black, 0, &[true; 8]);
}

#[test]
fn prefix_decoder_rejects_truncated_eofb() {
    let error = decode_mmr_prefix(&[0x80, 0x08, 0x00], 8, 1, &Limits::default())
        .expect_err("23 post-bitmap bits cannot contain EOFB");
    assert!(matches!(error, MmrError::MissingEofb));
}

#[test]
fn prefix_decoder_rejects_nonzero_alignment_padding() {
    let error = decode_mmr_prefix(&[0x80, 0x08, 0x00, 0x81], 8, 1, &Limits::default())
        .expect_err("bits after EOFB and before the byte boundary must be zero");
    assert!(matches!(error, MmrError::NonZeroEofbPadding));
}

#[test]
fn extension_mode_is_rejected() {
    let error = decode_mmr(
        &[0x02, 0x00],
        8,
        1,
        &Limits::default(),
        EofbPolicy::Optional,
    )
    .expect_err("T.6 extension mode is forbidden by T.88");
    assert!(matches!(error, MmrError::ExtensionMode { .. }));
}

#[test]
fn truncated_horizontal_run_is_rejected() {
    let error = decode_mmr(&[0x26], 8, 1, &Limits::default(), EofbPolicy::Optional)
        .expect_err("truncated horizontal run must fail");
    assert!(matches!(
        error,
        MmrError::Reader(MalformedError::UnexpectedEof { .. })
    ));
}

#[test]
fn run_overshoot_is_rejected() {
    let error = decode_mmr(&[0x33], 4, 1, &Limits::default(), EofbPolicy::Optional)
        .expect_err("white run 8 must not fit a width-four row");
    assert!(matches!(error, MmrError::RunExceedsRow));
}

#[test]
fn work_limit_is_enforced_before_decoding() {
    let limits = Limits {
        max_work_units: 15,
        ..Limits::default()
    };
    let error = decode_mmr(&[0x80], 8, 1, &limits, EofbPolicy::Optional)
        .expect_err("two raster passes already exceed the work limit");
    assert!(matches!(
        error,
        MmrError::Bitmap(DecodeError::LimitExceeded(LimitExceeded {
            kind: LimitKind::Work,
            limit: 15,
            actual: 16
        }))
    ));
}

#[test]
fn both_transition_vectors_are_counted_as_managed_memory() {
    let limits = Limits {
        max_decoder_managed_memory_bytes: 80,
        ..Limits::default()
    };
    let error = decode_mmr(&[0x80], 8, 1, &limits, EofbPolicy::Optional)
        .expect_err("reserved bitmap plus both transition vectors exceed the budget");
    assert_limit(error, LimitKind::Memory, 80, bitmap_capacity_charge(8) + 80);
}

#[test]
fn optional_eofb_accepts_bounded_remainder() {
    let bitmap = decode_mmr(
        &[0x80, 0xFF],
        8,
        1,
        &Limits::default(),
        EofbPolicy::Optional,
    )
    .expect("known-length remainder may be skipped");
    assert_row(&bitmap, 0, &[false; 8]);
}

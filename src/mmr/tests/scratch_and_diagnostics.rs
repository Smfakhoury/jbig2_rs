#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

use super::*;

#[test]
fn scratch_clear_retains_and_release_drops_both_vectors_idempotently() {
    let bitmap = Bitmap::blank(128, 1, &Limits::default()).unwrap();
    let mut scratch = MmrScratch::default();
    prepare_transition_storage(128, &bitmap, &Limits::default(), &mut scratch).unwrap();
    let charge = capacity_charge(&scratch);
    assert_eq!(charge, fresh_capacity_charge(128));
    assert_eq!(scratch.reserve_events, 2);
    scratch.transitions.extend_from_slice(&[0, 128, 128]);
    scratch.row_transitions.extend_from_slice(&[64, 128, 128]);
    let capacities = (
        scratch.transitions.capacity(),
        scratch.row_transitions.capacity(),
    );
    scratch.clear();
    scratch.clear();
    assert_cleared(&scratch, charge);
    assert_eq!(
        capacities,
        (
            scratch.transitions.capacity(),
            scratch.row_transitions.capacity()
        )
    );
    assert_eq!(scratch.reserve_events, 2);
    scratch.release();
    scratch.release();
    assert_cleared(&scratch, 0);
    assert_eq!(scratch.transitions.capacity(), 0);
    assert_eq!(scratch.row_transitions.capacity(), 0);
    assert_eq!(scratch.reserve_events, 0);
}

#[test]
fn reused_scratch_recovers_after_decode_and_framing_errors_then_shrinks() {
    let mut scratch = MmrScratch::default();
    let limits = Limits::default();
    let wide =
        decode_mmr_reused(&[0x80], 128, 1, &limits, EofbPolicy::Optional, &mut scratch).unwrap();
    assert_eq!(wide.bytes(), &[0; 16]);
    let charge = capacity_charge(&scratch);
    assert_eq!(charge, fresh_capacity_charge(128));
    assert_cleared(&scratch, charge);
    for (data, expected) in [
        (&[0x02][..], MmrErrorKind::ExtensionMode { bit_offset: 0 }),
        (&[0x00][..], MmrErrorKind::InvalidMode { bit_offset: 0 }),
    ] {
        let error =
            decode_mmr_reused(data, 8, 1, &limits, EofbPolicy::Optional, &mut scratch).unwrap_err();
        assert_kind(error, expected);
        assert_cleared(&scratch, charge);
        assert_eq!(scratch.reserve_events, 2);
    }
    let error =
        decode_mmr_reused(&[0x80], 8, 1, &limits, EofbPolicy::Required, &mut scratch).unwrap_err();
    assert_kind(error, MmrErrorKind::MissingEofb);
    assert_cleared(&scratch, charge);
    let below_requested = Limits {
        max_decoder_managed_memory_bytes: 80,
        ..limits
    };
    let error = decode_mmr_reused(
        &[0x80],
        8,
        1,
        &below_requested,
        EofbPolicy::Optional,
        &mut scratch,
    )
    .unwrap_err();
    assert_limit(error, LimitKind::Memory, 80, bitmap_capacity_charge(8) + 80);
    // The requested-size rejection precedes any retained-capacity release.
    assert_cleared(&scratch, charge);
    assert_eq!(scratch.reserve_events, 2);
    let tight = Limits {
        max_decoder_managed_memory_bytes: bitmap_capacity_charge(8) + fresh_capacity_charge(8),
        ..limits
    };
    assert!(charge + bitmap_capacity_charge(8) > tight.max_decoder_managed_memory_bytes);
    let bitmap = decode_mmr_reused(
        &[0x26, 0xA2, 0x80],
        8,
        1,
        &tight,
        EofbPolicy::Optional,
        &mut scratch,
    )
    .unwrap();
    assert_eq!(bitmap.bytes(), &[0xFF]);
    assert_cleared(&scratch, fresh_capacity_charge(8));
    // A retained-over-budget release resets the counter before two fresh reserves.
    assert_eq!(scratch.reserve_events, 2);
}

#[test]
fn requested_and_actual_capacity_limits_are_distinct_inclusive_and_recoverable() {
    let width = 1;
    let bitmap_bytes = bitmap_capacity_charge(width);
    let requested = bitmap_bytes + u64::from(width + 2) * 2 * 4;
    let actual = bitmap_bytes + fresh_capacity_charge(width);
    assert!(
        requested < actual,
        "the minimum u32 Vec capacity rounds three elements up"
    );
    let mut scratch = MmrScratch::default();
    for (limit, expected_actual) in [
        (requested - 1, requested),
        (requested, actual),
        (actual - 1, actual),
    ] {
        let limits = Limits {
            max_decoder_managed_memory_bytes: limit,
            ..Limits::default()
        };
        let error = decode_mmr_reused(
            &[0x80],
            width,
            1,
            &limits,
            EofbPolicy::Optional,
            &mut scratch,
        )
        .unwrap_err();
        assert_limit(error, LimitKind::Memory, limit, expected_actual);
        assert_cleared(&scratch, 0);
        assert_eq!(scratch.reserve_events, 0);
    }
    let limits = Limits {
        max_decoder_managed_memory_bytes: actual,
        ..Limits::default()
    };
    let bitmap = decode_mmr_reused(
        &[0x80],
        width,
        1,
        &limits,
        EofbPolicy::Optional,
        &mut scratch,
    )
    .unwrap();
    assert_eq!((bitmap.width, bitmap.height, bitmap.stride), (1, 1, 1));
    assert_eq!(bitmap.bytes(), &[0]);
    assert_cleared(&scratch, actual - bitmap_bytes);
    assert_eq!(scratch.reserve_events, 2);
}

#[test]
fn stream_and_each_mode_work_boundary_preserve_retained_scratch_and_recover() {
    let mut scratch = MmrScratch::default();
    let bitmap = decode_mmr_reused(
        &[0xC0],
        8,
        2,
        &Limits::default(),
        EofbPolicy::Optional,
        &mut scratch,
    )
    .unwrap();
    assert_eq!(bitmap.bytes(), &[0, 0]);
    let charge = capacity_charge(&scratch);
    for (stream, work, kind, limit, actual) in [
        (0, 0, LimitKind::StreamBytes, 0, 1),
        (1, 31, LimitKind::Work, 31, 32),
        (1, 32, LimitKind::Work, 32, 33),
        (1, 33, LimitKind::Work, 33, 34),
    ] {
        let limits = Limits {
            max_stream_bytes: stream,
            max_work_units: work,
            ..Limits::default()
        };
        let error = decode_mmr_reused(&[0xC0], 8, 2, &limits, EofbPolicy::Optional, &mut scratch)
            .unwrap_err();
        assert_limit(error, kind, limit, actual);
        assert_cleared(&scratch, charge);
        assert_eq!(scratch.reserve_events, 2);
        let exact = Limits {
            max_stream_bytes: 1,
            max_work_units: 34,
            ..Limits::default()
        };
        let recovered =
            decode_mmr_reused(&[0xC0], 8, 2, &exact, EofbPolicy::Optional, &mut scratch).unwrap();
        assert_eq!(recovered.bytes(), &[0, 0]);
        assert_cleared(&scratch, charge);
    }
}

#[test]
fn framing_at_every_alignment_has_exact_consumption_and_padding_errors() {
    let cases: &[(u8, &[u8])] = &[
        (0, &[0x00, 0x10, 0x01]),
        (1, &[0x00, 0x08, 0x00, 0x80]),
        (2, &[0x00, 0x04, 0x00, 0x40]),
        (3, &[0x00, 0x02, 0x00, 0x20]),
        (4, &[0x00, 0x01, 0x00, 0x10]),
        (5, &[0x00, 0x00, 0x80, 0x08]),
        (6, &[0x00, 0x00, 0x40, 0x04]),
        (7, &[0x00, 0x00, 0x20, 0x02]),
    ];
    for &(offset, bytes) in cases {
        let mut reader = bit_reader(bytes);
        reader.read_bits(offset).unwrap();
        finish_block(&mut reader, EofbPolicy::Required).unwrap();
        assert_eq!(reader.bit_position(), bytes.len() * 8);
        assert_eq!(reader.bytes, bytes);

        let mut with_suffix = bytes.to_vec();
        with_suffix.extend_from_slice(&[0xA5, 0x5A]);
        let mut prefix = bit_reader(&with_suffix);
        prefix.read_bits(offset).unwrap();
        finish_required_prefix(&mut prefix).unwrap();
        assert_eq!(prefix.bit_position(), bytes.len() * 8);
        assert_eq!(&prefix.bytes[prefix.bit_position() / 8..], &[0xA5, 0x5A]);
        let mut bounded = bit_reader(&with_suffix);
        bounded.read_bits(offset).unwrap();
        assert_kind(
            finish_block(&mut bounded, EofbPolicy::Required).unwrap_err(),
            MmrErrorKind::TrailingData,
        );
        assert_eq!(bounded.bit_position(), usize::from(offset) + 24);
        let mut optional = bit_reader(&with_suffix);
        optional.read_bits(offset).unwrap();
        finish_block(&mut optional, EofbPolicy::Optional).unwrap();
        assert_eq!(optional.bit_position(), usize::from(offset));

        if offset != 0 {
            let mut nonzero = bytes.to_vec();
            *nonzero.last_mut().unwrap() |= 1;
            for prefix_mode in [false, true] {
                let mut reader = bit_reader(&nonzero);
                reader.read_bits(offset).unwrap();
                let error = if prefix_mode {
                    finish_required_prefix(&mut reader)
                } else {
                    finish_block(&mut reader, EofbPolicy::Required)
                }
                .unwrap_err();
                assert_kind(error, MmrErrorKind::NonZeroPadding);
                assert_eq!(reader.bit_position(), nonzero.len() * 8);
                assert_eq!(reader.bytes, nonzero);
            }
        }
    }
}

#[test]
fn missing_and_invalid_eofb_have_distinct_exact_reader_effects() {
    for prefix_mode in [false, true] {
        for bytes in [&[][..], &[0x00][..], &[0x00, 0x10][..]] {
            let mut reader = bit_reader(bytes);
            let error = if prefix_mode {
                finish_required_prefix(&mut reader)
            } else {
                finish_block(&mut reader, EofbPolicy::Required)
            }
            .unwrap_err();
            assert_kind(error, MmrErrorKind::MissingEofb);
            assert_eq!(reader.bit_position(), 0);
            assert_eq!(reader.bytes, bytes);
        }
        let mut reader = bit_reader(&[0x00, 0x10, 0x00, 0xA5]);
        let error = if prefix_mode {
            finish_required_prefix(&mut reader)
        } else {
            finish_block(&mut reader, EofbPolicy::Required)
        }
        .unwrap_err();
        assert_kind(error, MmrErrorKind::InvalidEofb);
        assert_eq!(reader.bit_position(), 24);
        assert_eq!(reader.bytes, &[0x00, 0x10, 0x00, 0xA5]);
    }
    let (bitmap, consumed) =
        decode_mmr_prefix(&[0x00, 0x10, 0x01, 0xA5], 0, 0, &Limits::default()).unwrap();
    assert_eq!((bitmap.width, bitmap.height, bitmap.stride), (0, 0, 0));
    assert_eq!(bitmap.bytes(), &[]);
    assert_eq!(consumed, 3);
}

#[test]
fn invalid_mode_and_run_codes_preserve_start_offsets_and_exact_eof_states() {
    let mut mode = bit_reader(&[0x80, 0x00]);
    assert_eq!(mode.read_bits(3), Ok(4));
    assert_kind(
        decode_mode(&mut mode).unwrap_err(),
        MmrErrorKind::InvalidMode { bit_offset: 3 },
    );
    assert_eq!(mode.bit_position(), 10);
    let mut truncated = bit_reader(&[0x00]);
    truncated.read_bits(2).unwrap();
    let DecodeError::Malformed(error) = DecodeError::from(decode_mode(&mut truncated).unwrap_err())
    else {
        panic!("mode truncation is a reader error");
    };
    assert_eq!(
        error,
        MalformedError::UnexpectedEof {
            offset: 8,
            needed: 1,
            remaining: 0,
        }
    );
    assert_eq!(truncated.bit_position(), 8);
    for black in [false, true] {
        let mut invalid = bit_reader(&[0x80, 0x00, 0x00]);
        invalid.read_bits(3).unwrap();
        assert_kind(
            decode_run_code(&mut invalid, black).unwrap_err(),
            MmrErrorKind::InvalidRunCode {
                bit_offset: 3,
                black,
            },
        );
        assert_eq!(invalid.bit_position(), 16);
        assert_eq!(invalid.bytes, &[0x80, 0x00, 0x00]);
        let mut truncated = bit_reader(&[0x80, 0x00]);
        truncated.read_bits(4).unwrap();
        let MmrError::Reader(error) = decode_run_code(&mut truncated, black).unwrap_err() else {
            panic!("run truncation is a reader error");
        };
        assert_eq!(
            error,
            MalformedError::UnexpectedEof {
                offset: 16,
                needed: 1,
                remaining: 0,
            }
        );
        assert_eq!(truncated.bit_position(), 16);
    }
}

#[test]
fn static_vertical_two_and_three_cases_have_literal_pixels_and_bounds() {
    for (data, expected) in [
        (&[0x0A][..], &[0x03][..]),
        (&[0x05][..], &[0x07][..]),
        (&[0x26, 0xA2, 0x87][..], &[0xFF, 0x3F][..]),
        (&[0x26, 0xA2, 0x83, 0x80][..], &[0xFF, 0x1F][..]),
    ] {
        let height = u32::try_from(expected.len()).unwrap();
        let bitmap = decode(data, 8, height);
        assert_eq!((bitmap.width, bitmap.height, bitmap.stride), (8, height, 1));
        assert_eq!(bitmap.bytes(), expected);
    }
    for (data, width, height) in [
        (&[0x0C][..], 8, 1),
        (&[0x06][..], 8, 1),
        (&[0x08][..], 1, 1),
        (&[0x04][..], 2, 1),
        (&[0x26, 0xA2, 0x84][..], 8, 2),
        (&[0x26, 0xA2, 0x82][..], 8, 2),
    ] {
        assert_kind(
            decode_mmr(
                data,
                width,
                height,
                &Limits::default(),
                EofbPolicy::Required,
            )
            .unwrap_err(),
            MmrErrorKind::InvalidTransition,
        );
    }
}

#[test]
fn horizontal_zero_pair_is_nonprogress_and_run_bounds_stop_at_the_code() {
    assert_kind(
        decode_mmr(
            &[0x26, 0xA1, 0xB8],
            8,
            1,
            &Limits::default(),
            EofbPolicy::Optional,
        )
        .unwrap_err(),
        MmrErrorKind::NonProgress,
    );
    for (bytes, black, consumed) in [(&[0x98][..], false, 5), (&[0x14][..], true, 6)] {
        let mut reader = bit_reader(bytes);
        assert_kind(
            decode_run(&mut reader, black, 7).unwrap_err(),
            MmrErrorKind::RunExceedsRow,
        );
        assert_eq!(reader.bit_position(), consumed);
        assert_eq!(reader.bytes, bytes);
        let mut exact = bit_reader(bytes);
        assert_eq!(decode_run(&mut exact, black, 8).unwrap(), 8);
        assert_eq!(exact.bit_position(), consumed);
    }
}

#[test]
fn makeup_runs_require_a_terminal_and_enforce_the_exact_remaining_width() {
    for (bytes, black, value, consumed) in [
        (&[0xD9, 0xA8][..], false, 64, 13),
        (&[0x03, 0xC3, 0x70][..], true, 64, 20),
        (&[0xDE, 0xCD, 0x40][..], false, 128, 18),
        (&[0x03, 0xC0, 0xF0, 0xDC][..], true, 128, 30),
    ] {
        let mut exact = bit_reader(bytes);
        assert_eq!(decode_run(&mut exact, black, value).unwrap(), value);
        assert_eq!(exact.bit_position(), consumed);
        assert_eq!(exact.bytes, bytes);
        let mut short = bit_reader(bytes);
        assert_kind(
            decode_run(&mut short, black, 63).unwrap_err(),
            MmrErrorKind::RunExceedsRow,
        );
        assert_eq!(short.bit_position(), if black { 10 } else { 5 });
    }
    for (bytes, black) in [(&[0xD8][..], false), (&[0x03, 0xC0][..], true)] {
        let mut reader = bit_reader(bytes);
        let MmrError::Reader(error) = decode_run(&mut reader, black, 64).unwrap_err() else {
            panic!("make-up alone does not terminate a run");
        };
        assert_eq!(
            error,
            MalformedError::UnexpectedEof {
                offset: (bytes.len() * 8) as u64,
                needed: 1,
                remaining: 0,
            }
        );
        assert_eq!(reader.bit_position(), bytes.len() * 8);
        assert_eq!(reader.bytes, bytes);
    }
}

#[test]
fn defensive_reference_errors_preserve_inputs_and_exact_cursor_effects() {
    for transitions in [&[][..], &[8][..]] {
        assert_kind(
            reference_elements(transitions, 0, false, 8, true).unwrap_err(),
            MmrErrorKind::InvalidTransition,
        );
    }
    let mut cursor = 0;
    assert_kind(
        reference_elements_from(&[], 0, false, 8, true, &mut cursor).unwrap_err(),
        MmrErrorKind::InvalidTransition,
    );
    assert_eq!(cursor, 0);
    cursor = 2;
    assert_kind(
        reference_elements_from(&[8, 8], 0, false, 8, true, &mut cursor).unwrap_err(),
        MmrErrorKind::InvalidTransition,
    );
    assert_eq!(cursor, 2);
    let transitions = [0, 2, 1, 8, 8];
    cursor = 0;
    assert_kind(
        reference_elements_from(&transitions, 2, false, 8, true, &mut cursor).unwrap_err(),
        MmrErrorKind::InvalidTransition,
    );
    assert_eq!(cursor, 1);
    assert_eq!(transitions, [0, 2, 1, 8, 8]);
}

#[test]
fn error_conversion_preserves_all_classifications_and_payloads() {
    for (error, expected) in [
        (
            MmrError::InvalidMode { bit_offset: 37 },
            MmrErrorKind::InvalidMode { bit_offset: 37 },
        ),
        (
            MmrError::ExtensionMode { bit_offset: 41 },
            MmrErrorKind::ExtensionMode { bit_offset: 41 },
        ),
        (
            MmrError::InvalidRunCode {
                bit_offset: 43,
                black: true,
            },
            MmrErrorKind::InvalidRunCode {
                bit_offset: 43,
                black: true,
            },
        ),
        (
            MmrError::InvalidRunCode {
                bit_offset: 47,
                black: false,
            },
            MmrErrorKind::InvalidRunCode {
                bit_offset: 47,
                black: false,
            },
        ),
        (MmrError::RunExceedsRow, MmrErrorKind::RunExceedsRow),
        (MmrError::InvalidTransition, MmrErrorKind::InvalidTransition),
        (MmrError::NonProgress, MmrErrorKind::NonProgress),
        (MmrError::TooManyModes, MmrErrorKind::TooManyModes),
        (MmrError::MissingEofb, MmrErrorKind::MissingEofb),
        (MmrError::InvalidEofb, MmrErrorKind::InvalidEofb),
        (MmrError::TrailingData, MmrErrorKind::TrailingData),
        (MmrError::NonZeroEofbPadding, MmrErrorKind::NonZeroPadding),
    ] {
        assert_kind(error, expected);
    }
    let DecodeError::Allocation { requested_bytes } = DecodeError::from(MmrError::Allocation {
        requested_bytes: 4096,
    }) else {
        panic!("allocation size must survive conversion");
    };
    assert_eq!(requested_bytes, 4096);
    let DecodeError::Malformed(error) = DecodeError::from(MmrError::IntegerOverflow) else {
        panic!("integer overflow must survive conversion");
    };
    assert_eq!(error, MalformedError::IntegerOverflow);
    let DecodeError::Malformed(error) = DecodeError::from(MmrError::Bitmap(
        DecodeError::Malformed(MalformedError::InvalidBitCount(33)),
    )) else {
        panic!("bitmap errors must not be reclassified");
    };
    assert_eq!(error, MalformedError::InvalidBitCount(33));
}

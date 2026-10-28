#![allow(clippy::duplicated_attributes)]
#![cfg(test)]

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

#[test]
fn raw_gray_shape_guards_and_skip_rectangles_use_exact_half_open_geometry() {
    for (width, height) in [(8, 2), (9, 1)] {
        let mut current = bitmap(&["101010101", "010101010"]);
        let previous = Bitmap::blank(width, height, &Limits::default()).unwrap();
        let before = current.bytes().to_vec();
        assert!(matches!(
            xor_gray_plane_bytes(&mut current, &previous),
            Err(HalftoneError::Decode(DecodeError::Malformed(
                MalformedError::IntegerOverflow
            )))
        ));
        assert_eq!(current.bytes(), before);
    }
    for x in [-10_i64, -2, -1, 0, 8, 9, 10] {
        for y in [-10_i64, -2, -1, 0, 2, 3, 4] {
            for width in [1_u32, 2, 9] {
                for height in [1_u32, 2, 3] {
                    let right = x + i64::from(width);
                    let bottom = y + i64::from(height);
                    let overlaps = x < 9 && right > 0 && y < 3 && bottom > 0;
                    assert_eq!(
                        half_open_outside(x, y, right, bottom, width, height, 9, 3),
                        !overlaps
                    );
                }
            }
        }
    }
}

#[test]
fn raw_halftone_memory_peaks_match_wide_stage_arithmetic() {
    let patterns = [bitmap(&["101010101", "010101010"])];
    for (grid_width, grid_height) in [
        (0, 0),
        (1, 1),
        (9, 3),
        (u32::MAX, 1_000_000_000),
        (u32::MAX, u32::MAX),
    ] {
        for (width, height) in [(0, 0), (9, 3), (u32::MAX, u32::MAX)] {
            for enable_skip in [false, true] {
                for depth in [0, 1, 2, u8::MAX] {
                    let header = HalftoneHeader {
                        mmr: false,
                        template: 0,
                        enable_skip,
                        operator: CombinationOperator::Or,
                        default_pixel: false,
                        grid_width,
                        grid_height,
                        grid_x: 0,
                        grid_y: 0,
                        grid_vector_x: 256,
                        grid_vector_y: 0,
                    };
                    let cells = u128::from(grid_width) * u128::from(grid_height);
                    let plane = u128::from(grid_width).div_ceil(8) * u128::from(grid_height);
                    let skip = if enable_skip { plane } else { 0 };
                    let active = u128::from(depth.min(2));
                    let output = u128::from(width).div_ceil(8) * u128::from(height);
                    let fixed = 4 + 4 * cells + skip;
                    let peak = (fixed + active * plane).max(fixed + output);
                    let representable = u64::try_from(peak).unwrap_or(u64::MAX);
                    for limit in [0, representable.saturating_sub(1), representable, u64::MAX] {
                        let limits = Limits {
                            max_decoder_managed_memory_bytes: limit,
                            ..Limits::default()
                        };
                        let result = enforce_halftone_memory(
                            width, height, &header, &patterns, depth, &limits,
                        );
                        if peak > u128::from(u64::MAX) {
                            assert!(matches!(
                                result,
                                Err(HalftoneError::Decode(DecodeError::Malformed(
                                    MalformedError::IntegerOverflow
                                )))
                            ));
                        } else if peak > u128::from(limit) {
                            assert!(matches!(result, Err(HalftoneError::Decode(
                                DecodeError::LimitExceeded(LimitExceeded {
                                    kind: LimitKind::Memory, limit: actual_limit, actual,
                                })
                            )) if actual_limit == limit && u128::from(actual) == peak));
                        } else {
                            result.unwrap();
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn raw_pattern_and_bitmap_preflights_preserve_exact_fields_and_priority() {
    for width in [0, 1, u8::MAX] {
        for height in [0, 1, u8::MAX] {
            for template in 0..=u8::MAX {
                for mmr in [false, true] {
                    let header = PatternDictionaryHeader {
                        mmr,
                        template,
                        pattern_width: width,
                        pattern_height: height,
                        gray_max: u32::MAX,
                    };
                    let result = validate_pattern_header(header);
                    if width == 0 || height == 0 {
                        assert!(matches!(result,
                            Err(HalftoneError::InvalidPatternDimensions { width: w, height: h })
                                if w == width && h == height));
                    } else if template > 3 || (mmr && template != 0) {
                        let encoded =
                            u8::try_from((u16::from(template) * 2) & 255).unwrap() | u8::from(mmr);
                        assert!(matches!(result,
                            Err(HalftoneError::InvalidPatternDictionaryFlags(actual))
                                if actual == encoded));
                    } else {
                        result.unwrap();
                    }
                }
            }
        }
    }
    let limits = Limits::default();
    for (width, height) in [(0, 0), (0, 1), (1, 0), (1, 1), (9, 2)] {
        let bitmap = Bitmap::blank(width, height, &limits).unwrap();
        for expected_width in [0, 1, 9, u32::MAX] {
            for expected_height in [0, 1, 2, u32::MAX] {
                let result = require_bitmap_dimensions(&bitmap, expected_width, expected_height);
                if (width, height) == (expected_width, expected_height) {
                    result.unwrap();
                } else {
                    assert!(
                        matches!(result, Err(HalftoneError::InvalidBitmapDimensions {
                        expected_width: ew, expected_height: eh,
                        actual_width: aw, actual_height: ah,
                    }) if (ew, eh, aw, ah) == (expected_width, expected_height, width, height))
                    );
                }
            }
        }
    }
}

#[test]
fn raw_collective_slicing_matches_literal_pixels_and_exact_budget_boundaries() {
    let generous = Limits::default();
    for count in [1_u32, 2, 3, 8] {
        for width in [1_u8, 7, 8, 9] {
            let total_width = count * u32::from(width);
            let mut collective = Bitmap::blank(total_width, 2, &generous).unwrap();
            for y in 0..2 {
                for x in 0..total_width {
                    collective.set_pixel(x, y, (x + y * 3) % 5 < 2).unwrap();
                }
            }
            let before = collective.bytes().to_vec();
            let header = PatternDictionaryHeader {
                mmr: false,
                template: 0,
                pattern_width: width,
                pattern_height: 2,
                gray_max: count - 1,
            };
            let memory = u64::from(total_width).div_ceil(8) * 2
                + u64::from(count)
                    * (u64::from(width).div_ceil(8) * 2
                        + u64::try_from(core::mem::size_of::<Bitmap>()).unwrap());
            let pixels = u64::from(total_width) * 2;
            for memory_limit in [memory - 1, memory] {
                for work_limit in [pixels - 1, pixels] {
                    let limits = Limits {
                        max_symbols: count,
                        max_decoder_managed_memory_bytes: memory_limit,
                        max_work_units: work_limit,
                        ..generous
                    };
                    let mut work = WorkBudget::new(&limits);
                    let result = slice_collective_bitmap(&collective, header, &limits, &mut work);
                    if memory_limit < memory {
                        assert!(matches!(result, Err(HalftoneError::Decode(
                            DecodeError::LimitExceeded(LimitExceeded {
                                kind: LimitKind::Memory, limit, actual,
                            })
                        )) if limit == memory_limit && actual == memory));
                        assert_eq!(work.used(), 0);
                    } else if work_limit < pixels {
                        assert!(matches!(result, Err(HalftoneError::Decode(
                            DecodeError::LimitExceeded(LimitExceeded {
                                kind: LimitKind::Work, limit, actual,
                            })
                        )) if limit == work_limit && actual == pixels));
                        assert_eq!(work.used(), 0);
                    } else {
                        let patterns = result.unwrap();
                        assert_eq!(patterns.len(), usize::try_from(count).unwrap());
                        for (index, pattern) in patterns.iter().enumerate() {
                            let mut expected = vec![0_u8; usize::from(width).div_ceil(8) * 2];
                            for y in 0..2_usize {
                                for x in 0..usize::from(width) {
                                    if (index * usize::from(width) + x + y * 3) % 5 < 2 {
                                        expected[y * usize::from(width).div_ceil(8) + x / 8] |=
                                            0x80 >> (x % 8);
                                    }
                                }
                            }
                            assert_eq!(pattern.bytes(), expected);
                        }
                        assert_eq!(work.used(), pixels);
                    }
                    assert_eq!(collective.bytes(), before);
                }
            }
        }
    }
}

#[test]
fn raw_render_extreme_coordinates_preserve_storage_on_checked_overflow() {
    let pattern = bitmap(&["101", "010"]);
    for x in [i64::MIN, -3, -1, 0, 8, i64::MAX - 2, i64::MAX] {
        for y in [i64::MIN, -2, -1, 0, 2, i64::MAX - 1, i64::MAX] {
            for operator in [
                CombinationOperator::Or,
                CombinationOperator::And,
                CombinationOperator::Xor,
                CombinationOperator::Xnor,
                CombinationOperator::Replace,
            ] {
                let mut destination = bitmap(&["101010101", "010101010", "111000111"]);
                let before = destination.bytes().to_vec();
                let result = render_clipped_pattern(&mut destination, &pattern, x, y, operator);
                let overflows = i128::from(x) + 3 > i128::from(i64::MAX)
                    || i128::from(y) + 2 > i128::from(i64::MAX);
                if overflows {
                    assert!(matches!(
                        result,
                        Err(HalftoneError::Decode(DecodeError::Malformed(
                            MalformedError::IntegerOverflow
                        )))
                    ));
                    assert_eq!(destination.bytes(), before);
                } else {
                    result.unwrap();
                    let mut expected = bitmap(&["101010101", "010101010", "111000111"]);
                    scalar_render_clipped_pattern_oracle(&mut expected, &pattern, x, y, operator)
                        .unwrap();
                    assert_eq!(destination.bytes(), expected.bytes());
                }
            }
        }
    }
}

#[test]
fn generic_adapter_preserves_all_variants_and_full_width_fields() {
    let cases = [
        (
            GenericRegionError::ReservedFlags(255),
            GenericRegionErrorKind::ReservedFlags(255),
        ),
        (
            GenericRegionError::InvalidFlags(255),
            GenericRegionErrorKind::InvalidFlags(255),
        ),
        (
            GenericRegionError::InvalidAdaptivePixel {
                number: 255,
                x: i32::MIN,
                y: i32::MAX,
            },
            GenericRegionErrorKind::InvalidAdaptivePixel {
                number: 255,
                x: i32::MIN,
                y: i32::MAX,
            },
        ),
        (
            GenericRegionError::InvalidSkipDimensions,
            GenericRegionErrorKind::InvalidSkipDimensions,
        ),
        (
            GenericRegionError::InvalidArithmeticTerminator,
            GenericRegionErrorKind::InvalidArithmeticTerminator,
        ),
        (
            GenericRegionError::UnexpectedArithmeticMarker {
                offset: usize::MAX,
                code: 255,
            },
            GenericRegionErrorKind::UnexpectedArithmeticMarker {
                offset: usize::MAX,
                code: 255,
            },
        ),
        (
            GenericRegionError::RowCountMismatch {
                expected: u32::MAX,
                actual: 0,
            },
            GenericRegionErrorKind::RowCountMismatch {
                expected: u32::MAX,
                actual: 0,
            },
        ),
        (
            GenericRegionError::ContextIndex {
                index: usize::MAX,
                count: usize::MAX - 1,
            },
            GenericRegionErrorKind::ContextIndex {
                index: usize::MAX,
                count: usize::MAX - 1,
            },
        ),
    ];
    for (error, expected) in cases {
        assert_decode_error_exact(
            generic_error_into_decode(error),
            DecodeError::Malformed(MalformedError::InvalidGenericRegion(expected)),
        );
    }
    assert_decode_error_exact(
        generic_error_into_decode(GenericRegionError::Decode(DecodeError::Allocation {
            requested_bytes: u64::MAX,
        })),
        DecodeError::Allocation {
            requested_bytes: u64::MAX,
        },
    );
    assert_decode_error_exact(
        generic_error_into_decode(GenericRegionError::Decode(DecodeError::Io(
            crate::IoErrorKind::PermissionDenied,
        ))),
        DecodeError::Io(crate::IoErrorKind::PermissionDenied),
    );
}

#[test]
fn mmr_adapter_preserves_every_kind_and_delegated_error() {
    let cases = [
        (
            MmrError::InvalidMode {
                bit_offset: usize::MAX,
            },
            MmrErrorKind::InvalidMode {
                bit_offset: usize::MAX,
            },
        ),
        (
            MmrError::ExtensionMode {
                bit_offset: usize::MAX,
            },
            MmrErrorKind::ExtensionMode {
                bit_offset: usize::MAX,
            },
        ),
        (
            MmrError::InvalidRunCode {
                bit_offset: usize::MAX,
                black: true,
            },
            MmrErrorKind::InvalidRunCode {
                bit_offset: usize::MAX,
                black: true,
            },
        ),
        (
            MmrError::InvalidRunCode {
                bit_offset: 0,
                black: false,
            },
            MmrErrorKind::InvalidRunCode {
                bit_offset: 0,
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
    ];
    for (error, expected) in cases {
        assert_decode_error_exact(
            mmr_error_into_decode(error),
            DecodeError::Malformed(MalformedError::InvalidMmr(expected)),
        );
    }
    assert_decode_error_exact(
        mmr_error_into_decode(MmrError::IntegerOverflow),
        DecodeError::Malformed(MalformedError::IntegerOverflow),
    );
    assert_decode_error_exact(
        mmr_error_into_decode(MmrError::Allocation {
            requested_bytes: u64::MAX,
        }),
        DecodeError::Allocation {
            requested_bytes: u64::MAX,
        },
    );
    assert_decode_error_exact(
        mmr_error_into_decode(MmrError::Reader(MalformedError::UnexpectedEof {
            offset: u64::MAX,
            needed: u64::MAX,
            remaining: 0,
        })),
        DecodeError::Malformed(MalformedError::UnexpectedEof {
            offset: u64::MAX,
            needed: u64::MAX,
            remaining: 0,
        }),
    );
    assert_decode_error_exact(
        mmr_error_into_decode(MmrError::Bitmap(DecodeError::LimitExceeded(
            LimitExceeded {
                kind: LimitKind::Memory,
                limit: u64::MAX - 1,
                actual: u64::MAX,
            },
        ))),
        DecodeError::LimitExceeded(LimitExceeded {
            kind: LimitKind::Memory,
            limit: u64::MAX - 1,
            actual: u64::MAX,
        }),
    );
}

#[test]
fn halftone_adapter_preserves_every_variant_and_standard_forwarder() {
    let cases = [
        (
            HalftoneError::InvalidPatternDictionaryFlags(255),
            HalftoneErrorKind::InvalidPatternDictionaryFlags(255),
        ),
        (
            HalftoneError::InvalidPatternDimensions {
                width: 255,
                height: 0,
            },
            HalftoneErrorKind::InvalidPatternDimensions {
                width: 255,
                height: 0,
            },
        ),
        (
            HalftoneError::InvalidHalftoneFlags(255),
            HalftoneErrorKind::InvalidHalftoneFlags(255),
        ),
        (
            HalftoneError::InvalidHalftoneConfiguration,
            HalftoneErrorKind::InvalidConfiguration,
        ),
        (
            HalftoneError::InvalidBitmapDimensions {
                expected_width: u32::MAX,
                expected_height: 0,
                actual_width: 1,
                actual_height: u32::MAX - 1,
            },
            HalftoneErrorKind::InvalidBitmapDimensions {
                expected_width: u32::MAX,
                expected_height: 0,
                actual_width: 1,
                actual_height: u32::MAX - 1,
            },
        ),
        (
            HalftoneError::InvalidGrayBitDepth(255),
            HalftoneErrorKind::InvalidGrayBitDepth(255),
        ),
        (
            HalftoneError::EmptyPatternDictionary,
            HalftoneErrorKind::EmptyPatternDictionary,
        ),
        (
            HalftoneError::InconsistentPatternDimensions,
            HalftoneErrorKind::InconsistentPatternDimensions,
        ),
        (
            HalftoneError::InvalidPatternIndex {
                index: u32::MAX,
                pattern_count: u64::MAX,
            },
            HalftoneErrorKind::InvalidPatternIndex {
                index: u32::MAX,
                pattern_count: u64::MAX,
            },
        ),
    ];
    for (error, expected) in cases {
        assert_decode_error_exact(
            DecodeError::from(error),
            DecodeError::Malformed(MalformedError::InvalidHalftone(expected)),
        );
    }
    assert_decode_error_exact(
        halftone_error_into_decode(HalftoneError::from(MalformedError::IntegerOverflow)),
        DecodeError::Malformed(MalformedError::IntegerOverflow),
    );
    assert_decode_error_exact(
        halftone_error_into_decode(HalftoneError::from(LimitExceeded {
            kind: LimitKind::Work,
            limit: u64::MAX - 1,
            actual: u64::MAX,
        })),
        DecodeError::LimitExceeded(LimitExceeded {
            kind: LimitKind::Work,
            limit: u64::MAX - 1,
            actual: u64::MAX,
        }),
    );
}

#[test]
fn arithmetic_state_constructors_enforce_exact_input_and_context_budgets() {
    let coded = [0, 0, 0xff, 0xac];
    for (code, template, bits) in [
        (0, GenericTemplate::Template0, 16),
        (1, GenericTemplate::Template1, 13),
        (2, GenericTemplate::Template2, 10),
        (3, GenericTemplate::Template3, 10),
    ] {
        let bytes = 2_u64 << bits;
        let limits = Limits {
            max_stream_bytes: 4,
            max_decoder_managed_memory_bytes: bytes,
            ..Limits::default()
        };
        for pattern in [false, true] {
            let create = |data, limits: &Limits| {
                if pattern {
                    new_pattern_arithmetic_state(data, template, code, limits)
                } else {
                    new_halftone_arithmetic_state(data, template, code, limits)
                }
            };
            assert!(create(&coded, &limits).is_ok());
            for data in [&[][..], &[0][..], &[0, 0][..], &[0xff, 0x90][..]] {
                let Err(error) = create(data, &limits) else {
                    panic!("missing terminator accepted")
                };
                assert_decode_error_exact(
                    error,
                    DecodeError::Malformed(MalformedError::InvalidGenericRegion(
                        GenericRegionErrorKind::InvalidArithmeticTerminator,
                    )),
                );
            }
            let Err(error) = create(
                &coded,
                &Limits {
                    max_decoder_managed_memory_bytes: bytes - 1,
                    ..limits
                },
            ) else {
                panic!("context budget exceeded")
            };
            assert_decode_error_exact(
                error,
                DecodeError::LimitExceeded(LimitExceeded {
                    kind: LimitKind::Memory,
                    limit: bytes - 1,
                    actual: bytes,
                }),
            );
            let Err(error) = create(
                &coded,
                &Limits {
                    max_stream_bytes: 3,
                    max_decoder_managed_memory_bytes: 0,
                    ..limits
                },
            ) else {
                panic!("input budget exceeded")
            };
            assert_decode_error_exact(
                error,
                DecodeError::LimitExceeded(LimitExceeded {
                    kind: LimitKind::StreamBytes,
                    limit: 3,
                    actual: 4,
                }),
            );
        }
    }
}

#[test]
fn arithmetic_state_rejects_mode_and_template_changes_without_consumption() {
    let coded = [0, 0, 0xff, 0xac];
    for (code, template) in [
        (0, GenericTemplate::Template0),
        (1, GenericTemplate::Template1),
        (2, GenericTemplate::Template2),
        (3, GenericTemplate::Template3),
    ] {
        for pattern in [false, true] {
            let mut state = if pattern {
                new_pattern_arithmetic_state(&coded, template, code, &Limits::default()).unwrap()
            } else {
                new_halftone_arithmetic_state(&coded, template, code, &Limits::default()).unwrap()
            };
            for (coding, reason) in [
                (
                    GenericRegionCoding::Mmr {
                        require_eofb: false,
                    },
                    if pattern {
                        "pattern dictionary coding mode changed"
                    } else {
                        "halftone coding mode changed"
                    },
                ),
                (
                    GenericRegionCoding::Arithmetic {
                        template: (code + 1) % 4,
                        adaptive_pixels: [(0, 0); 4],
                    },
                    if pattern {
                        "pattern dictionary template changed"
                    } else {
                        "halftone template changed"
                    },
                ),
            ] {
                let Err(error) = state.decode_generic_region(empty_request(coding)) else {
                    panic!("changed coding accepted")
                };
                assert_decode_error_exact(
                    error,
                    DecodeError::Malformed(MalformedError::InvalidPageState(reason)),
                );
                let GenericRegionState::Arithmetic { session, .. } = &state else {
                    panic!("state variant changed")
                };
                assert_eq!(session.decoder.byte_position(), 1);
                assert_eq!(session.decoder.marker(), None);
                assert_eq!(session.work_used, 0);
            }
            let image = state
                .decode_generic_region(empty_request(GenericRegionCoding::Arithmetic {
                    template: code,
                    adaptive_pixels: gray_adaptive_pixels(code),
                }))
                .expect("zero-area helper bitmap");
            assert_eq!((image.width(), image.height(), image.data.len()), (0, 0, 0));
            let Err(error) = state.decode_generic_region(GenericRegionRequest {
                width: 1,
                height: 1,
                coding: GenericRegionCoding::Arithmetic {
                    template: code,
                    adaptive_pixels: [(0, 0); 4],
                },
                ..empty_request(GenericRegionCoding::Arithmetic {
                    template: code,
                    adaptive_pixels: [(0, 0); 4],
                })
            }) else {
                panic!("noncausal adaptive pixel accepted")
            };
            assert_decode_error_exact(
                error,
                DecodeError::Malformed(MalformedError::InvalidGenericRegion(
                    GenericRegionErrorKind::InvalidAdaptivePixel {
                        number: 1,
                        x: 0,
                        y: 0,
                    },
                )),
            );
            state.finish().expect("no wrong marker was observed");
        }
    }
}

#[test]
fn pattern_mmr_is_single_use_and_mode_errors_do_not_consume_it() {
    let limits = Limits::default();
    let correct = empty_request(GenericRegionCoding::Mmr {
        require_eofb: false,
    });
    let mut state = new_pattern_mmr_state(&[], &limits);
    let Err(error) = state.decode_generic_region(empty_request(GenericRegionCoding::Mmr {
        require_eofb: true,
    })) else {
        panic!("changed mode accepted")
    };
    assert_decode_error_exact(
        error,
        DecodeError::Malformed(MalformedError::InvalidPageState(
            "pattern dictionary coding mode changed",
        )),
    );
    let image = state
        .decode_generic_region(correct)
        .expect("empty helper bitmap");
    assert_eq!((image.width(), image.height(), image.data.len()), (0, 0, 0));
    let Err(error) = state.decode_generic_region(correct) else {
        panic!("reused MMR bitmap accepted")
    };
    assert_decode_error_exact(
        error,
        DecodeError::Malformed(MalformedError::InvalidPageState(
            "pattern dictionary requested multiple MMR bitmaps",
        )),
    );
    state.finish().unwrap();
    let mut state = new_pattern_mmr_state(&[], &limits);
    let Err(error) = state.decode_generic_region(GenericRegionRequest {
        width: 1,
        height: 1,
        ..correct
    }) else {
        panic!("missing mode accepted")
    };
    assert_decode_error_exact(
        error,
        DecodeError::Malformed(MalformedError::UnexpectedEof {
            offset: 0,
            needed: 1,
            remaining: 0,
        }),
    );
    let Err(error) = state.decode_generic_region(correct) else {
        panic!("failed state reused")
    };
    assert_decode_error_exact(
        error,
        DecodeError::Malformed(MalformedError::InvalidPageState(
            "pattern dictionary requested multiple MMR bitmaps",
        )),
    );
}

#[test]
fn gray_mmr_consumes_exact_prefixes_and_requires_all_declared_bytes() {
    let coded = [0, 0x10, 1, 0, 0x10, 1];
    let mut state = new_gray_mmr_state(&coded, &Limits::default());
    assert_decode_error_exact(
        state.finish().unwrap_err(),
        DecodeError::Malformed(MalformedError::TrailingData(6)),
    );
    let Err(error) = state.decode_generic_region(empty_request(GenericRegionCoding::Mmr {
        require_eofb: false,
    })) else {
        panic!("optional EOFB accepted for gray plane")
    };
    assert_decode_error_exact(
        error,
        DecodeError::Malformed(MalformedError::InvalidPageState(
            "halftone coding mode changed",
        )),
    );
    let request = empty_request(GenericRegionCoding::Mmr { require_eofb: true });
    for expected_offset in [3, 6] {
        let bitmap = state
            .decode_generic_region(request)
            .expect("exact EOFB prefix");
        assert_eq!((bitmap.width(), bitmap.height()), (0, 0));
        let GenericRegionState::GrayMmr { offset, .. } = &state else {
            panic!("wrong state")
        };
        assert_eq!(*offset, expected_offset);
        if expected_offset == 3 {
            assert_decode_error_exact(
                state.finish().unwrap_err(),
                DecodeError::Malformed(MalformedError::TrailingData(3)),
            );
        }
    }
    state.finish().unwrap();
    let Err(error) = state.decode_generic_region(request) else {
        panic!("missing EOFB accepted")
    };
    assert_decode_error_exact(
        error,
        DecodeError::Malformed(MalformedError::InvalidMmr(MmrErrorKind::MissingEofb)),
    );
    let GenericRegionState::GrayMmr { offset, .. } = state else {
        panic!("wrong state")
    };
    assert_eq!(offset, 6);
}

#[test]
#[allow(clippy::too_many_lines)]
fn header_flags_fields_and_every_truncation_keep_exact_cursor_and_errors() {
    for flags in 0..=u8::MAX {
        let pattern = [flags, 1, 2, 0xff, 0xff, 0xff, 0xff];
        let mut reader = ByteReader::new(&pattern);
        if flags >= 8 || flags & 1 != 0 && flags & 6 != 0 {
            assert!(matches!(parse_pattern_dictionary_header(&mut reader),
                Err(HalftoneError::InvalidPatternDictionaryFlags(actual)) if actual == flags));
            assert_eq!(reader.position(), 1);
        } else {
            let result = parse_pattern_dictionary_header(&mut reader).unwrap();
            assert_eq!(
                (
                    result.mmr,
                    result.template,
                    result.pattern_width,
                    result.pattern_height
                ),
                (flags & 1 != 0, flags >> 1, 1, 2)
            );
            assert_eq!(result.pattern_count(), u64::from(u32::MAX) + 1);
            assert_eq!(reader.position(), 7);
        }
        let mut bytes = vec![flags];
        for value in [u32::MAX, 0, 0x8000_0000, 0x7fff_ffff] {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        bytes.extend_from_slice(&[0xff, 0xff, 0, 0]);
        let mut reader = ByteReader::new(&bytes);
        let result = parse_halftone_header(&mut reader);
        let operator = (flags >> 4) & 7;
        if flags & 1 != 0 && flags & 14 != 0 {
            assert!(
                matches!(result, Err(HalftoneError::InvalidHalftoneFlags(actual)) if actual == flags)
            );
            assert_eq!(reader.position(), 1);
        } else if operator > 4 {
            assert!(
                matches!(result, Err(HalftoneError::Decode(DecodeError::Malformed(
                MalformedError::InvalidCombinationOperator(actual)))) if actual == operator)
            );
            assert_eq!(reader.position(), 1);
        } else {
            let result = result.unwrap();
            assert_eq!(
                (
                    result.grid_width,
                    result.grid_height,
                    result.grid_x,
                    result.grid_y
                ),
                (u32::MAX, 0, i32::MIN, i32::MAX)
            );
            assert_eq!((result.grid_vector_x, result.grid_vector_y), (u16::MAX, 0));
            assert_eq!(
                result.operator,
                [
                    CombinationOperator::Or,
                    CombinationOperator::And,
                    CombinationOperator::Xor,
                    CombinationOperator::Xnor,
                    CombinationOperator::Replace
                ][usize::from(operator)]
            );
            assert_eq!(
                (
                    result.mmr,
                    result.template,
                    result.enable_skip,
                    result.default_pixel
                ),
                (
                    flags & 1 != 0,
                    (flags >> 1) & 3,
                    flags & 8 != 0,
                    flags & 0x80 != 0
                )
            );
            assert_eq!(reader.position(), 21);
        }
    }
    for (bytes, lengths) in [
        (vec![0, 1, 2, 0, 0, 0, 0], vec![1, 1, 1, 4]),
        (vec![0; 21], vec![1, 4, 4, 4, 4, 2, 2]),
    ] {
        for cut in 0..bytes.len() {
            let mut offset = 0;
            let needed = *lengths
                .iter()
                .find(|&&length| {
                    if offset + length > cut {
                        true
                    } else {
                        offset += length;
                        false
                    }
                })
                .unwrap();
            let mut reader = ByteReader::new(&bytes[..cut]);
            let error = if bytes.len() == 7 {
                parse_pattern_dictionary_header(&mut reader).unwrap_err()
            } else {
                parse_halftone_header(&mut reader).unwrap_err()
            };
            assert!(
                matches!(error, HalftoneError::Decode(DecodeError::Malformed(
                MalformedError::UnexpectedEof { offset: actual, needed: actual_needed, remaining }
            )) if actual == offset as u64 && actual_needed == needed as u64 && remaining == (cut - offset) as u64)
            );
            assert_eq!(reader.position(), offset);
        }
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

#[test]
fn parses_pattern_header_and_rejects_invalid_flags_and_sizes() {
    let mut reader = ByteReader::new(&[0, 3, 2, 0, 0, 0, 1]);
    let header = parse_pattern_dictionary_header(&mut reader).expect("valid header should parse");
    assert_eq!(header.pattern_width, 3);
    assert_eq!(header.pattern_height, 2);
    assert_eq!(header.pattern_count(), 2);
    assert!(matches!(
        parse_pattern_dictionary_header(&mut ByteReader::new(&[3, 1, 1, 0, 0, 0, 0])),
        Err(HalftoneError::InvalidPatternDictionaryFlags(3))
    ));
    assert!(matches!(
        parse_pattern_dictionary_header(&mut ByteReader::new(&[0, 0, 1, 0, 0, 0, 0])),
        Err(HalftoneError::InvalidPatternDimensions {
            width: 0,
            height: 1
        })
    ));
}

#[test]
fn parses_halftone_header_and_signed_offsets() {
    let bytes = [
        0xca, 0, 0, 0, 2, 0, 0, 0, 3, 0xff, 0xff, 0xff, 0xff, 0, 0, 1, 0, 3, 0, 1, 0,
    ];
    let header = parse_halftone_header(&mut ByteReader::new(&bytes))
        .expect("valid halftone header should parse");
    assert!(!header.mmr);
    assert_eq!(header.template, 1);
    assert!(header.enable_skip);
    assert_eq!(header.operator, CombinationOperator::Replace);
    assert!(header.default_pixel);
    assert_eq!(header.grid_x, -1);
    assert!(matches!(
        parse_halftone_header(&mut ByteReader::new(&[
            0x39, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
        ])),
        Err(HalftoneError::InvalidHalftoneFlags(0x39))
    ));
    assert!(matches!(
        parse_halftone_header(&mut ByteReader::new(&[
            0x50, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
        ])),
        Err(HalftoneError::Decode(DecodeError::Malformed(
            MalformedError::InvalidCombinationOperator(5)
        )))
    ));
}

#[test]
fn slices_collective_bitmap_without_row_aliasing() {
    let collective = bitmap(&["101011", "010110"]);
    let header = PatternDictionaryHeader {
        mmr: false,
        template: 0,
        pattern_width: 3,
        pattern_height: 2,
        gray_max: 1,
    };
    let mut work = WorkBudget::new(&generous_limits());
    let patterns = slice_collective_bitmap(&collective, header, &generous_limits(), &mut work)
        .expect("collective bitmap should slice");
    assert_eq!(patterns, [bitmap(&["101", "010"]), bitmap(&["011", "110"])]);
}

#[test]
fn pattern_decode_supplies_normative_generic_parameters() {
    let header = PatternDictionaryHeader {
        mmr: false,
        template: 0,
        pattern_width: 3,
        pattern_height: 2,
        gray_max: 1,
    };
    let mut calls = 0;
    let mut adapter = |request: GenericRegionRequest<'_>| {
        calls += 1;
        assert_eq!((request.width, request.height), (6, 2));
        assert!(!request.typical_prediction);
        assert!(!request.extended_template);
        assert!(request.skip.is_none());
        assert_eq!(
            request.coding,
            GenericRegionCoding::Arithmetic {
                template: 0,
                adaptive_pixels: [(-3, 0), (-3, -1), (2, -2), (-2, -2)]
            }
        );
        Ok(bitmap(&["101011", "010110"]))
    };
    let mut decoder = GenericRegionState::scripted(&mut adapter);
    let limits = generous_limits();
    let mut work = WorkBudget::new(&limits);
    let patterns = decode_pattern_dictionary(header, &mut decoder, &limits, &mut work)
        .expect("pattern dictionary should decode");
    assert_eq!(calls, 1);
    assert_eq!(patterns.len(), 2);
}

#[test]
fn bytewise_gray_xor_matches_scalar_pixels_randomized() {
    let mut state = 0x6a18_ef03_91c5_d247;
    for width in [1, 2, 7, 8, 9, 15, 31, 32, 33, 63, 127] {
        for height in [1, 2, 5, 17] {
            let current = random_bitmap(width, height, &mut state);
            let previous = random_bitmap(width, height, &mut state);
            let mut expected = current.clone();
            let mut actual = current;
            scalar_xor_gray_plane_oracle(&mut expected, &previous).expect("scalar gray conversion");
            xor_gray_plane_bytes(&mut actual, &previous).expect("bytewise gray conversion");
            assert_eq!(actual, expected, "dimensions {width}x{height}");
        }
    }
}

#[test]
fn fixed_gray_extraction_matches_scalar_oracle() {
    let mut state = 0x39d2_a175_64fe_08bc;
    for plane_index in [0, 1, 7, 15, 23, 31] {
        for byte in 0..=u8::MAX {
            let mut expected = vec![0_u32; 24];
            for value in &mut expected {
                *value = next_random(&mut state);
            }
            let mut actual = expected.clone();
            scalar_accumulate_gray_byte_oracle(&mut expected, 8, byte, plane_index);
            accumulate_gray_byte(&mut actual, 8, byte, plane_index).expect("fixed extraction");
            assert_eq!(actual, expected);
        }
    }
}

#[test]
#[ignore = "microbenchmark; run explicitly with --release --ignored --nocapture"]
fn benchmark_bytewise_gray_xor_against_scalar_oracle() {
    let mut state = 0x842c_017d_a639_5bef;
    let current = random_bitmap(2048, 2048, &mut state);
    let previous = random_bitmap(2048, 2048, &mut state);
    let mut bytewise_times = Vec::new();
    let mut scalar_times = Vec::new();
    for sample in 0..11 {
        let mut bytewise = current.clone();
        let mut scalar = current.clone();
        if sample & 1 == 0 {
            let start = Instant::now();
            xor_gray_plane_bytes(&mut bytewise, &previous).expect("bytewise xor");
            bytewise_times.push(start.elapsed());
            black_box(bytewise.bytes());

            let start = Instant::now();
            scalar_xor_gray_plane_oracle(&mut scalar, &previous).expect("scalar xor");
            scalar_times.push(start.elapsed());
            black_box(scalar.bytes());
        } else {
            let start = Instant::now();
            scalar_xor_gray_plane_oracle(&mut scalar, &previous).expect("scalar xor");
            scalar_times.push(start.elapsed());
            black_box(scalar.bytes());

            let start = Instant::now();
            xor_gray_plane_bytes(&mut bytewise, &previous).expect("bytewise xor");
            bytewise_times.push(start.elapsed());
            black_box(bytewise.bytes());
        }
        assert_eq!(bytewise, scalar);
    }
    let bytewise = median_duration(bytewise_times);
    let scalar = median_duration(scalar_times);
    println!(
        "gray-xor scalar_median_ns={} bytewise_median_ns={} speedup={:.3}",
        scalar.as_nanos(),
        bytewise.as_nanos(),
        scalar.as_secs_f64() / bytewise.as_secs_f64(),
    );
}

#[test]
#[ignore = "microbenchmark; run explicitly with --release --ignored --nocapture"]
fn benchmark_fixed_gray_extraction_against_scalar_oracle() {
    let mut state = 0x19b4_7c2e_d560_83af;
    let mut bytes = vec![0_u8; 262_144];
    for byte in &mut bytes {
        *byte = u8::try_from(next_random(&mut state) & 0xff).expect("bounded byte");
    }
    let original = vec![0_u32; bytes.len() * 8];
    let mut fixed_times = Vec::new();
    let mut scalar_times = Vec::new();
    for sample in 0..11 {
        let mut fixed = original.clone();
        let mut scalar = original.clone();
        let run_fixed = |values: &mut [u32]| {
            let start = Instant::now();
            for (index, byte) in bytes.iter().copied().enumerate() {
                accumulate_gray_byte(values, index * 8, byte, 13).expect("fixed extraction");
            }
            let elapsed = start.elapsed();
            black_box(values);
            elapsed
        };
        let run_scalar = |values: &mut [u32]| {
            let start = Instant::now();
            for (index, byte) in bytes.iter().copied().enumerate() {
                scalar_accumulate_gray_byte_oracle(values, index * 8, byte, 13);
            }
            let elapsed = start.elapsed();
            black_box(values);
            elapsed
        };
        if sample & 1 == 0 {
            fixed_times.push(run_fixed(&mut fixed));
            scalar_times.push(run_scalar(&mut scalar));
        } else {
            scalar_times.push(run_scalar(&mut scalar));
            fixed_times.push(run_fixed(&mut fixed));
        }
        assert_eq!(fixed, scalar);
    }
    let fixed = median_duration(fixed_times);
    let scalar = median_duration(scalar_times);
    println!(
        "gray-extraction scalar_median_ns={} fixed_median_ns={} speedup={:.3}",
        scalar.as_nanos(),
        fixed.as_nanos(),
        scalar.as_secs_f64() / fixed.as_secs_f64(),
    );
}

#[test]
fn reconstructs_gray_planes_most_significant_first() {
    let mut planes = VecDeque::from([bitmap(&["1110"]), bitmap(&["0111"]), bitmap(&["0010"])]);
    let mut adapter = |request: GenericRegionRequest<'_>| {
        assert!(matches!(
            request.coding,
            GenericRegionCoding::Arithmetic { template: 0, .. }
        ));
        assert!(!request.typical_prediction);
        assert!(!request.extended_template);
        Ok(planes.pop_front().expect("three planes are supplied"))
    };
    let mut decoder = GenericRegionState::scripted(&mut adapter);
    let mut work = WorkBudget::new(&generous_limits());
    let values = decode_gray_planes(
        4,
        1,
        3,
        false,
        0,
        None,
        &mut decoder,
        &generous_limits(),
        &mut work,
    )
    .expect("planes should reconstruct");
    assert_eq!(values, [7, 4, 5, 3]);
    assert!(planes.is_empty());
}

#[test]
fn mmr_gray_plane_requires_eofb() {
    let mut calls = 0;
    let mut adapter = |request: GenericRegionRequest<'_>| {
        calls += 1;
        assert_eq!(
            request.coding,
            GenericRegionCoding::Mmr { require_eofb: true }
        );
        Ok(bitmap(&["0"]))
    };
    let mut decoder = GenericRegionState::scripted(&mut adapter);
    let limits = generous_limits();
    let mut work = WorkBudget::new(&limits);
    let values = decode_gray_planes(1, 1, 1, true, 0, None, &mut decoder, &limits, &mut work)
        .expect("MMR gray plane should decode");
    assert_eq!(calls, 1);
    assert_eq!(values, [0]);
}

#[test]
fn one_pattern_uses_no_gray_plane() {
    let mut calls = 0;
    let mut adapter = |_request: GenericRegionRequest<'_>| {
        calls += 1;
        Bitmap::blank(1, 1, &generous_limits())
    };
    let mut decoder = GenericRegionState::scripted(&mut adapter);
    let mut work = WorkBudget::new(&generous_limits());
    let values = decode_gray_planes(
        2,
        2,
        gray_bit_depth(1).expect("one pattern has a depth"),
        false,
        0,
        None,
        &mut decoder,
        &generous_limits(),
        &mut work,
    )
    .expect("zero-plane image should decode");
    assert_eq!(values, [0, 0, 0, 0]);
    assert_eq!(calls, 0);
}

#[test]
fn gray_depth_is_exact_at_every_power_of_two() {
    assert!(gray_bit_depth(0).is_err());
    assert_eq!(gray_bit_depth(1).expect("one pattern is valid"), 0);
    for depth in 1_u8..=32 {
        let power = 1_u64 << depth;
        assert_eq!(
            gray_bit_depth(power).expect("power-of-two count is valid"),
            depth
        );
        if depth < 32 {
            assert_eq!(
                gray_bit_depth(power + 1).expect("next count is valid"),
                depth + 1
            );
        }
    }
    assert!(gray_bit_depth((1_u64 << 32) + 1).is_err());
}

#[test]
fn grid_math_uses_signed_arithmetic_shift() {
    let header = HalftoneHeader {
        mmr: false,
        template: 0,
        enable_skip: false,
        operator: CombinationOperator::Or,
        default_pixel: false,
        grid_width: 1,
        grid_height: 1,
        grid_x: -1,
        grid_y: 512,
        grid_vector_x: 768,
        grid_vector_y: 256,
    };
    assert_eq!(
        grid_position(&header, 0, 0).expect("position fits"),
        (-1, 2)
    );
    assert_eq!(grid_position(&header, 2, 3).expect("position fits"), (8, 9));
}

#[test]
fn grid_math_extremes_fit_signed_intermediates() {
    let header = HalftoneHeader {
        mmr: false,
        template: 0,
        enable_skip: false,
        operator: CombinationOperator::Or,
        default_pixel: false,
        grid_width: 1,
        grid_height: 1,
        grid_x: i32::MIN,
        grid_y: i32::MAX,
        grid_vector_x: u16::MAX,
        grid_vector_y: u16::MAX,
    };
    let grid_x = u32::MAX;
    let grid_y = u32::MAX;
    let x_fixed = i128::from(header.grid_x)
        + i128::from(grid_y) * i128::from(header.grid_vector_y)
        + i128::from(grid_x) * i128::from(header.grid_vector_x);
    let y_fixed = i128::from(header.grid_y) + i128::from(grid_y) * i128::from(header.grid_vector_x)
        - i128::from(grid_x) * i128::from(header.grid_vector_y);
    assert_eq!(
        grid_position(&header, grid_x, grid_y).expect("all grid products fit"),
        (
            i64::try_from(x_fixed >> 8).expect("x position fits"),
            i64::try_from(y_fixed >> 8).expect("y position fits")
        )
    );
}

#[test]
fn skip_mask_obeys_half_open_boundary_rules() {
    let header = HalftoneHeader {
        mmr: false,
        template: 0,
        enable_skip: true,
        operator: CombinationOperator::Or,
        default_pixel: false,
        grid_width: 4,
        grid_height: 1,
        grid_x: -3 * 256,
        grid_y: 0,
        grid_vector_x: 256,
        grid_vector_y: 0,
    };
    let mut work = WorkBudget::new(&generous_limits());
    let skip = generate_skip_mask(&header, 10, 8, 3, 2, &generous_limits(), &mut work)
        .expect("skip mask should fit");
    assert_eq!(
        (0..4).map(|x| skip.pixel(x, 0)).collect::<Vec<_>>(),
        [Some(true), Some(false), Some(false), Some(false)]
    );
}

#[test]
fn clipped_replace_rendering_and_order_are_deterministic() {
    let limits = generous_limits();
    let patterns = [bitmap(&["101", "011"]), bitmap(&["010", "110"])];
    let header = HalftoneHeader {
        mmr: false,
        template: 0,
        enable_skip: false,
        operator: CombinationOperator::Replace,
        default_pixel: false,
        grid_width: 2,
        grid_height: 1,
        grid_x: -256,
        grid_y: 0,
        grid_vector_x: 512,
        grid_vector_y: 0,
    };
    let mut planes = VecDeque::from([bitmap(&["01"])]);
    let mut adapter =
        |_request: GenericRegionRequest<'_>| Ok(planes.pop_front().expect("one plane is supplied"));
    let mut decoder = GenericRegionState::scripted(&mut adapter);
    let mut work = WorkBudget::new(&limits);
    let output = decode_halftone_region(&header, 4, 2, &patterns, &mut decoder, &limits, &mut work)
        .expect("halftone should render");
    assert_eq!(output, bitmap(&["0010", "1110"]));
}

#[test]
fn clipped_pattern_rendering_exhaustively_matches_scalar_oracle() {
    let limits = generous_limits();
    let operators = [
        CombinationOperator::Or,
        CombinationOperator::And,
        CombinationOperator::Xor,
        CombinationOperator::Xnor,
        CombinationOperator::Replace,
    ];
    for pattern_width in 1..=10 {
        for destination_width in 1..=10 {
            let pattern = Bitmap::filled(pattern_width, 3, true, &limits).expect("pattern bitmap");
            let original =
                Bitmap::filled(destination_width, 4, false, &limits).expect("destination");
            for x in -12..=12 {
                for y in -4..=4 {
                    for operator in operators {
                        let mut expected = original.clone();
                        let mut actual = original.clone();
                        scalar_render_clipped_pattern_oracle(
                            &mut expected,
                            &pattern,
                            x,
                            y,
                            operator,
                        )
                        .expect("scalar render");
                        render_clipped_pattern(&mut actual, &pattern, x, y, operator)
                            .expect("chunk render");
                        assert_eq!(actual, expected);
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "microbenchmark; run explicitly with --release --ignored --nocapture"]
fn benchmark_halftone_render_route_against_6333205_scalar_oracle() {
    let limits = generous_limits();
    let pattern = Bitmap::filled(1024, 512, true, &limits).expect("pattern");
    let original = Bitmap::blank(1024, 512, &limits).expect("destination");
    let mut chunk_times = Vec::new();
    let mut scalar_times = Vec::new();
    for sample in 0..11 {
        let mut chunk = original.clone();
        let mut scalar = original.clone();
        if sample % 2 == 0 {
            let start = Instant::now();
            render_clipped_pattern(&mut chunk, &pattern, -3, -2, CombinationOperator::Xor)
                .expect("chunk render");
            chunk_times.push(start.elapsed());
            black_box(chunk.bytes());

            let start = Instant::now();
            scalar_render_clipped_pattern_oracle(
                &mut scalar,
                &pattern,
                -3,
                -2,
                CombinationOperator::Xor,
            )
            .expect("scalar render");
            scalar_times.push(start.elapsed());
            black_box(scalar.bytes());
        } else {
            let start = Instant::now();
            scalar_render_clipped_pattern_oracle(
                &mut scalar,
                &pattern,
                -3,
                -2,
                CombinationOperator::Xor,
            )
            .expect("scalar render");
            scalar_times.push(start.elapsed());
            black_box(scalar.bytes());

            let start = Instant::now();
            render_clipped_pattern(&mut chunk, &pattern, -3, -2, CombinationOperator::Xor)
                .expect("chunk render");
            chunk_times.push(start.elapsed());
            black_box(chunk.bytes());
        }
        assert_eq!(chunk, scalar);
    }
    let chunk = median_duration(chunk_times);
    let scalar = median_duration(scalar_times);
    println!(
        "halftone scalar_6333205_median_ns={} chunk_median_ns={} speedup={:.3}",
        scalar.as_nanos(),
        chunk.as_nanos(),
        scalar.as_secs_f64() / chunk.as_secs_f64(),
    );
}

#[test]
fn rejects_unused_gray_code_and_work_overrun() {
    assert!(matches!(
        validate_pattern_indices(&[0, 1, 3], 3),
        Err(HalftoneError::InvalidPatternIndex {
            index: 3,
            pattern_count: 3
        })
    ));
    let limits = Limits {
        max_work_units: 3,
        ..generous_limits()
    };
    let mut work = WorkBudget::new(&limits);
    assert!(work.charge(3).is_ok());
    assert!(matches!(
        work.charge(1),
        Err(HalftoneError::Decode(DecodeError::LimitExceeded(
            LimitExceeded {
                kind: LimitKind::Work,
                limit: 3,
                actual: 4
            }
        )))
    ));
}

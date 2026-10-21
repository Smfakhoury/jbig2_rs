//! Deterministic regressions for the bounded structured context-fuzz program.

#[path = "support/context_scenarios.rs"]
mod scenarios;

#[test]
fn generated_shapes_match_known_pixels_through_context_reuse() {
    for width in 0..=u8::MAX {
        scenarios::exercise(&[
            width,
            width % 16,
            width % 3,
            width & 1,
            255 - width,
            15,
            2,
            1,
        ]);
    }
}

#[test]
fn every_state_operation_recovers_after_errors_and_scratch_release() {
    for operation in 0..10 {
        let mut input = vec![63, 15, 2, 1, 0, 0, 0, 0];
        for argument in [0_u16, 1, 2, 13, 24, 35, 255, u16::MAX] {
            input.push(operation);
            input.extend_from_slice(&argument.to_be_bytes());
        }
        input.extend_from_slice(b"generated invalid standalone input");
        scenarios::exercise(&input);
    }
}

#[test]
fn short_and_oversized_control_programs_remain_bounded() {
    for length in 0..=40 {
        scenarios::exercise(&vec![255; length]);
    }
    scenarios::exercise(&vec![255; 16_384]);
}

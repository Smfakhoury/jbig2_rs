#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

use super::*;

#[test]
fn eof_projection_preserves_full_width_fields() {
    let values = [
        0,
        1,
        usize::from(u16::MAX),
        usize::MAX / 2,
        usize::MAX - 1,
        usize::MAX,
    ];
    for offset in values {
        for needed in values {
            for remaining in values {
                assert_eq!(
                    unexpected_eof(offset, needed, remaining),
                    eof_numeric_error(offset, needed, remaining)
                );
            }
        }
    }
}

#[test]
fn signal_error_projection_matches_numeric_policy_and_prior_conversion_path() {
    let bytes = [0];
    for position in [0, 1, usize::MAX / 2, usize::MAX - 1, usize::MAX] {
        let mut decoder = decision_fixture(&bytes, MIN_INTERVAL, 0, 0);
        decoder.byte_position = position;
        for signal in 0..=u8::MAX {
            let expected = if signal == RASTER_SIGNAL_EOF && position < usize::MAX {
                eof_numeric_error(position + 1, 1, 0)
            } else {
                ArithmeticError::Malformed(MalformedError::IntegerOverflow)
            };
            assert_eq!(decoder.signal_error(signal), expected);
            let prior = if signal == RASTER_SIGNAL_EOF {
                match position.checked_add(1) {
                    Some(offset) => unexpected_eof(offset, 1, 0),
                    None => MalformedError::IntegerOverflow.into(),
                }
            } else {
                MalformedError::IntegerOverflow.into()
            };
            assert_eq!(decoder.signal_error(signal), prior);
        }
    }
}

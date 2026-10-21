//! Ordinary coverage and independent pixel/error oracles for the fuzz grammar.

#[path = "support/codec_scenarios.rs"]
mod codec_scenarios;

use codec_scenarios::{exercise, limits, Fixture, FAULTS, LIMITS, OPERATIONS, SHAPE_BYTES};
use jbig2_rs::Decoder;

#[test]
fn halftone_family_matches_literal_pixels_errors_limits_and_boundaries() {
    check_family(0);
    check_operations(0);
    check_shapes_and_truncations(0);
}

#[test]
fn huffman_text_family_matches_literal_pixels_errors_limits_and_boundaries() {
    check_family(1);
    check_operations(1);
    check_shapes_and_truncations(1);
}

fn check_family(family: u8) {
    for width in [1, 7, 8, 9, 15, 16, 17, 32, 33] {
        for count in 1..=4 {
            for operator in 0..5 {
                for flags in 0..20 {
                    let controls = [
                        family,
                        width - 1,
                        8,
                        8,
                        2,
                        count - 1,
                        8,
                        4,
                        operator,
                        flags,
                        flags,
                        0,
                        6,
                        flags.wrapping_mul(23),
                        flags,
                        flags,
                    ];
                    let fixture = Fixture::new(&controls);
                    fixture.check(
                        &Decoder::new(limits())
                            .decode(&fixture.document(false, None))
                            .expect("first-party codec"),
                    );
                }
            }
        }
    }
}

fn check_operations(family: u8) {
    for operation in 0..OPERATIONS {
        for argument in 0..LIMITS {
            let mut controls = vec![family, 16, 8, 8, 2, 3, 8, 4, 4, 3, 7, 0, 6, 27, 7, 7];
            for _ in 0..4 {
                controls.extend_from_slice(&[operation, argument]);
            }
            exercise(&controls);
        }
    }
    for fault in 0..FAULTS {
        Fixture::new(&[family]).check_fault(fault);
    }
}

#[test]
fn generated_codec_grammar_is_zero_padded_and_has_a_bounded_suffix() {
    exercise(&[]);
    for family in 0..2 {
        for prefix in 0..SHAPE_BYTES + 8 {
            let mut controls = [255; SHAPE_BYTES + 8];
            controls[0] = family;
            exercise(&controls[..prefix]);
        }

        let controls = vec![family; SHAPE_BYTES + 8];
        let mut extended = controls.clone();
        extended.extend_from_slice(&[0xff; 4096]);
        exercise(&extended);
        let first = Fixture::new(&controls);
        let second = Fixture::new(&extended);
        assert_eq!(first.document(false, None), second.document(false, None));
        assert_eq!(first.expected(), second.expected());
    }
}

fn check_shapes_and_truncations(family: u8) {
    for pattern_width in 1..=9 {
        for pattern_height in 1..=3 {
            for coding in 0..2 {
                let controls = [
                    family,
                    32,
                    8,
                    pattern_width - 1,
                    pattern_height - 1,
                    2,
                    8,
                    4,
                    2,
                    3,
                    1,
                    3,
                    3,
                    11,
                    coding,
                    0,
                ];
                let fixture = Fixture::new(&controls);
                let valid = fixture.document(false, None);
                let decoder = Decoder::new(limits());
                let mut context = decoder.context();
                for end in 0..valid.len() {
                    match context.decode(&valid[..end]) {
                        Ok(document) => fixture.check(&document),
                        Err(jbig2_rs::DecodeError::Malformed(_)) => {}
                        outcome => panic!("unexpected prefix outcome: {outcome:?}"),
                    }
                }
                fixture.check(&context.decode(&valid).expect("recovery after all prefixes"));
            }
        }
    }
}

#[test]
fn tiny_generated_families_have_literal_packed_oracles() {
    // The formula selects pattern zero's first black pixel. The halftone's
    // fixed origin is -1/256, so shift it right one whole pixel to land at zero.
    for family in 0..2 {
        let mut controls = [0; SHAPE_BYTES];
        controls[0] = family;
        controls[10] = 1;
        controls[11] = 4;
        controls[12] = 3;
        let fixture = Fixture::new(&controls);
        assert_eq!(fixture.expected(), [0x80]);
        fixture.check(
            &Decoder::new(limits())
                .decode(&fixture.document(false, None))
                .expect("literal one black pixel"),
        );
    }
}

#[test]
fn canonical_codec_seed_manifest_executes_all_sixty_structured_seeds() {
    let seeds = codec_scenarios::seeds();
    assert_eq!(seeds.len(), 60);
    let mut names = std::collections::BTreeSet::new();
    for (name, controls) in &seeds {
        assert!(name.starts_with("generated-codecs-v1-"));
        assert!(!name.contains('/'));
        assert!(names.insert(name));
        assert_eq!(controls.len(), SHAPE_BYTES + 8);
        exercise(controls);
    }
    assert_eq!(seeds, codec_scenarios::seeds());
}

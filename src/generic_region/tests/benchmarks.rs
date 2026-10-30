#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

use super::*;

#[test]
#[ignore = "microbenchmark; run explicitly with --release --ignored --nocapture"]
fn benchmark_specialized_generic_kernels_against_44a2b6e_oracle() {
    let limits = Limits::default();
    let width = 768;
    let height = 384;
    let coded = {
        let mut bytes = vec![0x55; 48_000];
        bytes.extend_from_slice(&ARITHMETIC_TERMINATOR);
        bytes
    };
    for template in [
        GenericTemplate::Template0,
        GenericTemplate::Template1,
        GenericTemplate::Template2,
        GenericTemplate::Template3,
    ] {
        let header = generic_header(width, height, template, false, true);
        let mut optimized_times = Vec::new();
        let mut oracle_times = Vec::new();
        for sample in 0..11 {
            let run = |optimized: bool| {
                let mut bitmap = Bitmap::blank(width, height, &limits).expect("bitmap");
                let mut decoder = ArithmeticDecoder::new(&coded).expect("decoder");
                let mut contexts = vec![ArithmeticContext::new(); generic_context_count(template)];
                let start = Instant::now();
                if optimized {
                    decode_generic_with(
                        &mut bitmap,
                        height,
                        template,
                        false,
                        &header.adaptive,
                        true,
                        None,
                        &mut decoder,
                        &mut contexts,
                    )
                    .expect("optimized decode");
                } else {
                    decode_generic_with_oracle(
                        &mut bitmap,
                        height,
                        template,
                        false,
                        &header.adaptive,
                        true,
                        None,
                        &mut decoder,
                        &mut contexts,
                    )
                    .expect("oracle decode");
                }
                let elapsed = start.elapsed();
                black_box((bitmap, contexts, decoder));
                elapsed
            };
            if sample & 1 == 0 {
                optimized_times.push(run(true));
                oracle_times.push(run(false));
            } else {
                oracle_times.push(run(false));
                optimized_times.push(run(true));
            }
        }
        let optimized = median_duration(optimized_times);
        let oracle = median_duration(oracle_times);
        println!(
            "generic_{template:?} scalar_44a2b6e_median_ns={} specialized_median_ns={} speedup={:.3}",
            oracle.as_nanos(),
            optimized.as_nanos(),
            oracle.as_secs_f64() / optimized.as_secs_f64(),
        );
    }
}

#[test]
#[ignore = "microbenchmark; run explicitly with --release --ignored --nocapture"]
fn benchmark_specialized_refinement_kernels_against_44a2b6e_oracle() {
    let limits = Limits::default();
    let width = 512;
    let height = 384;
    let mut state = 0x914a_d27c_650e_b83f;
    let reference = random_bitmap(width + 5, height + 3, &mut state);
    let coded = {
        let mut bytes = vec![0x35; 32_000];
        bytes.extend_from_slice(&ARITHMETIC_TERMINATOR);
        bytes
    };
    for template in [RefinementTemplate::Template0, RefinementTemplate::Template1] {
        let adaptive = [TemplateOffset::new(-1, -1), TemplateOffset::new(-1, -1)];
        let parameters = RefinementParameters {
            reference_dx: -2,
            reference_dy: 1,
        };
        let mut optimized_times = Vec::new();
        let mut oracle_times = Vec::new();
        for sample in 0..11 {
            let run = |optimized: bool| {
                let mut bitmap = Bitmap::blank(width, height, &limits).expect("bitmap");
                let mut decoder = ArithmeticDecoder::new(&coded).expect("decoder");
                let mut contexts =
                    vec![ArithmeticContext::new(); refinement_context_count(template)];
                let start = Instant::now();
                if optimized {
                    decode_refinement_with(
                        &mut bitmap,
                        &reference,
                        template,
                        adaptive,
                        true,
                        parameters,
                        &mut decoder,
                        &mut contexts,
                    )
                    .expect("optimized decode");
                } else {
                    decode_refinement_with_oracle(
                        &mut bitmap,
                        &reference,
                        template,
                        adaptive,
                        true,
                        parameters,
                        &mut decoder,
                        &mut contexts,
                    )
                    .expect("oracle decode");
                }
                let elapsed = start.elapsed();
                black_box((bitmap, contexts, decoder));
                elapsed
            };
            if sample & 1 == 0 {
                optimized_times.push(run(true));
                oracle_times.push(run(false));
            } else {
                oracle_times.push(run(false));
                optimized_times.push(run(true));
            }
        }
        let optimized = median_duration(optimized_times);
        let oracle = median_duration(oracle_times);
        println!(
            "refinement_{template:?} scalar_44a2b6e_median_ns={} specialized_median_ns={} speedup={:.3}",
            oracle.as_nanos(),
            optimized.as_nanos(),
            oracle.as_secs_f64() / optimized.as_secs_f64(),
        );
    }
}

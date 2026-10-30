#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

use super::*;

#[test]
#[ignore = "microbenchmark; run explicitly with --release --ignored --nocapture"]
fn benchmark_chunk_composition_against_fa117f0_scalar_oracle() {
    let operators = [
        CombinationOperator::Or,
        CombinationOperator::And,
        CombinationOperator::Xor,
        CombinationOperator::Xnor,
        CombinationOperator::Replace,
    ];
    let source = patterned_bitmap(1024, 512, 0x1234_5678_9abc_def0);
    let original = patterned_bitmap(1032, 512, 0xfedc_ba98_7654_3210);
    let mut chunk_times = Vec::new();
    let mut scalar_times = Vec::new();
    for sample in 0..11 {
        let mut chunk = original.clone();
        let mut scalar = original.clone();
        if sample % 2 == 0 {
            let start = Instant::now();
            for x in [0, 3] {
                for operator in operators {
                    chunk
                        .compose(&source, x, 0, operator)
                        .expect("chunk composition succeeds");
                }
            }
            chunk_times.push(start.elapsed());
            black_box(chunk.bytes());

            let start = Instant::now();
            for x in [0, 3] {
                for operator in operators {
                    scalar_compose(&mut scalar, &source, x, 0, operator)
                        .expect("scalar composition succeeds");
                }
            }
            scalar_times.push(start.elapsed());
            black_box(scalar.bytes());
        } else {
            let start = Instant::now();
            for x in [0, 3] {
                for operator in operators {
                    scalar_compose(&mut scalar, &source, x, 0, operator)
                        .expect("scalar composition succeeds");
                }
            }
            scalar_times.push(start.elapsed());
            black_box(scalar.bytes());

            let start = Instant::now();
            for x in [0, 3] {
                for operator in operators {
                    chunk
                        .compose(&source, x, 0, operator)
                        .expect("chunk composition succeeds");
                }
            }
            chunk_times.push(start.elapsed());
            black_box(chunk.bytes());
        }
        assert_eq!(chunk, scalar);
    }
    let chunk = median_duration(chunk_times);
    let scalar = median_duration(scalar_times);
    println!(
        "composition scalar_fa117f0_median_ns={} chunk_median_ns={} speedup={:.3}",
        scalar.as_nanos(),
        chunk.as_nanos(),
        scalar.as_secs_f64() / chunk.as_secs_f64(),
    );
}

#[test]
#[ignore = "microbenchmark; run explicitly with --release --ignored --nocapture"]
fn benchmark_source_offset_composition_against_6333205_scalar_oracle() {
    let operators = [
        CombinationOperator::Or,
        CombinationOperator::And,
        CombinationOperator::Xor,
        CombinationOperator::Xnor,
        CombinationOperator::Replace,
    ];
    let source = patterned_bitmap(1032, 512, 0x0ddc_0ffe_e15e_beef);
    let original = patterned_bitmap(1040, 512, 0x5ca1_ab1e_0123_4567);
    let mut chunk_times = Vec::new();
    let mut scalar_times = Vec::new();
    for sample in 0..11 {
        let mut chunk = original.clone();
        let mut scalar = original.clone();
        if sample % 2 == 0 {
            let start = Instant::now();
            for operator in operators {
                chunk
                    .compose_region(&source, 3, 0, 5, 0, 1024, 512, operator)
                    .expect("chunk region composition succeeds");
            }
            chunk_times.push(start.elapsed());
            black_box(chunk.bytes());

            let start = Instant::now();
            for operator in operators {
                scalar_compose_region(&mut scalar, &source, 3, 0, 5, 0, 1024, 512, operator)
                    .expect("scalar region composition succeeds");
            }
            scalar_times.push(start.elapsed());
            black_box(scalar.bytes());
        } else {
            let start = Instant::now();
            for operator in operators {
                scalar_compose_region(&mut scalar, &source, 3, 0, 5, 0, 1024, 512, operator)
                    .expect("scalar region composition succeeds");
            }
            scalar_times.push(start.elapsed());
            black_box(scalar.bytes());

            let start = Instant::now();
            for operator in operators {
                chunk
                    .compose_region(&source, 3, 0, 5, 0, 1024, 512, operator)
                    .expect("chunk region composition succeeds");
            }
            chunk_times.push(start.elapsed());
            black_box(chunk.bytes());
        }
        assert_eq!(chunk, scalar);
    }
    let chunk = median_duration(chunk_times);
    let scalar = median_duration(scalar_times);
    println!(
        "source-offset-composition scalar_6333205_median_ns={} chunk_median_ns={} \
         speedup={:.3}",
        scalar.as_nanos(),
        chunk.as_nanos(),
        scalar.as_secs_f64() / chunk.as_secs_f64(),
    );
}

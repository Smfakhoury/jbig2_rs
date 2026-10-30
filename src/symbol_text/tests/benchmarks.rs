#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

use super::*;

#[test]
#[ignore = "microbenchmark; run explicitly with --release --ignored --nocapture"]
fn benchmark_clipped_text_route_against_6333205_scalar_oracle() {
    let limits = Limits::default();
    let source = Bitmap::filled(1024, 512, true, &limits).expect("source");
    let original = Bitmap::blank(1024, 512, &limits).expect("destination");
    let placement = Placement { x: -3, y: -2 };
    let mut chunk_times = Vec::new();
    let mut scalar_times = Vec::new();
    for sample in 0..11 {
        let mut chunk = original.clone();
        let mut scalar = original.clone();
        let mut chunk_resources = ResourceTracker::default();
        let mut scalar_resources = ResourceTracker::default();
        if sample % 2 == 0 {
            let start = Instant::now();
            compose_placement(
                &mut chunk,
                &source,
                placement,
                CombinationOperator::Or,
                &limits,
                &mut chunk_resources,
            )
            .expect("chunk placement");
            chunk_times.push(start.elapsed());
            black_box(chunk.bytes());

            let start = Instant::now();
            scalar_compose_placement_oracle(
                &mut scalar,
                &source,
                placement,
                CombinationOperator::Or,
                &limits,
                &mut scalar_resources,
            )
            .expect("scalar placement");
            scalar_times.push(start.elapsed());
            black_box(scalar.bytes());
        } else {
            let start = Instant::now();
            scalar_compose_placement_oracle(
                &mut scalar,
                &source,
                placement,
                CombinationOperator::Or,
                &limits,
                &mut scalar_resources,
            )
            .expect("scalar placement");
            scalar_times.push(start.elapsed());
            black_box(scalar.bytes());

            let start = Instant::now();
            compose_placement(
                &mut chunk,
                &source,
                placement,
                CombinationOperator::Or,
                &limits,
                &mut chunk_resources,
            )
            .expect("chunk placement");
            chunk_times.push(start.elapsed());
            black_box(chunk.bytes());
        }
        assert_eq!(chunk, scalar);
        assert_eq!(chunk_resources, scalar_resources);
    }
    let chunk = median_duration(chunk_times);
    let scalar = median_duration(scalar_times);
    println!(
        "text scalar_6333205_median_ns={} chunk_median_ns={} speedup={:.3}",
        scalar.as_nanos(),
        chunk.as_nanos(),
        scalar.as_secs_f64() / chunk.as_secs_f64(),
    );
}

#[test]
#[ignore = "performance microbenchmark"]
fn benchmark_row_aligned_text_placement_batch() {
    let limits = Limits::default();
    let source = Bitmap::filled(8, 1, true, &limits).expect("source");
    let original = Bitmap::blank(32_768, 1, &limits).expect("destination");
    let placements = (0..4_096)
        .map(|index| {
            (
                &source,
                Placement {
                    x: i64::from(index * 8),
                    y: 0,
                },
            )
        })
        .collect::<Vec<_>>();
    let mut batched_times = Vec::new();
    let mut sequential_times = Vec::new();
    for sample in 0..11 {
        let mut batched = original.clone();
        let mut sequential = original.clone();
        let mut batched_resources = ResourceTracker::default();
        let mut sequential_resources = ResourceTracker::default();
        for batched_first in [sample % 2 == 0, sample % 2 != 0] {
            let start = Instant::now();
            if batched_first {
                batched_compose_placement(
                    &mut batched,
                    &placements,
                    CombinationOperator::Or,
                    &limits,
                    &mut batched_resources,
                )
                .expect("batched placements");
                batched_times.push(start.elapsed());
                black_box(batched.bytes());
            } else {
                for &(source, placement) in &placements {
                    compose_placement(
                        &mut sequential,
                        source,
                        placement,
                        CombinationOperator::Or,
                        &limits,
                        &mut sequential_resources,
                    )
                    .expect("sequential placement");
                }
                sequential_times.push(start.elapsed());
                black_box(sequential.bytes());
            }
        }
        assert_eq!(batched, sequential);
        assert_eq!(batched_resources, sequential_resources);
    }
    let batched = median_duration(batched_times);
    let sequential = median_duration(sequential_times);
    println!(
        "text-batch sequential_median_ns={} batched_median_ns={} speedup={:.3}",
        sequential.as_nanos(),
        batched.as_nanos(),
        sequential.as_secs_f64() / batched.as_secs_f64(),
    );
}

#[test]
#[ignore = "performance microbenchmark"]
#[allow(clippy::cast_precision_loss)]
fn benchmark_reused_standard_huffman_materialization() {
    use std::hint::black_box;
    use std::time::Instant;

    fn choices() -> HuffmanFieldChoices<'static> {
        let mut choices = HuffmanFieldChoices::empty();
        choices.first_s = Some(BoundHuffmanTable::Standard(StandardHuffmanTable::A));
        choices.subsequent_s = Some(BoundHuffmanTable::Standard(StandardHuffmanTable::A));
        choices.strip_delta = Some(BoundHuffmanTable::Standard(StandardHuffmanTable::A));
        choices.refinement_width = Some(BoundHuffmanTable::Standard(StandardHuffmanTable::O));
        choices.refinement_height = Some(BoundHuffmanTable::Standard(StandardHuffmanTable::O));
        choices.refinement_x = Some(BoundHuffmanTable::Standard(StandardHuffmanTable::O));
        choices.refinement_y = Some(BoundHuffmanTable::Standard(StandardHuffmanTable::O));
        choices.refinement_size = Some(BoundHuffmanTable::Standard(StandardHuffmanTable::A));
        choices
    }

    let limits = Limits::default();
    let mut reused_samples = Vec::new();
    let mut repeated_samples = Vec::new();
    for round in 0..9 {
        for repeated in [round % 2 != 0, round % 2 == 0] {
            let start = Instant::now();
            for _ in 0..64 {
                if repeated {
                    let tables = vec![
                        Box::new(
                            HuffmanTable::standard(StandardHuffmanTable::A, &limits)
                                .expect("table A"),
                        ),
                        Box::new(
                            HuffmanTable::standard(StandardHuffmanTable::A, &limits)
                                .expect("table A"),
                        ),
                        Box::new(
                            HuffmanTable::standard(StandardHuffmanTable::A, &limits)
                                .expect("table A"),
                        ),
                        Box::new(
                            HuffmanTable::standard(StandardHuffmanTable::O, &limits)
                                .expect("table O"),
                        ),
                        Box::new(
                            HuffmanTable::standard(StandardHuffmanTable::O, &limits)
                                .expect("table O"),
                        ),
                        Box::new(
                            HuffmanTable::standard(StandardHuffmanTable::O, &limits)
                                .expect("table O"),
                        ),
                        Box::new(
                            HuffmanTable::standard(StandardHuffmanTable::O, &limits)
                                .expect("table O"),
                        ),
                        Box::new(
                            HuffmanTable::standard(StandardHuffmanTable::A, &limits)
                                .expect("table A"),
                        ),
                    ];
                    black_box(tables);
                } else {
                    let tables = (|| -> Result<_, SymbolTextError> {
                        materialize_huffman_tables!(
                            HuffmanChoiceSet::TextRegion(choices()),
                            &limits
                        )
                    })()
                    .expect("materialized tables");
                    black_box(tables);
                }
            }
            let elapsed = start.elapsed().as_nanos();
            if repeated {
                repeated_samples.push(elapsed);
            } else {
                reused_samples.push(elapsed);
            }
        }
    }
    reused_samples.sort_unstable();
    repeated_samples.sort_unstable();
    let reused = reused_samples[reused_samples.len() / 2];
    let repeated = repeated_samples[repeated_samples.len() / 2];
    let gain = (repeated - reused) as f64 * 100.0 / repeated as f64;
    eprintln!("huffman-materialization: repeated={repeated}ns reused={reused}ns gain={gain:.2}%");
}

#[test]
#[ignore = "performance microbenchmark"]
#[allow(clippy::cast_precision_loss)]
fn benchmark_borrowed_dictionary_text_codebook() {
    let limits = Limits::default();
    let mut symbols = Vec::new();
    for value in 0_u16..2_048 {
        let mut bitmap = Bitmap::blank(128, 8, &limits).expect("symbol");
        bitmap.data.fill(u8::try_from(value & 0xff).expect("byte"));
        symbols.push(bitmap);
    }
    let mut borrowed_samples = Vec::new();
    let mut cloned_samples = Vec::new();
    let mut borrowed = Vec::new();
    borrowed
        .try_reserve(symbols.len())
        .expect("borrowed codebook capacity");
    for round in 0..9 {
        for cloned in [round % 2 != 0, round % 2 == 0] {
            let start = Instant::now();
            for _ in 0..32 {
                if cloned {
                    let mut owned = Vec::new();
                    owned.try_reserve(symbols.len()).expect("owned codebook");
                    for symbol in &symbols {
                        owned.push(clone_bitmap(symbol, &limits).expect("symbol clone"));
                    }
                    black_box(owned);
                } else {
                    borrowed.clear();
                    for symbol in &symbols {
                        borrowed.push(symbol);
                    }
                    black_box(&borrowed);
                }
            }
            let elapsed = start.elapsed().as_nanos();
            if cloned {
                cloned_samples.push(elapsed);
            } else {
                borrowed_samples.push(elapsed);
            }
        }
    }
    borrowed_samples.sort_unstable();
    cloned_samples.sort_unstable();
    let borrowed = borrowed_samples[borrowed_samples.len() / 2];
    let cloned = cloned_samples[cloned_samples.len() / 2];
    let gain = (cloned - borrowed) as f64 * 100.0 / cloned as f64;
    eprintln!("symbol-codebook: cloned={cloned}ns borrowed={borrowed}ns gain={gain:.2}%");
}

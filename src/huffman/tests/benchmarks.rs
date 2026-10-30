#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

use super::*;

#[test]
#[ignore = "performance microbenchmark"]
#[allow(clippy::cast_precision_loss)]
fn benchmark_fixed_width_prefix_lookup_against_canonical() {
    use std::hint::black_box;
    use std::time::Instant;

    let lines = (0..=u8::MAX)
        .map(|value| HuffmanLine::normal(i32::from(value), 8, 0))
        .collect::<Vec<_>>();
    let table = HuffmanTable::new(&lines, &Limits::default()).expect("fixed-width table");
    let bytes = (0..=u8::MAX).cycle().take(65_536).collect::<Vec<_>>();
    let mut optimized_samples = Vec::new();
    let mut canonical_samples = Vec::new();
    for round in 0..9 {
        for canonical in [round % 2 != 0, round % 2 == 0] {
            let start = Instant::now();
            let mut reader = bit_reader(&bytes);
            for _ in 0..bytes.len() {
                let value = if canonical {
                    table.decode_canonical(&mut reader)
                } else {
                    table.decode(&mut reader)
                }
                .expect("complete fixed-width value");
                black_box(value);
            }
            let elapsed = start.elapsed().as_nanos();
            if canonical {
                canonical_samples.push(elapsed);
            } else {
                optimized_samples.push(elapsed);
            }
        }
    }
    optimized_samples.sort_unstable();
    canonical_samples.sort_unstable();
    let optimized = optimized_samples[optimized_samples.len() / 2];
    let canonical = canonical_samples[canonical_samples.len() / 2];
    let gain = (canonical - optimized) as f64 * 100.0 / canonical as f64;
    eprintln!("huffman-prefix: canonical={canonical}ns optimized={optimized}ns gain={gain:.2}%");
}

#[test]
#[ignore = "performance microbenchmark"]
#[allow(clippy::cast_precision_loss)]
fn benchmark_symbol_id_source_without_line_staging() {
    use std::hint::black_box;
    use std::time::Instant;

    let lengths = vec![12_u8; 4_096];
    let limits = Limits::default();
    let mut specialized_samples = Vec::new();
    let mut staged_samples = Vec::new();
    for round in 0..9 {
        for staged in [round % 2 != 0, round % 2 == 0] {
            let start = Instant::now();
            for _ in 0..32 {
                if staged {
                    let lines = lengths
                        .iter()
                        .copied()
                        .enumerate()
                        .map(|(index, length)| {
                            HuffmanLine::normal(
                                i32::try_from(index).expect("small index"),
                                length,
                                0,
                            )
                        })
                        .collect::<Vec<_>>();
                    black_box(HuffmanTable::new(&lines, &limits).expect("staged table"));
                } else {
                    black_box(
                        HuffmanTable::new_symbol_ids(&lengths, &limits).expect("specialized table"),
                    );
                }
            }
            let elapsed = start.elapsed().as_nanos();
            if staged {
                staged_samples.push(elapsed);
            } else {
                specialized_samples.push(elapsed);
            }
        }
    }
    specialized_samples.sort_unstable();
    staged_samples.sort_unstable();
    let specialized = specialized_samples[specialized_samples.len() / 2];
    let staged = staged_samples[staged_samples.len() / 2];
    let gain = (staged - specialized) as f64 * 100.0 / staged as f64;
    eprintln!("symbol-id-table: staged={staged}ns specialized={specialized}ns gain={gain:.2}%");
}

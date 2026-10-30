#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

use super::*;

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

//! Small shared timing/statistics helpers; no decoder-output validation is timed.

use std::time::Instant;

pub fn percentile_index(length: usize, percentile: usize) -> usize {
    assert!(length > 0 && (1..=100).contains(&percentile));
    (length * percentile).div_ceil(100) - 1
}

pub fn sample<F, T, V>(
    iterations: usize,
    mut operation: F,
    mut validate: V,
) -> Result<Vec<u128>, String>
where
    F: FnMut() -> T,
    V: FnMut(&T) -> Result<(), String>,
{
    assert!(iterations > 0);
    for _ in 0..3 {
        let output = operation();
        validate(&output)?;
        std::hint::black_box(output);
    }
    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        let output = operation();
        let elapsed = start.elapsed().as_nanos();
        validate(&output)?;
        std::hint::black_box(output);
        samples.push(elapsed);
    }
    Ok(samples)
}

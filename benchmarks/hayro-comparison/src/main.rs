use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use hayro_jbig2::{Decoder as HayroDecoder, Image};
use jbig2_rs::Decoder;
use serde::Serialize;

const SIGNATURE: [u8; 8] = [0x97, 0x4A, 0x42, 0x32, 0x0D, 0x0A, 0x1A, 0x0A];

#[derive(Serialize)]
struct BitmapResult {
    width: u32,
    height: u32,
    bytes: usize,
    hash: u64,
}

fn parse_ours(input: &[u8]) -> Result<u64, String> {
    let info = Decoder::default()
        .inspect(input)
        .map_err(|error| format!("error:{error}"))?;
    Ok(u64::from(info.segment_count()) << 32 | info.pages().len() as u64)
}

fn parse_hayro(input: &[u8]) -> Result<u64, String> {
    let image = Image::new(input).map_err(|error| format!("error:{error}"))?;
    Ok(u64::from(image.width()) << 32 | u64::from(image.height()))
}

#[derive(Serialize)]
struct DecoderResult {
    status: String,
    bitmap: Option<BitmapResult>,
}

#[derive(Serialize)]
struct VectorResult {
    name: String,
    input_bytes: usize,
    jbig2_rs: DecoderResult,
    hayro_jbig2: DecoderResult,
    exact_match: bool,
}

#[derive(Serialize)]
struct TimingResult {
    width: u32,
    height: u32,
    input_bytes: usize,
    output_bytes: usize,
    iterations: usize,
    jbig2_rs_parse_median_ns: u128,
    jbig2_rs_parse_p95_ns: u128,
    jbig2_rs_median_ns: u128,
    jbig2_rs_p95_ns: u128,
    hayro_jbig2_parse_median_ns: u128,
    hayro_jbig2_parse_p95_ns: u128,
    hayro_jbig2_median_ns: u128,
    hayro_jbig2_p95_ns: u128,
    hayro_over_jbig2_rs: f64,
}

struct TimingStats {
    median_ns: u128,
    p95_ns: u128,
}

struct PackedCollector {
    width: u32,
    height: u32,
    stride: usize,
    row: u32,
    x: u32,
    data: Vec<u8>,
}

impl PackedCollector {
    fn new(width: u32, height: u32) -> Result<Self, String> {
        let stride = usize::try_from(width.div_ceil(8)).map_err(|_| "stride overflow")?;
        let length = stride
            .checked_mul(usize::try_from(height).map_err(|_| "height overflow")?)
            .ok_or("bitmap allocation overflow")?;
        Ok(Self {
            width,
            height,
            stride,
            row: 0,
            x: 0,
            data: vec![0; length],
        })
    }

    fn finish(self) -> Result<BitmapResult, String> {
        if self.row != self.height {
            return Err(format!(
                "decoder emitted {} rows, expected {}",
                self.row, self.height
            ));
        }
        Ok(bitmap_result(
            self.width,
            self.height,
            self.stride,
            &self.data,
        ))
    }
}

impl HayroDecoder for PackedCollector {
    fn push_pixel(&mut self, black: bool) {
        if black && self.row < self.height && self.x < self.width {
            let index = self.row as usize * self.stride + self.x as usize / 8;
            self.data[index] |= 0x80 >> (self.x % 8);
        }
        self.x += 1;
    }

    fn push_pixel_chunk(&mut self, black: bool, chunk_count: u32) {
        let byte = if black { 0xff } else { 0 };
        for _ in 0..chunk_count {
            if self.row < self.height && self.x + 8 <= self.width {
                let index = self.row as usize * self.stride + self.x as usize / 8;
                self.data[index] = byte;
            }
            self.x += 8;
        }
    }

    fn next_line(&mut self) {
        self.row += 1;
        self.x = 0;
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    let output = match arguments.as_slice() {
        [mode] if mode == "generated" => serde_json::to_value(run_generated_benchmarks(50)?)?,
        [mode, iterations] if mode == "generated" => {
            serde_json::to_value(run_generated_benchmarks(iterations.parse()?)?)?
        }
        [mode, directory] if mode == "vectors" => {
            serde_json::to_value(run_vectors(Path::new(directory))?)?
        }
        [mode, engine, width, height, iterations] if mode == "single" => {
            let width = width.parse()?;
            let height = height.parse()?;
            let iterations = iterations.parse()?;
            serde_json::to_value(run_single(engine, width, height, iterations)?)?
        }
        [mode, engine, path, iterations] if mode == "single-file" => {
            let input = fs::read(path)?;
            serde_json::to_value(run_single_input(engine, &input, iterations.parse()?)?)?
        }
        _ => {
            return Err("usage: jbig2-hayro-comparison generated [iterations] | vectors DIRECTORY | single ENGINE WIDTH HEIGHT ITERATIONS | single-file ENGINE PATH ITERATIONS".into());
        }
    };
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

fn run_single(
    engine: &str,
    width: u32,
    height: u32,
    iterations: usize,
) -> Result<TimingResult, String> {
    let input = all_white_mmr_document(width, height);
    run_single_input(engine, &input, iterations)
}

fn run_single_input(engine: &str, input: &[u8], iterations: usize) -> Result<TimingResult, String> {
    let operation: fn(&[u8]) -> Result<BitmapResult, String> = match engine {
        "jbig2_rs" => decode_ours,
        "hayro_jbig2" => decode_hayro,
        _ => return Err("engine must be jbig2_rs or hayro_jbig2".into()),
    };
    let output = operation(input)?;
    let parse_operation: fn(&[u8]) -> Result<u64, String> = match engine {
        "jbig2_rs" => parse_ours,
        "hayro_jbig2" => parse_hayro,
        _ => return Err("engine must be jbig2_rs or hayro_jbig2".into()),
    };
    let parse_timing = timing_stats(iterations, || parse_operation(input))?;
    let timing = timing_stats(iterations, || operation(input))?;
    Ok(TimingResult {
        width: output.width,
        height: output.height,
        input_bytes: input.len(),
        output_bytes: output.bytes,
        iterations,
        jbig2_rs_parse_median_ns: if engine == "jbig2_rs" {
            parse_timing.median_ns
        } else {
            0
        },
        jbig2_rs_parse_p95_ns: if engine == "jbig2_rs" {
            parse_timing.p95_ns
        } else {
            0
        },
        jbig2_rs_median_ns: if engine == "jbig2_rs" {
            timing.median_ns
        } else {
            0
        },
        hayro_jbig2_parse_median_ns: if engine == "hayro_jbig2" {
            parse_timing.median_ns
        } else {
            0
        },
        hayro_jbig2_parse_p95_ns: if engine == "hayro_jbig2" {
            parse_timing.p95_ns
        } else {
            0
        },
        jbig2_rs_p95_ns: if engine == "jbig2_rs" {
            timing.p95_ns
        } else {
            0
        },
        hayro_jbig2_median_ns: if engine == "hayro_jbig2" {
            timing.median_ns
        } else {
            0
        },
        hayro_jbig2_p95_ns: if engine == "hayro_jbig2" {
            timing.p95_ns
        } else {
            0
        },
        hayro_over_jbig2_rs: 0.0,
    })
}

fn run_generated_benchmarks(iterations: usize) -> Result<Vec<TimingResult>, String> {
    let mut results = Vec::new();
    for (width, height) in [(64, 64), (512, 512), (2048, 2048)] {
        let input = all_white_mmr_document(width, height);
        let expected = decode_ours(&input)?;
        let competitor = decode_hayro(&input)?;
        if expected.hash != competitor.hash
            || expected.width != competitor.width
            || expected.height != competitor.height
        {
            return Err(format!("generated {width}x{height} bitmap mismatch"));
        }
        for _ in 0..3 {
            let _ = parse_ours(&input)?;
            let _ = parse_hayro(&input)?;
            let _ = decode_ours(&input)?;
            let _ = decode_hayro(&input)?;
        }
        let ours_parse = timing_stats(iterations, || parse_ours(&input))?;
        let hayro_parse = timing_stats(iterations, || parse_hayro(&input))?;
        let ours = timing_stats(iterations, || decode_ours(&input))?;
        let hayro = timing_stats(iterations, || decode_hayro(&input))?;
        results.push(TimingResult {
            width,
            height,
            input_bytes: input.len(),
            output_bytes: expected.bytes,
            iterations,
            jbig2_rs_parse_median_ns: ours_parse.median_ns,
            jbig2_rs_parse_p95_ns: ours_parse.p95_ns,
            jbig2_rs_median_ns: ours.median_ns,
            jbig2_rs_p95_ns: ours.p95_ns,
            hayro_jbig2_parse_median_ns: hayro_parse.median_ns,
            hayro_jbig2_parse_p95_ns: hayro_parse.p95_ns,
            hayro_jbig2_median_ns: hayro.median_ns,
            hayro_jbig2_p95_ns: hayro.p95_ns,
            hayro_over_jbig2_rs: hayro.median_ns as f64 / ours.median_ns as f64,
        });
    }
    Ok(results)
}

fn timing_stats<F, T>(iterations: usize, mut operation: F) -> Result<TimingStats, String>
where
    F: FnMut() -> Result<T, String>,
{
    if iterations == 0 {
        return Err("iterations must be nonzero".into());
    }
    let mut timings = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        let result = operation()?;
        std::hint::black_box(result);
        timings.push(start.elapsed().as_nanos());
    }
    timings.sort_unstable();
    let p95_index = (timings.len() - 1) * 95 / 100;
    Ok(TimingStats {
        median_ns: timings[timings.len() / 2],
        p95_ns: timings[p95_index],
    })
}

fn run_vectors(directory: &Path) -> Result<Vec<VectorResult>, Box<dyn std::error::Error>> {
    let mut paths = fs::read_dir(directory)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "jb2"))
        .collect::<Vec<PathBuf>>();
    paths.sort();
    let mut results = Vec::new();
    for path in paths {
        let input = fs::read(&path)?;
        let ours = decoder_result(decode_ours(&input));
        let hayro = decoder_result(decode_hayro(&input));
        let exact_match = match (&ours.bitmap, &hayro.bitmap) {
            (Some(left), Some(right)) => {
                left.width == right.width && left.height == right.height && left.hash == right.hash
            }
            _ => false,
        };
        results.push(VectorResult {
            name: path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_owned(),
            input_bytes: input.len(),
            jbig2_rs: ours,
            hayro_jbig2: hayro,
            exact_match,
        });
    }
    Ok(results)
}

fn decoder_result(result: Result<BitmapResult, String>) -> DecoderResult {
    match result {
        Ok(bitmap) => DecoderResult {
            status: "decoded".into(),
            bitmap: Some(bitmap),
        },
        Err(error) => DecoderResult {
            status: error,
            bitmap: None,
        },
    }
}

fn decode_ours(input: &[u8]) -> Result<BitmapResult, String> {
    let document = Decoder::default()
        .decode(input)
        .map_err(|error| format!("error:{error}"))?;
    let page = document
        .pages()
        .first()
        .ok_or_else(|| "error:no page".to_owned())?;
    let bitmap = page.bitmap();
    Ok(bitmap_result(
        bitmap.width(),
        bitmap.height(),
        bitmap.stride(),
        bitmap.bytes(),
    ))
}

fn decode_hayro(input: &[u8]) -> Result<BitmapResult, String> {
    let image = Image::new(input).map_err(|error| format!("error:{error}"))?;
    let mut collector = PackedCollector::new(image.width(), image.height())?;
    image
        .decode(&mut collector)
        .map_err(|error| format!("error:{error}"))?;
    collector.finish()
}

fn bitmap_result(width: u32, height: u32, stride: usize, data: &[u8]) -> BitmapResult {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash ^= u64::from(width);
    hash = hash.wrapping_mul(0x100000001b3);
    hash ^= u64::from(height);
    BitmapResult {
        width,
        height,
        bytes: stride * height as usize,
        hash,
    }
}

fn all_white_mmr_document(width: u32, height: u32) -> Vec<u8> {
    let mut page = Vec::new();
    page.extend_from_slice(&width.to_be_bytes());
    page.extend_from_slice(&height.to_be_bytes());
    page.extend_from_slice(&0_u32.to_be_bytes());
    page.extend_from_slice(&0_u32.to_be_bytes());
    page.push(0x40);
    page.extend_from_slice(&0_u16.to_be_bytes());

    let coded_len = height.div_ceil(8) as usize;
    let mut generic = Vec::new();
    generic.extend_from_slice(&width.to_be_bytes());
    generic.extend_from_slice(&height.to_be_bytes());
    generic.extend_from_slice(&0_u32.to_be_bytes());
    generic.extend_from_slice(&0_u32.to_be_bytes());
    generic.push(0);
    generic.push(1);
    generic.extend(std::iter::repeat_n(0xff, coded_len));
    if !height.is_multiple_of(8) {
        *generic.last_mut().expect("generated byte") = 0xff << (8 - (height % 8));
    }

    let mut stream = SIGNATURE.to_vec();
    stream.push(0x01);
    stream.extend_from_slice(&1_u32.to_be_bytes());
    stream.extend_from_slice(&segment_header(0, 48, 1, page.len()));
    stream.extend_from_slice(&page);
    stream.extend_from_slice(&segment_header(1, 38, 1, generic.len()));
    stream.extend_from_slice(&generic);
    stream.extend_from_slice(&segment_header(2, 49, 1, 0));
    stream.extend_from_slice(&segment_header(3, 51, 0, 0));
    stream
}

fn segment_header(number: u32, segment_type: u8, page: u8, length: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&number.to_be_bytes());
    bytes.push(segment_type);
    bytes.push(0);
    bytes.push(page);
    bytes.extend_from_slice(&(length as u32).to_be_bytes());
    bytes
}

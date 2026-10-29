//! Generated all-white MMR scaling smoke benchmark.

use std::time::Instant;

use jbig2_decoder::Decoder;

#[path = "../benchmarks/generated.rs"]
mod generated;

use generated::all_white_mmr_document;

fn main() {
    let cases = [(64_u32, 64_u32), (512, 512), (2048, 2048)];
    println!("{{\"cases\":[");
    for (index, (width, height)) in cases.into_iter().enumerate() {
        let stream = all_white_mmr_document(width, height);
        let start = Instant::now();
        let document = Decoder::default()
            .decode(&stream)
            .expect("generated stream");
        let elapsed = start.elapsed();
        let page = document.pages()[0].bitmap();
        if index != 0 {
            println!(",");
        }
        print!(
            "{{\"width\":{width},\"height\":{height},\"input_bytes\":{},\"output_bytes\":{},\"elapsed_micros\":{}}}",
            stream.len(),
            page.bytes().len(),
            elapsed.as_micros()
        );
    }
    println!("]}}");
}

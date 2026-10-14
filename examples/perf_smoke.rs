//! Generated all-white MMR scaling smoke benchmark.

use std::time::Instant;

use jbig2_rs::Decoder;

const SIGNATURE: [u8; 8] = [0x97, 0x4A, 0x42, 0x32, 0x0D, 0x0A, 0x1A, 0x0A];

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

fn all_white_mmr_document(width: u32, height: u32) -> Vec<u8> {
    let mut page = Vec::new();
    page.extend_from_slice(&width.to_be_bytes());
    page.extend_from_slice(&height.to_be_bytes());
    page.extend_from_slice(&0_u32.to_be_bytes());
    page.extend_from_slice(&0_u32.to_be_bytes());
    page.push(0x40);
    page.extend_from_slice(&0_u16.to_be_bytes());

    let coded_len = usize::try_from(height.div_ceil(8)).expect("generated height");
    let mut generic = Vec::new();
    generic.extend_from_slice(&width.to_be_bytes());
    generic.extend_from_slice(&height.to_be_bytes());
    generic.extend_from_slice(&0_u32.to_be_bytes());
    generic.extend_from_slice(&0_u32.to_be_bytes());
    generic.push(0);
    generic.push(1);
    generic.extend(std::iter::repeat_n(0xFF, coded_len));
    if height % 8 != 0 {
        let final_mask = 0xFF_u8 << (8 - (height % 8));
        *generic.last_mut().expect("generated MMR byte") = final_mask;
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
    bytes.extend_from_slice(
        &u32::try_from(length)
            .expect("generated payload length")
            .to_be_bytes(),
    );
    bytes
}

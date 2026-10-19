#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: (u8, Option<u32>, Vec<u8>)| {
    let (feature_flags, page_count, trailing) = input;
    let mut bytes = vec![0x97, 0x4A, 0x42, 0x32, 0x0D, 0x0A, 0x1A, 0x0A];
    let organization = feature_flags & 0x01;
    let extensions = feature_flags & 0x0C;
    let count_flag = if page_count.is_some() { 0 } else { 0x02 };
    bytes.push(organization | extensions | count_flag);
    if let Some(count) = page_count {
        bytes.extend_from_slice(&count.to_be_bytes());
    }
    bytes.extend_from_slice(&trailing);
    let _ = jbig2_rs::inspect_file_header(&bytes);
});

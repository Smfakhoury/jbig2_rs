use std::io::Cursor;

use jbig2_rs::{DecodeError, Decoder, Limits, Page};

fn segment(number: u32, kind: u8, page: u8, payload: &[u8], output: &mut Vec<u8>) {
    output.extend_from_slice(&number.to_be_bytes());
    output.extend_from_slice(&[kind, 0, page]);
    output.extend_from_slice(&u32::try_from(payload.len()).unwrap().to_be_bytes());
    output.extend_from_slice(payload);
}

fn generated_page() -> (Vec<u8>, Vec<u8>) {
    let mut page = Vec::new();
    page.extend_from_slice(&8_u32.to_be_bytes());
    page.extend_from_slice(&1_u32.to_be_bytes());
    page.extend_from_slice(&[0; 8]);
    page.extend_from_slice(&[0x40, 0, 0]);

    let mut region = Vec::new();
    region.extend_from_slice(&8_u32.to_be_bytes());
    region.extend_from_slice(&1_u32.to_be_bytes());
    region.extend_from_slice(&[0; 8]);
    region.extend_from_slice(&[0, 1, 0x80]);

    let mut embedded = Vec::new();
    segment(0, 48, 1, &page, &mut embedded);
    segment(1, 38, 1, &region, &mut embedded);
    segment(2, 49, 1, &[], &mut embedded);
    let mut standalone = b"\x97JB2\r\n\x1a\n".to_vec();
    standalone.push(1);
    standalone.extend_from_slice(&1_u32.to_be_bytes());
    standalone.extend_from_slice(&embedded);
    segment(3, 51, 0, &[], &mut standalone);
    (standalone, embedded)
}

fn check_page(page: &Page) {
    assert_eq!(page.number(), 1);
    assert_eq!(page.bitmap().width(), 8);
    assert_eq!(page.bitmap().height(), 1);
    assert_eq!(page.bitmap().stride(), 1);
    assert_eq!(page.bitmap().bytes(), &[0]);
}

fn main() -> Result<(), DecodeError> {
    let decoder = Decoder::new(Limits::default());
    let (standalone, embedded) = generated_page();
    let document = decoder.decode(&standalone)?;
    assert_eq!(document.pages().len(), 1);
    check_page(&document.pages()[0]);
    assert_eq!(decoder.inspect(&standalone)?, *document.info());

    let from_reader = decoder.decode_reader(&mut Cursor::new(&standalone))?;
    assert_eq!(document, from_reader);
    let mut pages = Vec::new();
    let mut sink = |page: Page| {
        check_page(&page);
        pages.push(page);
        Ok(())
    };
    decoder.decode_with_sink(&standalone, &mut sink)?;
    assert_eq!(pages, document.pages());

    let globals = decoder.parse_globals(&[])?;
    let embedded_document = decoder.decode_embedded_with_globals(&embedded, &globals)?;
    assert_eq!(embedded_document.pages(), document.pages());
    let mut context = decoder.context();
    assert_eq!(context.decode(&standalone)?, document);
    assert!(context.decode(b"invalid input").is_err());
    assert_eq!(context.decode(&standalone)?, document);
    assert_eq!(
        context
            .decode_embedded_with_globals(&embedded, &globals)?
            .pages(),
        document.pages()
    );
    context.release_scratch();
    assert_eq!(context.retained_scratch_bytes(), 0);
    Ok(())
}

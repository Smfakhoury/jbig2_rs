use std::io::Cursor;

use jbig2_rs::{DecodeError, DecodedDocument, Decoder, LimitKind, Limits, MalformedError};

#[allow(dead_code)]
#[path = "../../benchmarks/generated.rs"]
mod generated;

const STEPS: usize = 8;

fn assert_errors(actual: DecodeError, expected: DecodeError) {
    match (actual, expected) {
        (DecodeError::Malformed(actual), DecodeError::Malformed(expected)) => {
            assert_eq!(actual, expected);
        }
        (DecodeError::Unsupported(actual), DecodeError::Unsupported(expected)) => {
            assert_eq!(actual, expected);
        }
        (DecodeError::LimitExceeded(actual), DecodeError::LimitExceeded(expected)) => {
            assert_eq!(actual, expected);
        }
        (
            DecodeError::Allocation {
                requested_bytes: actual,
            },
            DecodeError::Allocation {
                requested_bytes: expected,
            },
        ) => assert_eq!(actual, expected),
        (DecodeError::Io(actual), DecodeError::Io(expected)) => assert_eq!(actual, expected),
        (actual, expected) => panic!("different typed errors: {actual:?} / {expected:?}"),
    }
}

fn assert_results<T: std::fmt::Debug + PartialEq>(
    actual: Result<T, DecodeError>,
    expected: Result<T, DecodeError>,
) {
    match (actual, expected) {
        (Ok(actual), Ok(expected)) => assert_eq!(actual, expected),
        (Err(actual), Err(expected)) => assert_errors(actual, expected),
        (actual, expected) => panic!("different outcomes: {actual:?} / {expected:?}"),
    }
}

fn byte(input: &[u8], index: usize) -> u8 {
    input.get(index).copied().unwrap_or(0)
}

struct Fixture {
    input: Vec<u8>,
    width: u32,
    height: u32,
    pages: u8,
    alternating: bool,
}

impl Fixture {
    fn new(input: &[u8], offset: usize) -> Self {
        let alternating = byte(input, offset + 3) & 1 != 0;
        let width = if alternating {
            2 * (u32::from(byte(input, offset) % 64) + 1)
        } else {
            u32::from(byte(input, offset) % 128) + 1
        };
        let height = u32::from(byte(input, offset + 1) % 16) + 1;
        let pages = byte(input, offset + 2) % 3 + 1;
        Self {
            input: generated::mmr_document(width, height, pages, alternating),
            width,
            height,
            pages,
            alternating,
        }
    }

    fn embedded(&self) -> &[u8] {
        &self.input[13..self.input.len() - 11]
    }

    fn changed(&self, argument: u16) -> Vec<u8> {
        let mut changed = self.input.clone();
        let position = usize::from(argument / 3) % changed.len();
        match argument % 3 {
            0 => changed.truncate(position),
            1 => changed[position] ^= 1 << (argument % 8),
            _ => {
                changed.pop();
            }
        }
        changed
    }

    fn check(&self, document: &DecodedDocument) {
        assert_eq!(document.pages().len(), usize::from(self.pages));
        let stride = usize::try_from(self.width.div_ceil(8)).expect("bounded stride");
        for (index, page) in document.pages().iter().enumerate() {
            let bitmap = page.bitmap();
            assert_eq!(
                page.number(),
                u32::try_from(index + 1).expect("bounded page")
            );
            assert_eq!(bitmap.width(), self.width);
            assert_eq!(bitmap.height(), self.height);
            assert_eq!(bitmap.stride(), stride);
            assert_eq!(
                bitmap.bytes().len(),
                stride * usize::try_from(self.height).expect("bounded height")
            );
            for row in bitmap.bytes().chunks_exact(stride) {
                for (column, actual) in row.iter().enumerate() {
                    let mask = if column + 1 == stride && self.width % 8 != 0 {
                        0xff << (8 - self.width % 8)
                    } else {
                        0xff
                    };
                    let expected = if self.alternating { 0x55 & mask } else { 0 };
                    assert_eq!(*actual, expected);
                }
            }
        }
    }

    fn check_reader_and_sink(&self, decoder: &Decoder) {
        let document = decoder
            .decode_reader(&mut Cursor::new(&self.input))
            .expect("generated reader document");
        self.check(&document);
        let mut pages = Vec::new();
        decoder
            .decode_with_sink(&self.input, &mut |page| {
                pages.push(page);
                Ok(())
            })
            .expect("generated sink");
        assert_eq!(pages, document.pages());
    }

    fn check_globals(&self, decoder: &Decoder, context: &mut jbig2_rs::DecoderContext<'_>) {
        assert_errors(
            context
                .parse_globals(&self.input)
                .expect_err("invalid globals"),
            decoder
                .parse_globals(&self.input)
                .expect_err("fresh invalid globals"),
        );
        assert_eq!(
            context
                .parse_globals(&[])
                .expect("globals recovery")
                .segment_count(),
            0
        );
    }
}

fn limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_width = 128;
    limits.max_height = 16;
    limits.max_pixels_per_page = 2048;
    limits.max_decoder_managed_memory_bytes = 1 << 21;
    limits.max_stream_bytes = 8192;
    limits.max_pages = 3;
    limits.max_segments = 16;
    limits.max_symbols = 64;
    limits.max_references_per_segment = 16;
    limits.max_total_references = 64;
    limits.max_reference_graph_depth = 8;
    limits.max_nesting_depth = 8;
    limits.max_work_units = 1 << 18;
    limits.max_cumulative_decoded_bitmap_bytes = 1 << 16;
    limits.max_retained_state_bytes = 1 << 18;
    limits
}

/// Two zero-padded four-byte shapes, then at most eight opcode/u16 argument triples.
/// Remaining bytes supply at most 256 bytes of arbitrary input to one operation.
pub fn exercise(input: &[u8]) {
    let first = Fixture::new(input, 0);
    let second = Fixture::new(input, 4);
    let decoder = Decoder::new(limits());
    let globals = decoder.parse_globals(&[]).expect("empty generated globals");
    let mut context = decoder.context();
    first.check(&context.decode(&first.input).expect("generated warmup"));
    for step in 0..STEPS {
        let offset = 8 + 3 * step;
        let operation = input
            .get(offset)
            .copied()
            .unwrap_or(u8::try_from(step).expect("bounded step"))
            % 10;
        let argument = u16::from_be_bytes([byte(input, offset + 1), byte(input, offset + 2)]);
        match operation {
            0 | 1 => {
                let fixture = if operation == 0 { &first } else { &second };
                let fresh = decoder
                    .decode(&fixture.input)
                    .expect("fresh generated document");
                let reused = context
                    .decode(&fixture.input)
                    .expect("reused generated document");
                fixture.check(&reused);
                assert_eq!(reused, fresh);
            }
            2 => {
                assert_eq!(
                    context.inspect(&first.input).expect("reused inspection"),
                    decoder.inspect(&first.input).expect("fresh inspection")
                );
                assert_eq!(
                    context
                        .inspect_embedded_with_globals(first.embedded(), &globals)
                        .expect("reused embedded inspection"),
                    decoder
                        .inspect_embedded_with_globals(first.embedded(), &globals)
                        .expect("fresh embedded inspection")
                );
            }
            3 => {
                let fresh = decoder
                    .decode_embedded_with_globals(first.embedded(), &globals)
                    .expect("fresh embedded document");
                let reused = context
                    .decode_embedded_with_globals(first.embedded(), &globals)
                    .expect("reused embedded document");
                first.check(&reused);
                assert_eq!(reused, fresh);
            }
            4 => {
                let changed = first.changed(argument);
                let fresh = decoder.decode(&changed);
                if argument % 3 == 2 {
                    assert!(matches!(
                        fresh,
                        Err(DecodeError::Malformed(MalformedError::UnexpectedEof { .. }))
                    ));
                }
                assert_results(context.decode(&changed), fresh);
            }
            5 => {
                let bytes = input.get(32..).unwrap_or(&[]);
                let bytes = &bytes[..bytes.len().min(256)];
                assert_results(context.decode(bytes), decoder.decode(bytes));
                assert_results(context.inspect(bytes), decoder.inspect(bytes));
            }
            6 => {
                let mut oversized = first.input.clone();
                oversized[24..28].copy_from_slice(&129_u32.to_be_bytes());
                let fresh = decoder.decode(&oversized);
                assert!(matches!(
                    &fresh,
                    Err(DecodeError::LimitExceeded(detail))
                        if detail.kind == LimitKind::Width
                            && detail.limit == 128 && detail.actual == 129
                ));
                assert_results(context.decode(&oversized), fresh);
            }
            7 => {
                context.release_scratch();
                assert_eq!(context.retained_scratch_bytes(), 0);
            }
            8 => second.check_reader_and_sink(&decoder),
            9 => first.check_globals(&decoder, &mut context),
            _ => unreachable!("bounded operation"),
        }
        first.check(
            &context
                .decode(&first.input)
                .expect("recovery after operation"),
        );
        assert!(context.retained_scratch_bytes() <= decoder.limits().max_retained_state_bytes);
    }
    context.release_scratch();
    assert_eq!(context.retained_scratch_bytes(), 0);
}

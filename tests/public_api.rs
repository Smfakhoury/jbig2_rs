//! Generated public-API regressions for output parity, failures, and limits.

use std::cell::Cell;
use std::io::{self, Cursor, Read};

use jbig2_rs::{DecodeError, Decoder, IoErrorKind, LimitKind, Limits, MalformedError, Page};

#[path = "../tools/distribution_consumer.rs"]
mod distribution_consumer;

struct ChunkedReader<'a> {
    input: Cursor<&'a [u8]>,
    chunk: usize,
}

impl Read for ChunkedReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let count = output.len().min(self.chunk);
        self.input.read(&mut output[..count])
    }
}

struct FailingReader(io::ErrorKind);

impl Read for FailingReader {
    fn read(&mut self, _output: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::new(self.0, "reader sentinel"))
    }
}

fn assert_io(error: &DecodeError, expected: IoErrorKind) {
    assert!(matches!(error, DecodeError::Io(actual) if *actual == expected));
}

fn assert_limit(error: DecodeError, expected: LimitKind, limit: u64) {
    match error {
        DecodeError::LimitExceeded(detail) => {
            assert_eq!(detail.kind, expected);
            assert_eq!(detail.limit, limit);
            assert!(detail.actual > limit);
        }
        other => panic!("expected {expected:?} limit failure, got {other:?}"),
    }
}

#[test]
fn downstream_distribution_smoke_is_also_a_runtime_regression() {
    distribution_consumer::main().expect("public API smoke");
}

#[test]
fn short_reads_preserve_inspection_decode_and_sink_output() {
    let (input, _) = distribution_consumer::generated_page();
    let decoder = Decoder::default();
    let expected = decoder.decode(&input).expect("slice decode");
    for chunk in [1, 3, 8192] {
        let reader = || ChunkedReader {
            input: Cursor::new(input.as_slice()),
            chunk,
        };
        assert_eq!(
            decoder
                .inspect_reader(&mut reader())
                .expect("short-read inspect"),
            *expected.info()
        );
        assert_eq!(
            decoder
                .decode_reader(&mut reader())
                .expect("short-read decode"),
            expected
        );
        let mut pages = Vec::new();
        let info = decoder
            .decode_reader_with_sink(&mut reader(), &mut |page| {
                pages.push(page);
                Ok(())
            })
            .expect("reader with sink");
        assert_eq!(info, *expected.info());
        assert_eq!(pages, expected.pages());
    }
}

#[test]
fn reader_errors_remain_typed_and_never_call_the_sink() {
    let decoder = Decoder::default();
    for (kind, expected) in [
        (
            io::ErrorKind::PermissionDenied,
            IoErrorKind::PermissionDenied,
        ),
        (io::ErrorKind::Interrupted, IoErrorKind::Interrupted),
        (io::ErrorKind::UnexpectedEof, IoErrorKind::UnexpectedEof),
    ] {
        assert_io(
            &decoder
                .inspect_reader(&mut FailingReader(kind))
                .expect_err("reader failure"),
            expected,
        );
        assert_io(
            &decoder
                .decode_reader(&mut FailingReader(kind))
                .expect_err("reader failure"),
            expected,
        );
        let calls = Cell::new(0);
        let error = decoder
            .decode_reader_with_sink(&mut FailingReader(kind), &mut |_| {
                calls.set(calls.get() + 1);
                Ok(())
            })
            .expect_err("reader failure before sink");
        assert_io(&error, expected);
        assert_eq!(calls.get(), 0);
    }
}

#[test]
fn complete_page_prefix_does_not_hide_a_late_reader_error() {
    let (input, _) = distribution_consumer::generated_page();
    let decoder = Decoder::default();
    let mut reader = Cursor::new(input).chain(FailingReader(io::ErrorKind::TimedOut));
    let calls = Cell::new(0);
    let error = decoder
        .decode_reader_with_sink(&mut reader, &mut |_| {
            calls.set(calls.get() + 1);
            Ok(())
        })
        .expect_err("failure after a complete valid prefix");
    assert_io(&error, IoErrorKind::TimedOut);
    assert_eq!(calls.get(), 0);
}

#[test]
fn stream_byte_limit_is_inclusive_for_slice_and_reader_apis() {
    let (input, _) = distribution_consumer::generated_page();
    let exact = u64::try_from(input.len()).expect("fixture length");
    let mut limits = Limits::default();
    limits.max_stream_bytes = exact;
    let decoder = Decoder::new(limits);
    let expected = decoder.decode(&input).expect("exact input limit");
    assert_eq!(
        decoder
            .decode_reader(&mut Cursor::new(&input))
            .expect("exact reader limit"),
        expected
    );
    limits.max_stream_bytes = exact - 1;
    let decoder = Decoder::new(limits);
    assert_limit(
        decoder.inspect(&input).expect_err("over limit"),
        LimitKind::StreamBytes,
        exact - 1,
    );
    assert_limit(
        decoder.decode(&input).expect_err("over limit"),
        LimitKind::StreamBytes,
        exact - 1,
    );
    for chunk in [1, 3, 8192] {
        let mut reader = ChunkedReader {
            input: Cursor::new(&input),
            chunk,
        };
        assert_limit(
            decoder
                .decode_reader(&mut reader)
                .expect_err("over reader limit"),
            LimitKind::StreamBytes,
            exact - 1,
        );
    }
}

#[test]
fn sink_failure_is_preserved_and_does_not_poison_the_decoder() {
    let (input, _) = distribution_consumer::generated_page();
    let decoder = Decoder::default();
    for from_reader in [false, true] {
        let calls = Cell::new(0);
        let mut sink = |page: Page| {
            assert_eq!(page.number(), 1);
            calls.set(calls.get() + 1);
            Err(DecodeError::Allocation {
                requested_bytes: 73,
            })
        };
        let error = if from_reader {
            decoder.decode_reader_with_sink(&mut Cursor::new(&input), &mut sink)
        } else {
            decoder.decode_with_sink(&input, &mut sink)
        }
        .expect_err("sink sentinel");
        assert!(matches!(
            error,
            DecodeError::Allocation {
                requested_bytes: 73
            }
        ));
        assert_eq!(calls.get(), 1);
        assert_eq!(
            decoder
                .decode(&input)
                .expect("decode after sink failure")
                .pages()
                .len(),
            1
        );
    }
}

#[test]
fn malformed_envelope_never_emits_pages() {
    let decoder = Decoder::default();
    let calls = Cell::new(0);
    let mut sink = |_| {
        calls.set(calls.get() + 1);
        Ok(())
    };
    let input = b"invalid JBIG2 input";
    assert!(matches!(
        decoder.decode_with_sink(input, &mut sink),
        Err(DecodeError::Malformed(MalformedError::InvalidFileSignature))
    ));
    assert!(decoder
        .decode_reader_with_sink(&mut Cursor::new(input), &mut sink)
        .is_err());
    assert_eq!(calls.get(), 0);
}

#[test]
fn page_dimensions_and_structural_work_limits_are_inclusive() {
    let (input, _) = distribution_consumer::generated_page();
    let mut limits = Limits::default();
    limits.max_width = 8;
    limits.max_height = 1;
    limits.max_pixels_per_page = 8;
    let decoder = Decoder::new(limits);
    assert!(decoder.decode(&input).is_ok());

    let mut width = limits;
    width.max_width = 7;
    assert_limit(
        Decoder::new(width).decode(&input).expect_err("width"),
        LimitKind::Width,
        7,
    );
    let mut height = limits;
    height.max_height = 0;
    assert_limit(
        Decoder::new(height).decode(&input).expect_err("height"),
        LimitKind::Height,
        0,
    );
    let mut pixels = limits;
    pixels.max_pixels_per_page = 7;
    assert_limit(
        Decoder::new(pixels).decode(&input).expect_err("pixels"),
        LimitKind::PagePixels,
        7,
    );

    let mut work = Limits::default();
    let input_work = u64::try_from(input.len()).expect("fixture length");
    work.max_work_units = input_work + 4;
    assert_eq!(
        Decoder::new(work)
            .inspect(&input)
            .expect("four segments, no references")
            .segment_count(),
        4
    );
    work.max_work_units -= 1;
    assert_limit(
        Decoder::new(work).inspect(&input).expect_err("input work"),
        LimitKind::Work,
        work.max_work_units,
    );
    work.max_work_units = input_work - 1;
    assert_limit(
        Decoder::new(work).inspect(&input).expect_err("input work"),
        LimitKind::Work,
        input_work - 1,
    );
}

#[test]
fn decoded_byte_and_page_limits_return_the_expected_resource_kind() {
    let (input, _) = distribution_consumer::generated_page();
    let mut limits = Limits::default();
    limits.max_pages = 0;
    assert_limit(
        Decoder::new(limits).decode(&input).expect_err("page limit"),
        LimitKind::Pages,
        0,
    );
    limits = Limits::default();
    limits.max_cumulative_decoded_bitmap_bytes = 0;
    assert_limit(
        Decoder::new(limits)
            .decode(&input)
            .expect_err("decoded byte limit"),
        LimitKind::DecodedBitmapBytes,
        0,
    );
}

#[test]
fn context_inspection_and_globals_recover_after_errors() {
    let (input, embedded) = distribution_consumer::generated_page();
    let decoder = Decoder::default();
    let mut context = decoder.context();
    assert_eq!(context.limits(), decoder.limits());
    let info = decoder.inspect(&input).expect("standalone info");
    assert!(context.inspect(&[0]).is_err());
    assert_eq!(context.inspect(&input).expect("context inspect"), info);
    let globals = context.parse_globals(&[]).expect("empty globals");
    assert_eq!(globals.segment_count(), 0);
    assert!(context.parse_globals(&embedded).is_err());
    assert_eq!(
        context
            .parse_globals(&[])
            .expect("recovered globals")
            .segment_count(),
        0
    );
    assert!(context.inspect_embedded(&[0]).is_err());
    let embedded_info = context
        .inspect_embedded(&embedded)
        .expect("embedded context");
    assert!(context
        .inspect_embedded_with_globals(&[0], &globals)
        .is_err());
    assert_eq!(
        context
            .inspect_embedded_with_globals(&embedded, &globals)
            .expect("globals context"),
        embedded_info
    );
    assert_eq!(
        context
            .decode_embedded(&embedded)
            .expect("embedded decode")
            .pages()
            .len(),
        1
    );
    assert_eq!(info.pages()[0].number(), 1);
    assert_eq!(info.pages()[0].width(), 8);
    assert_eq!(info.pages()[0].height(), 1);
    assert_eq!(info.pages()[0].x_resolution(), 0);
    assert_eq!(info.pages()[0].y_resolution(), 0);
    assert!(!info.pages()[0].is_striped());
}

#[test]
fn mandatory_page_retention_and_optional_scratch_have_distinct_limits() {
    let (input, _) = distribution_consumer::generated_page();
    let mut limits = Limits::default();
    limits.max_retained_state_bytes = 0;
    assert_limit(
        Decoder::new(limits)
            .decode(&input)
            .expect_err("mandatory page state"),
        LimitKind::RetainedState,
        0,
    );
    limits.max_retained_state_bytes = 1;
    let decoder = Decoder::new(limits);
    let mut context = decoder.context();
    for _ in 0..2 {
        assert_eq!(
            context
                .decode(&input)
                .expect("bounded context")
                .pages()
                .len(),
            1
        );
        assert_eq!(context.retained_scratch_bytes(), 0);
    }
}

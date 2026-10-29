//! First-party encoders and literal-pixel oracles; no sample/vector inputs.

use std::io::Cursor;

use jbig2_decoder::{
    DecodeError, DecodedDocument, Decoder, HalftoneErrorKind, HuffmanErrorKind, LimitKind, Limits,
    MalformedError,
};

const SIGNATURE: [u8; 8] = [0x97, 0x4a, 0x42, 0x32, 0x0d, 0x0a, 0x1a, 0x0a];
pub const SHAPE_BYTES: usize = 16;
pub const OPERATIONS: u8 = 8;
pub const FAULTS: u8 = 4;
pub const LIMITS: u8 = 14;

fn byte(input: &[u8], index: usize) -> u8 {
    input.get(index).copied().unwrap_or(0)
}

#[derive(Default)]
struct Bits {
    bytes: Vec<u8>,
    length: usize,
}

impl Bits {
    fn put(&mut self, value: u32, count: u8) {
        for shift in (0..count).rev() {
            if self.length % 8 == 0 {
                self.bytes.push(0);
            }
            if value & (1 << shift) != 0 {
                *self.bytes.last_mut().expect("generated bit") |= 0x80 >> (self.length % 8);
            }
            self.length += 1;
        }
    }

    fn align(&mut self) {
        self.length = self.bytes.len() * 8;
    }

    fn append(&mut self, bytes: &[u8]) {
        self.align();
        self.bytes.extend_from_slice(bytes);
        self.align();
    }

    fn table_a(&mut self, value: u32) {
        assert!(value <= 271);
        if value < 16 {
            self.put(0, 1);
            self.put(value, 4);
        } else {
            self.put(2, 2);
            self.put(value - 16, 8);
        }
    }

    fn width_delta(&mut self, width: u32) {
        match width {
            0 => self.put(0, 1),
            1 => self.put(2, 2),
            2 => self.put(6, 3),
            3..=9 => {
                self.put(14, 4);
                self.put(width - 3, 3);
            }
            _ => unreachable!("bounded width"),
        }
    }
}

fn pack(width: u32, height: u32, pixels: &[bool]) -> Vec<u8> {
    assert_eq!(pixels.len(), usize::try_from(width * height).unwrap());
    let stride = usize::try_from(width.div_ceil(8)).unwrap();
    let mut bytes = vec![0; stride * usize::try_from(height).unwrap()];
    for y in 0..height {
        for x in 0..width {
            if pixels[usize::try_from(y * width + x).unwrap()] {
                bytes[usize::try_from(y).unwrap() * stride + usize::try_from(x / 8).unwrap()] |=
                    0x80 >> (x % 8);
            }
        }
    }
    bytes
}

fn mmr(width: u32, height: u32, pixels: &[bool], eofb: bool) -> Vec<u8> {
    assert_eq!(pixels.len(), usize::try_from(width * height).unwrap());
    let mut bits = Bits::default();
    // One horizontal white/black pair per pixel. Zero-length runs keep the
    // encoder independent of reference rows, run lookup tables and the decoder.
    for pixel in pixels {
        bits.put(1, 3);
        if *pixel {
            bits.put(0b0011_0101, 8); // white zero
            bits.put(0b010, 3); // black one
        } else {
            bits.put(0b00_0111, 6); // white one
            bits.put(0b00_0011_0111, 10); // black zero
        }
    }
    if eofb {
        bits.put(0x0000_1001, 24);
    }
    bits.bytes
}

fn apply(operator: u8, old: bool, incoming: bool) -> bool {
    match operator {
        0 => old || incoming,
        1 => old && incoming,
        2 => old != incoming,
        3 => old == incoming,
        4 => incoming,
        _ => unreachable!("bounded operator"),
    }
}

fn paint(
    pixels: &mut [bool],
    size: (u32, u32),
    source_size: (u32, u32),
    origin: (i32, i32),
    operator: u8,
    source: impl Fn(u32, u32) -> bool,
) {
    for sy in 0..source_size.1 {
        for sx in 0..source_size.0 {
            let x = origin.0 + i32::try_from(sx).unwrap();
            let y = origin.1 + i32::try_from(sy).unwrap();
            if x >= 0
                && y >= 0
                && x < i32::try_from(size.0).unwrap()
                && y < i32::try_from(size.1).unwrap()
            {
                let offset =
                    usize::try_from(u32::try_from(y).unwrap() * size.0 + u32::try_from(x).unwrap())
                        .unwrap();
                pixels[offset] = apply(operator, pixels[offset], source(sx, sy));
            }
        }
    }
}

#[derive(Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct Fixture {
    pub family: u8,
    pub width: u32,
    pub height: u32,
    pattern_width: u32,
    pattern_height: u32,
    count: u32,
    grid_width: u32,
    grid_height: u32,
    operator: u8,
    default_pixel: bool,
    page_default: bool,
    page_operator: u8,
    corner: u8,
    transpose: bool,
    x: i32,
    y: i32,
    salt: u8,
    raw: bool,
    instances: u32,
    log_strip: u8,
    separate_strips: bool,
}

impl Fixture {
    pub fn new(input: &[u8]) -> Self {
        Self {
            family: byte(input, 0) % 2,
            width: u32::from(byte(input, 1) % 33) + 1,
            height: u32::from(byte(input, 2) % 9) + 1,
            pattern_width: u32::from(byte(input, 3) % 9) + 1,
            pattern_height: u32::from(byte(input, 4) % 3) + 1,
            count: u32::from(byte(input, 5) % 4) + 1,
            grid_width: u32::from(byte(input, 6) % 9) + 1,
            grid_height: u32::from(byte(input, 7) % 5) + 1,
            operator: byte(input, 8) % if byte(input, 0) % 2 == 0 { 5 } else { 4 },
            default_pixel: byte(input, 9) & 1 != 0,
            page_default: byte(input, 9) & 2 != 0,
            page_operator: byte(input, 9) / 4 % 5,
            corner: byte(input, 10) % 4,
            transpose: byte(input, 10) & 4 != 0,
            x: i32::from(byte(input, 11) % 7) - 3,
            y: i32::from(byte(input, 12) % 7) - 3,
            salt: byte(input, 13),
            raw: byte(input, 14) & 1 != 0,
            instances: u32::from(byte(input, 14) / 2 % 4) + 1,
            log_strip: byte(input, 15) % 4,
            separate_strips: byte(input, 15) & 4 != 0,
        }
    }

    fn pattern_pixel(&self, pattern: u32, x: u32, y: u32) -> bool {
        (pattern * 3 + x + y * 5 + u32::from(self.salt)) % 7 < 3
    }

    fn index(&self, x: u32, y: u32) -> u32 {
        (x + y * 3 + u32::from(self.salt)) % self.count
    }

    fn collective(&self) -> Vec<bool> {
        let width = self.pattern_width * self.count;
        (0..width * self.pattern_height)
            .map(|offset| {
                let x = offset % width;
                self.pattern_pixel(
                    x / self.pattern_width,
                    x % self.pattern_width,
                    offset / width,
                )
            })
            .collect()
    }

    fn dictionary(&self, fault: Option<u8>) -> Vec<u8> {
        let width = self.pattern_width * self.count;
        let collective = self.collective();
        if self.family == 0 {
            let mut data = vec![
                if fault == Some(0) { 0x81 } else { 1 },
                u8::try_from(self.pattern_width).unwrap(),
                u8::try_from(self.pattern_height).unwrap(),
            ];
            data.extend_from_slice(&(self.count - 1).to_be_bytes());
            data.extend_from_slice(&mmr(width, self.pattern_height, &collective, false));
            data
        } else {
            let mut data = if fault == Some(0) {
                vec![0xe0, 1]
            } else {
                vec![0, 1]
            };
            data.extend_from_slice(&self.count.to_be_bytes());
            data.extend_from_slice(&self.count.to_be_bytes());
            let mut bits = Bits::default();
            match self.pattern_height {
                1 => bits.put(0, 1),
                2 => bits.put(2, 2),
                3 => bits.put(6, 3),
                _ => unreachable!("bounded height"),
            }
            bits.width_delta(self.pattern_width);
            for _ in 1..self.count {
                bits.width_delta(0);
            }
            bits.put(63, 6); // Table B OOB closes the height class.
            let coded = if self.raw {
                pack(width, self.pattern_height, &collective)
            } else {
                mmr(width, self.pattern_height, &collective, false)
            };
            bits.table_a(if self.raw {
                0
            } else {
                u32::try_from(coded.len()).unwrap()
            });
            bits.append(&coded);
            bits.table_a(0); // Export none, then all newly decoded symbols.
            bits.table_a(self.count);
            data.extend_from_slice(&bits.bytes);
            data
        }
    }

    fn region(&self, fault: Option<u8>) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&self.width.to_be_bytes());
        data.extend_from_slice(&self.height.to_be_bytes());
        data.extend_from_slice(&[0; 8]);
        data.push(self.page_operator);
        if self.family == 0 {
            data.push(if fault == Some(1) {
                7
            } else {
                1 | self.operator << 4 | u8::from(self.default_pixel) << 7
            });
            data.extend_from_slice(&self.grid_width.to_be_bytes());
            data.extend_from_slice(&self.grid_height.to_be_bytes());
            data.extend_from_slice(&(self.x * 256 - 1).to_be_bytes());
            data.extend_from_slice(&(self.y * 256 + 127).to_be_bytes());
            data.extend_from_slice(&(if self.transpose { 0_u16 } else { 256 }).to_be_bytes());
            data.extend_from_slice(&(if self.transpose { 256_u16 } else { 0 }).to_be_bytes());
            let depth = 32 - (self.count - 1).leading_zeros();
            for plane in (0..depth).rev() {
                let pixels = (0..self.grid_width * self.grid_height)
                    .map(|offset| {
                        let index = self.index(offset % self.grid_width, offset / self.grid_width);
                        ((index ^ (index >> 1)) >> plane) & 1 != 0
                    })
                    .collect::<Vec<_>>();
                data.extend_from_slice(&mmr(self.grid_width, self.grid_height, &pixels, true));
            }
        } else {
            let flags = 1
                | u16::from(self.log_strip) << 2
                | u16::from(self.corner) << 4
                | u16::from(self.transpose) << 6
                | u16::from(self.operator) << 7
                | u16::from(self.default_pixel) << 9;
            data.extend_from_slice(&flags.to_be_bytes());
            data.extend_from_slice(&(if fault == Some(1) { 0x8000_u16 } else { 0 }).to_be_bytes());
            data.extend_from_slice(&self.instances.to_be_bytes());
            let mut bits = Bits::default();
            for code in 0..35 {
                bits.put(u32::from(code == 2), 4);
            }
            for _ in 0..self.count {
                bits.put(0, 1); // Every symbol ID has length two.
            }
            bits.align();
            bits.put(0, 1); // Table K initial strip value one.
            for instance in 0..self.instances {
                if instance == 0 || self.separate_strips {
                    bits.put(0, 1); // Table K strip delta one.
                    bits.put(0, 9); // Table F first-S zero (00 + seven bits).
                } else {
                    bits.put(0, 3); // Table H subsequent-S zero (00 + one bit).
                }
                bits.put(u32::from(self.salt) % (1 << self.log_strip), self.log_strip);
                bits.put(instance % self.count, 2);
                if self.separate_strips && instance + 1 < self.instances {
                    bits.put(1, 2); // Table H OOB starts another strip.
                }
            }
            data.extend_from_slice(&bits.bytes);
        }
        if fault == Some(3) {
            data.push(0xff);
        }
        data
    }

    fn segments(&self, fault: Option<u8>) -> Vec<(u8, u8, bool, Vec<u8>)> {
        let mut page = Vec::new();
        page.extend_from_slice(&self.width.to_be_bytes());
        page.extend_from_slice(&self.height.to_be_bytes());
        page.extend_from_slice(&[0; 8]);
        page.extend_from_slice(&[0x40 | u8::from(self.page_default) << 2, 0, 0]);
        vec![
            (
                if self.family == 0 { 16 } else { 0 },
                0,
                false,
                self.dictionary(fault),
            ),
            (48, 1, false, page),
            (
                if self.family == 0 { 22 } else { 6 },
                1,
                fault != Some(2),
                self.region(fault),
            ),
            (49, 1, false, Vec::new()),
            (51, 0, false, Vec::new()),
        ]
    }

    pub fn document(&self, random_access: bool, fault: Option<u8>) -> Vec<u8> {
        let mut output = SIGNATURE.to_vec();
        output.push(u8::from(!random_access));
        output.extend_from_slice(&1_u32.to_be_bytes());
        let segments = self.segments(fault);
        for (index, (kind, page, referred, data)) in segments.iter().enumerate() {
            append_header(
                &mut output,
                u32::try_from(index).unwrap(),
                *kind,
                *page,
                *referred,
                data.len(),
            );
            if !random_access {
                output.extend_from_slice(data);
            }
        }
        if random_access {
            for (_, _, _, data) in &segments {
                output.extend_from_slice(data);
            }
        }
        output
    }

    fn embedded(&self) -> (Vec<u8>, Vec<u8>) {
        let segments = self.segments(None);
        let mut globals = Vec::new();
        append_header(
            &mut globals,
            0,
            segments[0].0,
            0,
            false,
            segments[0].3.len(),
        );
        globals.extend_from_slice(&segments[0].3);
        let mut input = Vec::new();
        for (index, (kind, page, referred, data)) in segments.iter().enumerate().take(4).skip(1) {
            append_header(
                &mut input,
                u32::try_from(index).unwrap(),
                *kind,
                *page,
                *referred,
                data.len(),
            );
            input.extend_from_slice(data);
        }
        (globals, input)
    }

    pub fn expected(&self) -> Vec<u8> {
        let mut region =
            vec![self.default_pixel; usize::try_from(self.width * self.height).unwrap()];
        if self.family == 0 {
            for y in 0..self.grid_height {
                for x in 0..self.grid_width {
                    let (dx, dy) = if self.transpose {
                        (
                            self.x + i32::try_from(y).unwrap() - 1,
                            self.y - i32::try_from(x).unwrap(),
                        )
                    } else {
                        (
                            self.x + i32::try_from(x).unwrap() - 1,
                            self.y + i32::try_from(y).unwrap(),
                        )
                    };
                    let pattern = self.index(x, y);
                    paint(
                        &mut region,
                        (self.width, self.height),
                        (self.pattern_width, self.pattern_height),
                        (dx, dy),
                        self.operator,
                        |sx, sy| self.pattern_pixel(pattern, sx, sy),
                    );
                }
            }
        } else {
            for instance in 0..self.instances {
                let within = u32::from(self.salt) % (1 << self.log_strip);
                let strip = if self.separate_strips {
                    instance * (1 << self.log_strip)
                } else {
                    0
                };
                let cursor = if self.separate_strips { 0 } else { instance };
                let (x, y) = if self.transpose {
                    (
                        i32::try_from(strip + within).unwrap()
                            - if self.corner >= 2 {
                                i32::try_from(self.pattern_width - 1).unwrap()
                            } else {
                                0
                            },
                        i32::try_from(cursor * (self.pattern_height - 1)).unwrap(),
                    )
                } else {
                    (
                        i32::try_from(cursor * (self.pattern_width - 1)).unwrap(),
                        i32::try_from(strip + within).unwrap()
                            - if self.corner % 2 == 0 {
                                i32::try_from(self.pattern_height - 1).unwrap()
                            } else {
                                0
                            },
                    )
                };
                paint(
                    &mut region,
                    (self.width, self.height),
                    (self.pattern_width, self.pattern_height),
                    (x, y),
                    self.operator,
                    |sx, sy| self.pattern_pixel(instance % self.count, sx, sy),
                );
            }
        }
        let page = region
            .iter()
            .map(|incoming| apply(self.page_operator, self.page_default, *incoming))
            .collect::<Vec<_>>();
        pack(self.width, self.height, &page)
    }

    pub fn check(&self, document: &DecodedDocument) {
        assert_eq!(document.pages().len(), 1);
        let page = &document.pages()[0];
        assert_eq!(page.number(), 1);
        assert_eq!(page.bitmap().width(), self.width);
        assert_eq!(page.bitmap().height(), self.height);
        assert_eq!(
            page.bitmap().stride(),
            usize::try_from(self.width.div_ceil(8)).unwrap()
        );
        assert_eq!(page.bitmap().bytes(), self.expected());
    }

    pub fn check_fault(&self, fault: u8) {
        let error = Decoder::new(limits())
            .decode(&self.document(false, Some(fault)))
            .expect_err("generated malformed family");
        match (self.family, fault, error) {
            (
                0,
                0,
                DecodeError::Malformed(MalformedError::InvalidHalftone(
                    HalftoneErrorKind::InvalidPatternDictionaryFlags(0x81),
                )),
            )
            | (
                0,
                1,
                DecodeError::Malformed(MalformedError::InvalidHalftone(
                    HalftoneErrorKind::InvalidHalftoneFlags(7),
                )),
            )
            | (
                1,
                0,
                DecodeError::Malformed(MalformedError::InvalidSymbolText(
                    "reserved symbol-dictionary flags",
                )),
            )
            | (
                1,
                1,
                DecodeError::Malformed(MalformedError::InvalidSymbolText(
                    "reserved text Huffman flag",
                )),
            )
            | (0, 2, DecodeError::Malformed(MalformedError::InvalidSegmentReferences(2)))
            | (
                1,
                2,
                DecodeError::Malformed(MalformedError::InvalidHuffman(HuffmanErrorKind::NoCodes)),
            )
            | (0, 3, DecodeError::Malformed(MalformedError::TrailingData(1)))
            | (
                1,
                3,
                DecodeError::Malformed(MalformedError::InvalidSymbolText(
                    "Huffman text region did not exactly consume its segment data",
                )),
            ) => {}
            (_, _, error) => panic!(
                "family {} fault {fault}: unexpected typed error {error:?}",
                self.family
            ),
        }
    }

    pub fn check_limit(&self, selector: u8) {
        let mut limits = limits();
        let kind = match selector {
            0 => {
                limits.max_width = 0;
                LimitKind::Width
            }
            1 => {
                limits.max_height = 0;
                LimitKind::Height
            }
            2 => {
                limits.max_pixels_per_page = 0;
                LimitKind::PagePixels
            }
            3 => {
                limits.max_decoder_managed_memory_bytes = 0;
                LimitKind::Memory
            }
            4 => {
                limits.max_stream_bytes = 0;
                LimitKind::StreamBytes
            }
            5 => {
                limits.max_pages = 0;
                LimitKind::Pages
            }
            6 => {
                limits.max_segments = 0;
                LimitKind::Segments
            }
            7 => {
                limits.max_symbols = 0;
                LimitKind::Symbols
            }
            8 => {
                limits.max_references_per_segment = 0;
                LimitKind::ReferencesPerSegment
            }
            9 => {
                limits.max_total_references = 0;
                LimitKind::TotalReferences
            }
            10 => {
                limits.max_reference_graph_depth = 0;
                LimitKind::ReferenceDepth
            }
            11 => {
                limits.max_work_units = 0;
                LimitKind::Work
            }
            12 => {
                limits.max_cumulative_decoded_bitmap_bytes = 0;
                LimitKind::DecodedBitmapBytes
            }
            13 => {
                limits.max_retained_state_bytes = 0;
                LimitKind::RetainedState
            }
            _ => unreachable!("bounded limit selector"),
        };
        match Decoder::new(limits).decode(&self.document(false, None)) {
            Err(DecodeError::LimitExceeded(error)) => {
                assert_eq!(error.kind, kind);
                assert!(error.actual > error.limit);
            }
            outcome => panic!("family {} limit {selector}: {outcome:?}", self.family),
        }
    }
}

fn append_header(
    output: &mut Vec<u8>,
    number: u32,
    kind: u8,
    page: u8,
    referred: bool,
    length: usize,
) {
    output.extend_from_slice(&number.to_be_bytes());
    output.extend_from_slice(&[
        kind,
        if referred {
            0x21
        } else {
            u8::from(number == 0)
        },
    ]);
    if referred {
        output.push(0);
    }
    output.push(page);
    output.extend_from_slice(&u32::try_from(length).unwrap().to_be_bytes());
}

pub fn limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_width = 36;
    limits.max_height = 9;
    limits.max_pixels_per_page = 324;
    limits.max_decoder_managed_memory_bytes = 1 << 20;
    limits.max_stream_bytes = 4096;
    limits.max_pages = 1;
    limits.max_segments = 5;
    limits.max_symbols = 64;
    limits.max_references_per_segment = 1;
    limits.max_total_references = 1;
    limits.max_reference_graph_depth = 2;
    limits.max_nesting_depth = 2;
    limits.max_work_units = 1 << 18;
    limits.max_cumulative_decoded_bitmap_bytes = 4096;
    limits.max_retained_state_bytes = 1 << 16;
    limits
}

/// The deterministic manifest used by the seed example and ordinary tests.
pub fn seeds() -> Vec<(String, Vec<u8>)> {
    let mut seeds = Vec::new();
    for family in 0..2 {
        let shape = [family, 32, 8, 8, 2, 3, 8, 4, 4, 19, 7, 0, 6, 251, 7, 7];
        for (category, count, operation) in [
            ("opcode", OPERATIONS, None),
            ("fault", FAULTS, Some(5)),
            ("limit", LIMITS, Some(6)),
            ("count", 4, Some(0)),
        ] {
            for value in 0..count {
                let mut controls = shape.to_vec();
                if category == "count" {
                    controls[5] = value;
                }
                for _ in 0..4 {
                    controls.extend_from_slice(&[operation.unwrap_or(value), value]);
                }
                seeds.push((
                    format!("generated-codecs-v1-{family}-{category}-{value}"),
                    controls,
                ));
            }
        }
    }
    seeds
}

/// Sixteen zero-padded shape bytes, then at most four opcode/argument pairs.
/// No input byte is used as an unbounded length, dimension, count or allocation.
pub fn exercise(input: &[u8]) {
    let fixture = Fixture::new(input);
    let decoder = Decoder::new(limits());
    let mut context = decoder.context();
    let valid = fixture.document(false, None);
    fixture.check(&context.decode(&valid).expect("generated codec warmup"));
    for step in 0..4 {
        let operation = byte(input, SHAPE_BYTES + step * 2) % OPERATIONS;
        let argument = byte(input, SHAPE_BYTES + step * 2 + 1);
        match operation {
            0 => fixture.check(&context.decode(&valid).expect("reused generated codec")),
            1 => fixture.check(
                &decoder
                    .decode(&fixture.document(true, None))
                    .expect("random access codec"),
            ),
            2 => {
                let (global_bytes, embedded) = fixture.embedded();
                let globals = context
                    .parse_globals(&global_bytes)
                    .expect("generated global dictionary");
                fixture.check(
                    &context
                        .decode_embedded_with_globals(&embedded, &globals)
                        .expect("global codec"),
                );
                fixture.check(
                    &context
                        .decode_embedded_with_globals(&embedded, &globals)
                        .expect("reused global codec"),
                );
            }
            3 => fixture.check(
                &decoder
                    .decode_reader(&mut Cursor::new(&valid))
                    .expect("reader codec"),
            ),
            4 => {
                let mut pages = Vec::new();
                decoder
                    .decode_with_sink(&valid, &mut |page| {
                        pages.push(page);
                        Ok(())
                    })
                    .expect("sink codec");
                assert_eq!(pages.len(), 1);
                assert_eq!(pages[0].bitmap().bytes(), fixture.expected());
                assert_eq!(
                    (pages[0].bitmap().width(), pages[0].bitmap().height()),
                    (fixture.width, fixture.height)
                );
            }
            5 => {
                fixture.check_fault(argument % FAULTS);
                assert!(context
                    .decode(&fixture.document(false, Some(argument % FAULTS)))
                    .is_err());
                fixture.check(&context.decode(&valid).expect("codec error recovery"));
            }
            6 => fixture.check_limit(argument % LIMITS),
            7 => {
                let end = usize::from(argument) % valid.len();
                match context.decode(&valid[..end]) {
                    Ok(document) => fixture.check(&document),
                    Err(DecodeError::Malformed(_)) => {}
                    outcome => panic!("unexpected generated truncation outcome: {outcome:?}"),
                }
                fixture.check(&context.decode(&valid).expect("truncation recovery"));
            }
            _ => unreachable!("bounded opcode"),
        }
    }
}

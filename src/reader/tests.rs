#![cfg(test)]
#![allow(clippy::duplicated_attributes)]

use super::*;

#[test]
fn every_byte_position_and_width_has_exact_eof_fields_and_borrow_frame() {
    let bytes = [0x12, 0x34, 0x56, 0x78, 0x9a];
    for start in 0..=bytes.len() {
        for needed in (0..=8).chain([usize::MAX]) {
            let mut reader = ByteReader {
                bytes: &bytes,
                position: start,
            };
            if needed <= bytes.len() - start {
                let actual = reader.read_bytes(needed).expect("available bytes");
                assert_eq!(actual, &bytes[start..start + needed]);
                assert_eq!(actual.as_ptr(), bytes[start..].as_ptr());
                assert_eq!(reader.position(), start + needed);
            } else {
                assert_eq!(
                    reader.read_bytes(needed),
                    Err(MalformedError::UnexpectedEof {
                        offset: start as u64,
                        needed: needed as u64,
                        remaining: (bytes.len() - start) as u64,
                    })
                );
                assert_eq!(reader.position(), start);
            }
            assert_eq!(reader.bytes.as_ptr(), bytes.as_ptr());
            assert_eq!(reader.bytes, bytes);
        }
        for needed in [1, 2, 4] {
            let mut reader = ByteReader {
                bytes: &bytes,
                position: start,
            };
            let result = match needed {
                1 => reader.read_u8().map(u32::from),
                2 => reader.read_u16().map(u32::from),
                _ => reader.read_u32(),
            };
            if needed <= bytes.len() - start {
                let expected = bytes[start..start + needed]
                    .iter()
                    .fold(0_u64, |value, byte| value * 256 + u64::from(*byte));
                assert_eq!(result, Ok(u32::try_from(expected).unwrap()));
                assert_eq!(reader.position(), start + needed);
            } else {
                assert_eq!(
                    result,
                    Err(MalformedError::UnexpectedEof {
                        offset: start as u64,
                        needed: needed as u64,
                        remaining: (bytes.len() - start) as u64,
                    })
                );
                assert_eq!(reader.position(), start);
            }
            assert_eq!(reader.bytes.as_ptr(), bytes.as_ptr());
            assert_eq!(reader.bytes, bytes);
        }
    }
}

#[test]
fn failed_byte_read_preserves_position() {
    let mut reader = ByteReader::new(&[1, 2, 3]);
    assert!(reader.read_bytes(4).is_err());
    assert_eq!(reader.position(), 0);
    assert_eq!(
        reader.read_bytes(3).expect("three bytes remain"),
        &[1, 2, 3]
    );
}

#[test]
fn byte_reader_uses_big_endian_values() {
    let mut reader = ByteReader::new(&[0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);
    assert_eq!(reader.read_u32().expect("u32 is present"), 0x1234_5678);
    assert_eq!(reader.read_u16().expect("u16 is present"), 0x9ABC);
    assert_eq!(reader.remaining(), 0);
}

#[test]
fn bit_reader_crosses_byte_boundaries() {
    let mut reader = bit_reader(&[0b1011_0010, 0b0110_0000]);
    assert_eq!(reader.read_bits(3).expect("bits are present"), 0b101);
    assert_eq!(reader.read_bits(7).expect("bits are present"), 0b100_1001);
    assert_eq!(reader.bit_position(), 10);
}

#[test]
fn failed_bit_read_preserves_position() {
    let mut reader = bit_reader(&[0x80]);
    assert!(reader.read_bits(9).is_err());
    assert_eq!(reader.bit_position(), 0);
    assert_eq!(reader.read_bits(1).expect("one bit remains"), 1);
}

#[test]
fn every_bit_count_and_position_has_exact_value_error_and_frame() {
    let bytes = [0xA6, 0x39, 0xD0, 0xFF, 0x81];
    let packed = u64::from_be_bytes([0, 0, 0, 0xA6, 0x39, 0xD0, 0xFF, 0x81]);
    for start in 0..=40 {
        for count in 0..=u8::MAX {
            let mut reader = BitReader {
                bytes: &bytes,
                bit_position: start,
            };
            let result = reader.read_bits(count);
            if count > 32 {
                assert_eq!(result, Err(MalformedError::InvalidBitCount(count)));
                assert_eq!(reader.bit_position(), start);
            } else if usize::from(count) > 40 - start {
                assert_eq!(
                    result,
                    Err(MalformedError::UnexpectedEof {
                        offset: start as u64,
                        needed: u64::from(count),
                        remaining: (40 - start) as u64,
                    })
                );
                assert_eq!(reader.bit_position(), start);
            } else {
                let end = start + usize::from(count);
                let expected = (packed >> (40 - end)) & ((1_u64 << count) - 1);
                assert_eq!(result, Ok(u32::try_from(expected).unwrap()));
                assert_eq!(reader.bit_position(), end);
            }
            assert_eq!(reader.bytes, bytes);
        }
    }
    let mut empty = bit_reader(&[]);
    assert_eq!(empty.read_bits(0), Ok(0));
    assert_eq!(
        empty.read_bits(255),
        Err(MalformedError::InvalidBitCount(255))
    );
    assert_eq!(
        empty.read_bits(1),
        Err(MalformedError::UnexpectedEof {
            offset: 0,
            needed: 1,
            remaining: 0,
        })
    );
    assert_eq!(empty.bit_position(), 0);
}

#[test]
fn byte_reader_failures_and_empty_reads_preserve_exact_remainder() {
    let bytes = [0x12, 0x34, 0x56, 0x78, 0x9A];
    let mut reader = ByteReader::new(&bytes);
    assert_eq!(reader.read_u8(), Ok(0x12));
    assert_eq!(reader.read_bytes(0), Ok(&bytes[1..1]));
    assert_eq!(reader.read_u32(), Ok(0x3456_789A));
    for needed in [1, 2, 4, usize::MAX] {
        let error = match needed {
            1 => reader.read_u8().map(|_| ()),
            2 => reader.read_u16().map(|_| ()),
            4 => reader.read_u32().map(|_| ()),
            _ => reader.read_bytes(needed).map(|_| ()),
        };
        assert_eq!(
            error,
            Err(MalformedError::UnexpectedEof {
                offset: 5,
                needed: needed as u64,
                remaining: 0,
            })
        );
        assert_eq!(reader.position(), 5);
        assert_eq!(reader.remaining(), 0);
        assert_eq!(reader.remaining_bytes(), &[]);
        assert_eq!(reader.bytes, bytes);
    }
    assert_eq!(reader.read_bytes(0), Ok(&bytes[5..]));
}

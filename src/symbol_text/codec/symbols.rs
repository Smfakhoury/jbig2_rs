//! Symbols implementation.

#[cfg(verus_keep_ghost)]
use super::*;
#[cfg(not(verus_keep_ghost))]
use super::{
    enforce_limit, BitReader, DecodeError, HuffmanLine, HuffmanTable, HuffmanValue, LimitKind,
    Limits, MalformedError, ResourceTracker, SymbolCodec, SymbolIdRequest, SymbolLengthRun,
    SymbolTextError,
};

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires symbol_text_spec::work_units(old(resources)) <= limits.max_work_units,
    ensures match out {
        Ok(lengths) =>
            lengths@.len() == symbol_count
                && forall|index: int| 0 <= index < lengths@.len()
                    ==> #[trigger] lengths@[index] <= 31
                && symbol_text_spec::work_units(final(resources))
                    <= limits.max_work_units
                && symbol_text_spec::bitmap_bytes(final(resources))
                    == symbol_text_spec::bitmap_bytes(old(resources)),
        Err(_) => true,
    },
))]
pub(in crate::symbol_text) fn expand_symbol_code_lengths(
    symbol_count: u32,
    runs: &[SymbolLengthRun],
    limits: &Limits,
    resources: &mut ResourceTracker,
) -> Result<Vec<u8>, SymbolTextError> {
    enforce_limit(
        LimitKind::Symbols,
        u64::from(limits.max_symbols),
        u64::from(symbol_count),
    )?;
    let capacity = usize::try_from(symbol_count).map_err(|_| MalformedError::IntegerOverflow)?;
    let mut lengths: Vec<u8> = Vec::new();
    lengths
        .try_reserve(capacity)
        .map_err(|_| DecodeError::Allocation {
            requested_bytes: u64::from(symbol_count),
        })?;
    let mut run_index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            run_index <= runs@.len(),
            lengths@.len() <= capacity,
            symbol_text_spec::work_units(resources) <= limits.max_work_units,
            symbol_text_spec::bitmap_bytes(resources)
                == symbol_text_spec::bitmap_bytes(old(resources)),
            forall|index: int| 0 <= index < lengths@.len()
                ==> #[trigger] lengths@[index] <= 31,
        decreases runs@.len() - run_index,
    ))]
    while run_index < runs.len() {
        let run = runs[run_index];
        if lengths.len() == capacity {
            return Err(SymbolTextError::InvalidSymbolLengthRun);
        }
        resources.charge_work(1, limits)?;
        match run {
            SymbolLengthRun::Literal(length) if length <= 31 => {
                lengths.push(length);
            }
            SymbolLengthRun::Literal(_) => return Err(SymbolTextError::InvalidSymbolLengthRun),
            SymbolLengthRun::RepeatPrevious(count) if (3..=6).contains(&count) => {
                let previous = *lengths
                    .last()
                    .ok_or(SymbolTextError::InvalidSymbolLengthRun)?;
                extend_lengths(&mut lengths, capacity, previous, usize::from(count))?;
            }
            SymbolLengthRun::Zeros(count) if (3..=138).contains(&count) => {
                extend_lengths(&mut lengths, capacity, 0, usize::from(count))?;
            }
            SymbolLengthRun::RepeatPrevious(_) | SymbolLengthRun::Zeros(_) => {
                return Err(SymbolTextError::InvalidSymbolLengthRun)
            }
        }
        run_index += 1;
    }
    if lengths.len() != capacity {
        return Err(SymbolTextError::InvalidCount(
            "symbol code length count mismatch",
        ));
    }
    resources.charge_work(u64::from(symbol_count), limits)?;
    Ok(lengths)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        old(lengths)@.len() <= capacity,
        value <= 31,
        forall|i: int| 0 <= i < old(lengths)@.len()
            ==> #[trigger] old(lengths)@[i] <= 31,
    ensures match out {
        Ok(()) =>
            final(lengths)@.len() == old(lengths)@.len() + count
            && final(lengths)@.len() <= capacity
            && final(lengths)@.subrange(0, old(lengths)@.len() as int)
                == old(lengths)@
            && (forall|i: int| old(lengths)@.len() <= i < final(lengths)@.len()
                ==> final(lengths)@[i] == value)
            && (forall|i: int| 0 <= i < final(lengths)@.len()
                ==> #[trigger] final(lengths)@[i] <= 31),
        Err(_) => true,
    },
))]
pub(in crate::symbol_text) fn extend_lengths(
    lengths: &mut Vec<u8>,
    capacity: usize,
    value: u8,
    count: usize,
) -> Result<(), SymbolTextError> {
    #[cfg(verus_keep_ghost)]
    proof_decl! {
        let ghost original = lengths@;
    }
    let end = lengths
        .len()
        .checked_add(count)
        .ok_or(MalformedError::IntegerOverflow)?;
    if end > capacity {
        return Err(SymbolTextError::InvalidSymbolLengthRun);
    }
    lengths.resize(end, value);
    #[cfg(verus_keep_ghost)]
    proof! {
        assert forall|i: int| 0 <= i < lengths@.len()
            implies #[trigger] lengths@[i] <= 31 by {
            if i < original.len() {
                assert(lengths@[i] == original[i]);
            } else {
                assert(lengths@[i] == value);
            }
        }
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures
        out == symbol_text_spec::checked_symbol_id_result(id, available_symbols),
        match out {
        Ok(value) => value == id && value < available_symbols,
        Err(_) => id >= available_symbols,
    },
))]
pub(crate) fn checked_symbol_id(id: u32, available_symbols: u32) -> Result<u32, SymbolTextError> {
    if id >= available_symbols {
        return Err(SymbolTextError::SymbolIdOutOfRange {
            id,
            available: available_symbols,
        });
    }
    Ok(id)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires symbol_text_spec::codec_valid(old(codec)),
    ensures
        symbol_text_spec::checked_symbol_id_transition(old(codec), final(codec), request, out),
        match out {
        Ok(id) =>
            symbol_text_spec::codec_valid(final(codec))
                && id < request.available_symbols,
        Err(_) => true,
    },
))]
pub(in crate::symbol_text) fn decode_checked_symbol_id(
    codec: &mut SymbolCodec<'_>,
    request: SymbolIdRequest,
) -> Result<u32, SymbolTextError> {
    if request.available_symbols == 0 {
        return Err(SymbolTextError::InvalidCount(
            "symbol ID with no available symbols",
        ));
    }
    let id = codec.decode_symbol_id(request)?;
    checked_symbol_id(id, request.available_symbols)
}

// ---------------------------------------------------------------------------
// Shared symbol code length.
// ---------------------------------------------------------------------------

/// The smallest `k` with `2^k >= symbol_count.max(1)` (T.88 "SBSYMCODELEN").
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(length) =>
            length == symbol_text_spec::symbol_code_length_expected(symbol_count)
                && symbol_text_spec::symbol_code_length_bounds(symbol_count, length)
                && (forall|smaller: nat| smaller < length ==>
                    #[trigger] symbol_text_spec::pow2(smaller) < symbol_count as nat),
        Err(_) => false,
    },
))]
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn symbol_id_code_length(symbol_count: u32) -> Result<u8, SymbolTextError> {
    let length = if symbol_count <= 1 {
        0
    } else if symbol_count <= 2 {
        1
    } else if symbol_count <= 4 {
        2
    } else if symbol_count <= 8 {
        3
    } else if symbol_count <= 16 {
        4
    } else if symbol_count <= 32 {
        5
    } else if symbol_count <= 64 {
        6
    } else if symbol_count <= 128 {
        7
    } else if symbol_count <= 256 {
        8
    } else if symbol_count <= 512 {
        9
    } else if symbol_count <= 1_024 {
        10
    } else if symbol_count <= 2_048 {
        11
    } else if symbol_count <= 4_096 {
        12
    } else if symbol_count <= 8_192 {
        13
    } else if symbol_count <= 16_384 {
        14
    } else if symbol_count <= 32_768 {
        15
    } else if symbol_count <= 65_536 {
        16
    } else if symbol_count <= 131_072 {
        17
    } else if symbol_count <= 262_144 {
        18
    } else if symbol_count <= 524_288 {
        19
    } else if symbol_count <= 1_048_576 {
        20
    } else if symbol_count <= 2_097_152 {
        21
    } else if symbol_count <= 4_194_304 {
        22
    } else if symbol_count <= 8_388_608 {
        23
    } else if symbol_count <= 16_777_216 {
        24
    } else if symbol_count <= 33_554_432 {
        25
    } else if symbol_count <= 67_108_864 {
        26
    } else if symbol_count <= 134_217_728 {
        27
    } else if symbol_count <= 268_435_456 {
        28
    } else if symbol_count <= 536_870_912 {
        29
    } else if symbol_count <= 1_073_741_824 {
        30
    } else if symbol_count <= 2_147_483_648 {
        31
    } else {
        32
    };
    #[cfg(verus_keep_ghost)]
    production_symbol_code_length_bounds!(symbol_count);
    Ok(length)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires crate::huffman::huffman_spec::reader_valid(old(reader)),
    ensures crate::huffman::huffman_spec::reader_valid(final(reader)),
))]
pub(in crate::symbol_text) fn read_symbol_id_runcode_lines(
    reader: &mut BitReader<'_>,
) -> Result<[HuffmanLine; 35], SymbolTextError> {
    const RUNCODE_COUNT: usize = 35;
    let mut runcode_lines = [HuffmanLine::normal(0, 0, 0); RUNCODE_COUNT];
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index <= RUNCODE_COUNT,
            crate::huffman::huffman_spec::reader_valid(reader),
        decreases RUNCODE_COUNT - index,
    ))]
    while index < RUNCODE_COUNT {
        let prefix_len =
            u8::try_from(reader.read_bits(4)?).map_err(|_| MalformedError::IntegerOverflow)?;
        let value = i32::try_from(index).map_err(|_| MalformedError::IntegerOverflow)?;
        runcode_lines[index] = HuffmanLine::normal(value, prefix_len, 0);
        index += 1;
    }
    Ok(runcode_lines)
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires crate::huffman::huffman_spec::reader_valid(old(reader)),
    ensures crate::huffman::huffman_spec::reader_valid(final(reader)),
))]
pub(in crate::symbol_text) fn decode_symbol_id_code_lengths(
    reader: &mut BitReader<'_>,
    runcode_table: &HuffmanTable,
    symbol_count: u32,
    limits: &Limits,
) -> Result<Vec<u8>, SymbolTextError> {
    let capacity = usize::try_from(symbol_count).map_err(|_| MalformedError::IntegerOverflow)?;
    let mut lengths: Vec<u8> = Vec::new();
    lengths
        .try_reserve(capacity)
        .map_err(|_| DecodeError::Allocation {
            requested_bytes: u64::from(symbol_count),
        })?;
    let mut resources = ResourceTracker {
        work_units: 0,
        decoded_bitmap_bytes: 0,
    };
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            lengths@.len() <= capacity,
            forall|index: int| 0 <= index < lengths@.len()
                ==> #[trigger] lengths@[index] <= 31,
            symbol_text_spec::work_units(&resources) <= limits.max_work_units,
            crate::huffman::huffman_spec::reader_valid(reader),
        decreases capacity - lengths@.len(),
    ))]
    while lengths.len() < capacity {
        resources.charge_work(1, limits)?;
        let code = match runcode_table.decode(reader)? {
            HuffmanValue::Integer(value) => value,
            HuffmanValue::OutOfBand => {
                return Err(SymbolTextError::UnexpectedOutOfBand("symbol ID run code"))
            }
        };
        match code {
            0..=31 => {
                let length = u8::try_from(code).map_err(|_| MalformedError::IntegerOverflow)?;
                lengths.push(length);
            }
            32 => {
                let count = u8::try_from(
                    reader
                        .read_bits(2)?
                        .checked_add(3)
                        .ok_or(MalformedError::IntegerOverflow)?,
                )
                .map_err(|_| MalformedError::IntegerOverflow)?;
                let previous = *lengths
                    .last()
                    .ok_or(SymbolTextError::InvalidSymbolLengthRun)?;
                extend_lengths(&mut lengths, capacity, previous, usize::from(count))?;
            }
            33 => {
                let count = u16::try_from(
                    reader
                        .read_bits(3)?
                        .checked_add(3)
                        .ok_or(MalformedError::IntegerOverflow)?,
                )
                .map_err(|_| MalformedError::IntegerOverflow)?;
                extend_lengths(&mut lengths, capacity, 0, usize::from(count))?;
            }
            34 => {
                let count = u16::try_from(
                    reader
                        .read_bits(7)?
                        .checked_add(11)
                        .ok_or(MalformedError::IntegerOverflow)?,
                )
                .map_err(|_| MalformedError::IntegerOverflow)?;
                extend_lengths(&mut lengths, capacity, 0, usize::from(count))?;
            }
            _ => return Err(SymbolTextError::InvalidSymbolLengthRun),
        }
    }
    resources.charge_work(u64::from(symbol_count), limits)?;
    Ok(lengths)
}

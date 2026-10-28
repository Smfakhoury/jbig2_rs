//! Arithmetic and Huffman entropy adapters, symbol IDs and refinement geometry.

#[cfg(verus_keep_ghost)]
use super::symbol_text_spec;
use super::{
    bit_reader, check_dimensions, checked_unsigned_delta, decode_generic_on, decode_refinement_on,
    enforce_limit, required_integer, widen_bool_i64, ArithmeticCodec, ArithmeticContext,
    ArithmeticDecoder, ArithmeticInteger, BitReader, Bitmap, DecodeError, EntropyInteger,
    GenericArithmeticCall, GenericContexts, HuffmanCodec, HuffmanFieldTables, HuffmanLine,
    HuffmanTable, HuffmanValue, IntegerContexts, IntegerField, LimitKind, Limits, MalformedError,
    NormalizedArithmetic, RefinementArithmeticCall, RefinementContexts, RefinementGeometry,
    RefinementParameters, RefinementRequest, RefinementTemplate, ResourceTracker, SymbolCodec,
    SymbolIdRequest, SymbolLengthRun, SymbolTextError, TemplateOffset,
    BOUNDED_ARITHMETIC_TERMINATOR, INTEGER_FIELD_COUNT,
};
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

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
pub(super) fn expand_symbol_code_lengths(
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
pub(super) fn extend_lengths(
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
    ensures match out {
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
    ensures match out {
        Ok(id) =>
            symbol_text_spec::codec_valid(final(codec))
                && id < request.available_symbols,
        Err(_) => true,
    },
))]
pub(super) fn decode_checked_symbol_id(
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

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(geometry) =>
            geometry.width as int == base_width as int + delta_width as int
            && geometry.height as int == base_height as int + delta_height as int
            && geometry.width <= limits.max_width
            && geometry.height <= limits.max_height
            && geometry.reference_dx as int
                == symbol_text_spec::floor_half(delta_width as int) + delta_x as int
            && geometry.reference_dy as int
                == symbol_text_spec::floor_half(delta_height as int) + delta_y as int,
        Err(_) => true,
    },
))]
pub(crate) fn refinement_geometry(
    base_width: u32,
    base_height: u32,
    delta_width: i64,
    delta_height: i64,
    delta_x: i64,
    delta_y: i64,
    limits: &Limits,
) -> Result<RefinementGeometry, SymbolTextError> {
    let width = checked_unsigned_delta(base_width, delta_width, limits.max_width)?;
    let height = checked_unsigned_delta(base_height, delta_height, limits.max_height)?;
    check_dimensions(width, height, limits)?;
    let reference_x_offset = floor_divide_by_two(delta_width)
        .checked_add(delta_x)
        .ok_or(MalformedError::IntegerOverflow)?;
    let reference_y_offset = floor_divide_by_two(delta_height)
        .checked_add(delta_y)
        .ok_or(MalformedError::IntegerOverflow)?;
    Ok(RefinementGeometry {
        width,
        height,
        reference_dx: i32::try_from(reference_x_offset)
            .map_err(|_error| MalformedError::IntegerOverflow)?,
        reference_dy: i32::try_from(reference_y_offset)
            .map_err(|_error| MalformedError::IntegerOverflow)?,
    })
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures out as int == symbol_text_spec::floor_half(value as int),
))]
pub(super) fn floor_divide_by_two(value: i64) -> i64 {
    value / 2 - widen_bool_i64(value < 0 && value % 2 != 0)
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        symbol_text_spec::codec_valid(old(codec)),
        crate::spec::bitmap::storage_valid(reference),
    ensures match out {
        Ok(bitmap) =>
            symbol_text_spec::codec_valid(final(codec))
                && crate::spec::bitmap::storage_valid(&bitmap)
                && symbol_text_spec::work_units(final(resources))
                    == symbol_text_spec::work_units(old(resources)),
        Err(_) => true,
    },
))]
pub(super) fn decode_refined_bitmap(
    codec: &mut SymbolCodec<'_>,
    reference: &Bitmap,
    template: u8,
    adaptive_pixels: [(i8, i8); 2],
    huffman: bool,
    limits: &Limits,
    resources: &mut ResourceTracker,
) -> Result<Bitmap, SymbolTextError> {
    let delta_width = required_integer(codec, IntegerField::RefinementWidth)?;
    let delta_height = required_integer(codec, IntegerField::RefinementHeight)?;
    let delta_x = required_integer(codec, IntegerField::RefinementX)?;
    let delta_y = required_integer(codec, IntegerField::RefinementY)?;
    let declared_data_size = if huffman {
        let size = required_integer(codec, IntegerField::RefinementSize)?;
        Some(
            u32::try_from(size)
                .map_err(|_| SymbolTextError::InvalidEntropyValue("refinement size"))?,
        )
    } else {
        None
    };
    let geometry = refinement_geometry(
        reference.width(),
        reference.height(),
        delta_width,
        delta_height,
        delta_x,
        delta_y,
        limits,
    )?;
    resources.charge_bitmap(geometry.width, geometry.height, limits)?;
    let request = RefinementRequest {
        reference,
        geometry,
        template,
        adaptive_pixels,
        declared_data_size,
    };
    let bitmap = codec.decode_refinement(request, limits)?;
    if bitmap.width() != geometry.width || bitmap.height() != geometry.height {
        return Err(SymbolTextError::InvalidDimensions);
    }
    Ok(bitmap)
}

// ---------------------------------------------------------------------------
// Shared symbol code length.
// ---------------------------------------------------------------------------

/// The smallest `k` with `2^k >= symbol_count.max(1)` (T.88 "SBSYMCODELEN").
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(length) => length == symbol_text_spec::symbol_code_length_expected(symbol_count),
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
    Ok(length)
}

impl IntegerContexts {
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub(super) fn new() -> Self {
        Self {
            slots: [[ArithmeticContext::new(); 512]; INTEGER_FIELD_COUNT],
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(codec) => symbol_text_spec::arithmetic_codec_valid(&codec),
        Err(_) => true,
    },
))]
pub(super) fn new_arithmetic_codec<'a>(
    bytes: &'a [u8],
    iaid_code_length: u8,
    refinement_template: Option<RefinementTemplate>,
    limits: &Limits,
) -> Result<ArithmeticCodec<'a>, SymbolTextError> {
    if u32::from(iaid_code_length) >= usize::BITS {
        return Err(MalformedError::IntegerOverflow.into());
    }
    let count = 1_usize << iaid_code_length;
    let requested_bytes = count
        .checked_mul(core::mem::size_of::<ArithmeticContext>())
        .and_then(|bytes| u64::try_from(bytes).ok())
        .ok_or(MalformedError::IntegerOverflow)?;
    enforce_limit(
        LimitKind::Memory,
        limits.max_decoder_managed_memory_bytes,
        requested_bytes,
    )?;
    let mut iaid = Vec::new();
    iaid.try_reserve(count)
        .map_err(|_| DecodeError::Allocation { requested_bytes })?;
    iaid.resize(count, ArithmeticContext::new());
    Ok(ArithmeticCodec {
        decoder: ArithmeticDecoder::new(bytes)?,
        integers: IntegerContexts {
            slots: [[ArithmeticContext::new(); 512]; INTEGER_FIELD_COUNT],
        },
        iaid,
        refinement: match refinement_template {
            Some(template) => Some(RefinementContexts::new(template, limits)?),
            None => None,
        },
        refinement_work: 0,
    })
}

impl ArithmeticCodec<'_> {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires crate::arithmetic::arithmetic_spec::decoder_valid(&old(self).decoder),
            symbol_text_spec::integer_contexts_valid(&old(self).integers),
        ensures
            crate::arithmetic::arithmetic_spec::decoder_state_valid(&final(self).decoder),
            symbol_text_spec::integer_contexts_valid(&final(self).integers),
            final(self).iaid@ == old(self).iaid@,
            final(self).refinement == old(self).refinement,
            final(self).refinement_work == old(self).refinement_work,
            out == symbol_text_spec::arithmetic_integer_result(
                crate::arithmetic::arithmetic_spec::integer_state(
                    old(self).decoder,
                    old(self).integers.slots[symbol_text_spec::integer_field_slot(field) as int]@,
                ).0,
            ),
            final(self).decoder == crate::arithmetic::arithmetic_spec::integer_state(
                old(self).decoder,
                old(self).integers.slots[symbol_text_spec::integer_field_slot(field) as int]@,
            ).1,
            final(self).integers.slots[symbol_text_spec::integer_field_slot(field) as int]@
                == crate::arithmetic::arithmetic_spec::integer_state(
                    old(self).decoder,
                    old(self).integers.slots[symbol_text_spec::integer_field_slot(field) as int]@,
                ).2,
            forall|slot: int| 0 <= slot < INTEGER_FIELD_COUNT
                && slot != symbol_text_spec::integer_field_slot(field) ==>
                    final(self).integers.slots[slot]@ == old(self).integers.slots[slot]@,
            match out {
                Ok(_) => crate::arithmetic::arithmetic_spec::decoder_valid(
                    &final(self).decoder
                ),
                Err(_) => true,
            },
    ))]
    pub(super) fn decode_integer(
        &mut self,
        field: IntegerField,
    ) -> Result<EntropyInteger, SymbolTextError> {
        let slot = field.slot();
        let decoded = match self.decoder.decode_integer(&mut self.integers.slots[slot]) {
            Ok(decoded) => decoded,
            Err(error) => return Err(SymbolTextError::Decode(error.into_decode())),
        };
        match decoded {
            ArithmeticInteger::Value(value) => Ok(EntropyInteger::Value(value)),
            ArithmeticInteger::OutOfBand => Ok(EntropyInteger::OutOfBand),
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            crate::arithmetic::arithmetic_spec::decoder_valid(&old(self).decoder),
            symbol_text_spec::arithmetic_contexts_valid(old(self).iaid@),
        ensures
            crate::arithmetic::arithmetic_spec::decoder_state_valid(&final(self).decoder),
            symbol_text_spec::arithmetic_contexts_valid(final(self).iaid@),
            final(self).integers.slots@ == old(self).integers.slots@,
            final(self).refinement == old(self).refinement,
            final(self).refinement_work == old(self).refinement_work,
            out == symbol_text_spec::arithmetic_symbol_id_result(
                crate::arithmetic::arithmetic_spec::iaid_state(
                    old(self).decoder, old(self).iaid@, request.code_length,
                ).0,
            ),
            final(self).decoder == crate::arithmetic::arithmetic_spec::iaid_state(
                old(self).decoder, old(self).iaid@, request.code_length,
            ).1,
            final(self).iaid@ == crate::arithmetic::arithmetic_spec::iaid_state(
                old(self).decoder, old(self).iaid@, request.code_length,
            ).2,
            match out {
                Ok(_) => crate::arithmetic::arithmetic_spec::decoder_valid(
                    &final(self).decoder
                ),
                Err(_) => true,
            },
    ))]
    pub(super) fn decode_symbol_id(
        &mut self,
        request: SymbolIdRequest,
    ) -> Result<u32, SymbolTextError> {
        match self
            .decoder
            .decode_iaid(request.code_length, &mut self.iaid)
        {
            Ok(value) => Ok(value),
            Err(error) => Err(SymbolTextError::Decode(error.into_decode())),
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            crate::arithmetic::arithmetic_spec::decoder_valid(&old(self).decoder),
            crate::spec::bitmap::storage_valid(request.reference),
        ensures match out {
            Ok(bitmap) =>
                crate::spec::bitmap::storage_valid(&bitmap)
                    && crate::arithmetic::arithmetic_spec::decoder_valid(
                        &final(self).decoder
                    )
                    && final(self).integers.slots@ == old(self).integers.slots@
                    && final(self).iaid@ == old(self).iaid@,
            Err(_) => true,
        },
    ))]
    pub(super) fn decode_refinement(
        &mut self,
        request: RefinementRequest<'_>,
        limits: &Limits,
    ) -> Result<Bitmap, SymbolTextError> {
        let adaptive = [
            TemplateOffset::new(
                i32::from(request.adaptive_pixels[0].0),
                i32::from(request.adaptive_pixels[0].1),
            ),
            TemplateOffset::new(
                i32::from(request.adaptive_pixels[1].0),
                i32::from(request.adaptive_pixels[1].1),
            ),
        ];
        let call = RefinementArithmeticCall {
            width: request.geometry.width,
            height: request.geometry.height,
            adaptive,
            typical_prediction: false,
        };
        let parameters = RefinementParameters {
            reference_dx: request.geometry.reference_dx,
            reference_dy: request.geometry.reference_dy,
        };
        let contexts = self
            .refinement
            .as_mut()
            .ok_or(SymbolTextError::InvalidState(
                "missing arithmetic refinement contexts",
            ))?;
        Ok(decode_refinement_on(
            &mut self.decoder,
            contexts,
            call,
            request.reference,
            parameters,
            limits,
            &mut self.refinement_work,
        )?)
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            crate::arithmetic::arithmetic_spec::decoder_valid(&old(self).decoder),
            match call.skip {
                Some(mask) => crate::spec::bitmap::storage_valid(mask),
                None => true,
            },
        ensures match out {
            Ok(bitmap) =>
                crate::spec::bitmap::storage_valid(&bitmap)
                    && crate::arithmetic::arithmetic_spec::decoder_valid(
                        &final(self).decoder
                    )
                    && final(self).integers.slots@ == old(self).integers.slots@
                    && final(self).iaid@ == old(self).iaid@,
            Err(_) => true,
        },
    ))]
    pub(super) fn decode_generic(
        &mut self,
        contexts: &mut GenericContexts,
        call: GenericArithmeticCall<'_>,
        height: u32,
        limits: &Limits,
        cumulative_work: &mut u64,
    ) -> Result<Bitmap, SymbolTextError> {
        Ok(decode_generic_on(
            &mut self.decoder,
            contexts,
            call,
            height,
            limits,
            cumulative_work,
        )?)
    }
}

/// Validates that a bounded arithmetic slice ends in `FF AC` and contains no
/// earlier marker, mirroring `generic_region`'s equivalent (private) check
/// for the Huffman-declared-size refinement bitmaps embedded in symbol
/// dictionaries and text regions (T.88 6.5.8.2.2, 6.4.11).
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(()) =>
            data@.len() >= 2
            && data@[data@.len() - 2] == 0xff
            && data@[data@.len() - 1] == 0xac
            && forall|i: int| 0 <= i < data@.len() - 2 ==> !(
                #[trigger] data@[i] == 0xff && data@[i + 1] > 0x8f
            ),
        Err(_) => true,
    },
))]
pub(super) fn validate_bounded_arithmetic_terminator(data: &[u8]) -> Result<(), SymbolTextError> {
    if data.len() < 2
        || data[data.len() - 2] != BOUNDED_ARITHMETIC_TERMINATOR[0]
        || data[data.len() - 1] != BOUNDED_ARITHMETIC_TERMINATOR[1]
    {
        return Err(SymbolTextError::InvalidState(
            "embedded arithmetic refinement data is missing its FF AC terminator",
        ));
    }

    let final_offset = data
        .len()
        .checked_sub(2)
        .ok_or(MalformedError::IntegerOverflow)?;
    let mut offset = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            final_offset == data@.len() - 2,
            offset <= data@.len() - 1,
            forall|i: int| 0 <= i < offset ==> (
                !(#[trigger] data@[i] == 0xff && data@[i + 1] > 0x8f)
                || i == final_offset
            ),
        decreases data@.len() - 1 - offset,
    ))]
    while offset < data.len() - 1 {
        if data[offset] == 0xff && data[offset + 1] > 0x8f && offset != final_offset {
            return Err(SymbolTextError::InvalidState(
                "premature arithmetic marker in embedded refinement data",
            ));
        }
        offset += 1;
    }
    Ok(())
}

impl NormalizedArithmetic<'_> {
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub(super) fn bytes(&self) -> &[u8] {
        match self {
            Self::Borrowed(bytes) => bytes,
            Self::Owned(bytes) => bytes,
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub(super) fn len(&self) -> usize {
        self.bytes().len()
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(super) fn normalized_bounded_arithmetic<'a>(
    data: &'a [u8],
    limits: &Limits,
) -> Result<NormalizedArithmetic<'a>, SymbolTextError> {
    if data.ends_with(&BOUNDED_ARITHMETIC_TERMINATOR) {
        validate_bounded_arithmetic_terminator(data)?;
        return Ok(NormalizedArithmetic::Borrowed(data));
    }
    if data.last() != Some(&BOUNDED_ARITHMETIC_TERMINATOR[0]) {
        return Err(SymbolTextError::InvalidState(
            "bounded arithmetic data has no terminal marker",
        ));
    }
    let requested_bytes = u64::try_from(data.len())
        .map_err(|_| MalformedError::IntegerOverflow)?
        .checked_add(1)
        .ok_or(MalformedError::IntegerOverflow)?;
    enforce_limit(
        LimitKind::Memory,
        limits.max_decoder_managed_memory_bytes,
        requested_bytes,
    )?;
    let mut normalized = Vec::new();
    normalized
        .try_reserve(
            data.len()
                .checked_add(1)
                .ok_or(MalformedError::IntegerOverflow)?,
        )
        .map_err(|_| DecodeError::Allocation { requested_bytes })?;
    normalized.extend_from_slice(data);
    normalized.push(BOUNDED_ARITHMETIC_TERMINATOR[1]);
    validate_bounded_arithmetic_terminator(&normalized)?;
    Ok(NormalizedArithmetic::Owned(normalized))
}

/// Validates that any marker the decoder actually observed is the declared
/// final `FF AC`, after a successful bounded decode.
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(super) fn validate_bounded_arithmetic_marker(
    decoder: &ArithmeticDecoder<'_>,
    data_len: usize,
) -> Result<(), SymbolTextError> {
    if let Some(marker) = decoder.marker() {
        let expected_offset = data_len
            .checked_sub(2)
            .ok_or(MalformedError::IntegerOverflow)?;
        if marker.code != BOUNDED_ARITHMETIC_TERMINATOR[1] || marker.offset != expected_offset {
            return Err(SymbolTextError::InvalidState(
                "unexpected arithmetic marker in embedded refinement data",
            ));
        }
    }
    Ok(())
}

/// Rounds a bit reader forward to the next byte boundary.
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires old(reader).bit_position <= old(reader).bytes@.len() * 8,
    ensures
        final(reader).bytes@ == old(reader).bytes@,
        match out {
            Ok(()) =>
                final(reader).bit_position >= old(reader).bit_position
                && final(reader).bit_position <= final(reader).bytes@.len() * 8
                && final(reader).bit_position % 8 == 0
                && final(reader).bit_position < old(reader).bit_position + 8,
            Err(_) => final(reader).bit_position == old(reader).bit_position,
        },
))]
pub(super) fn byte_align(reader: &mut BitReader<'_>) -> Result<(), SymbolTextError> {
    let total_bits = reader
        .bytes
        .len()
        .checked_mul(8)
        .ok_or(MalformedError::IntegerOverflow)?;
    let aligned = reader
        .bit_position
        .checked_add(7)
        .ok_or(MalformedError::IntegerOverflow)?
        / 8
        * 8;
    if aligned > total_bits {
        let needed = aligned
            .checked_sub(reader.bit_position)
            .and_then(|bits| u64::try_from(bits).ok())
            .ok_or(MalformedError::IntegerOverflow)?;
        let remaining = total_bits
            .checked_sub(reader.bit_position)
            .and_then(|bits| u64::try_from(bits).ok())
            .ok_or(MalformedError::IntegerOverflow)?;
        return Err(MalformedError::UnexpectedEof {
            offset: u64::try_from(reader.bit_position)
                .map_err(|_error| MalformedError::IntegerOverflow)?,
            needed,
            remaining,
        }
        .into());
    }
    reader.bit_position = aligned;
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires crate::huffman::huffman_spec::reader_valid(old(reader)),
    ensures crate::huffman::huffman_spec::reader_valid(final(reader)),
))]
pub(super) fn read_symbol_id_runcode_lines(
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
pub(super) fn decode_symbol_id_code_lengths(
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

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    ensures match out {
        Ok(codec) => crate::huffman::huffman_spec::reader_valid(&codec.reader),
        Err(_) => true,
    },
))]
pub(super) fn new_huffman_codec<'a>(
    bytes: &'a [u8],
    tables: HuffmanFieldTables<'a>,
    log_strip_size: u8,
    refinement_template: Option<RefinementTemplate>,
    limits: &Limits,
) -> Result<HuffmanCodec<'a>, SymbolTextError> {
    if bytes.len() > usize::MAX / 8 {
        return Err(MalformedError::IntegerOverflow.into());
    }
    Ok(HuffmanCodec {
        reader: bit_reader(bytes),
        tables,
        log_strip_size,
        symbol_id_table: None,
        refinement: match refinement_template {
            Some(template) => Some(RefinementContexts::new(template, limits)?),
            None => None,
        },
        refinement_work: 0,
    })
}

impl HuffmanCodec<'_> {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires crate::huffman::huffman_spec::reader_valid(&old(self).reader),
        ensures crate::huffman::huffman_spec::reader_valid(&final(self).reader),
    ))]
    pub(super) fn read_raw_bits(&mut self, count: u8) -> Result<u32, SymbolTextError> {
        Ok(self.reader.read_bits(count)?)
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires crate::huffman::huffman_spec::reader_valid(&old(self).reader),
        ensures crate::huffman::huffman_spec::reader_valid(&final(self).reader),
    ))]
    pub(super) fn decode_integer(
        &mut self,
        field: IntegerField,
    ) -> Result<EntropyInteger, SymbolTextError> {
        match field {
            IntegerField::WithinStripT => Ok(EntropyInteger::Value(i64::from(
                self.read_raw_bits(self.log_strip_size)?,
            ))),
            IntegerField::RefinementIndicator => {
                Ok(EntropyInteger::Value(i64::from(self.read_raw_bits(1)?)))
            }
            _ => {
                let table = self.tables.table_for(field).ok_or(
                    SymbolTextError::InvalidTableAssociation("no Huffman table bound for field"),
                )?;
                match table.decode(&mut self.reader)? {
                    HuffmanValue::Integer(value) => Ok(EntropyInteger::Value(i64::from(value))),
                    HuffmanValue::OutOfBand => Ok(EntropyInteger::OutOfBand),
                }
            }
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires crate::huffman::huffman_spec::reader_valid(&old(self).reader),
        ensures crate::huffman::huffman_spec::reader_valid(&final(self).reader),
    ))]
    pub(super) fn decode_symbol_id(
        &mut self,
        request: SymbolIdRequest,
    ) -> Result<u32, SymbolTextError> {
        // When a symbol-ID Huffman table has been prepared (T.88 7.4.3.1.7,
        // normal text-region decoding), decode through it. Otherwise this is
        // the symbol dictionary's own single-instance aggregate refinement ID
        // (T.88 6.5.8.2.2), which T.88 specifies as a literal fixed-width
        // field of `code_length` bits, not a Huffman code.
        if let Some(table) = self.symbol_id_table.as_ref() {
            match table.decode(&mut self.reader)? {
                HuffmanValue::Integer(value) => u32::try_from(value)
                    .map_err(|_| SymbolTextError::InvalidEntropyValue("symbol ID")),
                HuffmanValue::OutOfBand => Err(SymbolTextError::UnexpectedOutOfBand("symbol ID")),
            }
        } else {
            self.read_raw_bits(request.code_length)
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            crate::huffman::huffman_spec::reader_valid(&old(self).reader),
            crate::spec::bitmap::storage_valid(request.reference),
        ensures
            crate::huffman::huffman_spec::reader_valid(&final(self).reader),
            match out {
                Ok(bitmap) => crate::spec::bitmap::storage_valid(&bitmap),
                Err(_) => true,
            },
    ))]
    pub(super) fn decode_refinement(
        &mut self,
        request: RefinementRequest<'_>,
        limits: &Limits,
    ) -> Result<Bitmap, SymbolTextError> {
        let declared_size = request
            .declared_data_size
            .ok_or(SymbolTextError::InvalidState(
                "Huffman refinement bitmap is missing its declared byte size",
            ))?;
        byte_align(&mut self.reader)?;
        let byte_offset = self
            .reader
            .bit_position
            .checked_div(8)
            .ok_or(MalformedError::IntegerOverflow)?;
        let size = usize::try_from(declared_size).map_err(|_| MalformedError::IntegerOverflow)?;
        let end = byte_offset
            .checked_add(size)
            .ok_or(MalformedError::IntegerOverflow)?;
        let remaining = if byte_offset <= self.reader.bytes.len() {
            self.reader.bytes.len() - byte_offset
        } else {
            0
        };
        let slice =
            self.reader
                .bytes
                .get(byte_offset..end)
                .ok_or(MalformedError::UnexpectedEof {
                    offset: u64::try_from(byte_offset)
                        .map_err(|_| MalformedError::IntegerOverflow)?,
                    needed: declared_size.into(),
                    remaining: u64::try_from(remaining)
                        .map_err(|_| MalformedError::IntegerOverflow)?,
                })?;
        let normalized = normalized_bounded_arithmetic(slice, limits)?;

        let adaptive = [
            TemplateOffset::new(
                i32::from(request.adaptive_pixels[0].0),
                i32::from(request.adaptive_pixels[0].1),
            ),
            TemplateOffset::new(
                i32::from(request.adaptive_pixels[1].0),
                i32::from(request.adaptive_pixels[1].1),
            ),
        ];
        let call = RefinementArithmeticCall {
            width: request.geometry.width,
            height: request.geometry.height,
            adaptive,
            typical_prediction: false,
        };
        let parameters = RefinementParameters {
            reference_dx: request.geometry.reference_dx,
            reference_dy: request.geometry.reference_dy,
        };
        let contexts = self
            .refinement
            .as_mut()
            .ok_or(SymbolTextError::InvalidState(
                "missing Huffman refinement contexts",
            ))?;
        let mut decoder = ArithmeticDecoder::new(normalized.bytes())?;
        let bitmap = decode_refinement_on(
            &mut decoder,
            contexts,
            call,
            request.reference,
            parameters,
            limits,
            &mut self.refinement_work,
        )?;
        validate_bounded_arithmetic_marker(&decoder, normalized.len())?;

        let advanced_bits = end.checked_mul(8).ok_or(MalformedError::IntegerOverflow)?;
        self.reader.bit_position = advanced_bits;
        Ok(bitmap)
    }
}

impl<'a> SymbolCodec<'a> {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires symbol_text_spec::codec_valid(old(self)),
        ensures match out {
            Ok(_) => symbol_text_spec::codec_valid(final(self)),
            Err(_) => true,
        },
    ))]
    pub(super) fn decode_integer(
        &mut self,
        field: IntegerField,
    ) -> Result<EntropyInteger, SymbolTextError> {
        match self {
            Self::Arithmetic(codec) => codec.decode_integer(field),
            Self::Huffman(codec) => codec.decode_integer(field),
            #[cfg(test)]
            Self::Scripted(codec) => codec
                .integers
                .pop_front()
                .ok_or(SymbolTextError::InvalidState("missing scripted integer")),
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires symbol_text_spec::codec_valid(old(self)),
        ensures match out {
            Ok(_) => symbol_text_spec::codec_valid(final(self)),
            Err(_) => true,
        },
    ))]
    pub(super) fn decode_symbol_id(
        &mut self,
        request: SymbolIdRequest,
    ) -> Result<u32, SymbolTextError> {
        match self {
            Self::Arithmetic(codec) => codec.decode_symbol_id(request),
            Self::Huffman(codec) => codec.decode_symbol_id(request),
            #[cfg(test)]
            Self::Scripted(codec) => Ok(codec.symbol_id),
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires
            symbol_text_spec::codec_valid(old(self)),
            crate::spec::bitmap::storage_valid(request.reference),
        ensures match out {
            Ok(bitmap) =>
                symbol_text_spec::codec_valid(final(self))
                    && crate::spec::bitmap::storage_valid(&bitmap),
            Err(_) => true,
        },
    ))]
    pub(super) fn decode_refinement(
        &mut self,
        request: RefinementRequest<'_>,
        limits: &Limits,
    ) -> Result<Bitmap, SymbolTextError> {
        match self {
            Self::Arithmetic(codec) => codec.decode_refinement(request, limits),
            Self::Huffman(codec) => codec.decode_refinement(request, limits),
            #[cfg(test)]
            Self::Scripted(_) => Ok(Bitmap::blank(
                request.geometry.width,
                request.geometry.height,
                limits,
            )?),
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires symbol_text_spec::codec_valid(old(self)),
        ensures match out {
            Ok(codec) =>
                crate::huffman::huffman_spec::reader_valid(&codec.reader),
            Err(_) => true,
        },
    ))]
    pub(super) fn huffman_mut(&mut self) -> Result<&mut HuffmanCodec<'a>, SymbolTextError> {
        match self {
            Self::Huffman(codec) => Ok(codec),
            Self::Arithmetic(_) => Err(SymbolTextError::InvalidState(
                "Huffman cursor requested from arithmetic state",
            )),
            #[cfg(test)]
            Self::Scripted(_) => Err(SymbolTextError::InvalidState(
                "Huffman cursor requested from scripted state",
            )),
        }
    }
}

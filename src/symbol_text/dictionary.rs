//! Height-class decoding, collective bitmaps and exported dictionary assembly.

use super::bitmap::{
    bitmap_from_packed_bytes, clone_bitmap, crop_bitmap_columns, generic_template_from_u8,
    refinement_template_from_u8,
};
use super::codec::{
    byte_align, decode_checked_symbol_id, decode_symbol_id_code_lengths, new_arithmetic_codec,
    new_huffman_codec, normalized_bounded_arithmetic, read_symbol_id_runcode_lines,
    validate_bounded_arithmetic_marker,
};
#[cfg(verus_keep_ghost)]
use super::symbol_text_spec;
use super::tables::{
    associate_dictionary_tables, materialize_huffman_table, select_dictionary_tables,
    wrap_custom_tables,
};
use super::text::decode_text_region_body;
use super::{
    check_dimensions, check_symbol_counts, checked_unsigned_delta, decode_mmr, enforce_limit,
    parse_symbol_dictionary_header, required_integer, symbol_by_id, symbol_id_code_length,
    ArithmeticDictionaryResult, BitReader, Bitmap, CombinationOperator, DecodeError,
    DictionaryClassState, DictionaryContexts, EntropyInteger, EofbPolicy, GenericArithmeticCall,
    GenericContexts, HuffmanChoiceSet, HuffmanFieldTables, HuffmanTable, IntegerField,
    LimitExceeded, LimitKind, Limits, MalformedError, ReferenceCorner, RefinementContextSnapshot,
    RefinementGeometry, RefinementRequest, ResourceTracker, StandardHuffmanTables, SymbolCodebook,
    SymbolCodec, SymbolDictionaryHeader, SymbolDictionaryInput, SymbolDictionaryOutput,
    SymbolIdRequest, SymbolTextError, TemplateOffset, TextRegionConfig,
};
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

impl DictionaryClassState {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures match out {
            Ok(state) =>
                symbol_text_spec::dictionary_state_valid(&state)
                && state.declared_new_symbols == declared_new_symbols
                && state.decoded_symbols == 0
                && !state.class_open,
            Err(_) => true,
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn new(declared_new_symbols: u32, limits: &Limits) -> Result<Self, SymbolTextError> {
        enforce_limit(
            LimitKind::Symbols,
            u64::from(limits.max_symbols),
            u64::from(declared_new_symbols),
        )?;
        Ok(Self {
            declared_new_symbols,
            decoded_symbols: 0,
            height: 0,
            width: 0,
            total_width: 0,
            class_open: false,
        })
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires symbol_text_spec::dictionary_state_valid(old(self)),
        ensures match out {
            Ok(height) =>
                symbol_text_spec::dictionary_state_valid(final(self))
                && symbol_text_spec::work_units(final(resources))
                    == symbol_text_spec::work_units(old(resources)) + 1
                && symbol_text_spec::work_units(final(resources))
                    <= limits.max_work_units
                && final(self).class_open
                && final(self).declared_new_symbols == old(self).declared_new_symbols
                && final(self).height == height
                && final(self).decoded_symbols == old(self).decoded_symbols
                && final(self).width == 0
                && final(self).total_width == 0,
            Err(_) => true,
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn begin_height_class(
        &mut self,
        delta: i64,
        limits: &Limits,
        resources: &mut ResourceTracker,
    ) -> Result<u32, SymbolTextError> {
        if self.class_open {
            return Err(SymbolTextError::InvalidState("height class already open"));
        }
        if self.decoded_symbols == self.declared_new_symbols {
            return Err(SymbolTextError::InvalidState(
                "height class after final symbol",
            ));
        }
        resources.charge_work(1, limits)?;
        self.height = checked_unsigned_delta(self.height, delta, limits.max_height)?;
        self.width = 0;
        self.total_width = 0;
        self.class_open = true;
        Ok(self.height)
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires symbol_text_spec::dictionary_state_valid(old(self)),
        ensures match out {
            Ok((width, height)) =>
                symbol_text_spec::dictionary_state_valid(final(self))
                && symbol_text_spec::work_units(final(resources))
                    == symbol_text_spec::work_units(old(resources)) + 1
                && symbol_text_spec::work_units(final(resources))
                    <= limits.max_work_units
                && final(self).class_open
                && final(self).declared_new_symbols == old(self).declared_new_symbols
                && final(self).decoded_symbols == old(self).decoded_symbols + 1
                && final(self).decoded_symbols <= final(self).declared_new_symbols
                && width == final(self).width
                && height == final(self).height
                && final(self).total_width <= limits.max_width,
            Err(_) => true,
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn add_symbol(
        &mut self,
        width_delta: i64,
        limits: &Limits,
        resources: &mut ResourceTracker,
    ) -> Result<(u32, u32), SymbolTextError> {
        if !self.class_open {
            return Err(SymbolTextError::InvalidState("no open height class"));
        }
        if self.decoded_symbols >= self.declared_new_symbols {
            return Err(SymbolTextError::InvalidCount("too many new symbols"));
        }
        resources.charge_work(1, limits)?;
        self.width = checked_unsigned_delta(self.width, width_delta, limits.max_width)?;
        self.total_width = self
            .total_width
            .checked_add(self.width)
            .ok_or(MalformedError::IntegerOverflow)?;
        if self.total_width > limits.max_width {
            return Err(LimitExceeded {
                kind: LimitKind::Width,
                limit: u64::from(limits.max_width),
                actual: u64::from(self.total_width),
            }
            .into());
        }
        check_dimensions(self.total_width, self.height, limits)?;
        self.decoded_symbols = self
            .decoded_symbols
            .checked_add(1)
            .ok_or(MalformedError::IntegerOverflow)?;
        Ok((self.width, self.height))
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires symbol_text_spec::dictionary_state_valid(old(self)),
        ensures match out {
            Ok(total_width) =>
                symbol_text_spec::dictionary_state_valid(final(self))
                && !final(self).class_open
                && final(self).declared_new_symbols == old(self).declared_new_symbols
                && final(self).decoded_symbols == old(self).decoded_symbols
                && total_width == final(self).total_width,
            Err(_) => true,
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn finish_height_class(&mut self) -> Result<u32, SymbolTextError> {
        if !self.class_open {
            return Err(SymbolTextError::InvalidState("no open height class"));
        }
        self.class_open = false;
        Ok(self.total_width)
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires symbol_text_spec::dictionary_state_valid(&self),
        ensures match out {
            Ok(()) =>
                !self.class_open
                && self.decoded_symbols == self.declared_new_symbols,
            Err(_) => true,
        },
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    pub(crate) fn finish(self) -> Result<(), SymbolTextError> {
        if self.class_open {
            return Err(SymbolTextError::InvalidState("height class still open"));
        }
        if self.decoded_symbols != self.declared_new_symbols {
            return Err(SymbolTextError::InvalidCount("new symbol count mismatch"));
        }
        Ok(())
    }
}

/// Decodes alternating export-flag runs, beginning with a zero run.
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        symbol_text_spec::codec_valid(old(codec)),
        symbol_text_spec::work_units(old(resources)) <= limits.max_work_units,
    ensures match out {
        Ok(exported) =>
            symbol_text_spec::codec_valid(final(codec))
                && exported@.len() == expected_exports
                && symbol_text_spec::export_indices_valid(
                    exported@, total_candidates
                )
                && symbol_text_spec::bitmap_bytes(final(resources))
                    == symbol_text_spec::bitmap_bytes(old(resources)),
        Err(_) => true,
    },
))]
pub(super) fn decode_export_runs(
    codec: &mut SymbolCodec<'_>,
    total_candidates: u32,
    expected_exports: u32,
    limits: &Limits,
    resources: &mut ResourceTracker,
) -> Result<Vec<u32>, SymbolTextError> {
    if expected_exports > total_candidates {
        return Err(SymbolTextError::InvalidCount(
            "export count exceeds candidate count",
        ));
    }
    let capacity =
        usize::try_from(expected_exports).map_err(|_| MalformedError::IntegerOverflow)?;
    let requested_bytes = u64::from(expected_exports)
        .checked_mul(4)
        .ok_or(MalformedError::IntegerOverflow)?;
    let mut exported: Vec<u32> = Vec::new();
    exported
        .try_reserve(capacity)
        .map_err(|_| DecodeError::Allocation { requested_bytes })?;
    let mut index = 0_u32;
    let mut export_flag = false;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            symbol_text_spec::codec_valid(codec),
            symbol_text_spec::work_units(resources) <= limits.max_work_units,
            symbol_text_spec::work_units(resources)
                >= symbol_text_spec::work_units(old(resources)),
            symbol_text_spec::bitmap_bytes(resources)
                == symbol_text_spec::bitmap_bytes(old(resources)),
            index <= total_candidates,
            capacity == expected_exports,
            exported@.len() <= capacity,
            forall|offset: int| 0 <= offset < exported@.len()
                ==> #[trigger] exported@[offset] < index,
        decreases limits.max_work_units - symbol_text_spec::work_units(resources),
    ))]
    while index < total_candidates {
        resources.charge_work(1, limits)?;
        let run = match codec.decode_integer(IntegerField::ExportRun)? {
            EntropyInteger::Value(value) => u32::try_from(value)
                .map_err(|_| SymbolTextError::InvalidEntropyValue("negative export run"))?,
            EntropyInteger::OutOfBand => {
                return Err(SymbolTextError::UnexpectedOutOfBand("export run"))
            }
        };
        let end = index
            .checked_add(run)
            .ok_or(MalformedError::IntegerOverflow)?;
        if end > total_candidates {
            return Err(SymbolTextError::InvalidCount(
                "export run exceeds candidate count",
            ));
        }
        if export_flag {
            let mut candidate = index;
            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant
                    index <= candidate <= end,
                    end <= total_candidates,
                    exported@.len() <= capacity,
                    forall|offset: int| 0 <= offset < exported@.len()
                        ==> #[trigger] exported@[offset] < candidate,
                decreases end - candidate,
            ))]
            while candidate < end {
                if exported.len() >= capacity {
                    return Err(SymbolTextError::InvalidCount("too many exported symbols"));
                }
                exported.push(candidate);
                candidate = candidate
                    .checked_add(1)
                    .ok_or(MalformedError::IntegerOverflow)?;
            }
        }
        index = end;
        export_flag = !export_flag;
    }
    if exported.len() != capacity {
        return Err(SymbolTextError::InvalidCount(
            "exported-symbol popcount mismatch",
        ));
    }
    Ok(exported)
}

/// Decodes one T.88 6.5.8.2.2 single-instance refinement/aggregate symbol
/// bitmap, shared by the arithmetic and Huffman symbol-dictionary decoders.
///
/// Unlike the text-region per-instance refinement procedure (handled by the
/// existing [`decode_refined_bitmap`] helper), this procedure never decodes a
/// width or height delta: the symbol's dimensions were already fixed by the
/// enclosing height-class loop, so only the symbol ID and reference offset
/// are decoded before invoking the shared generic-refinement callback.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        symbol_text_spec::codec_valid(old(codec)),
        symbol_text_spec::codebook_valid(imported),
        symbol_text_spec::bitmaps_valid(new_symbols@),
    ensures match out {
        Ok(bitmap) =>
            symbol_text_spec::codec_valid(final(codec))
                && crate::spec::bitmap::storage_valid(&bitmap)
                && crate::views::bitmap_width(&bitmap) == width
                && crate::views::bitmap_height(&bitmap) == height
                && symbol_text_spec::work_units(final(resources))
                    == symbol_text_spec::work_units(old(resources)),
        Err(_) => true,
    },
))]
pub(super) fn decode_dictionary_single_instance_refinement(
    codec: &mut SymbolCodec<'_>,
    imported: SymbolCodebook<'_>,
    new_symbols: &[Bitmap],
    code_length: u8,
    width: u32,
    height: u32,
    refinement_template: u8,
    refinement_adaptive: [(i8, i8); 2],
    huffman: bool,
    limits: &Limits,
    resources: &mut ResourceTracker,
) -> Result<Bitmap, SymbolTextError> {
    let available = imported
        .len()
        .checked_add(new_symbols.len())
        .and_then(|count| u32::try_from(count).ok())
        .ok_or(MalformedError::IntegerOverflow)?;
    let id = decode_checked_symbol_id(
        codec,
        SymbolIdRequest {
            available_symbols: available,
            code_length,
            huffman,
        },
    )?;
    let reference_x = required_integer(codec, IntegerField::RefinementX)?;
    let reference_y = required_integer(codec, IntegerField::RefinementY)?;
    let declared_size = if huffman {
        let size = required_integer(codec, IntegerField::RefinementSize)?;
        Some(
            u32::try_from(size)
                .map_err(|_| SymbolTextError::InvalidEntropyValue("refinement size"))?,
        )
    } else {
        None
    };
    let reference = symbol_by_id(imported, new_symbols, id)?;
    check_dimensions(width, height, limits)?;
    resources.charge_bitmap(width, height, limits)?;
    let geometry = RefinementGeometry {
        width,
        height,
        reference_dx: i32::try_from(reference_x).map_err(|_| MalformedError::IntegerOverflow)?,
        reference_dy: i32::try_from(reference_y).map_err(|_| MalformedError::IntegerOverflow)?,
    };
    let request = RefinementRequest {
        reference,
        geometry,
        template: refinement_template,
        adaptive_pixels: refinement_adaptive,
        declared_data_size: declared_size,
    };
    let bitmap = codec.decode_refinement(request, limits)?;
    if bitmap.width() != width || bitmap.height() != height {
        return Err(SymbolTextError::InvalidDimensions);
    }
    Ok(bitmap)
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        symbol_text_spec::codec_valid(old(codec)),
        symbol_text_spec::codebook_valid(imported),
        symbol_text_spec::bitmaps_valid(new_symbols@),
        symbol_text_spec::work_units(old(resources)) <= limits.max_work_units,
        header.adaptive_count <= 4,
    ensures match out {
        Ok(bitmap) =>
            symbol_text_spec::codec_valid(final(codec))
                && crate::spec::bitmap::storage_valid(&bitmap)
                && crate::views::bitmap_width(&bitmap) == width
                && crate::views::bitmap_height(&bitmap) == height
                && symbol_text_spec::work_units(final(resources))
                    <= limits.max_work_units
                && symbol_text_spec::work_units(final(resources))
                    >= symbol_text_spec::work_units(old(resources)),
        Err(_) => true,
    },
))]
pub(super) fn decode_one_new_symbol(
    header: &SymbolDictionaryHeader,
    codec: &mut SymbolCodec<'_>,
    imported: SymbolCodebook<'_>,
    new_symbols: &[Bitmap],
    generic_contexts: Option<&mut GenericContexts>,
    generic_work: &mut u64,
    aggregation_instances: Option<u32>,
    width: u32,
    height: u32,
    code_length: u8,
    refinement_adaptive: [(i8, i8); 2],
    limits: &Limits,
    resources: &mut ResourceTracker,
) -> Result<Bitmap, SymbolTextError> {
    let config = header.config;
    if config.refinement_aggregation {
        let instances = aggregation_instances.ok_or(SymbolTextError::InvalidState(
            "missing aggregation-instance count",
        ))?;
        if instances == 0 {
            return Err(SymbolTextError::InvalidCount(
                "zero-instance aggregate symbol",
            ));
        }
        if instances > 1 {
            let capacity = imported
                .len()
                .checked_add(new_symbols.len())
                .ok_or(MalformedError::IntegerOverflow)?;
            let requested_bytes = capacity
                .checked_mul(core::mem::size_of::<&Bitmap>())
                .and_then(|bytes| u64::try_from(bytes).ok())
                .ok_or(MalformedError::IntegerOverflow)?;
            let mut symbol_refs: Vec<&Bitmap> = Vec::new();
            symbol_refs
                .try_reserve(capacity)
                .map_err(|_| DecodeError::Allocation { requested_bytes })?;
            let mut imported_index = 0_usize;
            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant
                    imported_index <= symbol_text_spec::codebook_len(imported),
                    symbol_refs@.len() == imported_index,
                    symbol_text_spec::codebook_valid(imported),
                    symbol_text_spec::bitmap_refs_valid(symbol_refs@),
                decreases symbol_text_spec::codebook_len(imported) - imported_index,
            ))]
            while imported_index < imported.len() {
                symbol_refs.push(imported.get(imported_index).ok_or(
                    SymbolTextError::InvalidState("missing imported aggregate symbol"),
                )?);
                imported_index += 1;
            }
            let mut new_index = 0_usize;
            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant
                    new_index <= new_symbols@.len(),
                    symbol_refs@.len()
                        == symbol_text_spec::codebook_len(imported) + new_index,
                    symbol_text_spec::bitmaps_valid(new_symbols@),
                    symbol_text_spec::bitmap_refs_valid(symbol_refs@),
                decreases new_symbols@.len() - new_index,
            ))]
            while new_index < new_symbols.len() {
                symbol_refs.push(&new_symbols[new_index]);
                new_index += 1;
            }
            let nested_config = TextRegionConfig {
                huffman: config.huffman,
                refinement: true,
                strip_size: 1,
                reference_corner: ReferenceCorner::TopLeft,
                transposed: false,
                operator: CombinationOperator::Or,
                default_pixel: false,
                s_offset: 0,
                refinement_template: config.refinement_template,
                huffman_choices: None,
            };
            let work_before = resources.work_units;
            let bitmap = decode_text_region_body(
                codec,
                SymbolCodebook::Borrowed(&symbol_refs),
                code_length,
                width,
                height,
                instances,
                &nested_config,
                refinement_adaptive,
                limits,
                resources,
            )?;
            if resources.work_units < work_before {
                return Err(SymbolTextError::InvalidState(
                    "resource work counter decreased",
                ));
            }
            Ok(bitmap)
        } else {
            if aggregation_instances.is_some() {
                return Err(SymbolTextError::InvalidState(
                    "aggregation count on direct-coded symbol",
                ));
            }
            decode_dictionary_single_instance_refinement(
                codec,
                imported,
                new_symbols,
                code_length,
                width,
                height,
                config.refinement_template,
                refinement_adaptive,
                config.huffman,
                limits,
                resources,
            )
        }
    } else {
        resources.charge_bitmap(width, height, limits)?;
        let contexts = generic_contexts.ok_or(SymbolTextError::InvalidState(
            "missing generic contexts for a direct-coded dictionary",
        ))?;
        let mut full_adaptive = [TemplateOffset::new(0, 0); 12];
        full_adaptive[..usize::from(header.adaptive_count)]
            .copy_from_slice(&header.adaptive[..usize::from(header.adaptive_count)]);
        let call = GenericArithmeticCall {
            width,
            height,
            adaptive: full_adaptive,
            typical_prediction: false,
            skip: None,
        };
        let bitmap = match codec {
            SymbolCodec::Arithmetic(codec) => {
                codec.decode_generic(contexts, call, height, limits, generic_work)
            }
            SymbolCodec::Huffman(_) => Err(SymbolTextError::InvalidState(
                "direct dictionary bitmap requires arithmetic state",
            )),
            #[cfg(test)]
            SymbolCodec::Scripted(_) => Err(SymbolTextError::InvalidState(
                "direct dictionary bitmap requires arithmetic state",
            )),
        }?;
        if bitmap.width() != width || bitmap.height() != height {
            return Err(SymbolTextError::InvalidDimensions);
        }
        Ok(bitmap)
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires crate::huffman::huffman_spec::reader_valid(old(reader)),
    ensures
        crate::huffman::huffman_spec::reader_valid(final(reader)),
        match out {
            Ok(bitmap) => crate::spec::bitmap::storage_valid(&bitmap),
            Err(_) => true,
        },
))]
pub(super) fn decode_collective_bitmap(
    reader: &mut BitReader<'_>,
    bitmap_size: u32,
    width: u32,
    height: u32,
    limits: &Limits,
) -> Result<Bitmap, SymbolTextError> {
    byte_align(reader)?;
    let byte_offset = reader
        .bit_position
        .checked_div(8)
        .ok_or(MalformedError::IntegerOverflow)?;
    let length = if bitmap_size == 0 {
        (u64::from(width)
            .checked_add(7)
            .ok_or(MalformedError::IntegerOverflow)?
            / 8)
        .checked_mul(u64::from(height))
        .ok_or(MalformedError::IntegerOverflow)?
    } else {
        u64::from(bitmap_size)
    };
    let length_usize = usize::try_from(length).map_err(|_| MalformedError::IntegerOverflow)?;
    let end = byte_offset
        .checked_add(length_usize)
        .ok_or(MalformedError::IntegerOverflow)?;
    let remaining = if byte_offset <= reader.bytes.len() {
        reader.bytes.len() - byte_offset
    } else {
        0
    };
    let source = reader
        .bytes
        .get(byte_offset..end)
        .ok_or(MalformedError::UnexpectedEof {
            offset: u64::try_from(byte_offset).map_err(|_| MalformedError::IntegerOverflow)?,
            needed: length,
            remaining: u64::try_from(remaining).map_err(|_| MalformedError::IntegerOverflow)?,
        })?;
    let bitmap = if bitmap_size == 0 {
        bitmap_from_packed_bytes(width, height, source, limits)?
    } else {
        decode_mmr(source, width, height, limits, EofbPolicy::Optional)?
    };
    reader.bit_position = end.checked_mul(8).ok_or(MalformedError::IntegerOverflow)?;
    Ok(bitmap)
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        symbol_text_spec::codebook_valid(imported),
        header.adaptive_count <= 4,
        total_symbols as nat
            == symbol_text_spec::codebook_len(imported) + header.new_symbols as nat,
    ensures match out {
        Ok((exported, _, _, consumed)) =>
            symbol_text_spec::bitmaps_valid(exported@)
                && consumed == body@.len(),
        Err(_) => true,
    },
))]
pub(super) fn decode_symbol_dictionary_arithmetic(
    header: &SymbolDictionaryHeader,
    imported: SymbolCodebook<'_>,
    body: &[u8],
    code_length: u8,
    total_symbols: u32,
    refinement_adaptive: [(i8, i8); 2],
    used_contexts: Option<&DictionaryContexts>,
    limits: &Limits,
) -> ArithmeticDictionaryResult {
    let config = header.config;
    let normalized = normalized_bounded_arithmetic(body, limits)?;
    let refinement_template = config
        .refinement_aggregation
        .then(|| refinement_template_from_u8(config.refinement_template));
    let mut codec = SymbolCodec::Arithmetic(new_arithmetic_codec(
        normalized.bytes(),
        code_length,
        refinement_template,
        limits,
    )?);
    let mut resources = ResourceTracker {
        work_units: 0,
        decoded_bitmap_bytes: 0,
    };

    let mut generic_contexts = if config.refinement_aggregation {
        None
    } else {
        let mut contexts =
            GenericContexts::new(generic_template_from_u8(config.template), false, limits)?;
        if let Some(used) = used_contexts.and_then(|used| used.generic.as_ref()) {
            contexts.restore(used, limits)?;
        }
        Some(contexts)
    };
    let mut generic_work = 0_u64;

    if config.refinement_aggregation {
        if let Some(used) = used_contexts.and_then(|used| used.refinement.as_ref()) {
            let SymbolCodec::Arithmetic(arithmetic) = &mut codec else {
                return Err(SymbolTextError::InvalidState(
                    "arithmetic dictionary has non-arithmetic state",
                ));
            };
            let contexts = arithmetic
                .refinement
                .as_mut()
                .ok_or(SymbolTextError::InvalidState(
                    "missing arithmetic refinement contexts",
                ))?;
            contexts.restore(used, limits)?;
        }
    }

    let new_symbol_capacity =
        usize::try_from(header.new_symbols).map_err(|_| MalformedError::IntegerOverflow)?;
    let mut new_symbols: Vec<Bitmap> = Vec::new();
    new_symbols
        .try_reserve(new_symbol_capacity)
        .map_err(|_| DecodeError::Allocation {
            requested_bytes: u64::from(header.new_symbols),
        })?;

    let mut class_state = DictionaryClassState::new(header.new_symbols, limits)?;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            new_symbols@.len() <= header.new_symbols as nat,
            new_symbol_capacity == header.new_symbols as nat,
            class_state.decoded_symbols as nat == new_symbols@.len(),
            class_state.declared_new_symbols == header.new_symbols,
            !class_state.class_open,
            header.adaptive_count <= 4,
            symbol_text_spec::dictionary_state_valid(&class_state),
            symbol_text_spec::codebook_valid(imported),
            symbol_text_spec::bitmaps_valid(new_symbols@),
            symbol_text_spec::codec_valid(&codec),
            symbol_text_spec::work_units(&resources) <= limits.max_work_units,
        decreases limits.max_work_units - symbol_text_spec::work_units(&resources),
    ))]
    while u32::try_from(new_symbols.len()).map_err(|_| MalformedError::IntegerOverflow)?
        < header.new_symbols
    {
        #[cfg_attr(not(verus_keep_ghost), allow(unused_variables))]
        let class_start_work = resources.work_units;
        let height_delta = required_integer(&mut codec, IntegerField::HeightDelta)?;
        class_state.begin_height_class(height_delta, limits, &mut resources)?;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant
                new_symbols@.len() <= header.new_symbols as nat,
                new_symbol_capacity == header.new_symbols as nat,
                class_state.decoded_symbols as nat == new_symbols@.len(),
                class_state.declared_new_symbols == header.new_symbols,
                class_state.class_open,
                header.adaptive_count <= 4,
                symbol_text_spec::dictionary_state_valid(&class_state),
                symbol_text_spec::codebook_valid(imported),
                symbol_text_spec::bitmaps_valid(new_symbols@),
                symbol_text_spec::codec_valid(&codec),
                symbol_text_spec::work_units(&resources)
                    <= limits.max_work_units,
                resources.work_units > class_start_work,
            decreases limits.max_work_units
                - symbol_text_spec::work_units(&resources),
        ))]
        while let EntropyInteger::Value(width_delta) =
            codec.decode_integer(IntegerField::WidthDelta)?
        {
            let (width, height) = class_state.add_symbol(width_delta, limits, &mut resources)?;
            let aggregation_instances = if config.refinement_aggregation {
                let instances = required_integer(&mut codec, IntegerField::AggregationInstances)?;
                Some(
                    u32::try_from(instances).map_err(|_| {
                        SymbolTextError::InvalidEntropyValue("aggregation instances")
                    })?,
                )
            } else {
                None
            };
            let bitmap = decode_one_new_symbol(
                header,
                &mut codec,
                imported,
                &new_symbols,
                generic_contexts.as_mut(),
                &mut generic_work,
                aggregation_instances,
                width,
                height,
                code_length,
                refinement_adaptive,
                limits,
                &mut resources,
            )?;
            if new_symbols.len() >= new_symbol_capacity {
                return Err(SymbolTextError::InvalidCount("too many new symbols"));
            }
            new_symbols.push(bitmap);
        }
        class_state.finish_height_class()?;
    }
    class_state.finish()?;

    let exported = decode_export_runs(
        &mut codec,
        total_symbols,
        header.exported_symbols,
        limits,
        &mut resources,
    )?;

    let SymbolCodec::Arithmetic(arithmetic) = &codec else {
        return Err(SymbolTextError::InvalidState(
            "arithmetic dictionary has non-arithmetic state",
        ));
    };
    validate_bounded_arithmetic_marker(&arithmetic.decoder, normalized.len())?;

    let retained_generic = match generic_contexts.as_ref() {
        Some(contexts) => Some(contexts.try_snapshot(limits)?),
        None => None,
    };
    let retained_refinement = match arithmetic.refinement.as_ref() {
        Some(contexts) => Some(contexts.try_snapshot(limits)?),
        None => None,
    };

    let exported_symbols = assemble_exported_symbols(imported, &new_symbols, &exported, limits)?;
    Ok((
        exported_symbols,
        retained_generic,
        retained_refinement,
        body.len(),
    ))
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        symbol_text_spec::codebook_valid(imported),
        header.adaptive_count <= 4,
        total_symbols as nat
            == symbol_text_spec::codebook_len(imported) + header.new_symbols as nat,
    ensures match out {
        Ok((exported, _, consumed)) =>
            symbol_text_spec::bitmaps_valid(exported@)
                && consumed == body@.len(),
        Err(_) => true,
    },
))]
pub(super) fn decode_symbol_dictionary_huffman(
    header: &SymbolDictionaryHeader,
    imported: SymbolCodebook<'_>,
    body: &[u8],
    code_length: u8,
    total_symbols: u32,
    refinement_adaptive: [(i8, i8); 2],
    custom_tables: &[&HuffmanTable],
    limits: &Limits,
) -> Result<(Vec<Bitmap>, Option<RefinementContextSnapshot>, usize), SymbolTextError> {
    let config = header.config;
    let custom_wrapped = wrap_custom_tables(custom_tables)?;
    let bindings = associate_dictionary_tables(&config, &custom_wrapped)?;
    let choices = select_dictionary_tables(&config, &bindings)?;
    let tables = materialize_huffman_tables!(HuffmanChoiceSet::Dictionary(choices), limits)?;
    let refinement_template = config
        .refinement_aggregation
        .then(|| refinement_template_from_u8(config.refinement_template));
    let mut codec = SymbolCodec::Huffman(new_huffman_codec(
        body,
        tables,
        0,
        refinement_template,
        limits,
    )?);
    let mut resources = ResourceTracker {
        work_units: 0,
        decoded_bitmap_bytes: 0,
    };

    let new_symbol_capacity =
        usize::try_from(header.new_symbols).map_err(|_| MalformedError::IntegerOverflow)?;
    let mut new_symbols: Vec<Bitmap> = Vec::new();
    new_symbols
        .try_reserve(new_symbol_capacity)
        .map_err(|_| DecodeError::Allocation {
            requested_bytes: u64::from(header.new_symbols),
        })?;

    let mut class_state = DictionaryClassState::new(header.new_symbols, limits)?;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            new_symbols@.len() <= header.new_symbols as nat,
            new_symbol_capacity == header.new_symbols as nat,
            class_state.decoded_symbols as nat == new_symbols@.len(),
            class_state.declared_new_symbols == header.new_symbols,
            !class_state.class_open,
            header.adaptive_count <= 4,
            symbol_text_spec::dictionary_state_valid(&class_state),
            symbol_text_spec::codebook_valid(imported),
            symbol_text_spec::bitmaps_valid(new_symbols@),
            symbol_text_spec::codec_valid(&codec),
            symbol_text_spec::work_units(&resources) <= limits.max_work_units,
        decreases limits.max_work_units - symbol_text_spec::work_units(&resources),
    ))]
    while u32::try_from(new_symbols.len()).map_err(|_| MalformedError::IntegerOverflow)?
        < header.new_symbols
    {
        #[cfg_attr(not(verus_keep_ghost), allow(unused_variables))]
        let class_start_work = resources.work_units;
        let height_delta = required_integer(&mut codec, IntegerField::HeightDelta)?;
        let height = class_state.begin_height_class(height_delta, limits, &mut resources)?;

        if config.refinement_aggregation {
            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant
                    new_symbols@.len() <= header.new_symbols as nat,
                    new_symbol_capacity == header.new_symbols as nat,
                    class_state.decoded_symbols as nat == new_symbols@.len(),
                    class_state.declared_new_symbols == header.new_symbols,
                    class_state.class_open,
                    header.adaptive_count <= 4,
                    symbol_text_spec::dictionary_state_valid(&class_state),
                    symbol_text_spec::codebook_valid(imported),
                    symbol_text_spec::bitmaps_valid(new_symbols@),
                    symbol_text_spec::codec_valid(&codec),
                    symbol_text_spec::work_units(&resources)
                        <= limits.max_work_units,
                    resources.work_units > class_start_work,
                decreases limits.max_work_units
                    - symbol_text_spec::work_units(&resources),
            ))]
            while let EntropyInteger::Value(width_delta) =
                codec.decode_integer(IntegerField::WidthDelta)?
            {
                let (width, height) =
                    class_state.add_symbol(width_delta, limits, &mut resources)?;
                let instances = required_integer(&mut codec, IntegerField::AggregationInstances)?;
                let instances = u32::try_from(instances)
                    .map_err(|_| SymbolTextError::InvalidEntropyValue("aggregation instances"))?;
                if instances > 1 {
                    let SymbolCodec::Huffman(huffman) = &mut codec else {
                        return Err(SymbolTextError::InvalidState(
                            "Huffman dictionary has non-Huffman state",
                        ));
                    };
                    prepare_symbol_ids!(huffman, total_symbols, limits)?;
                    #[cfg(verus_keep_ghost)]
                    proof! {
                        reveal(symbol_text_spec::codec_valid);
                    }
                }
                let bitmap = decode_one_new_symbol(
                    header,
                    &mut codec,
                    imported,
                    &new_symbols,
                    None,
                    &mut 0,
                    Some(instances),
                    width,
                    height,
                    code_length,
                    refinement_adaptive,
                    limits,
                    &mut resources,
                )?;
                if new_symbols.len() >= new_symbol_capacity {
                    return Err(SymbolTextError::InvalidCount("too many new symbols"));
                }
                new_symbols.push(bitmap);
            }
        } else {
            // T.88 6.5.9: collect widths for one height class, then decode a
            // single collective bitmap and split it by recorded width.
            let mut widths: Vec<u32> = Vec::new();
            let decoded_count =
                u32::try_from(new_symbols.len()).map_err(|_| MalformedError::IntegerOverflow)?;
            let remaining = header
                .new_symbols
                .checked_sub(decoded_count)
                .and_then(|count| usize::try_from(count).ok())
                .ok_or(MalformedError::IntegerOverflow)?;
            let requested_width_bytes = remaining
                .checked_mul(core::mem::size_of::<u32>())
                .and_then(|bytes| u64::try_from(bytes).ok())
                .ok_or(MalformedError::IntegerOverflow)?;
            widths
                .try_reserve(remaining)
                .map_err(|_| DecodeError::Allocation {
                    requested_bytes: requested_width_bytes,
                })?;
            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant
                    widths@.len() <= remaining,
                    new_symbols@.len() + widths@.len()
                        == class_state.decoded_symbols as nat,
                    class_state.declared_new_symbols == header.new_symbols,
                    class_state.decoded_symbols <= class_state.declared_new_symbols,
                    class_state.class_open,
                    symbol_text_spec::dictionary_state_valid(&class_state),
                    symbol_text_spec::work_units(&resources)
                        <= limits.max_work_units,
                    resources.work_units > class_start_work,
                    symbol_text_spec::codec_valid(&codec),
                decreases limits.max_work_units
                    - symbol_text_spec::work_units(&resources),
            ))]
            while let EntropyInteger::Value(width_delta) =
                codec.decode_integer(IntegerField::WidthDelta)?
            {
                let (width, _) = class_state.add_symbol(width_delta, limits, &mut resources)?;
                if widths.len() >= remaining {
                    return Err(SymbolTextError::InvalidCount("too many new symbols"));
                }
                widths.push(width);
            }
            let mut total_width = 0_u32;
            let mut width_index = 0_usize;
            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant width_index <= widths@.len(),
                decreases widths@.len() - width_index,
            ))]
            while width_index < widths.len() {
                total_width = total_width
                    .checked_add(widths[width_index])
                    .ok_or(MalformedError::IntegerOverflow)?;
                width_index += 1;
            }
            let bitmap_size = match codec.decode_integer(IntegerField::DictionaryBitmapSize)? {
                EntropyInteger::Value(value) => u32::try_from(value)
                    .map_err(|_| SymbolTextError::InvalidEntropyValue("bitmap size"))?,
                EntropyInteger::OutOfBand => {
                    return Err(SymbolTextError::UnexpectedOutOfBand(
                        "collective bitmap size",
                    ))
                }
            };
            let SymbolCodec::Huffman(huffman) = &mut codec else {
                return Err(SymbolTextError::InvalidState(
                    "Huffman dictionary has non-Huffman state",
                ));
            };
            let collective = decode_collective_bitmap(
                &mut huffman.reader,
                bitmap_size,
                total_width,
                height,
                limits,
            )?;
            #[cfg(verus_keep_ghost)]
            proof! {
                reveal(symbol_text_spec::codec_valid);
            }
            resources.charge_bitmap(total_width, height, limits)?;

            if widths.len() == 1 {
                if new_symbols.len() >= new_symbol_capacity {
                    return Err(SymbolTextError::InvalidCount("too many new symbols"));
                }
                new_symbols.push(collective);
            } else {
                let mut x_start = 0_u32;
                #[cfg_attr(not(verus_keep_ghost), allow(unused_variables))]
                let symbols_before = new_symbols.len();
                let mut crop_index = 0_usize;
                #[cfg_attr(verus_keep_ghost, verus_spec(
                    invariant
                        crop_index <= widths@.len(),
                        new_symbols@.len() == symbols_before + crop_index,
                        symbol_text_spec::bitmaps_valid(new_symbols@),
                        crate::spec::bitmap::storage_valid(&collective),
                    decreases widths@.len() - crop_index,
                ))]
                while crop_index < widths.len() {
                    let width = widths[crop_index];
                    let cropped = crop_bitmap_columns(&collective, x_start, width, limits)?;
                    if new_symbols.len() >= new_symbol_capacity {
                        return Err(SymbolTextError::InvalidCount("too many new symbols"));
                    }
                    new_symbols.push(cropped);
                    x_start = x_start
                        .checked_add(width)
                        .ok_or(MalformedError::IntegerOverflow)?;
                    crop_index += 1;
                }
            }
        }
        class_state.finish_height_class()?;
    }
    class_state.finish()?;

    let exported = decode_export_runs(
        &mut codec,
        total_symbols,
        header.exported_symbols,
        limits,
        &mut resources,
    )?;

    let consumed_within_body = codec
        .huffman_mut()?
        .reader
        .bit_position
        .checked_add(7)
        .ok_or(MalformedError::IntegerOverflow)?
        / 8;
    if consumed_within_body != body.len() {
        return Err(SymbolTextError::InvalidState(
            "Huffman symbol dictionary did not exactly consume its segment data",
        ));
    }

    let SymbolCodec::Huffman(huffman) = &codec else {
        return Err(SymbolTextError::InvalidState(
            "Huffman dictionary has non-Huffman state",
        ));
    };
    let retained_refinement = match huffman.refinement.as_ref() {
        Some(contexts) => Some(contexts.try_snapshot(limits)?),
        None => None,
    };
    let exported_symbols = assemble_exported_symbols(imported, &new_symbols, &exported, limits)?;
    Ok((exported_symbols, retained_refinement, consumed_within_body))
}

#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        symbol_text_spec::codebook_valid(imported),
        symbol_text_spec::bitmaps_valid(new_symbols@),
        forall|index: int| 0 <= index < exported_indices@.len()
            ==> (#[trigger] exported_indices@[index] as nat)
                < symbol_text_spec::codebook_len(imported) + (new_symbols@).len(),
    ensures match out {
        Ok(exported) =>
            exported@.len() == exported_indices@.len()
                && symbol_text_spec::bitmaps_valid(exported@)
                && symbol_text_spec::exported_contents_match(
                    imported, new_symbols@, exported_indices@, exported@,
                ),
        Err(_) => true,
    },
))]
pub(super) fn assemble_exported_symbols(
    imported: SymbolCodebook<'_>,
    new_symbols: &[Bitmap],
    exported_indices: &[u32],
    limits: &Limits,
) -> Result<Vec<Bitmap>, SymbolTextError> {
    let mut exported_symbols: Vec<Bitmap> = Vec::new();
    let requested_bytes = exported_indices
        .len()
        .checked_mul(core::mem::size_of::<Bitmap>())
        .and_then(|bytes| u64::try_from(bytes).ok())
        .ok_or(MalformedError::IntegerOverflow)?;
    exported_symbols
        .try_reserve(exported_indices.len())
        .map_err(|_| DecodeError::Allocation { requested_bytes })?;
    let mut index = 0_usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index <= exported_indices@.len(),
            exported_symbols@.len() == index,
            symbol_text_spec::codebook_valid(imported),
            symbol_text_spec::bitmaps_valid(new_symbols@),
            symbol_text_spec::bitmaps_valid(exported_symbols@),
            forall|offset: int| 0 <= offset < exported_indices@.len()
                ==> (#[trigger] exported_indices@[offset] as nat)
                    < symbol_text_spec::codebook_len(imported) + (new_symbols@).len(),
            forall|offset: int| 0 <= offset < index ==>
                #[trigger] symbol_text_spec::symbol_id_selects(
                    imported, new_symbols@, exported_indices@[offset], &exported_symbols@[offset],
                ),
        decreases exported_indices@.len() - index,
    ))]
    while index < exported_indices.len() {
        let symbol_id = exported_indices[index];
        let source = symbol_by_id(imported, new_symbols, symbol_id)?;
        exported_symbols.push(clone_bitmap(source, limits)?);
        index += 1;
    }
    Ok(exported_symbols)
}

/// Decodes one complete symbol-dictionary segment payload (T.88 6.5, 7.4.3).
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires symbol_text_spec::codebook_valid(input.imported_symbols),
    ensures match out {
        Ok(output) =>
            symbol_text_spec::bitmaps_valid(output.exported_symbols@)
                && output.consumed_bytes <= input.payload@.len(),
        Err(_) => true,
    },
))]
pub(crate) fn decode_symbol_dictionary(
    input: &SymbolDictionaryInput<'_>,
) -> Result<SymbolDictionaryOutput, SymbolTextError> {
    let limits = input.limits;
    let (header, header_len) = parse_symbol_dictionary_header(input.payload, limits)?;
    let config = header.config;

    let imported_count =
        u32::try_from(input.imported_symbols.len()).map_err(|_| MalformedError::IntegerOverflow)?;
    let total_symbols = check_symbol_counts(
        imported_count,
        header.new_symbols,
        header.exported_symbols,
        limits,
    )?;
    let code_length = symbol_id_code_length(total_symbols)?;

    let uses_generic_contexts = !config.huffman && !config.refinement_aggregation;
    let uses_refinement_contexts = config.refinement_aggregation;
    if config.bitmap_context_used {
        let used = input
            .used_contexts
            .ok_or(SymbolTextError::InvalidState("missing retained contexts"))?;
        if uses_generic_contexts && used.generic.is_none() {
            return Err(SymbolTextError::InvalidState(
                "missing retained generic contexts",
            ));
        }
        if uses_refinement_contexts && used.refinement.is_none() {
            return Err(SymbolTextError::InvalidState(
                "missing retained refinement contexts",
            ));
        }
    } else if input.used_contexts.is_some() {
        return Err(SymbolTextError::InvalidState(
            "unexpected retained contexts without the context-used flag",
        ));
    }

    let body = input
        .payload
        .get(header_len..)
        .ok_or(MalformedError::IntegerOverflow)?;

    let refinement_adaptive: [(i8, i8); 2] = [
        (
            i8::try_from(header.refinement_adaptive[0].x)
                .map_err(|_| MalformedError::IntegerOverflow)?,
            i8::try_from(header.refinement_adaptive[0].y)
                .map_err(|_| MalformedError::IntegerOverflow)?,
        ),
        (
            i8::try_from(header.refinement_adaptive[1].x)
                .map_err(|_| MalformedError::IntegerOverflow)?,
            i8::try_from(header.refinement_adaptive[1].y)
                .map_err(|_| MalformedError::IntegerOverflow)?,
        ),
    ];

    let (exported_symbols, retained_generic, retained_refinement, consumed_within_body) =
        if config.huffman {
            let (exported, retained_refinement, consumed) = decode_symbol_dictionary_huffman(
                &header,
                input.imported_symbols,
                body,
                code_length,
                total_symbols,
                refinement_adaptive,
                input.custom_tables,
                limits,
            )?;
            (exported, None, retained_refinement, consumed)
        } else {
            if !input.custom_tables.is_empty() {
                return Err(SymbolTextError::InvalidTableAssociation(
                    "custom tables on an arithmetic symbol dictionary",
                ));
            }
            let (exported, retained_generic, retained_refinement, consumed) =
                decode_symbol_dictionary_arithmetic(
                    &header,
                    input.imported_symbols,
                    body,
                    code_length,
                    total_symbols,
                    refinement_adaptive,
                    input.used_contexts,
                    limits,
                )?;
            (exported, retained_generic, retained_refinement, consumed)
        };

    let retained_contexts = if config.bitmap_context_retained {
        Some(DictionaryContexts {
            generic: retained_generic,
            refinement: retained_refinement,
        })
    } else {
        None
    };

    let consumed_bytes = header_len
        .checked_add(consumed_within_body)
        .ok_or(MalformedError::IntegerOverflow)?;
    Ok(SymbolDictionaryOutput {
        exported_symbols,
        retained_contexts,
        consumed_bytes,
    })
}

use vstd::prelude::*;

verus! {

pub(crate) open spec fn work_units(tracker: &super::ResourceTracker) -> u64 {
    tracker.work_units
}

pub(crate) open spec fn bitmap_bytes(tracker: &super::ResourceTracker) -> u64 {
    tracker.decoded_bitmap_bytes
}

pub(super) open spec fn integer_contexts_valid(
    contexts: &super::IntegerContexts,
) -> bool {
    forall|slot: int, index: int|
        0 <= slot < super::INTEGER_FIELD_COUNT
            && 0 <= index < 512
            ==> crate::arithmetic::arithmetic_spec::context_valid(
                contexts.slots[slot][index]
            )
}

pub(super) open spec fn arithmetic_contexts_valid(
    contexts: Seq<crate::arithmetic::ArithmeticContext>,
) -> bool {
    forall|index: int| 0 <= index < contexts.len()
        ==> crate::arithmetic::arithmetic_spec::context_valid(contexts[index])
}

pub(super) open spec fn arithmetic_codec_valid(
    arithmetic: &super::ArithmeticCodec,
) -> bool {
    crate::arithmetic::arithmetic_spec::decoder_valid(&arithmetic.decoder)
        && integer_contexts_valid(&arithmetic.integers)
        && arithmetic_contexts_valid(arithmetic.iaid@)
}

pub(super) open spec fn codec_valid(codec: &super::SymbolCodec) -> bool {
    match codec {
        super::SymbolCodec::Arithmetic(arithmetic) =>
            arithmetic_codec_valid(&arithmetic),
        super::SymbolCodec::Huffman(huffman) => {
            crate::huffman::huffman_spec::reader_valid(&huffman.reader)
        },
    }
}

pub(super) open spec fn table_bindings_valid(
    bindings: &super::TableBindings,
) -> bool {
    bindings.len <= super::MAX_TABLE_BINDINGS
        && forall|index: int| 0 <= index < super::MAX_TABLE_BINDINGS ==>
            bindings.entries[index].is_some() == (index < bindings.len)
}

pub(super) open spec fn export_indices_valid(
    exported: Seq<u32>,
    total_candidates: u32,
) -> bool {
    forall|index: int| 0 <= index < exported.len()
        ==> #[trigger] exported[index] < total_candidates
}

pub(crate) open spec fn bitmaps_valid(bitmaps: Seq<crate::bitmap::Bitmap>) -> bool {
    forall|index: int| 0 <= index < bitmaps.len()
        ==> crate::spec::bitmap::storage_valid(#[trigger] &bitmaps[index])
}

pub(crate) open spec fn bitmap_refs_valid(
    bitmaps: Seq<&crate::bitmap::Bitmap>,
) -> bool {
    forall|index: int| 0 <= index < bitmaps.len()
        ==> crate::spec::bitmap::storage_valid(#[trigger] bitmaps[index])
}

pub(crate) open spec fn codebook_len(codebook: super::SymbolCodebook) -> nat {
    match codebook {
        super::SymbolCodebook::Contiguous(symbols) => symbols@.len(),
        super::SymbolCodebook::Borrowed(symbols) => symbols@.len(),
    }
}

pub(crate) open spec fn codebook_valid(codebook: super::SymbolCodebook) -> bool {
    match codebook {
        super::SymbolCodebook::Contiguous(symbols) => bitmaps_valid(symbols@),
        super::SymbolCodebook::Borrowed(symbols) => bitmap_refs_valid(symbols@),
    }
}

pub(crate) open spec fn two_bit_choice_result(
    encoded: u8,
    first: crate::huffman::StandardHuffmanTable,
    second: crate::huffman::StandardHuffmanTable,
    choice: super::TableChoice,
) -> bool {
    match encoded {
        0 => choice == super::TableChoice::Standard(first),
        1 => choice == super::TableChoice::Standard(second),
        3 => choice == super::TableChoice::Custom,
        _ => false,
    }
}

pub(crate) open spec fn four_way_choice_result(
    encoded: u8,
    first: crate::huffman::StandardHuffmanTable,
    second: crate::huffman::StandardHuffmanTable,
    third: crate::huffman::StandardHuffmanTable,
    choice: super::TableChoice,
) -> bool {
    match encoded {
        0 => choice == super::TableChoice::Standard(first),
        1 => choice == super::TableChoice::Standard(second),
        2 => choice == super::TableChoice::Standard(third),
        _ => choice == super::TableChoice::Custom,
    }
}

pub(crate) open spec fn dictionary_config_safe(
    flags: u16,
    config: super::SymbolDictionaryConfig,
) -> bool {
    flags & 0xe000 == 0
        && config.huffman == (flags & 1 != 0)
        && config.refinement_aggregation == (flags & 2 != 0)
}

pub(crate) open spec fn text_config_safe(
    flags: u16,
    huffman_flags: Option<u16>,
    colour_extension: bool,
    config: super::TextRegionConfig,
) -> bool {
    !colour_extension
        && config.huffman == (flags & 1 != 0)
        && config.refinement == (flags & 2 != 0)
        && (config.huffman ==> huffman_flags.is_some())
        && (!config.huffman ==> huffman_flags.is_none())
        && valid_strip_size(config.strip_size)
}

pub(crate) open spec fn text_huffman_choices_safe(
    flags: u16,
    refinement: bool,
    choices: super::TextHuffmanChoices,
) -> bool {
    flags & 0x8000 == 0
        && (!refinement ==> flags & 0x7fc0 == 0)
        && (flags & 0x4000 == 0 ==> (
            choices.refinement_size
                == super::TableChoice::Standard(
                    crate::huffman::StandardHuffmanTable::A
                )
        ))
        && (flags & 0x4000 != 0 ==> (
            choices.refinement_size == super::TableChoice::Custom
        ))
}

pub(crate) open spec fn refinement_choice_result(
    encoded: u8,
    choice: super::TableChoice,
) -> bool {
    match encoded {
        0 => choice == super::TableChoice::Standard(
            crate::huffman::StandardHuffmanTable::N
        ),
        1 => choice == super::TableChoice::Standard(
            crate::huffman::StandardHuffmanTable::O
        ),
        3 => choice == super::TableChoice::Custom,
        _ => false,
    }
}

pub(crate) open spec fn floor_half(value: int) -> int {
    value / 2
}

pub(crate) open spec fn signed_byte(value: u8) -> int {
    if value <= 127 { value as int } else { value as int - 256 }
}

pub(crate) open spec fn dictionary_state_valid(
    state: &super::DictionaryClassState,
) -> bool {
    state.decoded_symbols <= state.declared_new_symbols
        && state.width <= state.total_width
}

pub(crate) open spec fn text_state_valid(state: &super::TextRegionState) -> bool {
    state.decoded_instances <= state.declared_instances
}

pub(crate) open spec fn placement_within_extents(
    placement: super::Placement,
    width: u32,
    height: u32,
) -> bool {
    width > 0
        && height > 0
        && placement.x <= placement.x + width as int - 1
        && placement.y <= placement.y + height as int - 1
}

pub(crate) open spec fn prepared_placement_valid(
    prepared: super::PreparedPlacement,
    destination: &crate::bitmap::Bitmap,
) -> bool {
    crate::spec::bitmap::storage_valid(prepared.blit.source)
        && prepared.blit.source_x as nat + prepared.blit.width as nat
            <= prepared.blit.source.width as nat
        && prepared.source_y as nat + prepared.height as nat
            <= prepared.blit.source.height as nat
        && prepared.blit.destination_x as nat + prepared.blit.width as nat
            <= destination.width as nat
        && prepared.destination_y as nat + prepared.height as nat
            <= destination.height as nat
}

pub(crate) open spec fn placement_batch_valid(
    batch: &super::PlacementBatch,
    destination: &crate::bitmap::Bitmap,
) -> bool {
    batch.blits@.len() <= super::MAX_PLACEMENT_BATCH
        && forall|index: int| 0 <= index < batch.blits@.len() ==> {
            let blit = #[trigger] batch.blits@[index];
            crate::spec::bitmap::storage_valid(blit.source)
                && blit.source_x as nat + blit.width as nat
                    <= blit.source.width as nat
                && batch.source_y as nat + batch.height as nat
                    <= blit.source.height as nat
                && blit.destination_x as nat + blit.width as nat
                    <= destination.width as nat
                && batch.destination_y as nat + batch.height as nat
                    <= destination.height as nat
        }
}

pub(crate) open spec fn placement_for_current(
    state: &super::TextRegionState,
    within_strip_t: u8,
    width: u32,
    height: u32,
    placement: super::Placement,
) -> bool {
    match state.strip {
        None => false,
        Some(strip) => {
            let width_extent = width as int - 1;
            let height_extent = height as int - 1;
            let advances_before =
                (!state.config.transposed
                    && (state.config.reference_corner
                        == super::ReferenceCorner::TopRight
                        || state.config.reference_corner
                            == super::ReferenceCorner::BottomRight))
                || (state.config.transposed
                    && (state.config.reference_corner
                        == super::ReferenceCorner::BottomLeft
                        || state.config.reference_corner
                            == super::ReferenceCorner::BottomRight));
            let extent = if state.config.transposed {
                height_extent
            } else {
                width_extent
            };
            let symbol_s = strip.cursor_s as int
                + if advances_before { extent } else { 0 };
            let symbol_t = state.strip_t as int + within_strip_t as int;
            let reference_x = if state.config.transposed { symbol_t } else { symbol_s };
            let reference_y = if state.config.transposed { symbol_s } else { symbol_t };
            placement.x as int == if state.config.reference_corner
                    == super::ReferenceCorner::TopRight
                    || state.config.reference_corner
                        == super::ReferenceCorner::BottomRight
                { reference_x - width_extent } else { reference_x }
                && placement.y as int == if state.config.reference_corner
                    == super::ReferenceCorner::BottomLeft
                    || state.config.reference_corner
                        == super::ReferenceCorner::BottomRight
                { reference_y - height_extent } else { reference_y }
        }
    }
}

pub(crate) open spec fn pow2(exponent: nat) -> nat
    decreases exponent,
{
    if exponent == 0 { 1 } else { 2 * pow2((exponent - 1) as nat) }
}

pub(crate) open spec fn symbol_code_length_valid(count: u32, length: u8) -> bool {
    length <= 32
        && (count <= 1 ==> length == 0)
        && (count > 1 ==> length > 0)
}

pub(crate) open spec fn symbol_code_length_expected(count: u32) -> u8 {
    if count <= 1 { 0 }
    else if count <= 2 { 1 }
    else if count <= 4 { 2 }
    else if count <= 8 { 3 }
    else if count <= 16 { 4 }
    else if count <= 32 { 5 }
    else if count <= 64 { 6 }
    else if count <= 128 { 7 }
    else if count <= 256 { 8 }
    else if count <= 512 { 9 }
    else if count <= 1_024 { 10 }
    else if count <= 2_048 { 11 }
    else if count <= 4_096 { 12 }
    else if count <= 8_192 { 13 }
    else if count <= 16_384 { 14 }
    else if count <= 32_768 { 15 }
    else if count <= 65_536 { 16 }
    else if count <= 131_072 { 17 }
    else if count <= 262_144 { 18 }
    else if count <= 524_288 { 19 }
    else if count <= 1_048_576 { 20 }
    else if count <= 2_097_152 { 21 }
    else if count <= 4_194_304 { 22 }
    else if count <= 8_388_608 { 23 }
    else if count <= 16_777_216 { 24 }
    else if count <= 33_554_432 { 25 }
    else if count <= 67_108_864 { 26 }
    else if count <= 134_217_728 { 27 }
    else if count <= 268_435_456 { 28 }
    else if count <= 536_870_912 { 29 }
    else if count <= 1_073_741_824 { 30 }
    else if count <= 2_147_483_648 { 31 }
    else { 32 }
}

pub(crate) open spec fn generic_template_matches(
    value: u8,
    template: crate::generic_region::GenericTemplate,
) -> bool {
    match value {
        0 => template == crate::generic_region::GenericTemplate::Template0,
        1 => template == crate::generic_region::GenericTemplate::Template1,
        2 => template == crate::generic_region::GenericTemplate::Template2,
        _ => template == crate::generic_region::GenericTemplate::Template3,
    }
}

pub(crate) open spec fn refinement_template_matches(
    value: u8,
    template: crate::generic_region::RefinementTemplate,
) -> bool {
    if value == 0 {
        template == crate::generic_region::RefinementTemplate::Template0
    } else {
        template == crate::generic_region::RefinementTemplate::Template1
    }
}

pub(crate) open spec fn valid_strip_size(size: u8) -> bool {
    size == 1 || size == 2 || size == 4 || size == 8
}

pub(crate) open spec fn strip_log_valid(size: u8, log: u8) -> bool {
    (size == 1 && log == 0)
        || (size == 2 && log == 1)
        || (size == 4 && log == 2)
        || (size == 8 && log == 3)
}

pub(crate) open spec fn export_progress_measure(
    index: nat,
    work: nat,
    total: nat,
    work_limit: nat,
) -> int
    recommends index <= total, work <= work_limit,
{
    (work_limit - work) * (total + 1) + (total - index)
}

pub(crate) open spec fn custom_oob_compatible(
    requires_oob: bool,
    supplied_oob: bool,
) -> bool {
    requires_oob == supplied_oob
}

pub(crate) open spec fn clipped_start(origin: int) -> int {
    if origin < 0 { 0 } else { origin }
}

pub(crate) open spec fn clipped_end(origin: int, extent: nat, bound: nat) -> int {
    let far = origin + extent as int;
    if far < bound as int { far } else { bound as int }
}

}

# Test evidence

## DEV-952C7B7561

- **Mapping:** `cf94e7baaee3eb514973fe47` / `3ca0db7344b1bd7143c6c5fa0ad21ec73ccc524ae78e1b9ec37c9b393fcd0bf6`
- **Specification:** `43484d881cf32e9dce8d6d94`
- **Deviation:** `a5a619ad7186ad24fc88ddb1`
- **Test item:** `7e3fa4bb784ec0e77b3a6344`
- **Case:** `21ca4f10d5d8af3cd1d4d14e`
- **Exact standards quote:**

> When SDHUFF  1, set SBSYMCODES to an array of SBNUMSYMS codes, where the length of each code is:
>
>                                 max(log2 (SDNUMINSYMS  SDNUMNEWSYMS), 1)
>
> and the code SBSYMCODES[I] is I (for I between 0 and SBNUMSYMS – 1).

- **Entry:** `src/symbol_text/dictionary.rs:43:symbol_text::dictionary::decode_symbol_dictionary`
- **Command:** `python3 tools/deepspec_fixed_dictionary_ids.py run`
- **Setup:** All imported Bitmap values are constructed by production bitmap_from_packed_bytes as 8x1 images with stride=1 and length=1, asserted before each call. Counts range from 1 through 32 with valid in-range IDs, one new/exported 16x1 symbol, two unrefined instances, default resource limits and no context reuse/custom tables. The symbol-dictionary payload is an API-level input; no file/transport wrapper is required by this entry. No private storage mutation, out-of-contract helper invocation or production source edit is used.
- **Route:** The additive native dictionary_dispatch regression invokes production decode_symbol_dictionary with SDHUFF=1/SDREFAGG=1 and two instances, reaching decode_symbol_dictionary_huffman and its extra table read. Host-observed branch coverage reaches that exact branch 119 times.
- **Observed assertion:** `count=32 ids=[31, 12]: expected 16x1 [de, 19], got Decode(Malformed(UnexpectedEof { offset: 69, needed: 4, remaining: 3 }))`

**Full diagnosis:**

T.88 (08/2018) 6.5.8.2 sends multi-instance dictionary aggregates to the text decoder with Table 17 parameters. Section 6.5.8.2.3 supplies implicit equal-width index codes, not a 7.4.3.1.7 transmitted table header. Independent inspection of Tables B.1/B.2/B.4/B.6/B.8/B.11 confirms the fixture's DH=1, DW=16, aggregation count=2, initial and strip DT=1, FS=0, DS=1, OOB markers and export runs. The declared imported+new symbol total determines the fixed ID width; both instance IDs address already available imported bitmaps. RI=0 means no refinement arithmetic payload is needed. The expected output is the concatenation of the two original eight-pixel rows, derived from pre-state, not production output. Host replay reaches instances>1 and prepare_symbol_ids! on all 119 inputs (the retained LLVM branch receipt counts 119 true, 0 false at line 178). It rejects every complete encoded challenge as UnexpectedEof while trying to consume the extra table. The import-only control passes. This is a finite reproduced decoder deviation, not an oracle derived from agreement between implementation and its Verus model. The original per-case route marker was truncated from the host's output tail; the retained failure lines, exact native case and branch-coverage receipt still attribute the executed failure. Successful downstream aggregation is not observed, and removing this read alone is not claimed to repair all table bindings.

- **Production evidence:** [`src/symbol_text/dictionary/huffman.rs:L184`](../../src/symbol_text/dictionary/huffman.rs#L184)

```text
prepare_symbol_ids!(huffman, total_symbols, limits)?;
```

- **Production evidence:** [`src/symbol_text/macros.rs:L77`](../../src/symbol_text/macros.rs#L77)

```text
let runcode_lines = read_symbol_id_runcode_lines(&mut codec.reader)?;
```

- **Production evidence:** [`src/symbol_text/dictionary/huffman.rs:L178-L184`](../../src/symbol_text/dictionary/huffman.rs#L178-L184)

```text
                if instances > 1 {
                    let SymbolCodec::Huffman(huffman) = &mut codec else {
                        return Err(SymbolTextError::InvalidState(
                            "Huffman dictionary has non-Huffman state",
                        ));
                    };
                    prepare_symbol_ids!(huffman, total_symbols, limits)?;
```

- **Production evidence:** [`src/symbol_text/macros.rs:L77-L82`](../../src/symbol_text/macros.rs#L77-L82)

```text
        let runcode_lines = read_symbol_id_runcode_lines(&mut codec.reader)?;
        let runcode_table = HuffmanTable::new(&runcode_lines, limits)?;
        let lengths =
            decode_symbol_id_code_lengths(&mut codec.reader, &runcode_table, symbol_count, limits)?;
        codec.symbol_id_table = Some(HuffmanTable::new_symbol_ids(&lengths, limits)?);
        byte_align(&mut codec.reader)?;
```

- **Production evidence:** [`src/symbol_text/dictionary/arithmetic.rs:L203-L214`](../../src/symbol_text/dictionary/arithmetic.rs#L203-L214)

```text
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
```

- **Harness evidence:** [`src/symbol_text/tests/dictionary_dispatch.rs:L252`](tests/DEV-952C7B7561/files/src/symbol_text/tests/dictionary_dispatch.rs#L252): `let result = decode_symbol_dictionary(&SymbolDictionaryInput {`
- **Harness evidence:** [`src/symbol_text/tests/dictionary_dispatch.rs:L231`](tests/DEV-952C7B7561/files/src/symbol_text/tests/dictionary_dispatch.rs#L231): `assert_eq!(bitmap.bytes().len(), 1);`
- **Harness evidence:** [`src/symbol_text/tests/dictionary_dispatch.rs:L286`](tests/DEV-952C7B7561/files/src/symbol_text/tests/dictionary_dispatch.rs#L286): `assert_eq!(checked, 119);`
- **Test source:** [`src/symbol_text/tests/dictionary_dispatch.rs:L252`](tests/DEV-952C7B7561/files/src/symbol_text/tests/dictionary_dispatch.rs#L252)<br>[`src/symbol_text/tests/dictionary_dispatch.rs:L231`](tests/DEV-952C7B7561/files/src/symbol_text/tests/dictionary_dispatch.rs#L231)<br>[`src/symbol_text/tests/dictionary_dispatch.rs:L286`](tests/DEV-952C7B7561/files/src/symbol_text/tests/dictionary_dispatch.rs#L286)
- **Test source:** [`tools/deepspec_fixed_dictionary_ids.py`](tests/DEV-952C7B7561/files/tools/deepspec_fixed_dictionary_ids.py)
- **Exact mapping record:** [report.json](report.json)

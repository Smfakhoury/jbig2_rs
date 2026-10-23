# Verus model and proof-support index

This is a navigation and notation aid, **not normative approval**. The
[statement ledger](../validation/traceability/requirements.json) remains
authoritative for exact clause/figure/table locators, assumptions, contracts
and tests; [traceability](spec-traceability.md) explains evidence strengths.
All 266 selected records remain provisional, the normative denominator remains
incomplete, and no feature has an approved complete decomposition. No standard
prose, table entries, diagrams or official vectors are reproduced here.

## What to read, and what kind of assertion it is

| Layer | Entry points | Audit meaning |
|---|---|---|
| Normative-facing project models | [`specs/`](../specs), especially arithmetic `entry_*`, `refill_step`, `initialized_state`; MMR `exact_eofb`; header/segment and halftone flag models | Candidate translations to compare with the ledger's approved source identities and exact locators, not independently certified standards |
| Project/API semantics and results | Decode charge/release/allocation predicates; document metadata, counts and resource limits; bitmap `black_span_result`, `prefix_copy_result`, `row_copy_result` and `pixel_result` | Consequential success/failure, framing and result guarantees; retain these in the specification surface |
| Representation preconditions | Arithmetic `context_valid` and `decoder_*valid`; decode store/buffer validity; halftone `work_valid`, `bitmaps_valid`, `decoder_state_valid`; Huffman `reader_valid`, `wide_fits`, `line_decodable`; MMR `reader_valid`, `bitmap_storage_valid`, `transitions_ordered`, `reference_line_valid` | Bounds, storage, reachable-state and sentinel assumptions/results, not complete decoding semantics; auditors must inspect their use in production `requires`/`ensures` |
| Implementation accumulator support | [`transition_prefix_valid`](../proofs/model_support/mmr.rs) | Prefix ordering and scan bounds used by private helper contracts and loops, not an exact MMR raster specification |
| Model-access and arithmetic proof support | [`proofs/model_support/`](../proofs/model_support) | Nine relocated proof functions: arithmetic `entry_model_equations`, `initialization_code_bound`; decode `page_fields`, `document_fields`; document `decoder_fields`, `document_fields`, `global_fields`; halftone `work_fields`; Huffman `table_has_oob_definition` |
| Other proof support | [`proofs/shared/`](../proofs/shared), [`proofs/production_impl/`](../proofs/production_impl), [`proofs/spec_properties/`](../proofs/spec_properties) | Representation/termination lemmas, production ghost bridges and local-model properties; file/lemma names do not upgrade evidence strength |

The classification is by meaning, not a `*_valid`/`*_prefix` naming rule.
For example bitmap `prefix_copy_result` is output semantics, whereas
`transition_prefix_valid` is an implementation accumulator invariant. The latter
still requires `scanned <= width`, every stored position strictly below
`scanned`, and nondecreasing stored positions. Its definition remains open with
the same visibility and logical name, `mmr::mmr_spec::transition_prefix_valid`.
Inspect [`src/mmr.rs`](../src/mmr.rs) contracts for `record_transition_span`,
`fill_recorded_span` and `finish_transition_row` alongside that definition.
The finish helper still requires the prefix invariant at `(width, width)`;
its sentinel result is not a full line-pixel theorem.

The six specification files include support **outside** their `verus!` blocks.
Each support file supplies a `verus!` block in the same logical module, not a
child module. Thus closed-model reveal ownership, private-field access, helper
visibility, existing preconditions/postconditions and qualified proof names are
preserved. Moving bodies does not remove these dependencies from review.

## Notation and obligation bridges

The following are original parameter mappings, not transcriptions of algorithms.
Consult each requirement's exact locator in the ledger before judging fidelity.

| Source locator / notation to compare | Project model / production contract | Existing bridge and remaining obligation |
|---|---|---|
| T.88 E.3 registers `A`, `C`, `CT`; input position/current byte | [`ArithmeticDecoder`](../src/arithmetic.rs) fields `interval: u32`, `code: u32`, `bit_count: u8`, `byte_position: usize`, `current_byte: u8`; `MIN_INTERVAL` is the normalization boundary | Explicit field mapping for review, not a proof of normative register interpretation or pointer/caller conventions |
| E.3.5 / Figure E.20 INITDEC alignment `C << 7`, counter adjustment `CT - 7` | [`initialized_state`](../specs/arithmetic.rs) uses `refill.3 as nat * 128`, cast to `u32`, and `refill.4 - 7`; [`ArithmeticDecoder::new`](../src/arithmetic.rs) guarantees exact accepted-input/state correspondence | `new` proves `(1u32 << 7) == 128u32` with `bit_vector` before native `checked_mul(1 << 7)`; [`initialization_code_bound`](../proofs/model_support/arithmetic.rs) bounds every two-byte prefix by `u32::MAX / 128`. Thus scaling does not overflow and the alternate algebraic form agrees; normative translation remains unapproved |
| E.3.4 / Figure E.19 BYTEIN branches | `refill_step` tuple order is `(signal, position, current byte, code, count, marker)`; `byte_in_signal` and `byte_in` expose exact project-step relations | Checked byte positioning, marker/EOF classification and partial overflow state are specified. Review source register/marker meaning, branch alignment and callers independently |
| E.3.2 / Figures E.16–E.17 decision/context exchange; E.2.5 / Table E.1 estimator fields | `entry_decision`, `entry_index`, `entry_mps` read runtime `QE_STATES`; `index` selects a row and `mps` is the context sense. Compare `Qe` → `qe`, `NMPS` → `next_mps`, `NLPS` → `next_lps`, `SWITCH` → `switch_mps` | [`entry_model_equations`](../proofs/model_support/arithmetic.rs) exposes closed-model equations to the production decode proof. It proves consistency with implementation data, **not** an independent Table E.1 translation; complete project register composition is listed below, while independent caller/stream interpretation remains open |
| E.3.2 complete project step | `decision_state` binds private Qe rows and existing decision selection to the scalar `decision_registers`; `exchange_normalization` composes interval replacement with `normalized_state` | Both entry APIs expose exact signal/acceptance and all register/history fields, including partial failures; `decision_model_equations` remains in proof support. Typed `eof_error`/`error_for_signal` are project policy, not normative exception approval. Independent register/table interpretation and complete stream/caller semantics remain open |
| E.3.3 / Figure E.18 renormalization | `renormalization_steps` is a decreasing measure over `interval` | Termination support, not the complete register/byte-consumption recurrence or an end-to-end runtime bound |
| E.3.3 normalization output recurrence | `normalized_state` returns `(signal, interval, position, current byte, code, count, marker)`; it composes the project refill model with mathematical doubling and exact partial-error ordering | Both production helpers expose all output fields and preserved input. Numeric oracles and six compiling output-relation controls supplement the proof; normative register/pointer interpretation and caller composition remain unapproved |
| A.3 IAID prefix and result | `iaid_run` starts from prefix one, composes each exact decision/context update and finite count; `iaid_state` clears the sentinel and exposes project preflight/partial errors | Production `decode_iaid` equals the model for result, full decoder and full context bank. Six compiling contrasts and 3,600 numeric/prior-procedure comparisons supplement this project theorem. The 31-bit cap is project policy; normative caller length, address-space sufficiency and bitstream interpretation remain unapproved |
| A.1/A.2 signed integer procedure | `integer_step_state` exposes decision/decoder/bank/history; `integer_selection_state` enters at level zero and selects one of six count/offset pairs; `integer_bits_run` is the finite payload fold; `integer_state` composes seed/sign, leaf, payload and signed value/OOB | Production `decode_integer` exposes result/full-decoder/full-bank equality, and `decode_integer_bits` also exposes history. Nine compiling contrasts and 31,080 numeric/prior-procedure comparisons supplement these project contracts. Model totality guards are not native behavior. Procedure-bank/storage, fourteen independent Table A.1 representation relations, Qe/register translation and complete caller/stream interpretation remain unapproved |
| Concrete arithmetic caller boundary | `integer_field_slot` is the project enum ordinal; `arithmetic_integer_result` and `arithmetic_symbol_id_result` project values/OOB and exact `decode_error` fields | Both concrete codec methods expose full decoder/selected-bank relations and all other bank/refinement/work frames on every outcome. Eight compiling contrasts and 12,648 direct caller cases supplement the proof. These are project backend guarantees, not independent normative call-site field, symbol-count/SBSYMCODELEN, reset or full-stream approval |
| 7.4.4.1 and 7.4.5.1 selected header fields | `pattern_header_accepted` / `halftone_header_accepted` give exact project success conditions; `*_header_fields` bind raw flags, dimensions/index, signed grid origins and unsigned vector words | Native parsers promise success iff acceptance, exact successful fields and seven/twenty-one byte consumption. The halftone helper excludes previously parsed region information and retains raw 256-scaled quantities. Shared `segments::be_u32_at` supplies word order. Three pure flag/operator helpers become crate-visible solely to expose these postconditions; no native API changes. Nine compiling contrasts supplement the contracts, not independent translation, typed partial-error or full caller/raster approval |
| Bounded primitive reader API failures | `readers::eof_error` gives the exact offset/needed/remaining payload; `bit_read_error` preserves invalid-count priority and value, EOF data or absence of failure | All byte/bit primitives retain their existing value, input and cursor guarantees. Internal `bits_value_bound`, `bits_limit_monotone` and `bits_limit` live in proof support and exclude accumulation overflow after the existing <=32 preflight; native checks remain. Nine compiling contrasts and a new 78-case byte matrix supplement the existing exhaustive bit tests. Failure units follow the respective byte/bit API; no normative rejection or caller/stream approval is inferred |
| T.6 2.2.3 / Table 1 mode lookup | [`mode_code_in_implementation_table`](../specs/mmr.rs) is membership in runtime `MODE_CODES`; successful `decode_mode` binds that mode/length witness and its key bits to the actual consumed old-input `bits_value` | Exact project input-to-code relation, not independent normative table translation or complete acceptance/errors. Prefix composition uses an internal reader append lemma; four compiling wrong-key controls retain old membership/length/bounds |
| T.6 2.2.4 / Tables 2–3 run lookup | `run_code_in_implementation_tables` selects runtime tables; `find_run_code` relates the supplied key and successful `decode_run_code` also binds that key to consumed old-input bits | Exact project lookup-prefix relation, not independent standard-table correctness or complete run accumulation/raster semantics |
| T.6 framing and T.88 6.2.6 omission context | MMR `exact_eofb` relates successful required framing to local `EOFB` | Constant/model equality does not independently review standard framing, padding applicability or caller omission policy |

`mode_code_is_normative` and `run_code_is_normative` were renamed to the
implementation-membership names above; their definitions are otherwise
unchanged. Likewise the existing `normative_run_*` proof helper names establish
only bounds over those implementation tables. Table-driven tests that share
implementation data and synthetic semantic mutants are not normative oracles.

## Evidence and unchanged stop conditions

MMR `horizontal_pair` composes two opposite-colour `complete_run` witnesses
with exact endpoints, progress and total input extent. Its returned endpoints
are those used by the existing raster caller; dispatch and raster/caller
composition remain separate. Erased witness selection and normalization
assertions in the helper are proof support, not extra normative rules.

MMR `complete_run` is output semantics, not merely an accumulator invariant.
Its `run_prefix_matches`, `run_bit_length`, `run_value_sum` and
`run_all_makeups` components define the actual input sequence, consumed extent,
sum and earlier-code structure. Successful `decode_run` proves this relation
using an erased sequence accumulator. These models use implementation tables;
independent T.6 table fidelity, complete acceptance/errors and horizontal-pair/
row/raster/caller interpretation remain separate review obligations.

- Production `verus_spec` contracts in [`src/`](../src) remain the place to check
  actual preconditions and postconditions. A model definition or field-reveal
  bridge alone is not a production functional theorem.
- Existing ordinary regression tests and selected strong/weaker semantic
  controls can establish their stated project-contract sensitivity. The
  25-group/147-contrast required profile is neither added normative coverage nor a fresh
  replay merely because files moved. See the ledger and
  [traceability](spec-traceability.md) for exact linked identities and gaps.
- Unfiltered pinned verification with compilation, lifetime and erasure checks
  is distinct from selected-module debugging and from source-binding checks.
  Every included support file must be source-bound by the evidence tooling.
- Normative/model owner review, full decomposition, functional caller/raster
  composition and independent table review remain open. The existing
  `prepare_transition_storage` capacity-accounting native/proof divergence is
  unchanged; no upstream missing specification is replaced by a local trust
  declaration. The seven unresolved official cases receive no restored
  conformance/rejection credit. Refinement, aggregation and custom-family
  follow-up remains stopped; this index grants no release approval.

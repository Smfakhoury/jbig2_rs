# Verus model and proof-support index

This is a navigation and notation aid, **not normative approval**. The
[statement ledger](../validation/traceability/requirements.json) remains
authoritative for exact clause/figure/table locators, assumptions, contracts
and tests; [traceability](spec-traceability.md) explains evidence strengths.
All 249 selected records remain provisional, the normative denominator remains
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
| T.6 2.2.3 / Table 1 mode lookup | [`mode_code_in_implementation_table`](../specs/mmr.rs) is membership in runtime `MODE_CODES`; successful `decode_mode` has a table witness and consumed length | Not an independent normative table proof or a proof that consumed input bits equal the returned key |
| T.6 2.2.4 / Tables 2–3 run lookup | `run_code_in_implementation_tables` selects the runtime white/black terminating/make-up arrays and common make-up array; `find_run_code` relates a supplied `(bits, length)` key to a returned entry | Membership and key matching, not independent standard-table correctness or complete run accumulation/raster semantics |
| T.6 framing and T.88 6.2.6 omission context | MMR `exact_eofb` relates successful required framing to local `EOFB` | Constant/model equality does not independently review standard framing, padding applicability or caller omission policy |

`mode_code_is_normative` and `run_code_is_normative` were renamed to the
implementation-membership names above; their definitions are otherwise
unchanged. Likewise the existing `normative_run_*` proof helper names establish
only bounds over those implementation tables. Table-driven tests that share
implementation data and synthetic semantic mutants are not normative oracles.

## Evidence and unchanged stop conditions

- Production `verus_spec` contracts in [`src/`](../src) remain the place to check
  actual preconditions and postconditions. A model definition or field-reveal
  bridge alone is not a production functional theorem.
- Existing ordinary regression tests and selected strong/weaker semantic
  controls can establish their stated project-contract sensitivity. The
  17-group/92-contrast required profile is neither added normative coverage nor a fresh
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

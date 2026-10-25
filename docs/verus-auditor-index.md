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
| Project/API semantics and results | Decode charge/release/allocation predicates; document metadata, counts and resource limits; bitmap `black_span_result`, `prefix_copy_result`, `row_copy_result` and `pixel_result`; MMR cache/pixel models plus `raster_step`, `raster_steps`, `raster_rows`, `bitmap_decodes`, `bounded_bitmap_decodes`, `prefix_bitmap_decodes`, `required_eofb_prefix` | Consequential success/failure, input/raster, framing and representation results; retain these in the specification surface without treating project consistency as normative approval |
| Symbol lookup/export results | [`bitmap_contents_match`, `codebook_selects`, `symbol_id_selects`, `exported_contents_match`](../specs/symbol_text.rs) | Exact selected geometry/packed bytes and supplied output order/repeats; valid lookup acceptance has explicit representability conditions. This does not identify encoded export-run flags or prove complete caller/no-substitution/resource semantics |
| Representation preconditions | Arithmetic `context_valid` and `decoder_*valid`; decode store/buffer validity; halftone `work_valid`, `bitmaps_valid`, `decoder_state_valid`; Huffman `reader_valid`, `wide_fits`, `line_decodable`; MMR `reader_valid`, `bitmap_storage_valid`, `transitions_ordered`, `reference_line_valid` | Bounds, storage, reachable-state and sentinel assumptions/results, not complete decoding semantics; auditors must inspect their use in production `requires`/`ensures` |
| Implementation accumulator support | [`transition_prefix_valid`, `row_cache_prefix`, `future_rows_white`, `raster_outer`, `raster_inner`](../proofs/model_support/mmr.rs) | Ordering, pixel/cache framing and packaged loop invariants; unlike the observable finite trace relation, these are not additional normative rules |
| Model-access and arithmetic proof support | [`proofs/model_support/`](../proofs/model_support) | Nine relocated proof functions: arithmetic `entry_model_equations`, `initialization_code_bound`; decode `page_fields`, `document_fields`; document `decoder_fields`, `document_fields`, `global_fields`; halftone `work_fields`; Huffman `table_has_oob_definition`. New MMR `bounded_bitmap_witness`/`prefix_bitmap_witness` are same-owner proved reveals of public closed accepted-result models, not assumptions or native-type visibility changes |
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
its exact result is the old sequence followed by two width sentinels. A separate
composition lemma shows that these appends preserve visible pixel colour; the
actual raster loop establishes row/cache agreement before and after finishing
and swapping live references. This is not a full input-to-line theorem.

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

Document `segment_headers`, `header_byte_offset`, `payload_byte_offset` and the
opaque random-access entry/result predicates describe accepted separate header
and payload sections, exact original windows, increasing numbers and final EOF.
The standalone caller uses the exact file-header body origin. Internal header/
payload histories and proved append/offset/entry/completion support are not
additional normative rules. The native vector for-loop retains its operations
and uses the pinned named-iterator contract to preserve header identity.

Native Rust-derived enum equality was not connected to logical variant identity.
Exactly two EOF comparisons therefore use equivalent `matches!` expressions
instead of adding a `StructuralEq`, local standard specification or assumption.
The compiler-expansion audit permits only those two explicit substitutions,
and all22recognized literal codes check both classifications. Current whole
proof/compilation1231/0 and25real strong/weaker document contrasts include this
no-added-spec source. The shared fresh108/0partial document baseline covers all
12mutated bodies; every strong/weaker query remains separately executed.

The timing repair additionally shares pristine positive baselines across
explicit target sets in six existing arithmetic/helper groups. Narrow
common-prefix selectors still require every exact target identity and a
successful nonempty partial proof. Every mutation and weaker contract is
verified separately at the original limit. The collector checks configured
group membership and per-case baseline coverage; broader positive selection
does not admit unproved source or substitute for the whole-crate gate.

Document `payload_window` binds an accepted payload to its original consumed
slice and, when present, the declared known length. `sequential_offset` and
`sequential_container_fields` describe adjacent header/payload extents covering
the complete accepted body. Standalone sequential results also guarantee
increasing numbers at the exact file-header body origin; embedded results
preserve supplied order without imposing that standalone policy. Unknown-length
termination and decoded `ParsedData` are not proved by that sequential layer.
`sequential_windows` is an internal loop history in model support, not a
normative-facing rule. The ordinary proved completion lemma unfolds the opaque
result and relates it to that history; the native bridge consumes its result
without unfolding it a second time. This keeps genuine result-corruption
queries within the unchanged solver budget without removing any guarantee.

MMR `horizontal_pair` composes two opposite-colour `complete_run` witnesses
with exact endpoints, progress and total input extent. Its returned endpoints
are those used by the existing raster caller; dispatch and raster/caller
composition remain separate. Erased witness selection and normalization
assertions in the helper are proof support, not extra normative rules.

MMR `reference_position_eligible`, `first_reference_candidate` and
`reference_pair_matches` expose project changing-element geometry and parity:
positions strictly after `x`, equality when `initial` is set, even-index
post-transition black, the following `b2`, and width-sentinel fallback.
The native first call uses `(x = 0, initial = true)` for the imaginary start;
that mapping still needs independent comparison with T.6 notation.
`reference_cursor_matches` and `reference_prefix_skipped` are internal cache
optimization support. The verified suffix/global bridge is used by the real
raster caller, which maintains its skipped prefix across position/flag updates.
Those search models alone do not establish previous-row pixel correspondence.
The separate `row_cache_matches` relation now supplies that actual loop
composition: colour at `x` is odd parity of transitions at or before `x`,
including meaningful zero-position changes but excluding width sentinels from
visible pixels. `row_cache_prefix` and `future_rows_white` remain internal
support. Final physical scratch-buffer roles depend on height parity because
the native code swaps mutable references, not vector contents; no exported
parity result or complete input/mode-to-raster conformance is claimed.

The later accepted-result layer uses ghost `RasterState`, `RasterStep` and
`RasterRow` values to state actual input/mode/run/reference/cache/colour/cursor
relationships and every completed row's visible pixels. These finite
declarative histories belong to the **observable result** surface, not merely
loop support. Row starts and next references chain through the preceding
completed history; steps are admitted only before the coding position reaches
width. `transition_span_state` is the shared deterministic update in the
specification file. The old private `transition_span_result` remains a
one-line tuple-equality bridge to that same definition; no second algorithm is
silently introduced in proof support.

`required_eofb_prefix` spells out the immediate24-bit marker and0..7zero
alignment bits to an input-bounded octet boundary. The public bounded/prefix
result models are closed to preserve the private native Mode type; ordinary
proved same-owner witness helpers expose their successful producer contracts.
No new external declaration or proof exclusion is used. Actual whole proof/
compilation1219/0 includes those helpers, not just selected debug targets.
Complete valid-input acceptance/errors, independent normative table/coordinate/
policy interpretation and broader codec/container/resource composition remain
open despite this stronger accepted-input-to-raster result.

Decode `first_symbol_index`, `symbol_group_count`, `referred_symbol_count` and
`collected_symbol_count` are project lookup/count semantics. The full result
uses the complete reference length; prefix calls support the native loops.
Repeated references count again and missing groups contribute zero, without
asserting that a normative caller may supply an invalid reference. The existing
selection layer now additionally binds each contiguous/borrowed codebook
position, imported/new ID and owned export position to exact dimensions,
stride and all stored bytes. The producer uses the already verified fallible
`clone_bitmap`; no infallible Clone or standard specification is introduced.
The raw oracle uses independent literal descriptors rather than that producer
helper for expected values, and checks both representations, repeated/shuffled
selections and owned-buffer separation. Physical addresses/capacity are not
part of the formal content relation. Encoded selection/order and complete
dictionary/refinement/rendering pipelines still require separate proof.

The existing
codebook-length model is reused. These are not sequence-content/order or
observed-capacity theorems, and the export-order ledger record stays state-only.

MMR `span_result` is conditional project output semantics: the exact existing
primitive black-span relation or an unchanged white span. It is not a theorem
that arbitrary old white-span pixels are zero. `transition_span_result` lives
under model support because its exact vector append/emitted-state result
describes the internal transition cache, not a complete normative raster rule.
The existing weaker wrapper preconditions are retained.

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
  27-group/164-contrast required profile is neither added normative coverage nor a fresh
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

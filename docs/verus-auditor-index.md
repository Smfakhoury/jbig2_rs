# Verus auditor index

This is a model/navigation aid, **not normative approval**. The
[ledger](../validation/traceability/requirements.json) owns exact requirement
locators, assumptions, contracts and test identities. Its selected provisional
records are not an exhaustive denominator. Read
[traceability](spec-traceability.md) for strength labels,
[limitations](verifier-limitations.md) for trust/exact-source boundaries and
[publication readiness](publication-readiness.md) for accepted revision-bound
runs. No standard text, tables, diagrams or vectors are reproduced here.

## Layers to inspect

| Layer | Entry points | Audit interpretation |
| --- | --- | --- |
| Candidate normative-facing models | [`specs/`](../specs) | Original project translations to compare independently with approved source identities/locators |
| Observable project results | Arithmetic full-state relations; bitmap content/frame results; MMR finite raster histories; symbol content; container windows/metadata | Consequential output semantics under stated preconditions, not approval of the translation |
| Representation invariants | Arithmetic context/decoder validity; bitmap storage; decode stores; halftone work; Huffman widths/readers; MMR transitions/reference lines | Bounds, reachable state and representation requirements, not complete decoding semantics |
| Model-access and accumulator support | [`proofs/model_support/`](../proofs/model_support) | Same-owner proved reveals, field bridges and loop bookkeeping, not new normative rules or assumptions |
| Production proof bodies | [`proofs/production_impl/`](../proofs/production_impl), actual `verus_spec` contracts in [`src/`](../src) | Where to inspect the implementation's real preconditions/postconditions and proved bridges |
| Shared/local-model properties | [`proofs/shared/`](../proofs/shared), [`proofs/spec_properties/`](../proofs/spec_properties) | Representation/termination and model theorems; names do not upgrade evidence strength |

Classification follows meaning, not a `*_valid` or `*_prefix` suffix.
`prefix_copy_result` describes observable bytes; `transition_prefix_valid`
describes an internal accumulator. Support included outside a specification's
`verus!` block supplies another block in the **same logical module**, preserving
closed-model reveal ownership, private-field access and qualified names. Moving
proof bodies does not remove them from the evidence source boundary.

## Arithmetic notation and obligations

Read [`specs/arithmetic.rs`](../specs/arithmetic.rs),
[`src/arithmetic.rs`](../src/arithmetic.rs) and
[`proofs/model_support/arithmetic.rs`](../proofs/model_support/arithmetic.rs)
together.

| Standard locator / notation | Project bridge | What remains separate |
| --- | --- | --- |
| T.88 E.3 registers `A`, `C`, `CT` | `ArithmeticDecoder.interval`, `.code`, `.bit_count`; input position/current byte are `.byte_position`, `.current_byte`; `MIN_INTERVAL` is normalization boundary | Independent register, pointer and caller interpretation |
| E.3.5 / Fig. E.20 INITDEC | `initialized_state` scales refill code by 128 and subtracts 7 from count; `initialization_code_bound` proves native checked scaling cannot overflow for the accepted prefix | Normative alignment and stream interpretation, not just algebraic equality |
| E.3.4 / Fig. E.19 BYTEIN | `refill_step` tuple is `(signal, position, current byte, code, count, marker)`; entry helpers expose exact project branches and partial overflow/EOF state | Independent marker/fill meaning and complete callers |
| E.2.5 / Table E.1 | `Qe → qe`, `NMPS → next_mps`, `NLPS → next_lps`, `SWITCH → switch_mps`; `index` selects implementation `QE_STATES`, `mps` gives context sense | Independent table translation; `entry_model_equations` only connects the implementation data |
| E.3.2 / Figs. E.16–E.17 | `decision_state`, `decision_registers`, `exchange_normalization` compose selected entry, exchange and normalization into complete register/history results | Normative table/register/caller approval |
| E.3.3 / Fig. E.18 | `renormalization_steps` is a measure; `normalized_state` is the output recurrence `(signal, interval, position, byte, code, count, marker)` | A measure alone is not the recurrence or a global CPU bound |
| A.3 IAID | `iaid_run` begins at prefix one and composes finite exact decisions; `iaid_state` removes the sentinel and includes project preflight/partial errors | Caller SBSYMCODELEN, address-space sufficiency, normative bitstream meaning; 31-bit cap is policy |
| A.1/A.2 integer | `integer_step_state`, `integer_selection_state`, `integer_bits_run`, `integer_state` compose seed/sign, six leaf count/offset choices, payload/history and signed result/OOB | Procedure-bank storage, Table A.1 representations and full caller interpretation |
| Concrete codec boundary | `integer_field_slot`, `arithmetic_integer_result`, `arithmetic_symbol_id_result` and `decode_error` bind selected bank/result/errors and all other bank/refinement/work frames | Correct normative field choice, symbol count/reset timing and full dictionary/text stream |

Arithmetic reset quantifies zero index/MPS over the entire selected slice,
preserves length and needs no valid incoming contexts. Numeric state oracles
and compiling strong/weaker controls complement project theorems; neither
independently certifies the normative translation. Typed diagnostic fields and
priority are project policy, not standard-mandated Rust exceptions.

## MMR: code, reference, raster and framing layers

Entry points: [`specs/mmr.rs`](../specs/mmr.rs),
[`src/mmr.rs`](../src/mmr.rs),
[`proofs/model_support/mmr.rs`](../proofs/model_support/mmr.rs).

- T.6 2.2.3/Table 1 and 2.2.4/Tables 2–3:
  `mode_code_in_implementation_table` and
  `run_code_in_implementation_tables` mean membership in runtime tables.
  Lookup contracts bind keys to actual consumed old-input bits and lengths,
  not independent normative table fidelity.
- `complete_run` is an output relation: `run_prefix_matches`,
  `run_bit_length`, `run_value_sum` and `run_all_makeups` describe actual codes,
  consumed extent, sum and earlier make-ups. `horizontal_pair` composes two
  opposite-colour runs, endpoints and progress. Their proof sequence
  accumulators are internal support.
- `reference_position_eligible`, `first_reference_candidate` and
  `reference_pair_matches` describe strict-after-`x` selection, equality for
  `initial`, even-index post-transition black, following `b2` and width fallback.
  The native first call `(x = 0, initial = true)` models the imaginary start;
  independent T.6 coordinate comparison remains open.
- `reference_cursor_matches` / `reference_prefix_skipped` justify cached
  search. `row_cache_matches` separately connects actual previous-row pixels:
  visible colour is odd parity of transitions at/before `x`, including meaningful
  zero-position changes but excluding width sentinels.
- `transition_prefix_valid` requires `scanned <= width`, nondecreasing stored
  positions strictly below `scanned`. Finishing appends two width sentinels;
  composition proves visible pixels unchanged. `row_cache_prefix`,
  `future_rows_white`, `raster_outer` and `raster_inner` remain loop support.
- Observable `RasterState`, `RasterStep`, `RasterRow`, `raster_step`,
  `raster_steps`, `raster_rows`, `bitmap_decodes`, `bounded_bitmap_decodes`
  and `prefix_bitmap_decodes` relate actual consumed input/mode/run/reference/
  cache/colour/cursor steps and completed row pixels. Rows chain through prior
  completed histories; steps occur only while coding position is below width.
  These finite result relations are not merely accumulator invariants.
- `transition_span_state` is the shared deterministic update;
  private `transition_span_result` is its tuple-equality bridge, not a second
  proof-only raster algorithm. `span_result` promises exact black effects or a
  white no-op; it does not make arbitrary existing white-span pixels zero.
- `required_eofb_prefix` describes the immediate 24-bit marker and zero to seven
  zero alignment bits to an input-bounded octet boundary. Closed bounded/prefix
  result predicates preserve the private native `Mode`; ordinary proved
  same-owner witnesses expose successful producer contracts without assumptions.

Mutable scratch references swap by row, so final physical buffer roles depend
on height parity; no exported capacity/address/parity theorem is inferred.
Complete valid-input acceptance/errors, independent table/coordinate fidelity,
T.88 omission/fixed-block padding applicability and full caller/resources remain
open. Renaming former `*_is_normative` helpers to implementation-membership
names removes an overclaim; it does not add normative evidence.

## Readers, bitmaps and selection

- [`specs/huffman.rs`](../specs/huffman.rs): `canonical_assignment` composes
  original raw-line content/histogram/extent, checked-wide first-prefix
  recurrence, active bucket starts/range checks and stable original index order.
  `HuffmanTable::new` and successful Borrowed `from_line_source` expose the
  result; other source kinds, encoded parsing, lookup/entropy decoding,
  mathematical prefix-freedom and independent normative/model approval remain
  separate. `WideCode::zero` is one literal executable constructor on both paths,
  with its zero postcondition proved from initialization; derived `Default`
  remains available. The `huffman-assignment` checker independently corrupts
  returned code/start/order fields while retaining the other result clauses,
  and checks the zero seed; all four cases also have native mutation bindings.

- [`specs/readers.rs`](../specs/readers.rs): `eof_error` and `bit_read_error`
  state exact API units/payload/priority alongside preserved bytes/cursors.
  Internal `bits_value_bound`, `bits_limit_monotone`, `bits_limit` justify
  accumulation after the existing <=32 preflight; native checks remain.
- [`specs/bitmap.rs`](../specs/bitmap.rs): copy/blank/fill/padding relations
  describe exact packed bytes and frames, with visible-pixel bridges.
  Black-span and Boolean byte/word results are consequential helper semantics;
  full placement/clipping/shift/mask/caller interpretation remains separate.
- [`specs/decode.rs`](../specs/decode.rs): `first_symbol_index`,
  `symbol_group_count`, `referred_symbol_count`, `collected_symbol_count` give
  project selection/count semantics. Repeated references count again and
  missing groups contribute zero without approving invalid normative callers.
- [`specs/symbol_text.rs`](../specs/symbol_text.rs):
  `bitmap_contents_match`, `codebook_selects`, `symbol_id_selects`,
  `exported_contents_match` bind contiguous/borrowed and imported/new selections
  to geometry, stride, every packed byte, output order and repetitions.
  Fallible verified `clone_bitmap` supplies owned results; physical capacity
  and encoded export-run flags/full refinement/rendering are not established.

## Header, container and typed-metadata composition

Read [`specs/segments.rs`](../specs/segments.rs),
[`specs/document.rs`](../specs/document.rs) and their production callers.

- `pattern_header_accepted` / `halftone_header_accepted` state exact project
  acceptance; field models bind raw flags, dimensions/index, signed grid origins
  and unsigned vector words. Exact consumption is seven/twenty-one bytes after
  the relevant entry position; the halftone helper excludes earlier region
  information. Raw 256-scaled values are not a full raster interpretation.
- `payload_window`, `sequential_offset`, `sequential_container_fields` bind
  adjacent original header/payload extents and complete accepted body.
  Standalone adds file-header body origin and increasing numbers; embedded
  preserves supplied order without that policy. `sequential_windows` is an
  internal history, not a normative-facing result.
- `segment_headers`, `header_byte_offset`, `payload_byte_offset` and opaque
  random-access entry/result predicates bind separate original storage sections,
  deterministic offsets, increasing numbers and final EOF. Proved append/
  offset/completion support composes them through the unchanged native traversal.
- `page_information_fields` binds dimensions, height encoding, resolutions and
  flags. `profiles_payload_fields` binds count and every ordered raw ID.
  Append frames separately preserve old prefix, new value and reader extent:
  empty old prefixes must not make new-value clauses vacuous.
- `known_segment_data_fields` / `segment_metadata` connect those bytes to typed
  Page/Stripe/Profiles results and both container outputs, including empty
  EOP/EOF and accepted extension flags. Opaque table/entropy results do not prove
  codec validity.
- `stripe_accepted` / `stripe_result` bind the existing PageState row transition,
  including first distance from zero, exact typed rejection and complete failure
  frames. `page_finish_result` preserves duplicate-first error priority.
  `public_page_height` / `public_page_fields` bind known-height precedence,
  checked last-row-plus-one inference and successful public metadata fields.
  No new preconditions or sequencing checks are introduced: adding a stripe does
  not inspect `ended`, and conversion does not require it.

Opaque result predicates are unfolded by ordinary proved completion helpers;
native bridges consume those results without replacing them with loop histories.
Equivalent explicit enum pattern matches avoid introducing a trusted derived
equality specification. Unknown-length termination, complete errors/resources,
profile meaning, caller page/stripe lifecycle and full decoded output remain separate.

## Audit stop conditions

Full pinned verification **and compilation** are separate from selected-module
debugging, source binding, runtime tests and mutation sensitivity. Every included
support file must participate in source identity.

Do not infer standard/model approval, conformance or observed-capacity enforcement
from stronger project contracts. The held getter boundary, unresolved official
positives, retention conflict and protected encoded-family fixtures remain
blocked as recorded in [publication readiness](publication-readiness.md).

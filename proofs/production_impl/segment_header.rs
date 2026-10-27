// Source-only author navigation; named purposes are not normative approval.
// Guide, official acquisition and source identity: ../../specs/README.md
// All requirement links and inherited locators: ../../validation/traceability/requirements.json
// REQUIREMENT CONTEXT is not a direct normative equation, including for proof/policy helpers.

use vstd::prelude::*;

// Model navigation: macro::production_reference_width [proof-support]
// Purpose: Reference width, retention order and actual parsed-header field proof bridges.
// Audit: reference-width-facts
// REQUIREMENT CONTEXT: T88-SEGMENT-REFERENCE-WIDTH | ITU-T T.88 (08/2018) | 7.2.5
// Fidelity: unresolved; no independent normative approval.
macro_rules! production_reference_width {
    ($segment_number:ident) => {
        proof! {
            segment_header_shared::reference_width_runtime_fact($segment_number);
        }
    };
}

macro_rules! production_count_code_partition {
    ($count_code:ident) => {
        proof! {
            segment_header_shared::count_code_runtime_fact($count_code);
        }
    };
}

macro_rules! production_segment_short_flags {
    ($reader:ident, $start:ident, $bytes:ident) => {
        proof! {
            vstd::assert_seqs_equal!(
                $bytes@, $reader.bytes@.subrange($start as int, $start as int + 1),
            );
        }
    };
}

// Model navigation: macro::production_segment_header_fields [proof-support]
// Purpose: Reference width, retention order and actual parsed-header field proof bridges.
// Audit: segment-header-field-proof
// REQUIREMENT CONTEXT: T88-SEGMENT-TYPE-FIELD | ITU-T T.88 (08/2018) | 7.2.3 bits 0-5; 7.3
// Fidelity: unresolved; no independent normative approval.
macro_rules! production_segment_header_fields {
    () => {
        proof! {
            reveal(crate::spec::segments::segment_header_fields);
        }
    };
}

macro_rules! production_segment_count_fields {
    ($reader:ident, $start:ident, $count:ident, $retention:ident) => {
        proof! {
            assert($count == crate::spec::segments::header_reference_count(
                $reader.bytes@, $start as int,
            ));
            assert($reader.position as int == crate::spec::segments::header_references_start(
                $reader.bytes@, $start as int,
            ));
            assert(crate::spec::segments::retention_values_match(
                $retention.values@,
                $reader.bytes@.subrange(
                    crate::spec::segments::header_retention_start($reader.bytes@, $start as int),
                    $reader.position as int,
                ),
            ));
        }
    };
}

// Model navigation: macro::production_segment_reference_fields [proof-support]
// Purpose: Reference width, retention order and actual parsed-header field proof bridges.
// Audit: segment-reference-field-proof
// REQUIREMENT CONTEXT: T88-SEGMENT-RETENTION-WIRE-ORDER | ITU-T T.88 (08/2018) | 7.2.4
// Fidelity: unresolved; no independent normative approval.
macro_rules! production_segment_reference_fields {
    ($reader:ident, $start:ident, $count:ident, $number:ident, $width:ident,
        $references_begin:ident, $references:ident, $retention:ident $(,)?) => {
        proof! {
            let data = $reader.bytes@;
            let flags_start = crate::spec::segments::header_retention_start(data, $start as int);
            let references_start = crate::spec::segments::header_references_start(data, $start as int);
            assert($references@.len() == $count);
            assert(references_start == $references_begin as int);
            assert(flags_start <= references_start <= $reader.position <= data.len());
            assert($count <= 0x1fff_ffff);
            assert(data[$start as int] >> 5 <= 4 || data[$start as int] >> 5 == 7);
            assert(data[$start as int] >> 5 <= 4 || $start + 4 <= data.len());
            assert(if data[$start as int] >> 5 == 7 {
                $count > 4
            } else {
                crate::spec::segments::short_count_valid(data[$start as int])
            });
            assert($retention.values@.len() == $count as nat + 1);
            assert(data[$start as int] >> 5 == 7 ==>
                crate::spec::segments::unused_retention_bits_zero(
                    ($count + 1) as u32, data.subrange(flags_start, references_start),
                ));
            assert($reader.position as int == references_start
                + $references@.len() * crate::spec::segments::reference_width($number));
            assert forall|index: int| 0 <= index < $references@.len() implies
                #[trigger] $references@[index] < $number
                    && $references@[index] == crate::spec::segments::reference_value_at(
                        data, references_start + index * $width, $width,
                    ) by {
                assert(0 <= index < $count);
                assert($references@[0 + index] < $number);
                assert($references@[0 + index]
                    == crate::spec::segments::reference_value_at(
                        data, $references_begin + index * $width, $width,
                    ));
            }
            assert(crate::spec::segments::referred_header_fields(
                $reader.bytes@, $start as int, $number,
                $references@, $retention.values@, $reader.position as int,
            ));
        }
    };
}

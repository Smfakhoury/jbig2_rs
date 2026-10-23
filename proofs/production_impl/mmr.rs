use vstd::prelude::*;

macro_rules! production_mmr_span_byte {
    ($bitmap:ident, $row:ident, $x:ident) => {
        proof! {
            crate::mmr::mmr_shared::span_byte_is_in_bounds($bitmap, $row, $x);
        }
    };
}

macro_rules! production_mmr_run_entry {
    (
        $table:ident,
        $code:ident,
        $index:ident,
        $black:ident,
        $table_black:expr,
        $bits:ident,
        $length:ident
    ) => {
        proof! {
            assert($table@[$index as int] == $code);
            assert($table@.contains($code));
            reveal(crate::mmr::mmr_spec::run_code_in_implementation_tables);
            reveal(crate::mmr::mmr_spec::run_key_matches);
            assert($black == $table_black);
            assert(crate::mmr::mmr_spec::run_code_in_implementation_tables(
                $black,
                $code,
            ));
            assert(crate::mmr::mmr_spec::run_key_matches(
                $code,
                $bits,
                $length,
            ));
        }
    };
}

macro_rules! production_mmr_mode_entry {
    ($code:ident, $index:ident, $prefix:ident, $length:ident) => {
        proof! {
            assert(crate::mmr::MODE_CODES@[$index as int] == $code);
            assert(crate::mmr::MODE_CODES@.contains($code));
            reveal(crate::mmr::mmr_spec::mode_code_in_implementation_table);
            reveal(crate::mmr::mmr_spec::mode_matches);
            assert(crate::mmr::mmr_spec::mode_code_in_implementation_table($code));
            assert(crate::mmr::mmr_spec::mode_matches(
                $code,
                $prefix,
                $length,
            ));
        }
    };
}

macro_rules! production_mmr_run_code_bounds {
    ($black:ident, $code:ident) => {
        proof! {
            crate::mmr::mmr_properties::normative_run_length_is_bounded(
                $black,
                $code,
            );
        }
    };
}

macro_rules! production_mmr_makeup_bounds {
    ($black:ident, $code:ident) => {
        proof! {
            crate::mmr::mmr_properties::normative_run_value_is_positive_when_makeup(
                $black,
                $code,
            );
            crate::mmr::mmr_properties::normative_run_value_is_bounded(
                $black,
                $code,
            );
        }
    };
}

macro_rules! production_mmr_reference_bounds {
    ($transitions:ident, $index:ident, $next_index:ident, $position:ident, $b2:ident, $width:ident) => {
        proof! {
            reveal(crate::mmr::mmr_spec::reference_line_valid);
            reveal(crate::mmr::mmr_spec::transitions_ordered);
            assert($transitions@[$index as int] == $position);
            assert($transitions@[$next_index as int] == $b2);
            assert($position <= $b2);
            assert($b2 <= $width);
        }
    };
}

macro_rules! production_mmr_transition_empty {
    ($width:ident) => {
        proof! {
            crate::mmr::mmr_shared::transition_prefix_empty($width);
        }
    };
}

macro_rules! production_mmr_transition_finish {
    ($before:ident, $width:ident) => {
        proof! {
            crate::mmr::mmr_shared::transition_prefix_finish(
                $before,
                $width,
            );
        }
    };
}

macro_rules! production_mmr_transition_span {
    ($before:ident, $start:ident, $end:ident, $width:ident, $pushed:ident) => {
        proof! {
            crate::mmr::mmr_shared::transition_prefix_span(
                $before,
                $start,
                $end,
                $width,
                $pushed,
            );
        }
    };
}

macro_rules! production_mmr_pass_progress {
    ($x:ident, $b2:ident, $width:ident) => {
        proof! {
            crate::mmr::mmr_properties::pass_transition_bounds(
                $x,
                $b2,
                $width,
            );
        }
    };
}

macro_rules! production_mmr_horizontal_progress {
    ($x:ident, $width:ident, $first:ident, $second:ident) => {
        proof! {
            crate::mmr::mmr_properties::horizontal_transition_bounds(
                $x,
                $width,
                $first,
                $second,
            );
        }
    };
}

macro_rules! production_mmr_vertical_progress {
    ($x:ident, $target:ident, $width:ident, $black:ident) => {
        proof! {
            crate::mmr::mmr_properties::vertical_transition_progress(
                $x,
                $target,
                $width,
                $black,
            );
        }
    };
}

macro_rules! production_mmr_mode_budget {
    ($count:ident, $maximum:ident) => {
        proof! {
            crate::mmr::mmr_properties::mode_budget_decreases(
                $count,
                $maximum,
            );
        }
    };
}

macro_rules! production_mmr_row_termination {
    ($row:ident, $height:ident) => {
        proof! {
            crate::mmr::mmr_shared::row_counter_terminates(
                $row,
                $height,
            );
        }
    };
}

macro_rules! production_mmr_memory_bound {
    ($bitmap_bytes:ident, $transition_bytes:ident, $all_transition_bytes:ident, $limits:ident) => {
        proof! {
            crate::mmr::mmr_properties::managed_memory_components_bounded(
                $bitmap_bytes,
                $transition_bytes,
                $all_transition_bytes,
                $limits.max_decoder_managed_memory_bytes,
            );
        }
    };
}

macro_rules! production_mmr_trace_begin {
    ($rows:ident) => {
        proof_decl! {
            let ghost mut $rows = Seq::<crate::mmr::mmr_spec::RasterRow>::empty();
        }
    };
}

macro_rules! production_mmr_trace_row_start {
    ($start:ident, $steps:ident, $reader:ident) => {
        proof_decl! {
            let ghost $start = $reader.bit_position as nat;
            let ghost mut $steps = Seq::<crate::mmr::mmr_spec::RasterStep>::empty();
        }
    };
}

macro_rules! production_mmr_trace_before {
    ($before:ident, $reader:ident, $x:ident, $black:ident, $cache:ident, $emitted:ident) => {
        proof_decl! {
            let ghost $before = crate::mmr::mmr_spec::raster_state(
                $reader.bit_position as nat, $x, $black, $cache@, $emitted,
            );
        }
    };
}

macro_rules! production_mmr_trace_mode_end {
    ($end:ident, $reader:ident) => {
        proof_decl! { let ghost $end = $reader.bit_position as nat; }
    };
}

macro_rules! production_mmr_trace_step {
    (
        $data:ident, $reference:ident, $width:ident, $start:ident, $steps:ident,
        $before:ident, $mode:expr, $mode_end:ident, $b1:ident, $b2:ident, $split:ident,
        $reader:ident, $x:ident, $black:ident, $cache:ident, $emitted:ident
    ) => {
        proof! {
            let step = crate::mmr::mmr_spec::RasterStep {
                before: $before,
                after: crate::mmr::mmr_spec::raster_state(
                    $reader.bit_position as nat, $x, $black, $cache@, $emitted,
                ),
                mode: $mode,
                mode_end: $mode_end,
                b1: $b1,
                b2: $b2,
                split: $split,
            };
            assert(crate::mmr::mmr_spec::raster_step(
                $data@, $reference@, $width, $steps.len() == 0, step,
            ));
            crate::mmr::mmr_properties::raster_step_append(
                $data@, $reference@, $width, $start, $steps, step,
            );
            $steps = $steps.push(step);
        }
    };
}

macro_rules! production_mmr_trace_row_complete {
    ($row:ident, $start:ident, $steps:ident, $reader:ident, $x:ident,
     $black:ident, $cache:ident, $emitted:ident) => {
        proof_decl! {
            let ghost $row = crate::mmr::mmr_spec::RasterRow {
                start: $start,
                steps: $steps,
                state: crate::mmr::mmr_spec::raster_state(
                    $reader.bit_position as nat, $x, $black, $cache@, $emitted,
                ),
            };
        }
    };
}

macro_rules! production_mmr_trace_row_push {
    ($data:ident, $width:ident, $bitmap:ident, $rows:ident, $row:ident) => {
        proof! {
            crate::mmr::mmr_properties::raster_row_append($data@, $width, $rows, $row);
            crate::mmr::mmr_properties::rows_cache_append(&$bitmap, $rows, $row);
            $rows = $rows.push($row);
        }
    };
}

macro_rules! production_mmr_trace_output {
    ($data:ident, $width:ident, $height:ident, $bitmap:ident, $reader:ident, $rows:ident) => {
        proof! {
            assert($rows.len() == $height);
            assert(crate::mmr::mmr_spec::raster_rows($data@, $width, $rows));
            assert(crate::mmr::mmr_spec::rows_cache_pixels(&$bitmap, $rows));
            assert($reader.bit_position as nat == crate::mmr::mmr_spec::rows_position($rows));
            assert(crate::mmr::mmr_spec::bitmap_decodes(
                $data@, $width, $height, &$bitmap, $reader.bit_position as nat,
            ));
        }
    };
}

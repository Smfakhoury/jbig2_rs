use vstd::prelude::*;

macro_rules! production_page_information_count_step {
    ($segments:expr, $index:expr) => {
        proof! {
            reveal_with_fuel(document_spec::page_information_count, 1);
            assert(
                document_spec::page_information_count(($segments)@, ($index) as nat + 1)
                    == document_spec::page_information_count(($segments)@, ($index) as nat)
                        + if ($segments)@[($index) as int].header.page_association != 0
                            && ($segments)@[($index) as int].header.segment_type
                                == SegmentType::PageInformation {
                            1nat
                        } else {
                            0nat
                        }
            );
        }
    };
}

macro_rules! production_page_info_fields {
    ($page:ident) => {
        proof! {
            crate::views::page_info_fields($page);
        }
    };
}

macro_rules! production_decoder_fields {
    ($decoder:expr) => {
        proof! {
            document_spec::decoder_fields($decoder);
        }
    };
}

macro_rules! production_document_fields {
    ($document:expr) => {
        proof! {
            document_spec::document_fields($document);
        }
    };
}

macro_rules! production_global_fields {
    ($globals:expr) => {
        proof! {
            document_spec::global_fields($globals);
        }
    };
}

macro_rules! production_document_policy_bundle {
    () => {
        proof! {
            document_shared::prove_document_policy_bundle();
        }
    };
}

macro_rules! production_sequential_window_append {
    ($input:ident, $segments:ident, $old_segments:ident, $windows:ident,
        $header_start:ident, $payload_start:ident, $reader:ident, $ordered:ident) => {
        proof! {
            let segment = $segments@.last();
            document_spec::sequential_window_append(
                $input@, $old_segments, $windows, $header_start, segment,
                $payload_start, $reader.position as int, $ordered,
            );
            $windows = $windows.push((
                $header_start, $payload_start, $reader.position as int,
            ));
        }
    };
}

macro_rules! production_sequential_complete {
    ($input:ident, $segments:ident, $windows:ident, $reader:ident, $ordered:ident) => {
        proof! {
            assert($reader.position == $input@.len());
            document_spec::sequential_complete_fields($input@, $segments@, $windows, $ordered);
        }
    };
}

macro_rules! production_random_header_append {
    ($input:ident, $headers:ident, $windows:ident, $before:ident, $reader:ident) => {
        proof! {
            document_spec::random_header_window_append(
                $input@, $before.1, $windows, $before.0,
                $headers@.last(), $reader.position as int,
            );
            $windows = $windows.push(($before.0, $reader.position as int));
        }
    };
}

macro_rules! production_random_payload {
    ($headers:ident, $header_end:ident, $segments:ident,
        $windows:ident, $before:ident, $reader:ident) => {
        proof! {
            document_spec::random_payload_window_append(
                $reader.bytes@, $headers, $header_end, $before.1, $windows, $before.0,
                $segments@.last(), $reader.position as int,
            );
            $windows = $windows.push(($before.0, $reader.position as int));
        }
    };
}

macro_rules! production_random_access_complete {
    ($input:ident, $headers:ident, $header_windows:ident, $header_end:ident,
        $segments:ident, $payload_windows:ident) => {
        proof! {
            document_spec::random_access_complete_fields(
                $input@, $headers, $header_windows, $header_end, $segments@, $payload_windows,
            );
        }
    };
}

use vstd::prelude::*;

macro_rules! production_file_header_organization {
    ($header:ident) => {
        proof! {
            crate::views::file_header_fields($header);
        }
    };
}

macro_rules! production_file_header_page_count {
    ($header:ident) => {
        proof! {
            crate::views::file_header_fields($header);
        }
    };
}

macro_rules! production_file_header_extended_templates {
    ($header:ident) => {
        proof! {
            crate::views::file_header_fields($header);
        }
    };
}

macro_rules! production_file_header_colour_regions {
    ($header:ident) => {
        proof! {
            crate::views::file_header_fields($header);
        }
    };
}

macro_rules! production_file_header_byte_length {
    ($header:ident) => {
        proof! {
            crate::views::file_header_fields($header);
        }
    };
}

macro_rules! production_file_header_result {
    ($header:ident) => {
        proof! {
            crate::views::file_header_fields($header);
        }
    };
}

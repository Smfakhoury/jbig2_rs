use vstd::prelude::*;

verus! {

pub closed spec fn file_header_organization(
    header: crate::file_header::FileHeader,
) -> crate::file_header::Organization {
    header.organization
}

pub closed spec fn file_header_page_count(
    header: crate::file_header::FileHeader,
) -> Option<u32> {
    header.page_count
}

pub closed spec fn file_header_extended_templates(
    header: crate::file_header::FileHeader,
) -> bool {
    header.extended_templates
}

pub closed spec fn file_header_colour_regions(
    header: crate::file_header::FileHeader,
) -> bool {
    header.colour_regions
}

pub closed spec fn file_header_byte_length(
    header: crate::file_header::FileHeader,
) -> usize {
    header.byte_length
}

pub(crate) proof fn file_header_fields(header: crate::file_header::FileHeader)
    ensures
        file_header_organization(header) == header.organization,
        file_header_page_count(header) == header.page_count,
        file_header_extended_templates(header) == header.extended_templates,
        file_header_colour_regions(header) == header.colour_regions,
        file_header_byte_length(header) == header.byte_length,
{}

pub closed spec fn bitmap_width(bitmap: &crate::bitmap::Bitmap) -> u32 {
    bitmap.width
}

pub closed spec fn bitmap_height(bitmap: &crate::bitmap::Bitmap) -> u32 {
    bitmap.height
}

pub closed spec fn bitmap_stride(bitmap: &crate::bitmap::Bitmap) -> usize {
    bitmap.stride
}

pub closed spec fn bitmap_data(bitmap: &crate::bitmap::Bitmap) -> Seq<u8> {
    bitmap.data@
}

pub(crate) proof fn bitmap_fields(bitmap: &crate::bitmap::Bitmap)
    ensures
        bitmap_width(bitmap) == bitmap.width,
        bitmap_height(bitmap) == bitmap.height,
        bitmap_stride(bitmap) == bitmap.stride,
        bitmap_data(bitmap) == bitmap.data@,
{}

pub closed spec fn page_number(page: &crate::document::PageInfo) -> u32 {
    page.number
}

pub closed spec fn page_width(page: &crate::document::PageInfo) -> u32 {
    page.width
}

pub closed spec fn page_height(page: &crate::document::PageInfo) -> u32 {
    page.height
}

pub closed spec fn page_x_resolution(page: &crate::document::PageInfo) -> u32 {
    page.x_resolution
}

pub closed spec fn page_y_resolution(page: &crate::document::PageInfo) -> u32 {
    page.y_resolution
}

pub closed spec fn page_is_striped(page: &crate::document::PageInfo) -> bool {
    page.striped
}

pub(crate) proof fn page_info_fields(page: &crate::document::PageInfo)
    ensures
        page_number(page) == page.number,
        page_width(page) == page.width,
        page_height(page) == page.height,
        page_x_resolution(page) == page.x_resolution,
        page_y_resolution(page) == page.y_resolution,
        page_is_striped(page) == page.striped,
{}

pub closed spec fn decoded_page_number(page: &crate::decode::Page) -> u32 {
    page.number
}

pub(crate) proof fn decoded_page_fields(page: &crate::decode::Page)
    ensures decoded_page_number(page) == page.number,
{}

}

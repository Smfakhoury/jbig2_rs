#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// Explicit resource limits for untrusted JBIG2 input.
///
/// The defaults implement the approved Phase 1 resource envelope. Callers may
/// use stricter values. Every field is an inclusive maximum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub struct Limits {
    /// Maximum page or region width.
    pub max_width: u32,
    /// Maximum page or region height.
    pub max_height: u32,
    /// Maximum pixels in one page or bitmap.
    pub max_pixels_per_page: u64,
    /// Maximum concurrently decoder-managed bytes.
    pub max_decoder_managed_memory_bytes: u64,
    /// Maximum bytes consumed from one input stream.
    pub max_stream_bytes: u64,
    /// Maximum pages in one decode operation.
    pub max_pages: u32,
    /// Maximum segments in one decode operation.
    pub max_segments: u32,
    /// Maximum symbols in one decode operation.
    pub max_symbols: u32,
    /// Maximum references declared by one segment.
    pub max_references_per_segment: u32,
    /// Maximum references across one decode operation.
    pub max_total_references: u64,
    /// Maximum reference-graph traversal depth.
    pub max_reference_graph_depth: u32,
    /// Maximum nested decoding depth.
    pub max_nesting_depth: u32,
    /// Maximum abstract work units charged by a decode operation.
    pub max_work_units: u64,
    /// Maximum cumulative decoded bitmap bytes.
    pub max_cumulative_decoded_bitmap_bytes: u64,
    /// Maximum bytes retained for segment and global state.
    pub max_retained_state_bytes: u64,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl Default for Limits {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures
            out.max_width == 1_000_000,
            out.max_height == 1_000_000,
            out.max_pixels_per_page == 1_000_000_000,
            out.max_decoder_managed_memory_bytes == 1_073_741_824,
            out.max_stream_bytes == 1_073_741_824,
            out.max_pages == 10_000,
            out.max_segments == 100_000,
            out.max_symbols == 1_000_000,
            out.max_references_per_segment == 4_096,
            out.max_total_references == 1_000_000,
            out.max_reference_graph_depth == 128,
            out.max_nesting_depth == 128,
            out.max_work_units == 2_000_000_000,
            out.max_cumulative_decoded_bitmap_bytes == 4_294_967_296,
            out.max_retained_state_bytes == 536_870_912,
    ))]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    fn default() -> Self {
        Self {
            max_width: 1_000_000,
            max_height: 1_000_000,
            max_pixels_per_page: 1_000_000_000,
            max_decoder_managed_memory_bytes: 1_073_741_824,
            max_stream_bytes: 1_073_741_824,
            max_pages: 10_000,
            max_segments: 100_000,
            max_symbols: 1_000_000,
            max_references_per_segment: 4_096,
            max_total_references: 1_000_000,
            max_reference_graph_depth: 128,
            max_nesting_depth: 128,
            max_work_units: 2_000_000_000,
            max_cumulative_decoded_bitmap_bytes: 4_294_967_296,
            max_retained_state_bytes: 536_870_912,
        }
    }
}

// Included in document_spec to preserve closed-model and field visibility.
verus! {

pub(super) open spec fn sequential_windows(
    bytes: Seq<u8>,
    segments: Seq<crate::document::ParsedSegment<'_>>,
    windows: Seq<(int, int, int)>,
    end: int,
    ordered: bool,
) -> bool {
    &&& windows.len() == segments.len()
    &&& 0 <= end <= bytes.len()
    &&& if windows.len() == 0 { end == 0 } else { windows.last().2 == end }
    &&& forall|index: int| 0 <= index < segments.len() ==> {
        let window = #[trigger] windows[index];
        let segment = segments[index];
        &&& 0 <= window.0 < window.1 <= window.2 <= end
        &&& window.0 == if index == 0 { 0 } else { windows[index - 1].2 }
        &&& crate::spec::segments::segment_header_fields(
            &segment.header, bytes, window.0, window.1,
        )
        &&& payload_window(&segment.header, bytes, window.1, window.2, segment.payload@)
        &&& (ordered && index > 0 ==> segments[index - 1].header.number < segment.header.number)
    }
}

pub(super) proof fn decoder_fields(decoder: &crate::document::Decoder)
    ensures decoder_limits(decoder) == decoder.limits,
{}

pub(super) proof fn document_fields(document: &crate::document::DocumentInfo)
    ensures
        document_organization(document) == document.organization,
        document_declared_page_count(document) == document.declared_page_count,
        document_segment_count(document) == document.segment_count,
        document_pages(document) == document.pages@,
{}

pub(super) proof fn global_fields(globals: &crate::document::GlobalSegments)
    ensures
        global_segment_count(globals) == globals.types@.len(),
        global_bytes(globals) == globals.bytes@,
{}

pub(super) proof fn sequential_window_append<'a>(
    bytes: Seq<u8>,
    segments: Seq<crate::document::ParsedSegment<'a>>,
    windows: Seq<(int, int, int)>,
    end: int,
    segment: crate::document::ParsedSegment<'a>,
    payload_start: int,
    next_end: int,
    ordered: bool,
)
    requires
        sequential_windows(bytes, segments, windows, end, ordered),
        end < payload_start <= next_end <= bytes.len(),
        crate::spec::segments::segment_header_fields(
            &segment.header, bytes, end, payload_start,
        ),
        payload_window(&segment.header, bytes, payload_start, next_end, segment.payload@),
        ordered && segments.len() > 0 ==>
            segments.last().header.number < segment.header.number,
    ensures sequential_windows(
        bytes, segments.push(segment), windows.push((end, payload_start, next_end)),
        next_end, ordered,
    ),
{
    let extended = windows.push((end, payload_start, next_end));
    let appended = segments.push(segment);
    assert forall|index: int| 0 <= index < appended.len() implies {
        let window = #[trigger] extended[index];
        let current = appended[index];
        &&& 0 <= window.0 < window.1 <= window.2 <= next_end
        &&& window.0 == if index == 0 { 0 } else { extended[index - 1].2 }
        &&& crate::spec::segments::segment_header_fields(
            &current.header, bytes, window.0, window.1,
        )
        &&& payload_window(&current.header, bytes, window.1, window.2, current.payload@)
        &&& (ordered && index > 0 ==> appended[index - 1].header.number < current.header.number)
    } by {
        if index < segments.len() {
            assert(extended[index] == windows[index]);
            assert(appended[index] == segments[index]);
        } else {
            assert(index == segments.len());
            if index > 0 {
                assert(extended[index - 1].2 == end);
            }
        }
    }
}

pub(super) proof fn sequential_offset_window(
    bytes: Seq<u8>,
    segments: Seq<crate::document::ParsedSegment<'_>>,
    windows: Seq<(int, int, int)>,
    end: int,
    ordered: bool,
    prefix: nat,
)
    requires sequential_windows(bytes, segments, windows, end, ordered),
        prefix <= segments.len(),
    ensures sequential_offset(segments, prefix) == if prefix == 0 {
        0int
    } else {
        windows[(prefix - 1) as int].2
    },
    decreases prefix,
{
    reveal_with_fuel(sequential_offset, 1);
    if prefix > 0 {
        sequential_offset_window(bytes, segments, windows, end, ordered, (prefix - 1) as nat);
        let segment = segments[(prefix - 1) as int];
        let window = windows[(prefix - 1) as int];
        reveal(crate::spec::segments::segment_header_fields);
        assert(segment.header.byte_length == window.1 - window.0);
        assert(segment.payload@.len() == window.2 - window.1);
    }
}

pub(super) proof fn sequential_complete_fields(
    bytes: Seq<u8>,
    segments: Seq<crate::document::ParsedSegment<'_>>,
    windows: Seq<(int, int, int)>,
    ordered: bool,
)
    requires sequential_windows(bytes, segments, windows, bytes.len() as int, ordered),
    ensures sequential_container_fields(bytes, segments, ordered),
{
    sequential_offset_window(bytes, segments, windows, bytes.len() as int, ordered, segments.len());
    reveal(sequential_container_fields);
    assert forall|index: int| 0 <= index < segments.len() implies {
        let segment = #[trigger] segments[index];
        let start = sequential_offset(segments, index as nat) as int;
        let payload_start = start + segment.header.byte_length;
        let end = sequential_offset(segments, (index + 1) as nat) as int;
        &&& 0 <= start < payload_start <= end <= bytes.len()
        &&& crate::spec::segments::segment_header_fields(
            &segment.header, bytes, start, payload_start,
        )
        &&& payload_window(&segment.header, bytes, payload_start, end, segment.payload@)
        &&& (ordered && index > 0 ==> segments[index - 1].header.number < segment.header.number)
    } by {
        sequential_offset_window(bytes, segments, windows, bytes.len() as int, ordered, index as nat);
        sequential_offset_window(bytes, segments, windows, bytes.len() as int, ordered, (index + 1) as nat);
        let window = windows[index];
        reveal(crate::spec::segments::segment_header_fields);
        assert(segments[index].header.byte_length == window.1 - window.0);
    }
}

}

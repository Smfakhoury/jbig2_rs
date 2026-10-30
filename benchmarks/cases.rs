//! Approved generated inputs and exact results shared by measurement drivers.

use jbig2_decoder::{DecodeError, DecodedDocument, LimitKind, Limits, MalformedError};

#[path = "generated.rs"]
mod generated;

pub(crate) struct Case {
    pub(crate) id: &'static str,
    pub(crate) input: Vec<u8>,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pages: u8,
    pub(crate) pattern: u8,
    pub(crate) outcome: &'static str,
}

pub(crate) fn cases(selected: Option<&str>) -> Vec<Case> {
    let mut result = Vec::new();
    for (id, width, height, pages, alternating) in [
        ("white-64", 64, 64, 1, false),
        ("white-512", 512, 512, 1, false),
        ("white-2048", 2048, 2048, 1, false),
        ("transitions-64", 64, 64, 1, true),
        ("transitions-512", 512, 128, 1, true),
        ("many-pages-32", 64, 64, 32, false),
    ] {
        if selected.is_some_and(|selected| selected != id) {
            continue;
        }
        result.push(Case {
            id,
            input: generated::mmr_document(width, height, pages, alternating),
            width,
            height,
            pages,
            pattern: if alternating { 0x55 } else { 0 },
            outcome: "decoded",
        });
    }
    if selected.is_none_or(|selected| selected == "work-boundary") {
        result.push(Case {
            id: "work-boundary",
            input: generated::all_white_mmr_document(64, 64),
            width: 64,
            height: 64,
            pages: 0,
            pattern: 0,
            outcome: "work-limit",
        });
    }
    if selected.is_none_or(|selected| selected == "late-truncation") {
        let mut truncated = generated::mmr_document(64, 64, 32, false);
        truncated.pop();
        result.push(Case {
            id: "late-truncation",
            input: truncated,
            width: 64,
            height: 64,
            pages: 0,
            pattern: 0,
            outcome: "eof",
        });
    }
    result
}

pub(crate) fn validate(
    case: &Case,
    output: &Result<DecodedDocument, DecodeError>,
) -> Result<usize, String> {
    match (case.outcome, output) {
        ("decoded", Ok(document)) => {
            if document.pages().len() != usize::from(case.pages) {
                return Err(format!("{}: page count changed", case.id));
            }
            let mut bytes = 0;
            for (index, page) in document.pages().iter().enumerate() {
                let bitmap = page.bitmap();
                if page.number() != u32::try_from(index + 1).expect("generated page")
                    || bitmap.width() != case.width
                    || bitmap.height() != case.height
                    || bitmap.stride() != usize::try_from(case.width.div_ceil(8)).expect("stride")
                    || bitmap.bytes().len()
                        != bitmap.stride() * usize::try_from(case.height).expect("height")
                    || bitmap.bytes().iter().any(|byte| *byte != case.pattern)
                {
                    return Err(format!("{}: exact output mismatch", case.id));
                }
                bytes += bitmap.bytes().len();
            }
            Ok(bytes)
        }
        (_, Err(error)) if expected_error(case, error) => Ok(0),
        _ => Err(format!("{}: unexpected result {output:?}", case.id)),
    }
}

pub(crate) fn expected_error(case: &Case, error: &DecodeError) -> bool {
    match (case.outcome, error) {
        ("work-limit", DecodeError::LimitExceeded(limit)) => {
            limit.kind == LimitKind::Work
                && limit.actual == u64::try_from(case.input.len()).expect("input length")
                && limit.limit + 1 == limit.actual
        }
        (
            "eof",
            DecodeError::Malformed(MalformedError::UnexpectedEof {
                offset,
                needed: 4,
                remaining: 3,
            }),
        ) => *offset == u64::try_from(case.input.len() - 16).expect("body offset"),
        _ => false,
    }
}

pub(crate) fn case_limits(case: &Case) -> Result<Limits, Box<dyn std::error::Error>> {
    let mut limits = Limits::default();
    if case.outcome == "work-limit" {
        limits.max_work_units = u64::try_from(case.input.len() - 1)?;
    }
    Ok(limits)
}

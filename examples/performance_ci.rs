//! Generated correctness-gated runtime timings for same-host base/head comparison.

use jbig2_rs::{DecodeError, DecodedDocument, Decoder, LimitKind, Limits, MalformedError};

#[path = "../benchmarks/generated.rs"]
mod generated;
#[path = "../benchmarks/timing.rs"]
mod timing;

struct Case {
    id: &'static str,
    input: Vec<u8>,
    width: u32,
    height: u32,
    pages: u8,
    pattern: u8,
    outcome: &'static str,
}

fn cases() -> Vec<Case> {
    let mut result = Vec::new();
    for (id, width, height, pages, alternating) in [
        ("white-64", 64, 64, 1, false),
        ("white-512", 512, 512, 1, false),
        ("white-2048", 2048, 2048, 1, false),
        ("transitions-64", 64, 64, 1, true),
        ("transitions-512", 512, 128, 1, true),
        ("many-pages-32", 64, 64, 32, false),
    ] {
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
    result.push(Case {
        id: "work-boundary",
        input: generated::all_white_mmr_document(64, 64),
        width: 64,
        height: 64,
        pages: 0,
        pattern: 0,
        outcome: "work-limit",
    });
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
    result
}

fn validate(case: &Case, output: &Result<DecodedDocument, DecodeError>) -> Result<usize, String> {
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
        ("work-limit", Err(DecodeError::LimitExceeded(limit)))
            if limit.kind == LimitKind::Work
                && limit.actual == u64::try_from(case.input.len()).expect("input length")
                && limit.limit + 1 == limit.actual =>
        {
            Ok(0)
        }
        (
            "eof",
            Err(DecodeError::Malformed(MalformedError::UnexpectedEof {
                offset,
                needed: 4,
                remaining: 3,
            })),
        ) if *offset == u64::try_from(case.input.len() - 16).expect("body offset") => Ok(0),
        _ => Err(format!("{}: unexpected result {output:?}", case.id)),
    }
}

struct WorkQuota {
    admitted_limit: Option<u64>,
    rejected_limit: u64,
    rejected_actual: u64,
}

fn work_rejection(
    case: &Case,
    output: &Result<DecodedDocument, DecodeError>,
    limit: u64,
) -> Result<u64, String> {
    match output {
        Err(DecodeError::LimitExceeded(detail))
            if detail.kind == LimitKind::Work && detail.limit == limit && detail.actual > limit =>
        {
            Ok(detail.actual)
        }
        _ => Err(format!(
            "{}: expected typed work rejection at {limit}, got {output:?}",
            case.id
        )),
    }
}

fn measure_work_quota(case: &Case, limits: Limits) -> Result<WorkQuota, String> {
    if case.outcome == "work-limit" {
        return Ok(WorkQuota {
            admitted_limit: None,
            rejected_limit: limits.max_work_units,
            rejected_actual: work_rejection(
                case,
                &Decoder::new(limits).decode(&case.input),
                limits.max_work_units,
            )?,
        });
    }
    let mut lower = 0;
    let mut upper = limits.max_work_units;
    while lower < upper {
        let candidate = lower + (upper - lower) / 2;
        let mut probe = limits;
        probe.max_work_units = candidate;
        let output = Decoder::new(probe).decode(&case.input);
        if matches!(&output, Err(DecodeError::LimitExceeded(detail)) if detail.kind == LimitKind::Work)
        {
            work_rejection(case, &output, candidate)?;
            lower = candidate + 1;
        } else {
            validate(case, &output)?;
            upper = candidate;
        }
    }
    let rejected_limit = lower
        .checked_sub(1)
        .ok_or("missing work rejection boundary")?;
    let mut probe = limits;
    probe.max_work_units = lower;
    validate(case, &Decoder::new(probe).decode(&case.input))?;
    probe.max_work_units = rejected_limit;
    let rejected_actual = work_rejection(
        case,
        &Decoder::new(probe).decode(&case.input),
        rejected_limit,
    )?;
    Ok(WorkQuota {
        admitted_limit: Some(lower),
        rejected_limit,
        rejected_actual,
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let iterations: usize = arguments.first().ok_or("missing iterations")?.parse()?;
    if !(20..=200).contains(&iterations) || arguments.len() > 2 {
        return Err("usage: performance_ci ITERATIONS(20..200) [CASE]".into());
    }
    let selected = arguments.get(1);
    let cases: Vec<_> = cases()
        .into_iter()
        .filter(|case| selected.is_none_or(|id| id == case.id))
        .collect();
    if cases.is_empty() {
        return Err("unknown or empty performance case selection".into());
    }
    let mut records = Vec::new();
    for case in cases {
        let mut limits = Limits::default();
        if case.outcome == "work-limit" {
            limits.max_work_units = u64::try_from(case.input.len() - 1)?;
        }
        let decoder = Decoder::new(limits);
        let bytes = validate(&case, &decoder.decode(&case.input))?;
        let quota = measure_work_quota(&case, limits)?;
        let admitted_limit = quota
            .admitted_limit
            .map_or_else(|| "null".to_owned(), |value| value.to_string());
        let mut samples = timing::sample(
            iterations,
            || decoder.decode(&case.input),
            |output| validate(&case, output).map(|_| ()),
        )?;
        samples.sort_unstable();
        let median = samples[samples.len() / 2];
        let p95 = samples[timing::percentile_index(samples.len(), 95)];
        records.push(format!(
            "{{\"id\":\"{}\",\"input_bytes\":{},\"output_bytes\":{bytes},\"outcome\":\"{}\",\"exact_output_checked\":true,\"work_quota\":{{\"admitted_limit\":{admitted_limit},\"rejected_limit\":{},\"rejected_actual\":{},\"rejected_kind\":\"Work\"}},\"median_ns\":{median},\"p95_ns\":{p95},\"samples_ns\":{samples:?}}}",
            case.id, case.input.len(), case.outcome, quota.rejected_limit, quota.rejected_actual,
        ));
    }
    println!(
        "{{\"schema_version\":2,\"scope\":\"decode call and returned ownership; validation, work-quota probes, hashing and drop outside timing\",\"resource_scope\":\"public max_work_units admission boundaries, not internal work counters, allocations or RSS\",\"iterations\":{iterations},\"cases\":[{}]}}",
        records.join(",")
    );
    Ok(())
}

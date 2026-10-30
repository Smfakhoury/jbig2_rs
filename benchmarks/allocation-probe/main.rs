#![forbid(unsafe_code)]

use allocation_counter::AllocationInfo;
use jbig2_decoder::Decoder;

#[path = "../cases.rs"]
mod case_data;
mod phases;

const INFLATION_BYTES: usize = 64 * 1024;

fn validate_counters(info: &AllocationInfo) -> Result<(), &'static str> {
    if info.count_max > info.count_total || info.bytes_max > info.bytes_total {
        return Err("peak allocator counters exceed their totals");
    }
    if info.count_current != 0 || info.bytes_current != 0 {
        return Err("decode/validation left tracked allocations alive");
    }
    Ok(())
}

fn measure_case(
    case: &case_data::Case,
    inflate: bool,
) -> Result<(AllocationInfo, usize), Box<dyn std::error::Error>> {
    let limits = case_data::case_limits(case)?;
    let mut checked = None;
    let info = allocation_counter::measure(|| {
        let inflation = if inflate {
            vec![0_u8; INFLATION_BYTES]
        } else {
            Vec::new()
        };
        std::hint::black_box(&inflation);
        let output = Decoder::new(limits).decode(&case.input);
        checked = Some(case_data::validate(case, &output));
        drop(output);
        std::hint::black_box(&inflation);
        drop(inflation);
    });
    let output_bytes = checked.ok_or("measurement closure did not execute")??;
    validate_counters(&info)?;
    Ok((info, output_bytes))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let (phase_mode, inflate) = match arguments.as_slice() {
        [_] => (false, false),
        [_, option] if option == "--inflate" => (false, true),
        [_, option] if option == "--phases" => (true, false),
        [_, mode, option] if mode == "--phases" && option == "--inflate-validation" => (true, true),
        _ => {
            return Err(
                "usage: jbig2-allocation-probe CASE [--inflate | --phases [--inflate-validation]]"
                    .into(),
            )
        }
    };
    let selected = case_data::cases(Some(&arguments[0]));
    if selected.len() != 1 {
        return Err(format!("unknown generated case: {}", arguments[0]).into());
    }
    let case = &selected[0];
    if phase_mode {
        return phases::print(case, inflate);
    }
    let (info, output_bytes) = measure_case(case, inflate)?;
    println!(
        "{{\"schema_version\":1,\"id\":\"{}\",\"outcome\":\"{}\",\
         \"input_bytes\":{},\"output_bytes\":{},\"exact_output_checked\":true,\
         \"inflation_bytes\":{},\"tracked_allocations_total\":{},\
         \"peak_live_allocations\":{},\"final_live_allocations\":{},\
         \"total_requested_payload_bytes\":{},\"peak_requested_payload_bytes\":{},\
         \"final_requested_payload_bytes\":{}}}",
        case.id,
        case.outcome,
        case.input.len(),
        output_bytes,
        if inflate { INFLATION_BYTES } else { 0 },
        info.count_total,
        info.count_max,
        info.count_current,
        info.bytes_total,
        info.bytes_max,
        info.bytes_current,
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counters(info: &AllocationInfo) -> (u64, u64, i64, u64, u64, i64) {
        (
            info.count_total,
            info.count_max,
            info.count_current,
            info.bytes_total,
            info.bytes_max,
            info.bytes_current,
        )
    }

    #[test]
    fn real_live_inflation_changes_both_payload_counters_and_restores() {
        let case = case_data::cases(Some("white-64")).pop().unwrap();
        let (baseline, baseline_output) = measure_case(&case, false).unwrap();
        let (inflated, inflated_output) = measure_case(&case, true).unwrap();
        let (restored, restored_output) = measure_case(&case, false).unwrap();
        assert_eq!(baseline_output, inflated_output);
        assert_eq!(baseline_output, restored_output);
        assert_eq!(inflated.count_total, baseline.count_total + 1);
        assert_eq!(inflated.count_max, baseline.count_max + 1);
        assert_eq!(
            inflated.bytes_total,
            baseline.bytes_total + INFLATION_BYTES as u64
        );
        assert_eq!(
            inflated.bytes_max,
            baseline.bytes_max + INFLATION_BYTES as u64
        );
        assert_eq!(counters(&baseline), counters(&restored));
    }

    #[test]
    fn prepared_fixture_allocations_are_outside_the_measurement() {
        let case = case_data::cases(Some("white-64")).pop().unwrap();
        let (baseline, _) = measure_case(&case, false).unwrap();
        let prepared = vec![0_u8; 1024 * 1024];
        std::hint::black_box(&prepared);
        let (measured, _) = measure_case(&case, false).unwrap();
        assert_eq!(counters(&baseline), counters(&measured));
        std::hint::black_box(&prepared);
    }

    #[test]
    fn impossible_peaks_and_unreleased_allocations_are_rejected() {
        let mut info = allocation_counter::measure(|| {});
        info.bytes_max = 1;
        assert!(validate_counters(&info).is_err());
        info.bytes_max = 0;
        info.bytes_current = 1;
        assert!(validate_counters(&info).is_err());
    }
}

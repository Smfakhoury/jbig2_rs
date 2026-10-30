use allocation_counter::AllocationInfo;
use jbig2_decoder::{DecodeError, Decoder, DocumentInfo, Organization};

use super::{case_data, validate_counters, INFLATION_BYTES};

pub(super) struct Measurement {
    pub(super) overall: AllocationInfo,
    pub(super) decode: AllocationInfo,
    pub(super) validation: AllocationInfo,
    pub(super) drop: AllocationInfo,
    pub(super) output_bytes: usize,
    pub(super) inspection: AllocationInfo,
}

fn validate_inspection(
    case: &case_data::Case,
    result: &Result<DocumentInfo, DecodeError>,
) -> Result<(), String> {
    match result {
        Ok(info) if case.outcome == "decoded" => {
            if info.organization() != Organization::Sequential
                || info.declared_page_count() != Some(u32::from(case.pages))
                || info.segment_count() != u32::from(case.pages) * 3 + 1
                || info.pages().len() != usize::from(case.pages)
                || info.pages().iter().enumerate().any(|(index, page)| {
                    page.number() != u32::try_from(index + 1).expect("generated page")
                        || page.width() != case.width
                        || page.height() != case.height
                        || page.x_resolution() != 0
                        || page.y_resolution() != 0
                        || page.is_striped()
                })
            {
                return Err(format!("{}: envelope metadata mismatch", case.id));
            }
            Ok(())
        }
        Err(error) if case_data::expected_error(case, error) => Ok(()),
        _ => Err(format!(
            "{}: unexpected envelope inspection {result:?}",
            case.id
        )),
    }
}

pub(super) fn check_accounting(
    overall: &AllocationInfo,
    phases: [&AllocationInfo; 3],
) -> Result<(), &'static str> {
    validate_counters(overall)?;
    let (mut calls, mut bytes) = (0_u64, 0_u64);
    let (mut live, mut live_bytes) = (0_i128, 0_i128);
    let (mut peak, mut peak_bytes) = (0_i128, 0_i128);
    for phase in phases {
        if phase.count_max > phase.count_total
            || phase.bytes_max > phase.bytes_total
            || i128::from(phase.count_current) > i128::from(phase.count_max)
            || i128::from(phase.bytes_current) > i128::from(phase.bytes_max)
        {
            return Err("invalid phase-local allocation counters");
        }
        calls = calls
            .checked_add(phase.count_total)
            .ok_or("phase allocation total overflow")?;
        bytes = bytes
            .checked_add(phase.bytes_total)
            .ok_or("phase requested-payload total overflow")?;
        peak = peak.max(live + i128::from(phase.count_max));
        peak_bytes = peak_bytes.max(live_bytes + i128::from(phase.bytes_max));
        live += i128::from(phase.count_current);
        live_bytes += i128::from(phase.bytes_current);
        if live < 0 || live_bytes < 0 {
            return Err("phase released allocations outside the measured decode");
        }
    }
    if calls != overall.count_total
        || bytes != overall.bytes_total
        || live != i128::from(overall.count_current)
        || live_bytes != i128::from(overall.bytes_current)
        || peak != i128::from(overall.count_max)
        || peak_bytes != i128::from(overall.bytes_max)
    {
        return Err("phase counters do not reconstruct the independent complete replay");
    }
    Ok(())
}

pub(super) fn measure(
    case: &case_data::Case,
    inflate_validation: bool,
) -> Result<Measurement, Box<dyn std::error::Error>> {
    let limits = case_data::case_limits(case)?;
    let (mut output, mut checked) = (None, None);
    let decode = allocation_counter::measure(|| {
        output = Some(Decoder::new(limits).decode(&case.input));
    });
    let validation = allocation_counter::measure(|| {
        let inflation = if inflate_validation {
            vec![0_u8; INFLATION_BYTES]
        } else {
            Vec::new()
        };
        std::hint::black_box(&inflation);
        checked = Some(match output.as_ref() {
            Some(result) => case_data::validate(case, result),
            None => Err("decode phase did not execute".into()),
        });
        std::hint::black_box(&inflation);
        drop(inflation);
    });
    let drop_phase = allocation_counter::measure(|| drop(output.take()));
    let output_bytes = checked.ok_or("validation phase did not execute")??;
    let mut replay_checked = None;
    let overall = allocation_counter::measure(|| {
        let output = Decoder::new(limits).decode(&case.input);
        let inflation = if inflate_validation {
            vec![0_u8; INFLATION_BYTES]
        } else {
            Vec::new()
        };
        std::hint::black_box(&inflation);
        replay_checked = Some(case_data::validate(case, &output));
        std::hint::black_box(&inflation);
        drop(inflation);
        drop(output);
    });
    if replay_checked.ok_or("complete replay did not execute")?? != output_bytes {
        return Err("phase and complete replay output sizes differ".into());
    }
    let mut inspection_checked = None;
    let inspection = allocation_counter::measure(|| {
        let info = Decoder::new(limits).inspect(&case.input);
        inspection_checked = Some(validate_inspection(case, &info));
        drop(info);
    });
    inspection_checked.ok_or("envelope inspection did not execute")??;
    validate_counters(&inspection)?;
    let result = Measurement {
        overall,
        decode,
        validation,
        drop: drop_phase,
        output_bytes,
        inspection,
    };
    check_accounting(
        &result.overall,
        [&result.decode, &result.validation, &result.drop],
    )
    .map_err(|error| {
        format!(
            "{error}: overall={:?}, decode={:?}, validation={:?}, drop={:?}",
            result.overall, result.decode, result.validation, result.drop,
        )
    })?;
    Ok(result)
}

fn counters_json(info: &AllocationInfo) -> String {
    format!(
        "{{\"allocations_total\":{},\"live_allocations_delta\":{},\
         \"live_allocations_peak\":{},\"requested_payload_bytes_total\":{},\
         \"live_requested_payload_bytes_delta\":{},\"live_requested_payload_bytes_peak\":{}}}",
        info.count_total,
        info.count_current,
        info.count_max,
        info.bytes_total,
        info.bytes_current,
        info.bytes_max,
    )
}

pub(super) fn print(
    case: &case_data::Case,
    inflate: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let result = measure(case, inflate)?;
    println!(
        "{{\"schema_version\":2,\"id\":\"{}\",\"outcome\":\"{}\",\
         \"input_bytes\":{},\"output_bytes\":{},\"exact_output_checked\":true,\
         \"inflation_phase\":{},\"inflation_bytes\":{},\
         \"decode_scope\":\"public_decode_includes_envelope_parsing\",\
         \"phase_peaks_additive\":false,\"measurement_windows_nested\":false,\
         \"decode_executions\":2,\"inspection_executions\":1,\
         \"inspection_scope\":\"public_inspect_envelope_metadata_validation_and_drop_no_entropy\",\
         \"inspection_not_additive_with_decode\":true,\"inspection\":{},\"overall\":{},\
         \"phases\":{{\"decode\":{},\"output_validation\":{},\"output_drop\":{}}}}}",
        case.id,
        case.outcome,
        case.input.len(),
        result.output_bytes,
        if inflate {
            "\"output_validation\""
        } else {
            "null"
        },
        if inflate { INFLATION_BYTES } else { 0 },
        counters_json(&result.inspection),
        counters_json(&result.overall),
        counters_json(&result.decode),
        counters_json(&result.validation),
        counters_json(&result.drop),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_approved_cases_have_exact_non_nested_lifetime_accounting() {
        for case in case_data::cases(None) {
            let result = measure(&case, false).unwrap();
            assert_eq!(result.validation.count_total, 0, "{}", case.id);
            assert_eq!(result.validation.bytes_total, 0, "{}", case.id);
            assert_eq!(result.drop.count_total, 0, "{}", case.id);
            assert_eq!(result.drop.bytes_total, 0, "{}", case.id);
            assert_eq!(result.drop.count_current, -result.decode.count_current);
            assert_eq!(result.drop.bytes_current, -result.decode.bytes_current);
            if case.outcome == "decoded" {
                assert!(result.decode.bytes_current > 0);
                assert!(result.drop.bytes_current < 0);
            }
        }
    }

    #[test]
    fn validation_only_inflation_does_not_change_decode_or_drop() {
        let case = case_data::cases(Some("white-64")).pop().unwrap();
        let before = measure(&case, false).unwrap();
        let inflated = measure(&case, true).unwrap();
        let restored = measure(&case, false).unwrap();
        assert_eq!(before.decode, inflated.decode);
        assert_eq!(before.drop, inflated.drop);
        assert_eq!(before.inspection, inflated.inspection);
        assert_eq!(inflated.validation.count_total, 1);
        assert_eq!(inflated.validation.count_max, 1);
        assert_eq!(inflated.validation.bytes_total, INFLATION_BYTES as u64);
        assert_eq!(inflated.validation.bytes_max, INFLATION_BYTES as u64);
        assert_eq!(
            inflated.overall.bytes_total,
            before.overall.bytes_total + INFLATION_BYTES as u64,
        );
        assert!(
            inflated.overall.bytes_max < inflated.decode.bytes_max + inflated.validation.bytes_max
        );
        assert_eq!(before.overall, restored.overall);
        assert_eq!(before.validation, restored.validation);
        assert_eq!(before.output_bytes, inflated.output_bytes);
    }

    #[test]
    fn missing_release_or_forged_aggregate_peak_is_rejected() {
        let case = case_data::cases(Some("white-64")).pop().unwrap();
        let result = measure(&case, false).unwrap();
        let mut missing = result.drop;
        missing.bytes_current = 0;
        assert!(check_accounting(
            &result.overall,
            [&result.decode, &result.validation, &missing],
        )
        .is_err());
        let mut forged = result.overall;
        forged.bytes_max += 1;
        assert!(
            check_accounting(&forged, [&result.decode, &result.validation, &result.drop],).is_err()
        );
    }

    #[test]
    fn public_inspection_checks_exact_metadata_not_only_success() {
        let mut case = case_data::cases(Some("white-64")).pop().unwrap();
        let result = Decoder::default().inspect(&case.input);
        assert!(validate_inspection(&case, &result).is_ok());
        case.width += 1;
        assert!(validate_inspection(&case, &result).is_err());
        case.width -= 1;
        case.pages += 1;
        assert!(validate_inspection(&case, &result).is_err());
    }
}

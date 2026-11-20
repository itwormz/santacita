use ceh_core::timings::{RetryKind, deadline_millis, parse_timings};
use serde_json::Value;
fn floats(value: Value) -> Value {
    match value {
        Value::Number(n) => serde_json::json!(n.as_f64().unwrap()),
        Value::Array(values) => Value::Array(values.into_iter().map(floats).collect()),
        Value::Object(values) => {
            Value::Object(values.into_iter().map(|(k, v)| (k, floats(v))).collect())
        }
        other => other,
    }
}
#[test]
fn timing_validation_matches_python_defaults_bounds_and_legacy_upgrade() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../contracts/fixtures/timings.json"
    ))
    .unwrap();
    for case in corpus["cases"].as_array().unwrap() {
        let result = parse_timings(&case["input"]);
        if case["expected"]["valid"] == true {
            let actual = serde_json::to_value(result.unwrap()).unwrap();
            assert_eq!(
                floats(actual),
                floats(case["expected"]["value"].clone()),
                "case {}",
                case["id"]
            );
        } else {
            assert!(result.is_err(), "case {}", case["id"]);
        }
    }
}
#[test]
fn retry_schedules_reuse_last_delay_and_validate_sampled_jitter() {
    let timing = parse_timings(&serde_json::json!({"recovery_jitter_seconds":[1,3]})).unwrap();
    assert_eq!(
        timing.recovery_delay(RetryKind::Blocked, 0, 1.).unwrap(),
        31.
    );
    assert_eq!(
        timing
            .recovery_delay(RetryKind::Blocked, usize::MAX, 3.)
            .unwrap(),
        603.
    );
    assert_eq!(
        timing.recovery_delay(RetryKind::RateLimit, 0, 2.).unwrap(),
        182.
    );
    assert!(timing.recovery_delay(RetryKind::Blocked, 0, 0.).is_err());
    assert!(
        timing
            .recovery_delay(RetryKind::Blocked, 0, f64::NAN)
            .is_err()
    );
}
#[test]
fn virtual_deadlines_never_fire_early_and_reject_overflow() {
    assert_eq!(deadline_millis(100, 30.).unwrap(), 30100);
    assert_eq!(deadline_millis(100, 0.0001).unwrap(), 101);
    assert!(deadline_millis(100, -1.).is_err());
    assert!(deadline_millis(100, f64::INFINITY).is_err());
    assert!(deadline_millis(u64::MAX, 1.).is_err());
}

#[test]
fn adaptive_reload_flag_defaults_true_accepts_bool_and_rejects_other_values() {
    assert!(parse_timings(&Value::Null).unwrap().adaptive_reload_on_429());
    assert!(parse_timings(&serde_json::json!({})).unwrap().adaptive_reload_on_429());
    assert!(!parse_timings(&serde_json::json!({"adaptive_reload_on_429": false})).unwrap().adaptive_reload_on_429());
    for bad in [serde_json::json!("yes"), serde_json::json!(1), Value::Null] {
        let error = parse_timings(&serde_json::json!({"adaptive_reload_on_429": bad})).err().unwrap();
        assert_eq!(error.field, Some("adaptive_reload_on_429"));
    }
}

#[test]
fn an_unreachable_site_is_tried_again_at_one_fixed_interval_that_the_profile_can_change() {
    use ceh_core::timings::AssistTimings;
    let defaults = AssistTimings::default();
    for attempt in [0, 1, 7, 500] {
        assert_eq!(defaults.recovery_delay(RetryKind::Unreachable, attempt, 0.0).unwrap(), 300.0, "the same wait every time");
    }
    let one_minute = parse_timings(&serde_json::json!({"unreachable_seconds": 60})).unwrap();
    assert_eq!(one_minute.recovery_delay(RetryKind::Unreachable, 3, 0.0).unwrap(), 60.0);
    let half_hour = parse_timings(&serde_json::json!({"unreachable_seconds": "1800"})).unwrap();
    assert_eq!(half_hour.recovery_delay(RetryKind::Unreachable, 0, 0.0).unwrap(), 1800.0);
    for bad in [serde_json::json!(59.9), serde_json::json!(1801), serde_json::json!([300]), serde_json::json!("soon")] {
        assert!(parse_timings(&serde_json::json!({"unreachable_seconds": bad})).is_err(), "{bad}");
    }
    assert!(parse_timings(&serde_json::json!({"rate_limit_seconds": 180})).unwrap().recovery_delay(RetryKind::Unreachable, 0, 0.0).unwrap() == 300.0, "a profile saved before the setting existed gets the default");
}

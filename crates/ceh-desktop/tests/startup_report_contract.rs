use ceh_desktop::startup_report::{clipboard_text, phase_of, save_failure};

fn at(minute: u32) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(&format!("2026-11-10T10:{minute:02}:00Z")).unwrap().to_utc()
}

#[test]
fn a_failed_start_leaves_a_private_report_that_copies_without_paths_or_profile_data() {
    let root = tempfile::tempdir().unwrap();
    let id = save_failure(root.path(), "/Users/Fictional", "browser_start_failed", true, at(0)).unwrap();
    assert!(id.starts_with("20261110T100000-"));
    let text = clipboard_text(root.path(), "/Users/Fictional", &id).unwrap();
    assert!(text.starts_with(&format!("SantaCita — CEH-{id}")));
    assert!(text.contains("browser_launch") && text.contains("browser_start_failed"));
    assert!(!text.contains("/Users/Fictional"), "home directory is scrubbed: {text}");
    // Only the fixed fields are kept: nothing else can be smuggled in later.
    let file = root.path().join(format!("data/startup-errors/{id}.json"));
    let mut stored: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    stored["profile"] = serde_json::json!({"document_number": "Y1234567X"});
    std::fs::write(&file, stored.to_string()).unwrap();
    assert!(!clipboard_text(root.path(), "/Users/Fictional", &id).unwrap().contains("Y1234567X"));
}

#[test]
fn only_ten_reports_are_kept_and_unknown_ids_are_refused() {
    let root = tempfile::tempdir().unwrap();
    let ids: Vec<_> = (0..12).map(|i| save_failure(root.path(), "/Users/Fictional", "firefox_not_installed", false, at(i)).unwrap()).collect();
    let kept = std::fs::read_dir(root.path().join("data/startup-errors")).unwrap().count();
    assert_eq!(kept, 10);
    assert!(clipboard_text(root.path(), "/Users/Fictional", &ids[11]).is_ok());
    for bad in ["../x", "nope", "20261110T100000-ZZZZZZZZ", ""] {
        assert_eq!(clipboard_text(root.path(), "/Users/Fictional", bad), Err("report_unavailable"));
    }
}

#[test]
fn launch_failures_and_later_failures_are_told_apart() {
    for code in ["firefox_not_installed", "browser_profile_unavailable", "browser_start_failed"] {
        assert_eq!(phase_of(code), "browser_launch");
    }
    for code in ["search_unavailable", "statistics_unavailable", "browser_connection_lost"] {
        assert_eq!(phase_of(code), "worker");
    }
}

#[test]
fn a_stop_that_waits_for_a_person_leaves_a_report_with_the_province_and_service_but_no_identity() {
    use ceh_desktop::startup_report::save_session_stop;
    let root = tempfile::tempdir().unwrap();
    let report = serde_json::json!({
        "version": "0.4.3", "time": "2026-11-18T10:00:00Z", "system": "macos", "architecture": "aarch64",
        "phase": "session_stop", "error_type": "dispatched_page_unchanged", "message": "stopped",
        "browser": {"version": "Chrome 157"},
        "session": {
            "stop": {"reason": "dispatched_page_unchanged"},
            "settings": {"province": "Valencia", "procedure": "POLICIA-TOMA DE HUELLAS", "access_mode": "certificate"},
            "recent": [{"kind": "page", "page": "province"}],
        },
        // Not part of the report: dropped on the way in.
        "profile": {"document_number": "X1234567L", "full_name": "Maria Garcia"},
    });
    let id = save_session_stop(root.path(), "/Users/Fictional", report, at(0)).unwrap();
    let text = clipboard_text(root.path(), "/Users/Fictional", &id).unwrap();
    assert!(text.contains("Valencia") && text.contains("POLICIA-TOMA DE HUELLAS"), "{text}");
    assert!(text.contains("dispatched_page_unchanged") && text.contains("session_stop"));
    assert!(!text.contains("X1234567L") && !text.contains("Maria"), "{text}");
}

#[test]
fn the_newest_saved_report_can_be_found_for_the_about_window() {
    use ceh_desktop::startup_report::{latest_id, save_session_stop};
    let root = tempfile::tempdir().unwrap();
    assert_eq!(latest_id(root.path(), "/Users/Fictional"), None);
    let first = save_failure(root.path(), "/Users/Fictional", "browser_start_failed", true, at(1)).unwrap();
    assert_eq!(latest_id(root.path(), "/Users/Fictional").as_deref(), Some(first.as_str()));
    let report = serde_json::json!({"version": "0.4.3", "phase": "session_stop", "session": {"stop": {"reason": "x"}}});
    let second = save_session_stop(root.path(), "/Users/Fictional", report, at(5)).unwrap();
    assert_eq!(latest_id(root.path(), "/Users/Fictional").as_deref(), Some(second.as_str()));
    // Something that is not a report is never offered.
    std::fs::write(root.path().join("data/startup-errors/notes.json"), "{}").unwrap();
    assert_eq!(latest_id(root.path(), "/Users/Fictional").as_deref(), Some(second.as_str()));
}

#[test]
fn the_newest_report_is_the_last_one_saved_even_within_the_same_second() {
    use ceh_desktop::startup_report::latest_id;
    let root = tempfile::tempdir().unwrap();
    let mut last = String::new();
    // Same second, random suffixes: the order of saving decides, not the order of the suffixes.
    for _ in 0..12 {
        last = save_failure(root.path(), "/Users/Fictional", "browser_start_failed", true, at(3)).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(15));
        assert_eq!(latest_id(root.path(), "/Users/Fictional").as_deref(), Some(last.as_str()));
    }
    assert_eq!(latest_id(root.path(), "/Users/Fictional").as_deref(), Some(last.as_str()));
}

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
        // Equal modification times too (a coarse file system clock): only the saved order may decide.
        let file = std::fs::File::options().write(true).open(root.path().join(format!("data/startup-errors/{last}.json"))).unwrap();
        file.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000)).unwrap();
        assert_eq!(latest_id(root.path(), "/Users/Fictional").as_deref(), Some(last.as_str()));
    }
    assert_eq!(latest_id(root.path(), "/Users/Fictional").as_deref(), Some(last.as_str()));
}

#[test]
fn only_a_regular_report_file_can_be_the_newest() {
    use ceh_desktop::startup_report::latest_id;
    let root = tempfile::tempdir().unwrap();
    let real = save_failure(root.path(), "/Users/Fictional", "browser_start_failed", true, at(1)).unwrap();
    // A folder with a report's name must not hide the real report from About.
    std::fs::create_dir(root.path().join("data/startup-errors/20991231T235959-aaaaaaaa.json")).unwrap();
    assert_eq!(latest_id(root.path(), "/Users/Fictional").as_deref(), Some(real.as_str()));
}

#[test]
fn unknown_nested_fields_of_the_session_part_never_survive_saving_or_copying() {
    use ceh_desktop::startup_report::save_session_stop;
    let root = tempfile::tempdir().unwrap();
    let report = serde_json::json!({
        "version": "0.4.3", "phase": "session_stop", "error_type": "x", "message": "m",
        "session": {
            "stop": {"reason": "no_handler_for_page", "summary": "s", "document_number": "X1234567L"},
            "settings": {"province": "Valencia", "procedure": "TOMA DE HUELLAS", "full_name": "Maria Garcia", "phone": "600123456"},
            "header": {"app": "0.4.3", "os": "macos", "profile": {"persistent": true, "path": "/Users/Fictional/secret"}},
            "identity": {"email": "maria@example.com"},
            "recent": [
                {"kind": "page", "page": "province", "t": "10:00", "url": "https://x.es/acCitar?nie=X1234567L",
                 "flow": {"phase": "held", "hold_reason": "r", "note": "Maria"},
                 "detail": {"facts": {"office_count": 0, "name": "Maria"}, "page": {"endpoint": "acCitar", "title": "Maria Garcia", "visible_selectors": ["#form"],
                            "shape": {"forms": 1, "owner": "Maria"}, "frames": [{"status": 200, "text": "Maria Garcia"}]}}},
                {"kind": "step", "effects": ["hold"], "receipts": [{"action": "province", "outcome": "dispatched", "cookie": "SID=1"}], "debug": "Maria"}
            ],
        },
    });
    let id = save_session_stop(root.path(), "/Users/Fictional", report, at(0)).unwrap();
    let text = clipboard_text(root.path(), "/Users/Fictional", &id).unwrap();
    for kept in ["Valencia", "TOMA DE HUELLAS", "no_handler_for_page", "province", "#form", "acCitar", "dispatched"] {
        assert!(text.contains(kept), "{kept} lost from {text}");
    }
    for private in ["X1234567L", "Maria", "600123456", "example.com", "secret", "SID=1", "x.es"] {
        assert!(!text.contains(private), "{private} leaked into {text}");
    }
    // A file edited on disk is scrubbed again when it is read.
    let file = root.path().join(format!("data/startup-errors/{id}.json"));
    let mut stored: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    stored["session"]["settings"]["phone"] = serde_json::json!("600123456");
    std::fs::write(&file, stored.to_string()).unwrap();
    assert!(!clipboard_text(root.path(), "/Users/Fictional", &id).unwrap().contains("600123456"));
}

#[test]
fn a_newer_file_that_copy_cannot_read_does_not_hide_the_last_good_report() {
    use ceh_desktop::startup_report::latest_id;
    let root = tempfile::tempdir().unwrap();
    let good = save_failure(root.path(), "/Users/Fictional", "browser_start_failed", true, at(1)).unwrap();
    let dir = root.path().join("data/startup-errors");
    std::fs::write(dir.join("20991231T235959-00000000.json"), "not json at all").unwrap();
    std::fs::write(dir.join("20991231T235958-00000000.json"), vec![b' '; 200 * 1024]).unwrap();
    assert_eq!(latest_id(root.path(), "/Users/Fictional").as_deref(), Some(good.as_str()));
}

#[test]
fn the_saving_order_survives_a_clock_that_goes_backwards() {
    use ceh_desktop::startup_report::latest_id;
    let root = tempfile::tempdir().unwrap();
    let mut last = String::new();
    // Twelve reports whose timestamps decrease: the last one saved is still the newest and none of the new ones is pruned.
    for minute in (10..22).rev() {
        last = save_failure(root.path(), "/Users/Fictional", "browser_start_failed", true, at(minute)).unwrap();
        assert_eq!(latest_id(root.path(), "/Users/Fictional").as_deref(), Some(last.as_str()), "minute {minute}");
    }
    assert!(clipboard_text(root.path(), "/Users/Fictional", &last).is_ok());
}

#[test]
fn a_reason_that_is_not_a_plain_code_is_dropped_from_the_report() {
    use ceh_desktop::startup_report::save_session_stop;
    let root = tempfile::tempdir().unwrap();
    let report = serde_json::json!({
        "version": "0.4.3", "phase": "session_stop", "error_type": "x", "message": "m",
        "session": {"recent": [
            {"kind": "stale_document", "reason": "target_changed_after_scroll", "streak": 1},
            {"kind": "stale_document", "reason": "Hola Maria Garcia, su NIE X1234567L", "streak": 2},
            {"kind": "step", "effects": ["hold"], "detail": "before=\"Maria Garcia\" after=\"x\""},
        ]},
    });
    let id = save_session_stop(root.path(), "/Users/Fictional", report, at(0)).unwrap();
    let text = clipboard_text(root.path(), "/Users/Fictional", &id).unwrap();
    assert!(text.contains("target_changed_after_scroll"), "{text}");
    for private in ["Maria", "Garcia", "X1234567L", "Hola"] {
        assert!(!text.contains(private), "{private} leaked into {text}");
    }
}

mod start_failure_reason {
    use ceh_desktop::startup_report::{StartFacts, clipboard_text, note_start_failure, save_failure, save_failure_with, take_start_failure};

    fn at(minute: u32) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339(&format!("2026-11-10T10:{minute:02}:00Z")).unwrap().to_utc()
    }
    fn stored(root: &std::path::Path, id: &str) -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(root.join(format!("data/startup-errors/{id}.json"))).unwrap()).unwrap()
    }

    #[test]
    fn a_start_failure_report_names_the_browser_and_the_reason() {
        let root = tempfile::tempdir().unwrap();
        let facts = StartFacts { family: "chromium", found: true, reason: Some("startup_timeout"), detail: None };
        let id = save_failure_with(root.path(), "/Users/Fictional", "browser_start_failed", &facts, at(0)).unwrap();
        let report = stored(root.path(), &id);
        assert_eq!(report["browser"], serde_json::json!({"family": "chromium", "exists": true, "start_failure": "startup_timeout"}));
        let copied = clipboard_text(root.path(), "/Users/Fictional", &id).unwrap();
        assert!(copied.contains("\"start_failure\": \"startup_timeout\"") && copied.contains("\"family\": \"chromium\""), "{copied}");
    }
    #[test]
    fn a_failure_without_a_known_reason_leaves_the_field_out_and_the_old_call_still_means_firefox() {
        let root = tempfile::tempdir().unwrap();
        let facts = StartFacts { family: "edge", found: false, reason: None, detail: None };
        let id = save_failure_with(root.path(), "/Users/Fictional", "browser_not_installed", &facts, at(1)).unwrap();
        assert_eq!(stored(root.path(), &id)["browser"], serde_json::json!({"family": "edge", "exists": false}));
        let old = save_failure(root.path(), "/Users/Fictional", "firefox_not_installed", false, at(2)).unwrap();
        assert_eq!(stored(root.path(), &old)["browser"], serde_json::json!({"family": "firefox", "exists": false}));
    }
    #[test]
    fn a_report_of_a_browser_that_ended_at_once_carries_its_exit_status_and_version() {
        let root = tempfile::tempdir().unwrap();
        let detail = ceh_desktop::startup_report::StartDetail { browser_version: "158.0.1".into(), elapsed_ms: 412, exit_code: Some(0xC000_0135_u32 as i32) };
        let facts = StartFacts { family: "firefox", found: true, reason: Some("exited_early"), detail: Some(detail) };
        let id = save_failure_with(root.path(), "/Users/Fictional", "browser_start_failed", &facts, at(3)).unwrap();
        assert_eq!(
            stored(root.path(), &id)["browser"]["start_detail"],
            serde_json::json!({"browser_version": "158.0.1", "elapsed_ms": 412, "exit_code": -1073741515, "exit_code_hex": "0xc0000135"})
        );
        let copied = clipboard_text(root.path(), "/Users/Fictional", &id).unwrap();
        assert!(copied.contains("\"exit_code_hex\": \"0xc0000135\"") && copied.contains("\"start_failure\": \"exited_early\""), "{copied}");
    }
    #[test]
    fn a_report_without_a_status_leaves_the_status_out() {
        let root = tempfile::tempdir().unwrap();
        let detail = ceh_desktop::startup_report::StartDetail { browser_version: "158.0.1".into(), elapsed_ms: 20_005, exit_code: None };
        let facts = StartFacts { family: "firefox", found: true, reason: Some("startup_timeout"), detail: Some(detail) };
        let id = save_failure_with(root.path(), "/Users/Fictional", "browser_start_failed", &facts, at(4)).unwrap();
        assert_eq!(stored(root.path(), &id)["browser"]["start_detail"], serde_json::json!({"browser_version": "158.0.1", "elapsed_ms": 20005}));
    }
    #[test]
    fn a_noted_start_failure_is_taken_once_and_only_by_the_thread_that_noted_it() {
        assert_eq!(take_start_failure(), None);
        note_start_failure("firefox", "unavailable");
        std::thread::spawn(|| assert_eq!(take_start_failure(), None, "another worker has its own")).join().unwrap();
        assert_eq!(take_start_failure(), Some(("firefox", "unavailable")));
        assert_eq!(take_start_failure(), None, "taken once");
    }
}

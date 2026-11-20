use ceh_desktop::navigation_failure;

#[test]
fn a_page_the_person_closed_while_it_loaded_is_a_stop_not_a_failure() {
    assert_eq!(navigation_failure(false, false), None);
    assert_eq!(navigation_failure(false, true), None);
}

#[test]
fn a_rejected_first_page_names_the_missing_certificate_chain_when_it_could_not_be_prepared() {
    assert_eq!(navigation_failure(true, true), Some("browser_certificate_cache_failed"));
    assert_eq!(navigation_failure(true, false), Some("browser_navigation_failed"));
}

#[test]
fn the_macos_bundle_may_load_the_firefox_libraries_that_prepare_the_certificate_chain() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let config = std::fs::read_to_string(dir.join("tauri.conf.json")).unwrap();
    assert!(config.contains("\"entitlements\": \"entitlements.plist\""), "the bundle is signed with entitlements");
    let plist = std::fs::read_to_string(dir.join("entitlements.plist")).unwrap();
    assert!(plist.contains("com.apple.security.cs.disable-library-validation"), "hardened runtime rejects Mozilla's NSS otherwise");
}

#[test]
fn a_browser_error_after_the_person_closed_the_window_is_a_stop_not_an_error() {
    use ceh_desktop::closed_window_stop;
    for code in ["browser_connection_lost", "browser_observation_failed", "browser_action_failed", "browser_recovery_required", "browser_navigation_failed"] {
        assert_eq!(closed_window_stop(Err(code), "stop", || false), Ok("stop"), "{code}");
    }
    // The window is still there: the error stays an error, and no probing happens for other kinds of errors.
    assert_eq!(closed_window_stop(Err("browser_action_failed"), "stop", || true), Err("browser_action_failed"));
    assert_eq!(closed_window_stop(Err("statistics_unavailable"), "stop", || panic!("must not probe")), Err("statistics_unavailable"));
    assert_eq!(closed_window_stop(Ok("done"), "stop", || panic!("must not probe")), Ok("done"));
}

#[test]
fn the_certificate_reminder_rings_once_five_seconds_after_the_click_even_if_the_page_cannot_be_read() {
    use ceh_desktop::certificate_reminder_due;
    assert!(!certificate_reminder_due(None, false, 99_999));
    assert!(!certificate_reminder_due(Some(10_000), false, 14_999), "not yet");
    assert!(certificate_reminder_due(Some(10_000), false, 15_000));
    assert!(!certificate_reminder_due(Some(10_000), true, 60_000), "once");
}

#[test]
fn only_the_availability_request_counts_as_a_check_not_reloads_of_the_waiting_page() {
    use ceh_core::model::ActionKind;
    use ceh_desktop::counts_as_check;
    assert!(counts_as_check(ActionKind::Request), "the submitted Solicitar cita");
    for action in [ActionKind::RequestReload, ActionKind::ApplicantReload, ActionKind::Applicant, ActionKind::Province, ActionKind::Recovery, ActionKind::Cookie] {
        assert!(!counts_as_check(action), "{action:?}");
    }
}

mod window_closing_settles {
    use ceh_desktop::closed_window_stop_settled;
    use std::cell::Cell;

    #[test]
    fn a_window_still_listed_while_it_shuts_down_is_a_stop_once_it_is_gone() {
        // The read failed because the person closed the window; the browser needs a moment to disappear.
        let probes = Cell::new(0);
        let result = closed_window_stop_settled(Err("browser_observation_failed"), "stop", 12, || {
            probes.set(probes.get() + 1);
            probes.get() < 4
        }, || true);
        assert_eq!(result, Ok("stop"));
        assert_eq!(probes.get(), 4, "stops asking as soon as the window is gone");
    }
    #[test]
    fn a_window_that_stays_open_keeps_the_error_after_the_wait() {
        let probes = Cell::new(0);
        let pauses = Cell::new(0);
        let result = closed_window_stop_settled(Err("browser_observation_failed"), "stop", 5, || {
            probes.set(probes.get() + 1);
            true
        }, || { pauses.set(pauses.get() + 1); true });
        assert_eq!(result, Err("browser_observation_failed"));
        assert_eq!(probes.get(), 5);
        assert_eq!(pauses.get(), 4, "no pause after the last look");
    }
    #[test]
    fn a_closed_window_needs_no_waiting_and_other_results_never_probe() {
        let pauses = Cell::new(0);
        assert_eq!(closed_window_stop_settled(Err("browser_action_failed"), "stop", 12, || false, || { pauses.set(1); true }), Ok("stop"));
        assert_eq!(pauses.get(), 0);
        assert_eq!(closed_window_stop_settled(Err("statistics_unavailable"), "stop", 12, || panic!("must not probe"), || true), Err("statistics_unavailable"));
        assert_eq!(closed_window_stop_settled(Ok("done"), "stop", 12, || panic!("must not probe"), || true), Ok("done"));
    }
    #[test]
    fn pressing_stop_ends_the_wait_and_is_a_stop_not_an_error() {
        let probes = Cell::new(0);
        let result = closed_window_stop_settled(Err("browser_observation_failed"), "stop", 80, || {
            probes.set(probes.get() + 1);
            true
        }, || probes.get() < 3);
        assert_eq!(result, Ok("stop"), "the person ended the search: it is theirs, not a failure");
        assert_eq!(probes.get(), 3, "the third pause was refused: no more looks");
    }
    #[test]
    fn a_first_navigation_that_fails_while_the_window_closes_is_a_stop() {
        use ceh_desktop::navigation_failure_settled;
        let probes = Cell::new(0);
        assert_eq!(navigation_failure_settled(false, 12, || { probes.set(probes.get() + 1); probes.get() < 4 }, || true), None);
        assert_eq!(probes.get(), 4);
        // The window stays: the error keeps its kind, told apart by the certificate cache as before.
        assert_eq!(navigation_failure_settled(false, 3, || true, || true), Some("browser_navigation_failed"));
        assert_eq!(navigation_failure_settled(true, 3, || true, || true), Some("browser_certificate_cache_failed"));
        // Stop ends the wait, and the search with it.
        let looks = Cell::new(0);
        assert_eq!(navigation_failure_settled(false, 80, || { looks.set(looks.get() + 1); true }, || looks.get() < 2), None);
        assert_eq!(looks.get(), 2);
    }
    #[test]
    fn a_stop_pressed_during_the_last_pause_is_still_a_stop() {
        use ceh_desktop::requested_stop_is_a_stop;
        let stop = Cell::new(false);
        // The pause that returns `true` is the last one: Stop is pressed while it sleeps, the last look still
        // finds the window open, and the wait ends with the browser error.
        let looks = Cell::new(0);
        let settled = closed_window_stop_settled(Err("browser_observation_failed"), "stop", 3, || { looks.set(looks.get() + 1); true }, || {
            if looks.get() == 2 { stop.set(true); }
            true
        });
        assert_eq!(settled, Err("browser_observation_failed"));
        assert_eq!(requested_stop_is_a_stop(settled, "stop", stop.get()), Ok("stop"));
        // Without a Stop the error stays; only a browser error becomes a stop; a result that is fine stays.
        assert_eq!(requested_stop_is_a_stop(Err("browser_observation_failed"), "stop", false), Err("browser_observation_failed"));
        assert_eq!(requested_stop_is_a_stop(Err("statistics_unavailable"), "stop", true), Err("statistics_unavailable"));
        assert_eq!(requested_stop_is_a_stop(Ok("done"), "stop", true), Ok("done"));
    }
}

#[test]
fn a_site_that_did_not_open_in_a_window_that_is_still_there_is_waited_out_not_an_error() {
    use ceh_desktop::site_did_not_open;
    assert!(site_did_not_open(navigation_failure(true, false)), "the browser shows its own error page and the search tries again");
    assert!(!site_did_not_open(navigation_failure(true, true)), "a missing certificate chain is a setting, not a wait");
    assert!(!site_did_not_open(navigation_failure(false, false)), "a closed window is a stop");
    assert!(!site_did_not_open(Some("browser_start_failed")));
}

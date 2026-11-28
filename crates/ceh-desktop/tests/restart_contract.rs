use ceh_core::{model::PageKind, recovery::RecoveryAction};
use ceh_desktop::restart::{RestartKind, RestartRequest};

#[test]
fn only_restart_actions_make_a_request_and_their_kind_follows_python() {
    let pass = RestartRequest::from_action(&RecoveryAction::Restart { clear_profile: false, reopen: false }, PageKind::Loading).unwrap();
    assert_eq!(pass.kind(), RestartKind::NewPass);
    let blocked = RestartRequest::from_action(&RecoveryAction::Restart { clear_profile: true, reopen: false }, PageKind::Blocked).unwrap();
    assert_eq!(blocked.kind(), RestartKind::NewSession);
    assert!(blocked.clear_profile);
    let clave = RestartRequest::from_action(&RecoveryAction::Restart { clear_profile: false, reopen: true }, PageKind::ServerError).unwrap();
    assert_eq!(clave.kind(), RestartKind::NewSession);
    assert!(!clave.clear_profile);
    for other in [RecoveryAction::Hold, RecoveryAction::Reload, RecoveryAction::GetCurrent, RecoveryAction::GetEntry, RecoveryAction::ActivateOwnControl] {
        assert_eq!(RestartRequest::from_action(&other, PageKind::Blocked), None);
    }
}

#[test]
fn the_journal_reasons_are_pythons() {
    let reason = |action, kind| RestartRequest::from_action(&action, kind).unwrap().reason;
    assert_eq!(reason(RecoveryAction::Restart { clear_profile: true, reopen: false }, PageKind::Blocked), "блокировка сайта");
    assert_eq!(reason(RecoveryAction::Restart { clear_profile: false, reopen: true }, PageKind::ServerError), "ошибка портала Cl@ve — профиль и cookies сохранены");
    assert_eq!(reason(RecoveryAction::Restart { clear_profile: false, reopen: false }, PageKind::Loading), "таймаут ответа сайта — новый проход");
}

#[test]
fn the_journal_note_tells_whether_the_window_is_kept() {
    let pass = RestartRequest::from_action(&RecoveryAction::Restart { clear_profile: false, reopen: false }, PageKind::Loading).unwrap();
    assert_eq!(pass.note(), "Новый проход: таймаут ответа сайта — новый проход — окно и профиль сохранены");
    let blocked = RestartRequest::from_action(&RecoveryAction::Restart { clear_profile: true, reopen: false }, PageKind::Blocked).unwrap();
    assert_eq!(blocked.note(), "Новая сессия: блокировка сайта");
}

mod action_failure {
    use ceh_core::model::PageKind;
    use ceh_desktop::restart::{ActionFailure, FailureStep};
    use std::time::{Duration, Instant};

    #[test]
    fn a_failed_action_with_a_working_connection_starts_a_new_pass_in_the_same_window() {
        let mut failures = ActionFailure::default();
        assert_eq!(failures.decide(true, Some(PageKind::Auth), Instant::now()), FailureStep::NewPass);
    }

    #[test]
    fn a_lost_connection_starts_a_new_browser_with_the_same_profile() {
        let mut failures = ActionFailure::default();
        assert_eq!(failures.decide(false, Some(PageKind::Applicant), Instant::now()), FailureStep::NewBrowser);
    }

    #[test]
    fn a_lost_connection_near_a_booking_keeps_the_window_for_the_person() {
        for kind in [PageKind::Office, PageKind::Contact, PageKind::Booking, PageKind::Captcha, PageKind::AuthWait] {
            let mut failures = ActionFailure::default();
            assert_eq!(failures.decide(false, Some(kind), Instant::now()), FailureStep::Stop, "{kind:?}");
        }
    }

    #[test]
    fn a_failure_that_keeps_coming_back_stops_the_search_after_three_recoveries() {
        let mut failures = ActionFailure::default();
        let start = Instant::now();
        for minute in 0..3 {
            assert_eq!(failures.decide(true, None, start + Duration::from_secs(minute * 60)), FailureStep::NewPass);
        }
        assert_eq!(failures.decide(true, None, start + Duration::from_secs(4 * 60)), FailureStep::Stop);
    }

    #[test]
    fn failures_far_apart_are_recovered_every_time() {
        let mut failures = ActionFailure::default();
        let start = Instant::now();
        for hour in 0..10 {
            assert_eq!(failures.decide(true, None, start + Duration::from_secs(hour * 3600)), FailureStep::NewPass);
        }
    }
}

mod browser_after_block {
    use ceh_desktop::restart::{browser_after_block, other_browser};

    #[test]
    fn the_other_browser_is_of_the_other_kind_first_and_never_the_current_one() {
        assert_eq!(other_browser("chrome", &["chrome", "brave", "firefox"]), Some("firefox"));
        assert_eq!(other_browser("firefox", &["firefox", "brave", "chrome"]), Some("chrome"));
        assert_eq!(other_browser("chrome", &["chrome", "brave"]), Some("brave"));
        assert_eq!(other_browser("chrome", &["chrome"]), None);
        assert_eq!(other_browser("chrome", &[]), None);
    }

    #[test]
    fn a_cleared_profile_first_then_the_other_browser_then_back_to_the_profile_browser() {
        let next = |current, streak| browser_after_block("chrome", current, Some("firefox"), streak, true);
        assert_eq!(next("chrome", 1), "chrome", "the first block keeps the browser, its profile is cleared");
        assert_eq!(next("chrome", 2), "firefox", "the second moves to the other browser");
        assert_eq!(next("firefox", 3), "chrome", "the third goes back to the profile's browser");
        assert_eq!(next("chrome", 4), "chrome", "and stays there while the blocks go on");
    }

    #[test]
    fn a_browser_that_got_past_a_block_stays_until_the_next_run_of_blocks_moves_it_back() {
        let next = |current, streak| browser_after_block("chrome", current, Some("firefox"), streak, true);
        assert_eq!(next("firefox", 1), "firefox");
        assert_eq!(next("firefox", 2), "chrome");
    }

    #[test]
    fn without_the_option_or_another_browser_the_profile_browser_is_kept() {
        for streak in 1..5 {
            assert_eq!(browser_after_block("chrome", "chrome", Some("firefox"), streak, false), "chrome");
            assert_eq!(browser_after_block("chrome", "chrome", None, streak, true), "chrome");
        }
    }
}

use ceh_core::{flow::*, model::*, recovery::*, timings::parse_timings};
use serde_json::json;
fn observation(kind: PageKind) -> Observation {
    Observation {
        document: DocumentId::new("same-document").unwrap(),
        kind,
        method: RequestMethod::Get,
        operation_code: false,
        variant: String::new(),
    }
}
fn engine() -> FlowEngine {
    FlowEngine::new(SessionId::new("s").unwrap(), SearchOptions::default())
}
fn facts() -> PageFacts {
    PageFacts {
        recovery: RecoveryFacts {
            official_site: true,
            ..Default::default()
        },
        ..Default::default()
    }
}
#[test]
fn changing_a_pending_recovery_recalculates_from_its_original_start() {
    let mut e = engine();
    let o = observation(PageKind::RateLimited);
    assert_eq!(
        e.observe_with_facts(o.clone(), facts(), 1000).unwrap(),
        vec![FlowEffect::WaitUntil(181000)]
    );
    e.apply_timings(
        parse_timings(&json!({"rate_limit_seconds":240})).unwrap(),
        30.,
    )
    .unwrap();
    assert_eq!(
        e.observe_with_facts(o.clone(), facts(), 10000).unwrap(),
        vec![FlowEffect::WaitUntil(241000)]
    );
    assert!(
        e.observe_with_facts(o.clone(), facts(), 240999)
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        e.observe_with_facts(o, facts(), 241000).unwrap().as_slice(),
        [FlowEffect::Recover {
            action: RecoveryAction::GetCurrent,
            ..
        }]
    ));
}
#[test]
fn proven_unsent_request_can_retry_after_the_configured_pause() {
    let mut e = engine();
    let o = observation(PageKind::RequestMenu);
    let a = e.observe_with_facts(o.clone(), facts(), 0).unwrap();
    let FlowEffect::Activate { guard, action } = a[0].clone() else {
        panic!()
    };
    e.acknowledge_dispatch(&guard, action, DispatchOutcome::NotDispatched)
        .unwrap();
    assert_eq!(
        e.observe_with_facts(o.clone(), facts(), 1).unwrap(),
        vec![FlowEffect::WaitUntil(2000)]
    );
    assert!(matches!(
        e.observe_with_facts(o, facts(), 2000).unwrap().as_slice(),
        [FlowEffect::Activate {
            action: ActionKind::Request,
            ..
        }]
    ));
}
#[test]
fn ambiguous_request_is_not_replayed_and_gets_a_safe_bounded_exit() {
    let mut e = engine();
    let o = observation(PageKind::RequestMenu);
    let a = e.observe_with_facts(o.clone(), facts(), 0).unwrap();
    let FlowEffect::Activate { guard, action } = a[0].clone() else {
        panic!()
    };
    e.acknowledge_dispatch(&guard, action, DispatchOutcome::Unknown)
        .unwrap();
    assert_eq!(
        e.observe_with_facts(o.clone(), facts(), 1).unwrap(),
        vec![FlowEffect::WaitUntil(5000)]
    );
    assert_eq!(
        e.observe_with_facts(o.clone(), facts(), 5000).unwrap(),
        vec![FlowEffect::WaitUntil(10000)]
    );
    assert!(matches!(
        e.observe_with_facts(o, facts(), 10000).unwrap().as_slice(),
        [FlowEffect::Recover {
            action: RecoveryAction::GetEntry,
            ..
        }]
    ));
}
#[test]
fn new_wait_samples_do_not_move_existing_recovery_but_next_document_uses_them() {
    let timings =
        parse_timings(&json!({"rate_limit_seconds":180,"recovery_jitter_seconds":[0,10]})).unwrap();
    let mut e = FlowEngine::with_timings(
        SessionId::new("s").unwrap(),
        SearchOptions::default(),
        timings,
        30.,
    )
    .unwrap();
    e.set_wait_samples(30., 2.).unwrap();
    let mut o = observation(PageKind::RateLimited);
    assert_eq!(
        e.observe_with_facts(o.clone(), facts(), 0).unwrap(),
        vec![FlowEffect::WaitUntil(182000)]
    );
    e.set_wait_samples(60., 9.).unwrap();
    assert!(
        e.observe_with_facts(o.clone(), facts(), 1000)
            .unwrap()
            .is_empty()
    );
    o.document = DocumentId::new("next").unwrap();
    assert!(
        e.observe_with_facts(o.clone(), facts(), 2000)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        e.observe_with_facts(o, facts(), 2001).unwrap(),
        vec![FlowEffect::WaitUntil(191001)]
    );
}
#[test]
fn pause_then_settings_change_keeps_original_recovery_elapsed_time() {
    let mut e = engine();
    let o = observation(PageKind::RateLimited);
    e.observe_with_facts(o.clone(), facts(), 0).unwrap();
    e.pause(1000).unwrap();
    e.resume(11000).unwrap();
    e.apply_timings(
        parse_timings(&json!({"rate_limit_seconds":240})).unwrap(),
        30.,
    )
    .unwrap();
    assert_eq!(
        e.observe_with_facts(o, facts(), 12000).unwrap(),
        vec![FlowEffect::WaitUntil(250000)]
    );
}
#[test]
fn contact_ambiguity_never_discards_the_current_form() {
    let mut e = engine();
    let o = observation(PageKind::Contact);
    let a = e.observe_with_facts(o.clone(), facts(), 0).unwrap();
    let FlowEffect::Prepare { guard, action } = a[0].clone() else {
        panic!()
    };
    e.complete_preparation(&guard, action, PreparationOutcome::Completed)
        .unwrap();
    e.observe_with_facts(o.clone(), facts(), 1).unwrap();
    e.acknowledge_dispatch(&guard, action, DispatchOutcome::Unknown)
        .unwrap();
    assert_eq!(
        e.observe_with_facts(o.clone(), facts(), 5001).unwrap(),
        vec![FlowEffect::Hold, FlowEffect::Alert]
    );
    assert!(e.observe_with_facts(o, facts(), 100000).unwrap().is_empty());
}
#[test]
fn post_dispatch_wait_starts_after_the_browser_ack_not_before_input() {
    let mut e = engine();
    let o = observation(PageKind::RequestMenu);
    let a = e.observe_with_facts(o.clone(), facts(), 0).unwrap();
    let FlowEffect::Activate { guard, action } = a[0].clone() else {
        panic!()
    };
    e.acknowledge_dispatch_at(&guard, action, DispatchOutcome::Dispatched, 10000)
        .unwrap();
    assert_eq!(
        e.observe_with_facts(o, facts(), 10001).unwrap(),
        vec![FlowEffect::WaitUntil(15000)]
    );
}
#[test]
fn unsent_retries_are_bounded_and_booking_cancels_the_timeout_exit() {
    let mut e = engine();
    let o = observation(PageKind::RequestMenu);
    for at in [0, 2000, 7000] {
        let a = e.observe_with_facts(o.clone(), facts(), at).unwrap();
        let FlowEffect::Activate { guard, action } = a[0].clone() else {
            panic!("expected bounded retry")
        };
        e.acknowledge_dispatch_at(&guard, action, DispatchOutcome::NotDispatched, at)
            .unwrap();
        let deadline = if at == 0 { 2000 } else { at + 5000 };
        assert_eq!(
            e.observe_with_facts(o.clone(), facts(), at + 1).unwrap(),
            vec![FlowEffect::WaitUntil(deadline)]
        );
    }
    assert_eq!(
        e.observe_with_facts(o.clone(), facts(), 12000).unwrap(),
        vec![FlowEffect::WaitUntil(17000)]
    );
    assert!(matches!(
        e.observe_with_facts(observation(PageKind::Booking), facts(), 16000)
            .unwrap()
            .as_slice(),
        [FlowEffect::Handoff, FlowEffect::Alert]
    ));
    assert!(e.observe_with_facts(o, facts(), 17000).unwrap().is_empty());
}
#[test]
fn a_late_old_document_receipt_does_not_retime_the_new_document() {
    let mut e = engine();
    let first = observation(PageKind::RequestMenu);
    let a = e.observe_with_facts(first, facts(), 0).unwrap();
    let FlowEffect::Activate { guard, action } = a[0].clone() else {
        panic!()
    };
    let mut next = observation(PageKind::RequestMenu);
    next.document = DocumentId::new("new-document").unwrap();
    e.observe_with_facts(next.clone(), facts(), 1).unwrap();
    e.acknowledge_dispatch_at(&guard, action, DispatchOutcome::NotDispatched, 2)
        .unwrap();
    assert_eq!(
        e.observe_with_facts(next, facts(), 3).unwrap(),
        vec![FlowEffect::Hold]
    );
}
#[test]
fn an_interrupted_preparation_is_typed_again_twice_then_the_person_is_called() {
    // Preparation only types into the page: it is repeated on the same page after the unsent-click pauses
    // (2 s, then 5 s), counted from when the slow entry returned. Only then does the contact step wait for a person.
    for outcome in [PreparationOutcome::Failed, PreparationOutcome::Unknown] {
        let mut e = engine();
        let o = observation(PageKind::Contact);
        let mut now = 0;
        for wait in [2000, 5000] {
            let effects = e.observe_with_facts(o.clone(), facts(), now).unwrap();
            let FlowEffect::Prepare { guard, action } = effects[0].clone() else { panic!("{effects:?}") };
            now += 45000;
            e.complete_preparation_at(&guard, action, outcome, now).unwrap();
            assert_eq!(e.observe_with_facts(o.clone(), facts(), now + 1).unwrap(), vec![FlowEffect::WaitUntil(now + wait)]);
            assert!(e.observe_with_facts(o.clone(), facts(), now + wait - 1).unwrap().is_empty());
            now += wait;
        }
        let effects = e.observe_with_facts(o.clone(), facts(), now).unwrap();
        let FlowEffect::Prepare { guard, action } = effects[0].clone() else { panic!("{effects:?}") };
        e.complete_preparation_at(&guard, action, outcome, now + 1).unwrap();
        assert_eq!(e.observe_with_facts(o.clone(), facts(), now + 2).unwrap(), vec![FlowEffect::WaitUntil(now + 1 + 5000)]);
        assert_eq!(e.observe_with_facts(o, facts(), now + 1 + 5000).unwrap(), vec![FlowEffect::Hold, FlowEffect::Alert]);
    }
}

#[test]
fn unsent_no_slots_aliases_retry_instead_of_holding_and_remain_bounded() {
    for (mode, own_control, action) in [
        (NoSlotsMode::Salir, true, ActionKind::NoSlotsSalir),
        (NoSlotsMode::Reload, false, ActionKind::Recovery),
    ] {
        let mut e = FlowEngine::new(
            SessionId::new("s").unwrap(),
            SearchOptions {
                no_slots_mode: mode,
                ..SearchOptions::default()
            },
        );
        let o = observation(PageKind::RequestNoSlots);
        let f = PageFacts {
            appointment_ready_at: Some(0),
            recovery: RecoveryFacts {
                official_site: true,
                own_control,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(
            e.observe_with_facts(o.clone(), f, 0).unwrap(),
            vec![FlowEffect::WaitUntil(30000)]
        );
        for (at, retry_at, next_at) in [
            (30000, 32000, 62000),
            (62000, 67000, 97000),
            (97000, 102000, 107000),
        ] {
            let effects = e.observe_with_facts(o.clone(), f, at).unwrap();
            let guard = match &effects[0] {
                FlowEffect::Activate {
                    guard,
                    action: actual,
                } => {
                    assert_eq!(*actual, action);
                    guard.clone()
                }
                FlowEffect::Recover {
                    guard,
                    action: RecoveryAction::GetEntry,
                } => {
                    assert_eq!(action, ActionKind::Recovery);
                    guard.clone()
                }
                other => panic!("expected retry intent, got {other:?}"),
            };
            e.acknowledge_dispatch_at(&guard, action, DispatchOutcome::NotDispatched, at)
                .unwrap();
            assert_eq!(
                e.observe_with_facts(o.clone(), f, at + 1).unwrap(),
                vec![FlowEffect::WaitUntil(retry_at)]
            );
            assert_eq!(
                e.observe_with_facts(o.clone(), f, retry_at).unwrap(),
                vec![FlowEffect::WaitUntil(next_at)]
            );
        }
        assert!(matches!(
            e.observe_with_facts(o.clone(), f, 107000)
                .unwrap()
                .as_slice(),
            [FlowEffect::Recover {
                action: RecoveryAction::GetEntry,
                ..
            }]
        ));
        assert!(e.observe_with_facts(o, f, 107001).unwrap().is_empty());
    }
}

fn unreachable_page(document: &str) -> Observation {
    Observation { document: DocumentId::new(document).unwrap(), variant: "dnsNotFound".into(), ..observation(PageKind::Unreachable) }
}
#[test]
fn a_site_that_does_not_open_is_tried_again_at_the_same_interval_every_time() {
    let mut e = engine();
    let site_down = PageFacts::default();
    assert_eq!(e.observe_with_facts(unreachable_page("first"), site_down, 1000).unwrap(), vec![FlowEffect::WaitUntil(301_000)], "5 minutes by default");
    assert!(e.observe_with_facts(unreachable_page("first"), site_down, 300_999).unwrap().is_empty());
    let effects = e.observe_with_facts(unreachable_page("first"), site_down, 301_000).unwrap();
    assert!(matches!(effects.as_slice(), [FlowEffect::Recover { action: RecoveryAction::GetEntry, .. }]), "{effects:?}");
    // The try fails again and the browser shows its error page once more: the wait does not grow.
    let again = e.observe_with_facts(unreachable_page("second"), site_down, 302_000).unwrap();
    assert_eq!(again, vec![FlowEffect::WaitUntil(602_000)], "{again:?}");
    let third = e.observe_with_facts(unreachable_page("second"), site_down, 602_000).unwrap();
    assert!(matches!(third.as_slice(), [FlowEffect::Recover { action: RecoveryAction::GetEntry, .. }]), "{third:?}");
    let fourth = e.observe_with_facts(unreachable_page("third"), site_down, 603_000).unwrap();
    assert_eq!(fourth, vec![FlowEffect::WaitUntil(903_000)], "{fourth:?}");
}
#[test]
fn the_interval_for_an_unreachable_site_comes_from_the_profile() {
    let mut e = engine();
    e.apply_timings(parse_timings(&json!({"unreachable_seconds": 90})).unwrap(), 30.).unwrap();
    assert_eq!(e.observe_with_facts(unreachable_page("first"), PageFacts::default(), 0).unwrap(), vec![FlowEffect::WaitUntil(90_000)]);
}
#[test]
fn a_site_certificate_the_browser_could_not_verify_gets_a_new_browser_soon_not_the_five_minute_wait() {
    // Chromium remembers a failed check of the site's certificate for the life of its process, so opening the start page
    // again in it fails the same way: a new browser on the same profile (cookies kept) checks it again.
    for code in ["ERR_CERT_AUTHORITY_INVALID", "ERR_CERT_DATE_INVALID"] {
        let mut e = engine();
        let page = Observation { document: DocumentId::new("cert").unwrap(), variant: code.into(), ..observation(PageKind::Unreachable) };
        assert_eq!(e.observe_with_facts(page.clone(), PageFacts::default(), 1000).unwrap(), vec![FlowEffect::WaitUntil(3000)], "{code}");
        let effects = e.observe_with_facts(page, PageFacts::default(), 3000).unwrap();
        assert!(
            matches!(effects.as_slice(), [FlowEffect::Recover { action: RecoveryAction::Restart { clear_profile: false, reopen: true }, .. }]),
            "{code}: {effects:?}"
        );
    }
    // Any other error page of the browser keeps the profile's wait and opens the start page again in the same browser.
    let mut e = engine();
    assert_eq!(e.observe_with_facts(unreachable_page("first"), PageFacts::default(), 1000).unwrap(), vec![FlowEffect::WaitUntil(301_000)]);
    assert!(ceh_core::recovery::is_certificate_error("ERR_CERT_COMMON_NAME_INVALID"));
    assert!(!ceh_core::recovery::is_certificate_error("ERR_CONNECTION_RESET"));
    assert!(!ceh_core::recovery::is_certificate_error("dnsNotFound"));
}
